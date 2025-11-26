use crate::error::{FinancialHistoryError, Result};
use crate::llm::{client::GeminiClient, prompts, types::*};
use crate::schema::*;
use crate::{process_financial_history, verify_accounting_equation};
use futures::{future::try_join_all, try_join};
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

        // Prepare context for specific extractions
        let org_ctx = format!(
            "Organization: {}\nFY End Month: {}",
            discovery.organization_name, discovery.fiscal_year_end_month
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
            eprintln!("❌ Failed to parse Discovery JSON. Raw output dumped to debug_discovery_raw_output.json");
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
                let total_batches = total_batches;
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
                            eprintln!(
                                "❌ Failed to parse Balance Sheet JSON. Raw output dumped to debug_balance_sheet_raw_output.json"
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
                let total_batches = total_batches;
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
                            eprintln!(
                                "❌ Failed to parse Income Statement JSON. Raw output dumped to debug_income_statement_raw_output.json"
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
                    Some(65536),
                    stage_name,
                )
                .await
            {
                Ok(response) => {
                    let cleaned = extract_first_json_object(&response);
                    return Ok(cleaned);
                }
                Err(e) => {
                    eprintln!("⚠️ {} Attempt {} failed: {}", stage_name, attempt, e);
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

    async fn validate_and_fix(
        &self,
        mut config: FinancialHistoryConfig,
        documents: &[RemoteDocument],
        progress: &Option<Sender<ExtractionEvent>>,
    ) -> Result<FinancialHistoryConfig> {
        let max_fix_attempts = 5;
        let mut quality_check_completed = false;

        for attempt in 1..=max_fix_attempts {
            self.send_event(progress, ExtractionEvent::Validating { attempt })
                .await;

            // Check for validation errors (Math & Source fields only)
            let validation_error = validate_financial_logic(&config).err();

            // ALWAYS run final quality check at least once, even if validation passed
            let should_run_patch = validation_error.is_some() || !quality_check_completed;

            if should_run_patch {
                // Get the patch from the model
                let patch_result = self
                    .request_quality_patch(&config, documents, validation_error.as_deref(), attempt)
                    .await;

                match patch_result {
                    Ok(patch_json) => {
                        // Try to parse and apply the patch
                        match self.apply_patch(&mut config, &patch_json, attempt) {
                            Ok(true) => {
                                // Patch applied successfully, continue to next iteration
                                eprintln!("✓ Applied quality patch (attempt {})", attempt);
                                quality_check_completed = true;
                                continue;
                            }
                            Ok(false) => {
                                // Empty patch - config is perfect
                                eprintln!("✓ No changes needed - config validated");

                                // Final validation check
                                if let Err(e) = validate_financial_logic(&config) {
                                    return Err(FinancialHistoryError::ExtractionFailed(format!(
                                        "Final validation failed: {}",
                                        e
                                    )));
                                }

                                return Ok(config);
                            }
                            Err(e) => {
                                eprintln!(
                                    "⚠️ Patch application failed (attempt {}): {}",
                                    attempt, e
                                );

                                if attempt == max_fix_attempts {
                                    return Err(FinancialHistoryError::ExtractionFailed(format!(
                                        "Failed to apply validation patch: {}",
                                        e
                                    )));
                                }

                                continue;
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!(
                            "⚠️ Failed to get quality patch (attempt {}): {}",
                            attempt, e
                        );

                        if attempt == max_fix_attempts {
                            // If we still have validation errors, fail
                            if let Some(err) = validation_error {
                                return Err(FinancialHistoryError::ExtractionFailed(format!(
                                    "Validation failed: {}",
                                    err
                                )));
                            }
                            // No validation errors, just couldn't get patch - return config as is
                            return Ok(config);
                        }

                        // Continue to next attempt (retry)
                        continue;
                    }
                }
            } else {
                // No validation errors and quality check already completed - we're done
                return Ok(config);
            }
        }

        // Shouldn't reach here, but if we do, return the config
        Ok(config)
    }

    async fn request_quality_patch(
        &self,
        config: &FinancialHistoryConfig,
        documents: &[RemoteDocument],
        validation_error: Option<&str>,
        attempt: usize,
    ) -> Result<String> {
        let schema = FinancialHistoryConfig::get_gemini_response_schema()
            .map_err(FinancialHistoryError::SerializationError)?;

        let config_json = serde_json::to_string_pretty(config)
            .map_err(FinancialHistoryError::SerializationError)?;

        // 1. Check for suspicious duplicates (Soft check)
        let duplicate_warning = detect_suspicious_duplicates(config);

        // 2. Generate markdown tables if no validation errors
        let tables = if validation_error.is_none() {
            Some(generate_markdown_tables(config))
        } else {
            None
        };

        let mut prompt = prompts::SYSTEM_PROMPT_VALIDATION.to_string();

        // Inject Context sections
        if let Some(error) = validation_error {
            prompt.push_str(&format!(
                "\n\n## 🔴 CRITICAL VALIDATION ERRORS\n\
                The following errors MUST be fixed via JSON Patch:\n\
                ```\n{}\n```",
                error
            ));
        }

        if let Some(dup_warn) = duplicate_warning {
            prompt.push_str(&format!(
                "\n\n## ⚠️ POTENTIAL DATA INTEGRITY ISSUES\n\
                We detected potentially suspicious duplicate values. \
                Please verify against the attached documents if these are correct or double-counting:\n\
                ```\n{}\n```\n\
                If these are valid (e.g. coincidentally same value), ignore them. \
                If they are errors, remove the duplicate constraint via patch.",
                dup_warn
            ));
        }

        prompt.push_str(&format!(
            "\n\n## CURRENT CONFIGURATION\n```json\n{}\n```",
            config_json
        ));

        prompt.push_str(&format!(
            "\n\n## TARGET SCHEMA\n```json\n{}\n```",
            serde_json::to_string_pretty(&schema).unwrap_or_default()
        ));

        if let Some(tbl) = tables {
             prompt.push_str(&format!("\n\n## VISUAL REVIEW TABLES\n{}", tbl));
        }

        prompt.push_str("\n\n## YOUR TASK\n\
        Review the attached documents and the JSON above.\n\
        Generate a JSON Patch (RFC 6902) array to fix validation errors or data quality issues.\n\
        Return ONLY a valid JSON array `[]`.");

        let messages = vec![Content::user_with_files(prompt, documents)];

        let response = self
            .client
            .generate_content(
                &self.model,
                "You are a financial data auditor.",
                messages,
                None,
                "application/json",
                Some(65536),
                &format!("validation_patch_attempt_{}", attempt),
            )
            .await?;

        Ok(extract_first_json_array(&response))
    }

    fn resolve_patch_paths(
        config: &FinancialHistoryConfig,
        patch_ops: &mut [json_patch::PatchOperation],
    ) -> Result<()> {
        use json_patch::PatchOperation;

        for op in patch_ops {
            let path_str = match op {
                PatchOperation::Add(add_op) => add_op.path.to_string(),
                PatchOperation::Remove(remove_op) => remove_op.path.to_string(),
                PatchOperation::Replace(replace_op) => replace_op.path.to_string(),
                PatchOperation::Move(move_op) => move_op.path.to_string(),
                PatchOperation::Copy(copy_op) => copy_op.path.to_string(),
                PatchOperation::Test(test_op) => test_op.path.to_string(),
            };

            if !path_str.starts_with('/') {
                continue;
            }

            let parts: Vec<&str> = path_str.split('/').collect();
            if parts.len() < 3 {
                continue;
            }

            let section = parts[1];
            let identifier = parts[2];

            if identifier.parse::<usize>().is_ok() || identifier == "-" {
                continue;
            }

            let account_name = identifier.replace("~1", "/").replace("~0", "~");

            let resolved_index = match section {
                "balance_sheet" => config
                    .balance_sheet
                    .iter()
                    .position(|a| a.name == account_name),
                "income_statement" => config
                    .income_statement
                    .iter()
                    .position(|a| a.name == account_name),
                _ => None,
            };

            if let Some(idx) = resolved_index {
                let mut new_parts = parts.clone();
                let idx_str = idx.to_string();
                new_parts[2] = &idx_str;

                let new_path = new_parts.join("/");
                let new_path_parsed = new_path.parse().map_err(|e| {
                    FinancialHistoryError::ExtractionFailed(format!("Failed to rewrite patch path: {}", e))
                })?;

                match op {
                    PatchOperation::Add(add_op) => add_op.path = new_path_parsed,
                    PatchOperation::Remove(remove_op) => remove_op.path = new_path_parsed,
                    PatchOperation::Replace(replace_op) => replace_op.path = new_path_parsed,
                    PatchOperation::Move(move_op) => move_op.path = new_path_parsed,
                    PatchOperation::Copy(copy_op) => copy_op.path = new_path_parsed,
                    PatchOperation::Test(test_op) => test_op.path = new_path_parsed,
                }
            } else {
                eprintln!("⚠️ Patch Warning: Could not find account named '{}' in {}", account_name, section);
            }
        }
        Ok(())
    }

    fn apply_patch(
        &self,
        config: &mut FinancialHistoryConfig,
        patch_json: &str,
        attempt: usize,
    ) -> Result<bool> {
        // Parse the patch as a JSON value first to check if it's empty
        let patch_value: serde_json::Value = serde_json::from_str(patch_json).map_err(|e| {
            FinancialHistoryError::ExtractionFailed(format!("Invalid JSON patch syntax: {}", e))
        })?;

        // Check if it's an empty array
        if let Some(arr) = patch_value.as_array() {
            if arr.is_empty() {
                // Empty patch - no changes needed
                return Ok(false);
            }
        }

        // Parse as PatchOperation array
        let mut patch: Vec<json_patch::PatchOperation> =
            serde_json::from_value(patch_value).map_err(|e| {
                FinancialHistoryError::ExtractionFailed(format!("Invalid JSON patch format: {}", e))
            })?;

        // Resolve account names to indices before applying
        Self::resolve_patch_paths(config, &mut patch)?;

        // Convert config to JSON value
        let mut config_value =
            serde_json::to_value(&config).map_err(FinancialHistoryError::SerializationError)?;

        // Apply the patch
        json_patch::patch(&mut config_value, &patch).map_err(|e| {
            // Log the failed patch for debugging
            let _ = fs::write(
                format!("debug_failed_patch_attempt_{}.json", attempt),
                patch_json,
            );

            FinancialHistoryError::ExtractionFailed(format!(
                "Failed to apply JSON patch: {}. Patch dumped to debug_failed_patch_attempt_{}.json",
                e, attempt
            ))
        })?;

        // Convert back to FinancialHistoryConfig
        let new_config: FinancialHistoryConfig =
            serde_json::from_value(config_value).map_err(|e| {
                FinancialHistoryError::ExtractionFailed(format!(
                    "Patched JSON doesn't match schema: {}",
                    e
                ))
            })?;

        // Log the successful patch
        let _ = fs::write(
            format!("debug_applied_patch_attempt_{}.json", attempt),
            patch_json,
        );

        *config = new_config;
        Ok(true)
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

    // 2. Check Math (Keep this strict)
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
                    val, unique_accounts.join(", ")
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

fn extract_first_json_object(input: &str) -> String {
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
