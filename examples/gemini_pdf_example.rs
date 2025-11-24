use dotenv::dotenv;
use financial_history_builder::llm::{FinancialExtractor, GeminiClient};
use financial_history_builder::{
    process_financial_history, verify_accounting_equation, AccountType, DenseSeries,
    FinancialHistoryConfig,
};
use futures::future;
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::path::{Path, PathBuf};
use tokio::fs;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    dotenv().ok();
    let api_key = std::env::var("GEMINI_API_KEY").expect("GEMINI_API_KEY must be set");

    println!("🚀 Starting Enhanced Gemini PDF Extraction Workflow with Self-Correction...\n");

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
            if doc.is_active() { "ACTIVE" } else { &doc.state }
        );
    }
    println!();

    println!("🤖 Extracting financial data with self-correcting validation loop...");
    println!("   (This will automatically retry if validation errors are detected)\n");

    let config = extractor.extract(&documents).await?;

    println!("\n✅ Extraction successful!");
    println!("   Organization: {}", config.organization_name);
    println!("   Fiscal Year End: Month {}", config.fiscal_year_end_month);
    println!("   Balance Sheet Accounts: {}", config.balance_sheet.len());
    println!(
        "   Income Statement Accounts: {}",
        config.income_statement.len()
    );

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

    match verify_accounting_equation(&config, &dense_data, 1.0) {
        Ok(_) => println!("\n✅ Accounting Equation Holds (Assets == Liab + Equity)"),
        Err(e) => println!("\n⚠️  Balance Warning: {}", e),
    }

    let config_file = "extracted_config.json";
    let json = serde_json::to_string_pretty(&config)?;
    std::fs::write(config_file, json)?;
    println!("\n💾 Saved configuration to: {}", config_file);

    let csv_filename = format!(
        "{}_dense.csv",
        pdf_paths
            .first()
            .and_then(|p| p.file_stem().and_then(|s| s.to_str()))
            .unwrap_or("financial_history")
    );
    export_to_csv(&dense_data, &csv_filename).await?;
    println!("💾 Saved dense financial history to: {}", csv_filename);

    Ok(())
}

async fn export_to_csv(
    dense_data: &BTreeMap<String, DenseSeries>,
    filename: &str,
) -> Result<(), Box<dyn Error>> {
    let mut csv_out = String::new();
    let accounts: Vec<_> = dense_data.keys().cloned().collect();

    csv_out.push_str("Date");
    for acc in &accounts {
        csv_out.push_str(&format!(",{}", acc));
    }
    csv_out.push('\n');

    if let Some(first_series) = dense_data.values().next() {
        for date in first_series.keys() {
            csv_out.push_str(&date.to_string());
            for acc in &accounts {
                let val = dense_data
                    .get(acc)
                    .and_then(|s| s.get(date))
                    .map(|p| p.value)
                    .unwrap_or(0.0);
                csv_out.push_str(&format!(",{:.2}", val));
            }
            csv_out.push('\n');
        }
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
