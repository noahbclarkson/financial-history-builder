use dotenv::dotenv;
use financial_history_builder::llm::{FinancialExtractor, ForecastingSetupAgent, GeminiClient};
use financial_history_builder::{process_financial_history, AccountType, DenseSeries};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::path::Path;
use tokio::fs;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    dotenv().ok();
    let api_key = std::env::var("GEMINI_API_KEY").expect("GEMINI_API_KEY must be set");

    println!("🚀 Financial Forecasting Workflow Demonstration");
    println!("═══════════════════════════════════════════════════════════════\n");

    // 1. Setup Clients
    let client = GeminiClient::new(api_key);
    let extractor = FinancialExtractor::new(client.clone(), "gemini-2.5-flash-preview-09-2025");
    let forecaster = ForecastingSetupAgent::new(client.clone(), "gemini-2.5-flash-preview-09-2025");

    // 2. Load Documents
    let doc_dir = Path::new("examples").join("documents");
    if !doc_dir.exists() {
        fs::create_dir_all(&doc_dir).await?;
        println!("⚠️  Created 'examples/documents'. Please place a PDF there.");
        return Ok(());
    }

    let mut dir_stream = fs::read_dir(&doc_dir).await?;
    let mut pdf_paths = Vec::new();
    while let Ok(Some(entry)) = dir_stream.next_entry().await {
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "pdf") {
            pdf_paths.push(path);
        }
    }

    if pdf_paths.is_empty() {
        println!("⚠️  No PDF files found in {:?}.", doc_dir);
        println!("   Please add financial statements to this directory.");
        return Ok(());
    }

    println!("📤 Uploading documents...");
    let upload_futures: Vec<_> = pdf_paths
        .iter()
        .map(|path| client.upload_document(path))
        .collect();

    let docs = futures::future::try_join_all(upload_futures).await?;
    for doc in &docs {
        println!("   ✅ Uploaded: {}", doc.display_name);
    }
    println!();

    // 3. Phase 1: Initial Extraction (The "Raw" Truth)
    println!("═══════════════════════════════════════════════════════════════");
    println!("📊 PHASE 1: Raw Extraction");
    println!("═══════════════════════════════════════════════════════════════\n");

    let raw_config = extractor.extract(&docs, None).await?;

    println!("   ✅ Extraction Complete:");
    println!(
        "      Balance Sheet Accounts: {}",
        raw_config.balance_sheet.len()
    );
    println!(
        "      Income Statement Accounts: {}",
        raw_config.income_statement.len()
    );
    println!();

    // Display what was extracted
    if !raw_config.balance_sheet.is_empty() {
        println!("   📋 Balance Sheet Accounts:");
        for acc in &raw_config.balance_sheet {
            println!("      • {} ({:?})", acc.name, acc.account_type);
        }
        println!();
    }

    if !raw_config.income_statement.is_empty() {
        println!("   📋 Income Statement Accounts:");
        for acc in &raw_config.income_statement {
            println!("      • {} ({:?})", acc.name, acc.account_type);
        }
        println!();
    }

    // 4. Phase 2: Generate Strategic Overrides (The "Forecasting" Layer)
    println!("═══════════════════════════════════════════════════════════════");
    println!("🧠 PHASE 2: Generating Forecasting Overrides");
    println!("═══════════════════════════════════════════════════════════════\n");

    let instruction = "Ensure GST, Accounts Receivable, and Accounts Payable exist. \
                       Merge detailed utility expenses into 'Light, Power & Heating' if multiple utility accounts exist. \
                       Ensure there is a 'Current Year Earnings' in Equity if missing. \
                       If Interest expense exists but no Loan account, infer and create a Bank Loan account.";

    println!("   📝 Instruction: {}\n", instruction);

    let overrides = forecaster
        .generate_overrides(&raw_config, &docs, Some(instruction))
        .await?;

    println!("   ✅ Overrides Generated:");
    println!(
        "      New Balance Sheet Accounts: {}",
        overrides.new_balance_sheet_accounts.len()
    );
    println!(
        "      New Income Statement Accounts: {}",
        overrides.new_income_statement_accounts.len()
    );
    println!("      Modifications: {}", overrides.modifications.len());
    println!();

    if !overrides.new_balance_sheet_accounts.is_empty() {
        println!("   📌 New Balance Sheet Accounts to Add:");
        for acc in &overrides.new_balance_sheet_accounts {
            println!(
                "      • {} ({:?}) - {} snapshots",
                acc.name,
                acc.account_type,
                acc.snapshots.len()
            );
        }
        println!();
    }

    if !overrides.modifications.is_empty() {
        println!("   🔧 Modifications to Apply:");
        for (i, mod_op) in overrides.modifications.iter().enumerate() {
            println!("      {}. {:?}", i + 1, mod_op);
        }
        println!();
    }

    // 5. Phase 3: Apply Overrides
    println!("═══════════════════════════════════════════════════════════════");
    println!("⚡ PHASE 3: Applying Overrides");
    println!("═══════════════════════════════════════════════════════════════\n");

    let final_config = overrides.apply(&raw_config);

    println!("   ✅ Overrides Applied:");
    println!(
        "      Final Balance Sheet Accounts: {}",
        final_config.balance_sheet.len()
    );
    println!(
        "      Final Income Statement Accounts: {}",
        final_config.income_statement.len()
    );
    println!();

    // 6. Phase 4: Densification
    println!("═══════════════════════════════════════════════════════════════");
    println!("⚙️  PHASE 4: Densification & Validation");
    println!("═══════════════════════════════════════════════════════════════\n");

    let dense_data = process_financial_history(&final_config)?;

    println!("   ✅ Densification Complete:");
    println!("      Total Dense Accounts: {}", dense_data.len());
    println!();

    // 7. Final Verification
    println!("═══════════════════════════════════════════════════════════════");
    println!("✅ PHASE 5: Final Verification");
    println!("═══════════════════════════════════════════════════════════════\n");

    let ar_exists = final_config
        .balance_sheet
        .iter()
        .any(|a| a.name.to_lowercase().contains("receivable"));
    let ap_exists = final_config
        .balance_sheet
        .iter()
        .any(|a| a.name.to_lowercase().contains("payable"));
    let gst_exists = final_config
        .balance_sheet
        .iter()
        .any(|a| a.name.to_lowercase().contains("gst") || a.name.to_lowercase().contains("tax"));

    println!("   Forecasting Readiness Checklist:");
    println!(
        "   {} Accounts Receivable",
        if ar_exists { "✅" } else { "❌" }
    );
    println!(
        "   {} Accounts Payable",
        if ap_exists { "✅" } else { "❌" }
    );
    println!(
        "   {} GST/Tax Payable",
        if gst_exists { "✅" } else { "❌" }
    );
    println!(
        "   ✅ Dense Data Generated: {} accounts with monthly values",
        dense_data.len()
    );
    println!();

    // 8. Save Results
    println!("═══════════════════════════════════════════════════════════════");
    println!("💾 Saving Results");
    println!("═══════════════════════════════════════════════════════════════\n");

    let raw_json = serde_json::to_string_pretty(&raw_config)?;
    std::fs::write("forecasting_raw.json", raw_json)?;
    println!("   ✅ Saved raw extraction: forecasting_raw.json");

    let overrides_json = serde_json::to_string_pretty(&overrides)?;
    std::fs::write("forecasting_overrides.json", overrides_json)?;
    println!("   ✅ Saved overrides: forecasting_overrides.json");

    let final_json = serde_json::to_string_pretty(&final_config)?;
    std::fs::write("forecasting_final.json", final_json)?;
    println!("   ✅ Saved final config: forecasting_final.json");

    // Export dense data to CSV (similar to gemini_pdf_example)
    let base_name = docs
        .first()
        .and_then(|d| {
            Path::new(&d.display_name)
                .file_stem()
                .and_then(|s| s.to_str())
        })
        .unwrap_or("forecasting_output");

    let pl_accounts = collect_income_accounts(&final_config);
    let bs_accounts = collect_balance_accounts(&final_config);

    let pl_filename = format!("{}_pl.csv", base_name);
    export_to_csv_transposed(&pl_accounts, &dense_data, &pl_filename).await?;
    println!("   ✅ Saved P&L CSV: {}", pl_filename);

    let bs_filename = format!("{}_balance_sheet.csv", base_name);
    export_to_csv_transposed(&bs_accounts, &dense_data, &bs_filename).await?;
    println!("   ✅ Saved Balance Sheet CSV: {}", bs_filename);

    println!();
    println!("═══════════════════════════════════════════════════════════════");
    println!("🎉 Forecasting Workflow Complete!");
    println!("═══════════════════════════════════════════════════════════════");

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

fn collect_income_accounts(
    cfg: &financial_history_builder::FinancialHistoryConfig,
) -> Vec<(String, AccountType)> {
    cfg.income_statement
        .iter()
        .map(|a| (a.name.clone(), a.account_type.clone()))
        .collect()
}

fn collect_balance_accounts(
    cfg: &financial_history_builder::FinancialHistoryConfig,
) -> Vec<(String, AccountType)> {
    cfg.balance_sheet
        .iter()
        .map(|a| (a.name.clone(), a.account_type.clone()))
        .collect()
}
