use crate::error::{FinancialHistoryError, Result};
use crate::llm::{client::GeminiClient, prompts, types::*};
use crate::schema::*;
use crate::{process_financial_history, verify_accounting_equation};
use futures::{future::try_join_all, try_join};
use log::{error, warn};
use std::collections::HashMap;
use std::fs;
use tokio::sync::mpsc::Sender;

pub struct FinancialExtractor {
    client: GeminiClient,
    model: String,
}

impl FinancialExtractor {
    pub fn new(client: GeminiClient, model: impl Into<String>) -> Self {
        Self {
            client,
            model: model.into(),
        }
    }

    pub async fn extract(
        &self,
        documents: &[RemoteDocument],
        progress: Option<Sender<ExtractionEvent>>,
    ) -> Result<FinancialHistoryConfig> {
        self.send_event(&progress, ExtractionEvent::Starting).await;

        // 1. Map Documents to IDs
        let (manifest, id_map) = create_document_manifest(documents);

        // --- STEP 1: DISCOVERY ---
        self.send_event(&progress, ExtractionEvent::Step1Discovery)
            .await;
        let discovery = self.run_discovery(documents, &manifest).await?;

        // --- STEP 2: PARALLEL EXTRACTION ---
        self.send_event(&progress, ExtractionEvent::Step2Extraction)
            .await;

        // Prepare context with the NORMALIZED DATES
        let start_date_str = discovery
            .forecast_start_date
            .map(|d| d.format("%Y-%m-%d").to_string())
            .unwrap_or_else(|| "Unknown (Extract all available)".to_string());

        let end_date_str = discovery
            .forecast_end_date
            .map(|d| d.format("%Y-%m-%d").to_string())
            .unwrap_or_else(|| "Unknown".to_string());

        let org_ctx = format!(
            "Organization: {}\nFY End Month: {}\nGlobal Forecast Start Date: {}\nGlobal Forecast End Date: {}",
            discovery.organization_name,
            discovery.fiscal_year_end_month,
            start_date_str,
            end_date_str
        );

        let (bs_result, is_result) = try_join!(
            self.extract_balance_sheet(
                documents,
                &manifest,
                &org_ctx,
                &discovery.balance_sheet_account_names
            ),
            self.extract_income_statement(
                documents,
                &manifest,
                &org_ctx,
                &discovery.income_statement_account_names
            )
        )?;

        // --- STEP 3: ASSEMBLY & ID RESOLUTION ---
        self.send_event(&progress, ExtractionEvent::Step3Assembly)
            .await;

        let mut config = FinancialHistoryConfig {
            organization_name: discovery.organization_name,
            fiscal_year_end_month: discovery.fiscal_year_end_month,
            balance_sheet: bs_result.balance_sheet,
            income_statement: is_result.income_statement,
        };

        // Remap IDs "0", "1" back to real filenames
        self.resolve_document_ids(&mut config, &id_map);

        // --- STEP 4: FINAL VALIDATION & PATCHING ---
        config = self.validate_and_fix(config, documents, &progress).await?;

        self.send_event(&progress, ExtractionEvent::Success).await;
        Ok(config)
    }

    /// Refines an existing financial history based on a natural language instruction.
    ///
    /// This method allows you to make targeted changes to extracted data using natural
    /// language instructions. The LLM will generate JSON Patch operations to fulfill
    /// the instruction while maintaining data integrity.
    ///
    /// # Arguments
    ///
    /// * `config` - The current financial history configuration to refine
    /// * `documents` - Documents to provide as context to the LLM. You can:
    ///   - Reuse the original extraction documents
    ///   - Upload and pass additional supplementary documents
    ///   - Use entirely different documents for major revisions
    /// * `instruction` - Natural language instruction describing the desired changes
    ///   Examples: "Add Marketing Expenses account with Q1-Q4 data from page 5"
    ///   "Update Cash balance for Dec 2023 to $85,000"
    ///   "Remove duplicate Revenue entries"
    /// * `progress` - Optional channel for receiving progress events
    ///
    /// # Returns
    ///
    /// Returns the refined configuration with changes applied via JSON Patch operations.
    /// All changes are validated against the schema and accounting constraints.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // Upload additional document with updated forecast
    /// let updated_doc = client.upload_document("updated_forecast.pdf").await?;
    /// let mut docs = original_documents.clone();
    /// docs.push(updated_doc);
    ///
    /// // Refine the history with natural language instruction
    /// let refined = extractor.refine_history(
    ///     config,
    ///     &docs,
    ///     "Add Q1 2024 forecast data from the updated forecast document",
    ///     Some(progress_tx)
    /// ).await?;
    /// ```
    pub async fn refine_history(
        &self,
        config: FinancialHistoryConfig,
        documents: &[RemoteDocument],
        instruction: &str,
        progress: Option<Sender<ExtractionEvent>>,
    ) -> Result<FinancialHistoryConfig> {
        self.send_event(
            &progress,
            ExtractionEvent::CorrectionNeeded {
                reason: format!("Refining history: {}", instruction),
            },
        )
        .await;

        // Use the abstracted patch loop with a custom prompt strategy
        self.run_patch_loop(
            config,
            documents,
            &progress,
            "Refinement",
            |cfg, patch_errors| {
                // Determine if we show tables (usually helpful for context)
                let tables = generate_markdown_tables(cfg);

                // Construct the prompt context
                let mut context = format!(
                    "## USER INSTRUCTION\n\
                    The user wants to refine the data with the following instruction:\n\
                    \"{}\"\n\n\
                    ## VISUAL REVIEW TABLES\n{}",
                    instruction, tables
                );

                if !patch_errors.is_empty() {
                    context.push_str(&format!(
                        "\n\n## ⚠️ PREVIOUS PATCH ERRORS\n\
                        Your previous attempt to apply a patch failed with these errors:\n\
                        ```\n{}\n```\n\
                        Please adjust your JSON Pointer paths. Remember to use 'op: add' on root arrays if creating new accounts.",
                        patch_errors.join("\n")
                    ));
                }

                context
            },
        ).await
    }

    // --- SUB-ROUTINES ---

    async fn run_discovery(
        &self,
        docs: &[RemoteDocument],
        manifest: &str,
    ) -> Result<DiscoveryResponse> {
        let schema =
            DiscoveryResponse::get_schema().map_err(FinancialHistoryError::SerializationError)?;

        let prompt = format!(
            "{}\n\n{}\n\n## YOUR TASK\nAnalyze the provided financial documents and extract:\n\
            1. Organization name and fiscal year end\n\
            2. All Balance Sheet leaf account names\n\
            3. All Income Statement leaf account names\n\n\
            Return valid JSON matching the DiscoveryResponse schema.",
            prompts::SYSTEM_PROMPT_DISCOVERY,
            manifest
        );

        let content = self
            .call_llm_with_retry(&prompt, docs, Some(schema), "Discovery")
            .await?;

        serde_json::from_str(&content).map_err(|e| {
            // Dump raw output on parse failure
            let _ = fs::write("debug_discovery_raw_output.json", &content);
            error!("Failed to parse Discovery JSON. Raw output dumped to debug_discovery_raw_output.json");
            FinancialHistoryError::SerializationError(e)
        })
    }

    async fn extract_balance_sheet(
        &self,
        docs: &[RemoteDocument],
        manifest: &str,
        org_ctx: &str,
        accounts: &[String],
    ) -> Result<BalanceSheetExtractionResponse> {
        if accounts.is_empty() {
            return Ok(BalanceSheetExtractionResponse {
                balance_sheet: Vec::new(),
            });
        }

        let batches = distribute_into_batches(accounts, 25);
        let total_batches = batches.len();

        let schema = BalanceSheetExtractionResponse::get_schema()
            .map_err(FinancialHistoryError::SerializationError)?;

        let futures = batches
            .into_iter()
            .enumerate()
            .map(|(i, batch)| {
                let batch_index = i + 1;
                let batch_accounts = batch.clone();
                let manifest = manifest.to_string();
                let org_ctx = org_ctx.to_string();
                let schema = schema.clone();

                async move {
                    let account_list = batch_accounts.join("\n- ");
                    let batch_context = format!(
                        "## BATCH CONTEXT\nProcessing Batch {} of {}.\n\
                         EXTRACT DATA ONLY FOR THE ACCOUNTS LISTED BELOW.\n\
                         If you see data for accounts NOT in this list, IGNORE IT.",
                        batch_index, total_batches
                    );

                    let prompt = format!(
                        "{}\n\n{}\n\n{}\n\n## CONTEXT\n{}\n\n## EXTRACT SNAPSHOTS FOR THESE ACCOUNTS\n\
                        Extract balance sheet snapshots for each of the following accounts.\n\
                        Use the EXACT names below. Do not modify or rename them.\n\n- {}\n\n\
                        ## CRITICAL REMINDERS\n\
                        - Set EXACTLY ONE account as `is_balancing_account: true` (prefer Cash)\n\
                        - Use document IDs (\"0\", \"1\", etc.) in `source.document`\n\
                        - Extract ALL available dates (2023, 2022, mid-year if present)\n\
                        - Choose appropriate interpolation: Linear, Step, or Curve\n\n\
                        Return valid JSON matching the BalanceSheetExtractionResponse schema.",
                        prompts::SYSTEM_PROMPT_BS_EXTRACT,
                        batch_context,
                        manifest,
                        org_ctx,
                        account_list
                    );

                    let stage_label = format!("Balance Sheet Batch {}/{}", batch_index, total_batches);
                    let content = self
                        .call_llm_with_retry(&prompt, docs, Some(schema), &stage_label)
                        .await?;

                    let response: BalanceSheetExtractionResponse =
                        serde_json::from_str(&content).map_err(|e| {
                            // Dump raw output on parse failure
                            let _ = fs::write("debug_balance_sheet_raw_output.json", &content);
                            error!(
                                "Failed to parse Balance Sheet JSON. Raw output dumped to debug_balance_sheet_raw_output.json"
                            );
                            FinancialHistoryError::SerializationError(e)
                        })?;

                    Ok::<Vec<BalanceSheetAccount>, FinancialHistoryError>(response.balance_sheet)
                }
            });

        let results: Vec<Vec<BalanceSheetAccount>> = try_join_all(futures).await?;

        Ok(BalanceSheetExtractionResponse {
            balance_sheet: results.into_iter().flatten().collect(),
        })
    }

    async fn extract_income_statement(
        &self,
        docs: &[RemoteDocument],
        manifest: &str,
        org_ctx: &str,
        accounts: &[String],
    ) -> Result<IncomeStatementExtractionResponse> {
        if accounts.is_empty() {
            return Ok(IncomeStatementExtractionResponse {
                income_statement: Vec::new(),
            });
        }

        let batches = distribute_into_batches(accounts, 25);
        let total_batches = batches.len();

        let schema = IncomeStatementExtractionResponse::get_schema()
            .map_err(FinancialHistoryError::SerializationError)?;

        let futures = batches
            .into_iter()
            .enumerate()
            .map(|(i, batch)| {
                let batch_index = i + 1;
                let batch_accounts = batch.clone();
                let manifest = manifest.to_string();
                let org_ctx = org_ctx.to_string();
                let schema = schema.clone();

                async move {
                    let account_list = batch_accounts.join("\n- ");
                    let batch_context = format!(
                        "## BATCH CONTEXT\nProcessing Batch {} of {}.\n\
                         EXTRACT DATA ONLY FOR THE ACCOUNTS LISTED BELOW.\n\
                         If you see data for accounts NOT in this list, IGNORE IT.",
                        batch_index, total_batches
                    );

                    let prompt = format!(
                        "{}\n\n{}\n\n{}\n\n## CONTEXT\n{}\n\n## EXTRACT CONSTRAINTS FOR THESE ACCOUNTS\n\
                        Extract period constraints for each of the following accounts.\n\
                        Use the EXACT names below. Do not modify or rename them.\n\n- {}\n\n\
                        ## CRITICAL REMINDERS\n\
                        - Extract ALL available periods (annual, quarterly, monthly if present)\n\
                        - Use document IDs (\"0\", \"1\", etc.) in `source.document`\n\
                        - Choose appropriate seasonality: Flat (most common), RetailPeak, SummerHigh, or SaasGrowth\n\
                        - Do NOT extract calculated totals (Gross Profit, Net Income, EBITDA)\n\
                        - Include overlapping periods (e.g., both monthly AND annual totals)\n\n\
                        Return valid JSON matching the IncomeStatementExtractionResponse schema.",
                        prompts::SYSTEM_PROMPT_IS_EXTRACT,
                        batch_context,
                        manifest,
                        org_ctx,
                        account_list
                    );

                    let stage_label = format!("IS Batch {}/{}", batch_index, total_batches);
                    let content = self
                        .call_llm_with_retry(&prompt, docs, Some(schema), &stage_label)
                        .await?;

                    let response: IncomeStatementExtractionResponse =
                        serde_json::from_str(&content).map_err(|e| {
                            // Dump raw output on parse failure
                            let _ = fs::write("debug_income_statement_raw_output.json", &content);
                            error!(
                                "Failed to parse Income Statement JSON. Raw output dumped to debug_income_statement_raw_output.json"
                            );
                            FinancialHistoryError::SerializationError(e)
                        })?;

                    Ok::<Vec<IncomeStatementAccount>, FinancialHistoryError>(
                        response.income_statement,
                    )
                }
            });

        let results: Vec<Vec<IncomeStatementAccount>> = try_join_all(futures).await?;

        Ok(IncomeStatementExtractionResponse {
            income_statement: results.into_iter().flatten().collect(),
        })
    }

    // --- UTILITIES ---

    async fn call_llm_with_retry(
        &self,
        prompt: &str,
        docs: &[RemoteDocument],
        schema: Option<serde_json::Value>,
        stage_name: &str,
    ) -> Result<String> {
        let messages = vec![Content::user_with_files(prompt.to_string(), docs)];
        let max_retries = 3;

        for attempt in 1..=max_retries {
            match self
                .client
                .generate_content(
                    &self.model,
                    "You are a financial data extractor.",
                    messages.clone(),
                    schema.clone(),
                    "application/json",
                    stage_name,
                )
                .await
            {
                Ok(response) => {
                    let cleaned = extract_first_json_object(&response);
                    return Ok(cleaned);
                }
                Err(e) => {
                    warn!("{} attempt {} failed: {}", stage_name, attempt, e);
                    if attempt == max_retries {
                        return Err(e);
                    }
                    tokio::time::sleep(std::time::Duration::from_secs(2 * attempt as u64)).await;
                }
            }
        }
        Err(FinancialHistoryError::ExtractionFailed(format!(
            "{} failed after retries",
            stage_name
        )))
    }

    fn resolve_document_ids(
        &self,
        config: &mut FinancialHistoryConfig,
        id_map: &HashMap<String, String>,
    ) {
        for acc in &mut config.balance_sheet {
            for snap in &mut acc.snapshots {
                if let Some(src) = &mut snap.source {
                    if let Some(real_name) = id_map.get(&src.document_name) {
                        src.document_name = real_name.clone();
                    }
                }
            }
        }
        for acc in &mut config.income_statement {
            for constraint in &mut acc.constraints {
                if let Some(src) = &mut constraint.source {
                    if let Some(real_name) = id_map.get(&src.document_name) {
                        src.document_name = real_name.clone();
                    }
                }
            }
        }
    }

    /// REFACTORED: Now uses the abstract `run_patch_loop`
    async fn validate_and_fix(
        &self,
        config: FinancialHistoryConfig,
        documents: &[RemoteDocument],
        progress: &Option<Sender<ExtractionEvent>>,
    ) -> Result<FinancialHistoryConfig> {
        self.run_patch_loop(
            config,
            documents,
            progress,
            "Validation",
            |cfg, patch_errors| {
                // 1. Check for logical validation errors
                let logic_error = validate_financial_logic(cfg).err();

                // 2. Check for suspicious duplicates (Soft check)
                let duplicate_warning = detect_suspicious_duplicates(cfg);

                // 3. Generate markdown tables if no validation errors
                let tables = if logic_error.is_none() {
                    Some(generate_markdown_tables(cfg))
                } else {
                    None
                };

                // Construct specific Validation Prompt
                let mut context = String::new();

                if let Some(error) = logic_error {
                    context.push_str(&format!(
                        "\n\n## 🔴 CRITICAL LOGIC ERRORS\n\
                        The following errors MUST be fixed via JSON Patch:\n\
                        ```\n{}\n```",
                        error
                    ));
                }

                if !patch_errors.is_empty() {
                    context.push_str(&format!(
                        "\n\n## ⚠️ PREVIOUS PATCH ERRORS\n\
                        Some of your previous operations failed. \
                        It is likely you tried to modify an account that doesn't exist yet, or used an invalid path.\n\
                        **Errors:**\n```\n{}\n```\n\
                        **Instructions:**\n\
                        - If adding a NEW account, use `op: add` on the ROOT array (e.g. `/balance_sheet/-`), NOT `replace`.\n\
                        - Ensure account names in paths are exact.",
                        patch_errors.join("\n")
                    ));
                }

                if let Some(dup_warn) = duplicate_warning {
                    context.push_str(&format!(
                        "\n\n## ⚠️ POTENTIAL DATA INTEGRITY ISSUES\n\
                        We detected potentially suspicious duplicate values. \
                        Please verify against the attached documents if these are correct or double-counting:\n\
                        ```\n{}\n```\n\
                        If these are valid (e.g. coincidentally same value), ignore them. \
                        If they are errors, remove the duplicate constraint via patch.",
                        dup_warn
                    ));
                }

                if let Some(tbl) = tables {
                    context.push_str(&format!("\n\n## VISUAL REVIEW TABLES\n{}", tbl));
                }

                context
            }
        ).await
    }

    /// ABSTRACTED LOOP: Handles the "Prompt -> Patch -> Apply -> Retry" cycle
    async fn run_patch_loop<F>(
        &self,
        mut config: FinancialHistoryConfig,
        documents: &[RemoteDocument],
        progress: &Option<Sender<ExtractionEvent>>,
        label: &str,
        context_generator: F,
    ) -> Result<FinancialHistoryConfig>
    where
        F: Fn(&FinancialHistoryConfig, &[String]) -> String,
    {
        let max_fix_attempts = 5;
        let mut last_patch_errors: Vec<String> = Vec::new();

        for attempt in 1..=max_fix_attempts {
            self.send_event(progress, ExtractionEvent::Validating { attempt })
                .await;

            // 1. Generate the specific context (Validation errors OR User Instruction)
            let specific_context = context_generator(&config, &last_patch_errors);

            // 2. Request Patch
            let patch_result = self
                .request_patch(&config, documents, &specific_context, attempt, label)
                .await;

            match patch_result {
                Ok(patch_json) => {
                    // 3. Apply Patch Sequentially
                    let apply_result =
                        self.apply_patch_sequentially(&mut config, &patch_json, attempt);

                    let (any_applied, new_errors) = match apply_result {
                        Ok(result) => result,
                        Err(e) => {
                            let err_msg = format!(
                                "{} patch attempt {} could not be deserialized: {}",
                                label, attempt, e
                            );
                            warn!("{}", err_msg);
                            self.send_event(
                                progress,
                                ExtractionEvent::Retry {
                                    attempt,
                                    error: err_msg.clone(),
                                },
                            )
                            .await;

                            last_patch_errors = vec![err_msg];
                            if attempt == max_fix_attempts {
                                return Ok(config);
                            }
                            continue;
                        }
                    };

                    if !new_errors.is_empty() {
                        self.send_event(
                            progress,
                            ExtractionEvent::CorrectionNeeded {
                                reason: format!("{} patch operations failed", new_errors.len()),
                            },
                        )
                        .await;
                    } else if any_applied {
                        self.send_event(
                            progress,
                            ExtractionEvent::CorrectionNeeded {
                                reason: format!("Applied {} improvements", label),
                            },
                        )
                        .await;
                    } else if attempt > 1 {
                        // No changes made in a fix loop -> we are stuck or done
                        return Ok(config);
                    }

                    last_patch_errors = new_errors;
                }
                Err(e) => {
                    warn!("Failed to get {} patch (attempt {}): {}", label, attempt, e);
                    if attempt == max_fix_attempts {
                        return Ok(config); // Return what we have
                    }
                    tokio::time::sleep(std::time::Duration::from_secs(2 * attempt as u64)).await;
                }
            }
        }

        Ok(config)
    }

    /// Helper to generate the actual LLM request for a patch
    async fn request_patch(
        &self,
        config: &FinancialHistoryConfig,
        documents: &[RemoteDocument],
        specific_context: &str,
        attempt: usize,
        label: &str,
    ) -> Result<String> {
        let schema = FinancialHistoryConfig::get_gemini_response_schema()
            .map_err(FinancialHistoryError::SerializationError)?;

        let config_json = serde_json::to_string_pretty(config)
            .map_err(FinancialHistoryError::SerializationError)?;

        // Base System Prompt for Patching
        let mut prompt = prompts::SYSTEM_PROMPT_VALIDATION.to_string();

        // Inject the specific context (Validation errors or User Instruction)
        prompt.push_str(specific_context);

        prompt.push_str(&format!(
            "\n\n## CURRENT CONFIGURATION\n```json\n{}\n```",
            config_json
        ));

        prompt.push_str(&format!(
            "\n\n## TARGET SCHEMA\n```json\n{}\n```",
            serde_json::to_string_pretty(&schema).unwrap_or_default()
        ));

        prompt.push_str(
            "\n\n## YOUR TASK\n\
        Review the attached documents and the JSON above.\n\
        Generate a JSON Patch (RFC 6902) array to fulfill the requirements or fix errors.\n\
        Return ONLY a valid JSON array `[]`.",
        );

        let messages = vec![Content::user_with_files(prompt, documents)];

        let response = self
            .client
            .generate_content(
                &self.model,
                "You are a financial data auditor and editor.",
                messages,
                None,
                "application/json",
                &format!("{}_patch_attempt_{}", label.to_lowercase(), attempt),
            )
            .await?;

        Ok(extract_first_json_array(&response))
    }

    /// Resolves JSON Pointer paths, specifically handling "Add New Account" logic
    /// where the path uses a Name but the account doesn't exist yet.
    fn resolve_patch_op(
        config: &FinancialHistoryConfig,
        op: &mut json_patch::PatchOperation,
    ) -> Result<()> {
        use json_patch::PatchOperation;

        let path_string = match &op {
            PatchOperation::Add(o) => o.path.to_string(),
            PatchOperation::Remove(o) => o.path.to_string(),
            PatchOperation::Replace(o) => o.path.to_string(),
            PatchOperation::Move(o) => o.path.to_string(),
            PatchOperation::Copy(o) => o.path.to_string(),
            PatchOperation::Test(o) => o.path.to_string(),
        };
        if !path_string.starts_with('/') {
            return Ok(());
        }

        let parts: Vec<&str> = path_string.split('/').collect();
        if parts.len() < 3 {
            return Ok(());
        }

        let section = parts[1];
        let identifier = parts[2];

        // If it's already an index or "-", leave it alone
        if identifier.parse::<usize>().is_ok() || identifier == "-" {
            return Ok(());
        }

        // Unescape JSON Pointer syntax (~1 -> /, ~0 -> ~)
        let account_name = identifier.replace("~1", "/").replace("~0", "~");

        match section {
            "balance_sheet" | "income_statement" => {}
            _ => return Ok(()),
        }

        // Find index
        let found_idx = if section == "balance_sheet" {
            config
                .balance_sheet
                .iter()
                .position(|a| a.name == account_name)
        } else {
            config
                .income_statement
                .iter()
                .position(|a| a.name == account_name)
        };

        if let Some(idx) = found_idx {
            // Account exists: Replace name with index
            let mut new_parts = parts.clone();
            let idx_str = idx.to_string();
            new_parts[2] = &idx_str;
            let rewritten = new_parts.join("/").parse().map_err(|e| {
                FinancialHistoryError::ExtractionFailed(format!(
                    "Failed to rewrite patch path: {}",
                    e
                ))
            })?;
            match op {
                PatchOperation::Add(o) => o.path = rewritten,
                PatchOperation::Remove(o) => o.path = rewritten,
                PatchOperation::Replace(o) => o.path = rewritten,
                PatchOperation::Move(o) => o.path = rewritten,
                PatchOperation::Copy(o) => o.path = rewritten,
                PatchOperation::Test(o) => o.path = rewritten,
            }
        } else {
            // Account DOES NOT exist
            match op {
                PatchOperation::Add(_) => {
                    // If adding to a missing account at root, translate to append
                    if parts.len() == 3 {
                        let new_path = format!("/{}/-", section);
                        let rewritten = new_path.parse().map_err(|e| {
                            FinancialHistoryError::ExtractionFailed(format!(
                                "Failed to rewrite add path: {}",
                                e
                            ))
                        })?;
                        match op {
                            PatchOperation::Add(o) => o.path = rewritten,
                            PatchOperation::Remove(_)
                            | PatchOperation::Replace(_)
                            | PatchOperation::Move(_)
                            | PatchOperation::Copy(_)
                            | PatchOperation::Test(_) => {}
                        }
                    } else {
                        return Err(FinancialHistoryError::ExtractionFailed(format!(
                            "Account '{}' not found in {} for path {}",
                            account_name, section, path_string
                        )));
                    }
                }
                _ => {
                    // Trying to remove/replace/move a non-existent account -> Error
                    return Err(FinancialHistoryError::ExtractionFailed(format!(
                        "Account '{}' not found in {} (path: {})",
                        account_name, section, path_string
                    )));
                }
            }
        }
        Ok(())
    }

    /// Applies patch operations sequentially.
    /// Returns (true if any op succeeded, list of error messages for failed ops).
    fn apply_patch_sequentially(
        &self,
        config: &mut FinancialHistoryConfig,
        patch_json: &str,
        attempt: usize,
    ) -> Result<(bool, Vec<String>)> {
        let patch_value: serde_json::Value = serde_json::from_str(patch_json).map_err(|e| {
            FinancialHistoryError::ExtractionFailed(format!(
                "Invalid JSON patch syntax on attempt {}: {}",
                attempt, e
            ))
        })?;

        let patch_ops: Vec<json_patch::PatchOperation> = serde_json::from_value(patch_value)
            .map_err(|e| {
                FinancialHistoryError::ExtractionFailed(format!(
                    "Invalid JSON patch format on attempt {}: {}",
                    attempt, e
                ))
            })?;

        if patch_ops.is_empty() {
            return Ok((false, Vec::new()));
        }

        let mut errors = Vec::new();
        let mut any_success = false;

        for mut op in patch_ops {
            // 1. Try to resolve paths (Account Name -> Index)
            // We do this FRESH every operation because indices shift if we remove/add items
            if let Err(e) = Self::resolve_patch_op(config, &mut op) {
                errors.push(format!("Path resolution error: {}", e));
                continue;
            }

            // 2. Serialize current config to Value
            let mut config_value =
                serde_json::to_value(&config).map_err(FinancialHistoryError::SerializationError)?;

            // 3. Apply single operation
            let single_patch = json_patch::Patch(vec![op.clone()]);
            match json_patch::patch(&mut config_value, &single_patch) {
                Ok(_) => {
                    // 4. Deserialize back
                    match serde_json::from_value(config_value) {
                        Ok(new_config) => {
                            *config = new_config;
                            any_success = true;
                        }
                        Err(e) => {
                            errors.push(format!("Result invalid against schema: {}", e));
                        }
                    }
                }
                Err(e) => {
                    // Capture the specific error (e.g., "path not found")
                    let op_desc = serde_json::to_string(&op).unwrap_or_default();
                    errors.push(format!("Op failed ({}): {}", op_desc, e));
                }
            }
        }

        Ok((any_success, errors))
    }

    async fn send_event(&self, sender: &Option<Sender<ExtractionEvent>>, event: ExtractionEvent) {
        if let Some(tx) = sender {
            let _ = tx.send(event).await;
        }
    }
}

// --- HELPER FUNCTIONS ---

fn generate_markdown_tables(config: &FinancialHistoryConfig) -> String {
    // Run the densification engine to get the actual data
    let dense_data = match process_financial_history(config) {
        Ok(data) => data,
        Err(e) => {
            return format!(
                "⚠️ Unable to generate tables - Densification failed: {}\n\n\
                This indicates a problem with the configuration that needs to be fixed.",
                e
            );
        }
    };

    let mut output = String::new();

    // Collect account lists
    let pl_accounts: Vec<(String, AccountType)> = config
        .income_statement
        .iter()
        .map(|a| (a.name.clone(), a.account_type.clone()))
        .collect();

    let bs_accounts: Vec<(String, AccountType)> = config
        .balance_sheet
        .iter()
        .map(|a| (a.name.clone(), a.account_type.clone()))
        .collect();

    // Render Income Statement (P&L) Table
    if let Some(pl_table) = render_dense_table(&pl_accounts, &dense_data) {
        output.push_str("## Income Statement (P&L) - Densified Monthly Data\n\n");
        output.push_str(&pl_table);
        output.push_str("\n\n");
    } else {
        output.push_str("## Income Statement (P&L)\n\n⚠️ No data available\n\n");
    }

    // Render Balance Sheet Table
    if let Some(bs_table) = render_dense_table(&bs_accounts, &dense_data) {
        output.push_str("## Balance Sheet - Densified Monthly Data\n\n");
        output.push_str(&bs_table);
        output.push_str("\n\n");
    } else {
        output.push_str("## Balance Sheet\n\n⚠️ No data available\n\n");
    }

    output
}

fn render_dense_table(
    accounts: &[(String, AccountType)],
    dense_data: &std::collections::BTreeMap<String, crate::DenseSeries>,
) -> Option<String> {
    use std::collections::BTreeSet;

    let mut dates = BTreeSet::new();
    for (name, _) in accounts {
        if let Some(series) = dense_data.get(name) {
            for d in series.keys() {
                dates.insert(*d);
            }
        }
    }
    if dates.is_empty() {
        return None;
    }

    let mut rows = Vec::new();
    let mut header = String::from("| Account |");
    for d in &dates {
        header.push_str(&format!(" {} |", d));
    }
    rows.push(header);

    let mut sep = String::from("| --- |");
    for _ in &dates {
        sep.push_str(" --- |");
    }
    rows.push(sep);

    for (name, _ty) in accounts {
        let mut row = format!("| {} |", name);
        if let Some(series) = dense_data.get(name) {
            for d in &dates {
                let val = series
                    .get(d)
                    .map(|p| format!("{:.2}", p.value))
                    .unwrap_or_default();
                row.push_str(&format!(" {} |", val));
            }
        } else {
            for _ in &dates {
                row.push_str("  |");
            }
        }
        rows.push(row);
    }

    Some(rows.join("\n"))
}

fn extract_first_json_array(input: &str) -> String {
    let input = input.trim();
    let start_index = match input.find('[') {
        Some(i) => i,
        None => return input.to_string(),
    };

    let mut depth = 0;
    let mut in_string = false;
    let mut escape = false;

    let chars = input.char_indices().skip_while(|(i, _)| *i < start_index);

    for (idx, c) in chars {
        if escape {
            escape = false;
            continue;
        }
        if c == '\\' {
            escape = true;
            continue;
        }
        if c == '"' {
            in_string = !in_string;
            continue;
        }

        if !in_string {
            match c {
                '[' => depth += 1,
                ']' => {
                    depth -= 1;
                    if depth == 0 {
                        return input[start_index..=idx].to_string();
                    }
                }
                _ => {}
            }
        }
    }
    input[start_index..].to_string()
}

fn distribute_into_batches(items: &[String], max_per_batch: usize) -> Vec<Vec<String>> {
    if items.is_empty() || max_per_batch == 0 {
        return vec![];
    }

    let total = items.len();
    let num_batches = total.div_ceil(max_per_batch);
    let batch_size = total.div_ceil(num_batches);

    items
        .chunks(batch_size)
        .map(|chunk| chunk.to_vec())
        .collect()
}

fn create_document_manifest(documents: &[RemoteDocument]) -> (String, HashMap<String, String>) {
    let mut manifest = String::from(
        "═══════════════════════════════════════════════════════════════════\n\
         📂 DOCUMENT MANIFEST\n\
         ═══════════════════════════════════════════════════════════════════\n\n",
    );
    let mut id_map = HashMap::new();

    for (i, doc) in documents.iter().enumerate() {
        let id = i.to_string();
        manifest.push_str(&format!(
            "  Document ID: {}  →  \"{}\"\n",
            id, doc.display_name
        ));
        id_map.insert(id, doc.display_name.clone());
    }

    manifest.push_str(
        "\n═══════════════════════════════════════════════════════════════════\n\
         ⚠️  CRITICAL INSTRUCTION ⚠️\n\
         ═══════════════════════════════════════════════════════════════════\n\
         In ALL `source.document` fields, use ONLY the Document ID number.\n\n\
         ✅ CORRECT:   \"document\": \"0\"\n\
         ✅ CORRECT:   \"document\": \"1\"\n\
         ❌ WRONG:     \"document\": \"2023_Annual_Report.pdf\"\n\
         ❌ WRONG:     \"document\": \"Financial Statements.pdf\"\n\n\
         Do NOT use filenames. Use ONLY the numeric ID from the manifest above.\n\
         ═══════════════════════════════════════════════════════════════════\n\n",
    );

    (manifest, id_map)
}

fn validate_financial_logic(cfg: &FinancialHistoryConfig) -> std::result::Result<(), String> {
    // 1. Check Sources (Keep this strict)
    for acc in &cfg.balance_sheet {
        for (i, snap) in acc.snapshots.iter().enumerate() {
            if snap.source.is_none() {
                return Err(format!(
                    "Balance Sheet '{}' snapshot #{} missing `source`.",
                    acc.name, i
                ));
            }
        }
    }
    for acc in &cfg.income_statement {
        for (i, cons) in acc.constraints.iter().enumerate() {
            if cons.source.is_none() {
                return Err(format!(
                    "Income Statement '{}' constraint #{} missing `source`.",
                    acc.name, i
                ));
            }
        }
    }

    // 2. Check for Duplicate Names (NEW)
    // React keys require unique names, and BTreeMap logic implies last-write-wins if duplicates exist.
    let mut seen_bs = std::collections::HashSet::new();
    for acc in &cfg.balance_sheet {
        if !seen_bs.insert(&acc.name) {
            return Err(format!(
                "Duplicate Balance Sheet account detected: '{}'. Account names must be unique.",
                acc.name
            ));
        }
    }

    let mut seen_is = std::collections::HashSet::new();
    for acc in &cfg.income_statement {
        if !seen_is.insert(&acc.name) {
            return Err(format!(
                "Duplicate Income Statement account detected: '{}'. Account names must be unique.",
                acc.name
            ));
        }
    }

    // 3. Check Math (Keep this strict)
    match process_financial_history(cfg) {
        Ok(dense) => match verify_accounting_equation(cfg, &dense, 1.0) {
            Ok(_) => Ok(()),
            Err(e) => Err(format!("Accounting Equation Violation: {}", e)),
        },
        Err(e) => Err(format!("Processing Engine Error: {}", e)),
    }
}

/// Helper to generate warning text for AI review instead of hard error
fn detect_suspicious_duplicates(cfg: &FinancialHistoryConfig) -> Option<String> {
    let mut values_seen: HashMap<i64, Vec<String>> = HashMap::new();
    let mut warnings = Vec::new();

    for acc in &cfg.income_statement {
        for cons in &acc.constraints {
            let v = cons.value;
            // Only flag non-round numbers or large round numbers that look suspiciously specific
            if v > 100.0 && (v.fract() > 0.0 || v % 100.0 != 0.0) {
                let cents = (v * 100.0).round() as i64;
                values_seen.entry(cents).or_default().push(acc.name.clone());
            }
        }
    }

    for (cents, accounts) in values_seen {
        if accounts.len() > 1 {
            // Deduplicate account names
            let mut unique_accounts = accounts.clone();
            unique_accounts.sort();
            unique_accounts.dedup();

            if unique_accounts.len() > 1 {
                let val = cents as f64 / 100.0;
                warnings.push(format!(
                    "- Value {:.2} appears in multiple accounts: {}",
                    val,
                    unique_accounts.join(", ")
                ));
            }
        }
    }

    if warnings.is_empty() {
        None
    } else {
        Some(warnings.join("\n"))
    }
}

pub fn extract_first_json_object(input: &str) -> String {
    let input = input.trim();
    let start_index = match input.find('{') {
        Some(i) => i,
        None => return input.to_string(),
    };

    let mut depth = 0;
    let mut in_string = false;
    let mut escape = false;

    let chars = input.char_indices().skip_while(|(i, _)| *i < start_index);

    for (idx, c) in chars {
        if escape {
            escape = false;
            continue;
        }
        if c == '\\' {
            escape = true;
            continue;
        }
        if c == '"' {
            in_string = !in_string;
            continue;
        }

        if !in_string {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        return input[start_index..=idx].to_string();
                    }
                }
                _ => {}
            }
        }
    }
    input[start_index..].to_string()
}
