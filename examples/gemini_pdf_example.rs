use dotenv::dotenv;
use financial_history_builder::llm::{ExtractionEvent, FinancialExtractor, GeminiClient};
use financial_history_builder::{
    process_financial_history, verify_accounting_equation, AccountType, DenseSeries,
    FinancialHistoryConfig,
};
use futures::future;
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    dotenv().ok();
    let api_key = std::env::var("GEMINI_API_KEY").expect("GEMINI_API_KEY must be set");

    println!("🚀 Starting Observable Agentic Financial Extraction Workflow...\n");

    let doc_dir = Path::new("examples").join("documents");
    if !doc_dir.exists() {
        fs::create_dir_all(&doc_dir).await?;
        println!("⚠️  Created 'examples/documents'. Please place a PDF there.");
        return Ok(());
    }

    let mut dir_stream = fs::read_dir(&doc_dir).await?;
    let mut pdf_paths: Vec<PathBuf> = Vec::new();
    while let Ok(Some(entry)) = dir_stream.next_entry().await {
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "pdf") {
            pdf_paths.push(path);
        }
    }

    if pdf_paths.is_empty() {
        println!("⚠️  No PDF files found in {:?}.", doc_dir);
        return Ok(());
    }

    println!("📄 Processing PDFs:");
    for p in &pdf_paths {
        println!("   - {:?}", p.file_name().unwrap());
    }
    println!();

    let client = GeminiClient::new(api_key);
    let extractor = FinancialExtractor::new(client.clone(), "gemini-2.5-flash-preview-09-2025");

    println!("☁️  Uploading documents to Gemini in parallel...");
    let upload_futures: Vec<_> = pdf_paths
        .iter()
        .map(|path| client.upload_document(path))
        .collect();

    let documents = future::try_join_all(upload_futures).await?;

    for doc in &documents {
        println!(
            "   ✅ Uploaded: {} ({})",
            doc.display_name,
            if doc.is_active() {
                "ACTIVE"
            } else {
                &doc.state
            }
        );
    }
    println!();

    // Create a channel for observability
    let (tx, mut rx) = mpsc::channel(32);

    // Spawn the extraction in a separate task
    let extraction_handle =
        tokio::spawn(async move { extractor.extract(&documents, Some(tx)).await });

    // Poll the channel and print real-time updates
    tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            match event {
                ExtractionEvent::Starting => {
                    println!("🔄 Starting extraction workflow...");
                }
                ExtractionEvent::Uploading { filename } => {
                    println!("📤 Uploading: {}", filename);
                }
                ExtractionEvent::Step1Discovery => {
                    println!("🔍 STEP 1: Discovering organization info and chart of accounts...");
                }
                ExtractionEvent::Step2Extraction => {
                    println!(
                        "📊 STEP 2: Extracting Balance Sheet and Income Statement in parallel..."
                    );
                }
                ExtractionEvent::Step3Assembly => {
                    println!("🔧 STEP 3: Assembling and resolving document IDs...");
                }
                ExtractionEvent::DraftingResponse => {
                    println!("🤖 AI is reading documents and drafting initial JSON...");
                }
                ExtractionEvent::ProcessingResponse => {
                    println!("⚙️  Processing and parsing response...");
                }
                ExtractionEvent::Validating { attempt } => {
                    println!("🔍 Validating math and sources (Attempt {})...", attempt);
                }
                ExtractionEvent::CorrectionNeeded { reason } => {
                    println!("⚠️  Issue detected: {}", reason);
                }
                ExtractionEvent::Retry { attempt, error } => {
                    println!("🔄 Retry attempt {} - Previous error: {}", attempt, error);
                }
                ExtractionEvent::Success => {
                    println!("✅ Extraction and validation successful!");
                }
                ExtractionEvent::Failed { reason } => {
                    println!("❌ Extraction failed: {}", reason);
                }
            }
        }
    });

    // Await the extraction result
    let mut config = extraction_handle.await??;

    println!("\n✅ Initial Extraction Complete:");
    println!("   Organization: {}", config.organization_name);
    println!("   Fiscal Year End: Month {}", config.fiscal_year_end_month);
    println!("   Balance Sheet Accounts: {}", config.balance_sheet.len());
    println!(
        "   Income Statement Accounts: {}",
        config.income_statement.len()
    );

    // DEMONSTRATION: Interactive Refinement Workflow
    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("🔧 REFINEMENT WORKFLOW DEMONSTRATION");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("\nThe refine_history() method allows you to make targeted changes");
    println!("to the extracted data using natural language instructions.");
    println!("\nExample use cases:");
    println!("  • 'Add a new account called Marketing Expenses with quarterly data'");
    println!("  • 'Remove the duplicate Revenue entries from Q2 2023'");
    println!("  • 'Update the Cash balance for December 2023 to $85,000'");
    println!("  • 'Change the seasonality profile for Sales to RetailPeak'");

    // Optional: Uncomment to enable interactive refinement
    // print!("\n💬 Enter a refinement instruction (or press Enter to skip): ");
    // std::io::Write::flush(&mut std::io::stdout())?;
    // let mut user_instruction = String::new();
    // std::io::stdin().read_line(&mut user_instruction)?;
    // let user_instruction = user_instruction.trim();

    // For this demo, we'll use a hardcoded example instruction
    let demo_instruction = "Review all balance sheet and income statement accounts. \
                            If you notice any data quality issues, unusual values, or \
                            potential improvements, make the necessary corrections.";

    println!("\n📝 Demo Instruction: \"{}\"", demo_instruction);
    println!("\n🔄 Running refinement workflow...\n");

    // ═══════════════════════════════════════════════════════════════════════════
    // IMPORTANT: Document Handling in Refinement
    // ═══════════════════════════════════════════════════════════════════════════
    // You have multiple options for which documents to pass to refine_history():
    //
    // 1. REUSE ORIGINAL DOCUMENTS (most common):
    //    Pass the same documents used during extraction
    let refine_docs = documents.clone();

    // 2. ADD SUPPLEMENTARY DOCUMENTS (for additional context):
    //    Upload new PDFs that provide extra information for refinement
    //    Example (commented out):
    //    let supplementary_path = Path::new("examples/documents/updated_forecast.pdf");
    //    if supplementary_path.exists() {
    //        let supplementary_doc = client.upload_document(supplementary_path).await?;
    //        let mut combined_docs = documents.clone();
    //        combined_docs.push(supplementary_doc);
    //        refine_docs = combined_docs;
    //    }
    //
    // 3. USE ENTIRELY DIFFERENT DOCUMENTS (for major revisions):
    //    Upload and use completely new set of documents
    //    Example (commented out):
    //    let new_doc_path = Path::new("examples/documents/corrected_financials.pdf");
    //    let new_doc = client.upload_document(new_doc_path).await?;
    //    refine_docs = vec![new_doc];
    //
    // The LLM will have access to ALL provided documents when generating patches.
    // ═══════════════════════════════════════════════════════════════════════════

    // Create a new channel for refinement progress
    let (refine_tx, mut refine_rx) = mpsc::channel(32);

    // Clone necessary values for the refinement task
    let refine_extractor =
        FinancialExtractor::new(client.clone(), "gemini-2.5-flash-preview-09-2025");
    let refine_instruction = demo_instruction.to_string();

    // Spawn refinement in separate task
    let refinement_handle = tokio::spawn(async move {
        refine_extractor
            .refine_history(config, &refine_docs, &refine_instruction, Some(refine_tx))
            .await
    });

    // Monitor refinement progress
    tokio::spawn(async move {
        while let Some(event) = refine_rx.recv().await {
            match event {
                ExtractionEvent::Validating { attempt } => {
                    println!(
                        "🔍 Analyzing and applying refinements (Attempt {})...",
                        attempt
                    );
                }
                ExtractionEvent::CorrectionNeeded { reason } => {
                    println!("⚙️  {}", reason);
                }
                _ => {}
            }
        }
    });

    // Await refinement result
    match refinement_handle.await? {
        Ok(refined_config) => {
            println!("\n✅ Refinement Complete!");
            config = refined_config;
        }
        Err(e) => {
            println!("\n⚠️  Refinement encountered an issue: {}", e);
            println!("   Continuing with original configuration...");
        }
    }

    println!("\n⚙️  Running Densification Engine...");
    let dense_data = process_financial_history(&config)?;

    if let Some(pl_table) =
        render_dense_table_from_data(&collect_income_accounts(&config), &dense_data)
    {
        println!("\n📊 P/L (dense, dates on x-axis):\n{}", pl_table);
    }
    if let Some(bs_table) =
        render_dense_table_from_data(&collect_balance_accounts(&config), &dense_data)
    {
        println!("\n📊 Balance Sheet (dense, dates on x-axis):\n{}", bs_table);
    }

    // Print detailed audit trail for the first Revenue or income account
    if let Some((name, series)) = dense_data
        .iter()
        .find(|(k, _)| config.income_statement.iter().any(|a| &a.name == *k))
    {
        println!("\n🔍 DETAILED AUDIT TRAIL for '{}':", name);
        println!("   (Showing first 6 months for brevity)\n");
        for (i, (date, point)) in series.iter().enumerate() {
            if i >= 6 {
                break;
            }
            println!("  📅 {}: ${:.2}", date, point.value);
            println!("     Origin: {:?}", point.origin);
            if let Some(src) = &point.source {
                println!("     Source Doc: {}", src.document_name);
                if let Some(txt) = &src.original_text {
                    println!("     Context: \"{}\"", txt);
                }
            }
            if let Some(total) = point.derivation.original_period_value {
                println!(
                    "     Calculation: {} (Derived from total ${:.2} covering {} to {})",
                    point.derivation.logic,
                    total,
                    point.derivation.period_start.unwrap(),
                    point.derivation.period_end.unwrap()
                );
            } else {
                println!("     Calculation: {}", point.derivation.logic);
            }
            println!("     ------------------------------------------------");
        }
        println!(
            "   (... {} more months in full dataset)",
            series.len().saturating_sub(6)
        );
    }

    match verify_accounting_equation(&config, &dense_data, 1.0) {
        Ok(_) => println!("\n✅ Accounting Equation Holds (Assets == Liab + Equity)"),
        Err(e) => println!("\n⚠️  Balance Warning: {}", e),
    }

    let config_file = "extracted_config.json";
    let json = serde_json::to_string_pretty(&config)?;
    std::fs::write(config_file, json)?;
    println!("\n💾 Saved configuration to: {}", config_file);

    let base_name = pdf_paths
        .first()
        .and_then(|p| p.file_stem().and_then(|s| s.to_str()))
        .unwrap_or("financial_history");

    let pl_accounts = collect_income_accounts(&config);
    let bs_accounts = collect_balance_accounts(&config);

    let pl_filename = format!("{}_pl.csv", base_name);
    export_to_csv_transposed(&pl_accounts, &dense_data, &pl_filename).await?;
    println!("💾 Saved P/L to: {}", pl_filename);

    let bs_filename = format!("{}_balance_sheet.csv", base_name);
    export_to_csv_transposed(&bs_accounts, &dense_data, &bs_filename).await?;
    println!("💾 Saved Balance Sheet to: {}", bs_filename);

    Ok(())
}

async fn export_to_csv_transposed(
    accounts: &[(String, AccountType)],
    dense_data: &BTreeMap<String, DenseSeries>,
    filename: &str,
) -> Result<(), Box<dyn Error>> {
    let mut dates = BTreeSet::new();
    for (name, _) in accounts {
        if let Some(series) = dense_data.get(name) {
            for d in series.keys() {
                dates.insert(*d);
            }
        }
    }

    if dates.is_empty() {
        return Ok(());
    }

    let mut csv_out = String::new();

    csv_out.push_str("Account");
    for date in &dates {
        csv_out.push_str(&format!(",{}", date));
    }
    csv_out.push('\n');

    for (name, _) in accounts {
        csv_out.push_str(name);
        if let Some(series) = dense_data.get(name) {
            for date in &dates {
                let val = series.get(date).map(|p| p.value).unwrap_or(0.0);
                csv_out.push_str(&format!(",{:.2}", val));
            }
        } else {
            for _ in &dates {
                csv_out.push_str(",0.00");
            }
        }
        csv_out.push('\n');
    }

    fs::write(filename, csv_out).await?;
    Ok(())
}

fn render_dense_table_from_data(
    accounts: &[(String, AccountType)],
    dense_data: &BTreeMap<String, DenseSeries>,
) -> Option<String> {
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

fn collect_income_accounts(cfg: &FinancialHistoryConfig) -> Vec<(String, AccountType)> {
    cfg.income_statement
        .iter()
        .map(|a| (a.name.clone(), a.account_type.clone()))
        .collect()
}

fn collect_balance_accounts(cfg: &FinancialHistoryConfig) -> Vec<(String, AccountType)> {
    cfg.balance_sheet
        .iter()
        .map(|a| (a.name.clone(), a.account_type.clone()))
        .collect()
}
