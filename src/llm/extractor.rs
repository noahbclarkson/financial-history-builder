use std::collections::{HashMap, HashSet};

use futures::future::{try_join, try_join_all};
use gemini_rust::FileHandle;
use gemini_structured_output::StructuredClient;
use tokio::sync::mpsc::Sender;

use crate::error::{FinancialHistoryError, Result};
use crate::llm::{prompts, types::ExtractionEvent};
use crate::llm::utils::{build_prompt_parts, create_document_manifest};
use crate::schema::{
    AccountType, BalanceSheetAccount, BalanceSheetExtractionResponse, DiscoveryResponse,
    FinancialHistoryConfig, IncomeStatementAccount, IncomeStatementExtractionResponse,
};
use crate::{process_financial_history, verify_accounting_equation};

pub struct FinancialExtractor {
    client: StructuredClient,
}

impl FinancialExtractor {
    pub fn new(client: StructuredClient) -> Self {
        Self { client }
    }

    pub async fn extract(
        &self,
        documents: &[FileHandle],
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

        let (bs_result, is_result) = try_join(
            self.extract_balance_sheet(
                documents,
                &manifest,
                &org_ctx,
                &discovery.balance_sheet_account_names,
            ),
            self.extract_income_statement(
                documents,
                &manifest,
                &org_ctx,
                &discovery.income_statement_account_names,
            ),
        )
        .await?;

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
        resolve_document_ids(&mut config, &id_map);

        // --- STEP 4: FINAL VALIDATION & PATCHING ---
        config = self.validate_and_fix(config, documents, &progress).await?;

        self.send_event(&progress, ExtractionEvent::Success).await;
        Ok(config)
    }

    pub async fn refine_history(
        &self,
        config: FinancialHistoryConfig,
        documents: &[FileHandle],
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

        let (manifest, id_map) = create_document_manifest(documents);
        let manifest_context = manifest.clone();
        let outcome = self
            .client
            .refine(config, instruction)
            .with_documents(documents.to_vec())
            .with_context_generator(move |cfg| {
                let mut context = String::new();
                context.push_str(&manifest_context);
                context.push_str("\n## CURRENT DATA TABLES\n");
                context.push_str(&generate_markdown_tables(cfg));
                if let Some(warnings) = detect_suspicious_duplicates(cfg) {
                    context.push_str("\n## WARNINGS\n");
                    context.push_str(&warnings);
                }
                context
            })
            .with_validator(|cfg| validate_financial_logic(cfg).err())
            .execute()
            .await?;

        let mut refined = outcome.value;
        resolve_document_ids(&mut refined, &id_map);
        Ok(refined)
    }

    async fn run_discovery(
        &self,
        docs: &[FileHandle],
        manifest: &str,
    ) -> Result<DiscoveryResponse> {
        let prompt = format!(
            "{}\n\n## TASK\nAnalyze the attached documents and complete the discovery response.",
            manifest
        );
        let parts = build_prompt_parts(&prompt, docs)?;
        let outcome = self
            .client
            .request::<DiscoveryResponse>()
            .system(prompts::SYSTEM_PROMPT_DISCOVERY)
            .user_parts(parts)
            .execute()
            .await?;

        Ok(outcome.value)
    }

    async fn extract_balance_sheet(
        &self,
        docs: &[FileHandle],
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

        let futures = batches
            .into_iter()
            .enumerate()
            .map(|(i, batch)| {
                let batch_index = i + 1;
                let batch_accounts = batch.clone();
                let manifest = manifest.to_string();
                let org_ctx = org_ctx.to_string();
                let docs = docs.to_vec();
                let client = self.client.clone();

                async move {
                    let account_list = batch_accounts.join("\n- ");
                    let batch_context = format!(
                        "## BATCH CONTEXT\nProcessing Batch {} of {}.\n\
                         EXTRACT DATA ONLY FOR THE ACCOUNTS LISTED BELOW.\n\
                         If you see data for accounts NOT in this list, IGNORE IT.",
                        batch_index, total_batches
                    );

                    let prompt = format!(
                        "{}\n\n{}\n\n## CONTEXT\n{}\n\n## EXTRACT SNAPSHOTS FOR THESE ACCOUNTS\n\
                        Extract balance sheet snapshots for each of the following accounts.\n\
                        Use the EXACT names below. Do not modify or rename them.\n\n- {}\n\n\
                        ## CRITICAL REMINDERS\n\
                        - Set EXACTLY ONE account as `is_balancing_account: true` (prefer Cash)\n\
                        - Use document IDs (\"0\", \"1\", etc.) in `source.document`\n\
                        - Extract ALL available dates (2023, 2022, mid-year if present)\n\
                        - Choose appropriate interpolation: Linear, Step, or Curve",
                        batch_context,
                        manifest,
                        org_ctx,
                        account_list
                    );

                    let parts = build_prompt_parts(&prompt, &docs)?;
                    let outcome = client
                        .request::<BalanceSheetExtractionResponse>()
                        .system(prompts::SYSTEM_PROMPT_BS_EXTRACT)
                        .user_parts(parts)
                        .execute()
                        .await?;

                    Ok::<Vec<BalanceSheetAccount>, FinancialHistoryError>(outcome.value.balance_sheet)
                }
            });

        let results: Vec<Vec<BalanceSheetAccount>> = try_join_all(futures).await?;

        Ok(BalanceSheetExtractionResponse {
            balance_sheet: results.into_iter().flatten().collect(),
        })
    }

    async fn extract_income_statement(
        &self,
        docs: &[FileHandle],
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

        let futures = batches
            .into_iter()
            .enumerate()
            .map(|(i, batch)| {
                let batch_index = i + 1;
                let batch_accounts = batch.clone();
                let manifest = manifest.to_string();
                let org_ctx = org_ctx.to_string();
                let docs = docs.to_vec();
                let client = self.client.clone();

                async move {
                    let account_list = batch_accounts.join("\n- ");
                    let batch_context = format!(
                        "## BATCH CONTEXT\nProcessing Batch {} of {}.\n\
                         EXTRACT DATA ONLY FOR THE ACCOUNTS LISTED BELOW.\n\
                         If you see data for accounts NOT in this list, IGNORE IT.",
                        batch_index, total_batches
                    );

                    let prompt = format!(
                        "{}\n\n{}\n\n## CONTEXT\n{}\n\n## EXTRACT CONSTRAINTS FOR THESE ACCOUNTS\n\
                        Extract period constraints for each of the following accounts.\n\
                        Use the EXACT names below. Do not modify or rename them.\n\n- {}\n\n\
                        ## CRITICAL REMINDERS\n\
                        - Extract ALL available periods (annual, quarterly, monthly if present)\n\
                        - Use document IDs (\"0\", \"1\", etc.) in `source.document`\n\
                        - Choose appropriate seasonality: Flat (most common), RetailPeak, SummerHigh, or SaasGrowth\n\
                        - Do NOT extract calculated totals (Gross Profit, Net Income, EBITDA)\n\
                        - Include overlapping periods (e.g., both monthly AND annual totals)",
                        batch_context,
                        manifest,
                        org_ctx,
                        account_list
                    );

                    let parts = build_prompt_parts(&prompt, &docs)?;
                    let outcome = client
                        .request::<IncomeStatementExtractionResponse>()
                        .system(prompts::SYSTEM_PROMPT_IS_EXTRACT)
                        .user_parts(parts)
                        .execute()
                        .await?;

                    Ok::<Vec<IncomeStatementAccount>, FinancialHistoryError>(outcome.value.income_statement)
                }
            });

        let results: Vec<Vec<IncomeStatementAccount>> = try_join_all(futures).await?;

        Ok(IncomeStatementExtractionResponse {
            income_statement: results.into_iter().flatten().collect(),
        })
    }

    async fn validate_and_fix(
        &self,
        config: FinancialHistoryConfig,
        documents: &[FileHandle],
        progress: &Option<Sender<ExtractionEvent>>,
    ) -> Result<FinancialHistoryConfig> {
        self.send_event(progress, ExtractionEvent::Validating { attempt: 1 })
            .await;

        let (manifest, id_map) = create_document_manifest(documents);
        let manifest_context = manifest.clone();
        let outcome = self
            .client
            .refine(config, "Fix any issues so the configuration is valid and the accounting equation balances.")
            .with_documents(documents.to_vec())
            .with_context_generator(move |cfg| {
                let mut context = String::new();
                context.push_str(&manifest_context);
                context.push_str("\n## CURRENT DATA TABLES\n");
                context.push_str(&generate_markdown_tables(cfg));
                if let Some(warnings) = detect_suspicious_duplicates(cfg) {
                    context.push_str("\n## WARNINGS\n");
                    context.push_str(&warnings);
                }
                context
            })
            .with_validator(|cfg| validate_financial_logic(cfg).err())
            .execute()
            .await?;

        let mut fixed = outcome.value;
        resolve_document_ids(&mut fixed, &id_map);
        Ok(fixed)
    }

    async fn send_event(&self, sender: &Option<Sender<ExtractionEvent>>, event: ExtractionEvent) {
        if let Some(tx) = sender {
            let _ = tx.send(event).await;
        }
    }
}

// --- HELPER FUNCTIONS ---

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

fn resolve_document_ids(config: &mut FinancialHistoryConfig, id_map: &HashMap<String, String>) {
    for account in &mut config.balance_sheet {
        for snapshot in &mut account.snapshots {
            if let Some(source) = snapshot.source.as_mut() {
                if let Some(mapped) = id_map.get(&source.document_name) {
                    source.document_name = mapped.clone();
                }
            }
        }
    }

    for account in &mut config.income_statement {
        for constraint in &mut account.constraints {
            if let Some(source) = constraint.source.as_mut() {
                if let Some(mapped) = id_map.get(&source.document_name) {
                    source.document_name = mapped.clone();
                }
            }
        }
    }
}

fn generate_markdown_tables(config: &FinancialHistoryConfig) -> String {
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

    if let Some(pl_table) = render_dense_table(&pl_accounts, &dense_data) {
        output.push_str("## Income Statement (P&L) - Densified Monthly Data\n\n");
        output.push_str(&pl_table);
        output.push_str("\n\n");
    } else {
        output.push_str("## Income Statement (P&L)\n\n⚠️ No data available\n\n");
    }

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

fn validate_financial_logic(cfg: &FinancialHistoryConfig) -> std::result::Result<(), String> {
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

    let mut seen_bs = HashSet::new();
    for acc in &cfg.balance_sheet {
        if !seen_bs.insert(&acc.name) {
            return Err(format!(
                "Duplicate Balance Sheet account detected: '{}'. Account names must be unique.",
                acc.name
            ));
        }
    }

    let mut seen_is = HashSet::new();
    for acc in &cfg.income_statement {
        if !seen_is.insert(&acc.name) {
            return Err(format!(
                "Duplicate Income Statement account detected: '{}'. Account names must be unique.",
                acc.name
            ));
        }
    }

    match process_financial_history(cfg) {
        Ok(dense) => match verify_accounting_equation(cfg, &dense, 1.0) {
            Ok(_) => Ok(()),
            Err(e) => Err(format!("Accounting Equation Violation: {}", e)),
        },
        Err(e) => Err(format!("Processing Engine Error: {}", e)),
    }
}

fn detect_suspicious_duplicates(cfg: &FinancialHistoryConfig) -> Option<String> {
    let mut values_seen: HashMap<i64, Vec<String>> = HashMap::new();
    let mut warnings = Vec::new();

    for acc in &cfg.income_statement {
        for cons in &acc.constraints {
            let v = cons.value;
            if v > 100.0 && (v.fract() > 0.0 || v % 100.0 != 0.0) {
                let cents = (v * 100.0).round() as i64;
                values_seen.entry(cents).or_default().push(acc.name.clone());
            }
        }
    }

    for (cents, accounts) in values_seen {
        if accounts.len() > 1 {
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
