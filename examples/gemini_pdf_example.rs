use dotenv::dotenv;
use financial_history_builder::llm::{DocumentReference, ExtractionEvent, FinancialExtractor};
use financial_history_builder::{
    process_financial_history, verify_accounting_equation, AccountType, DenseSeries,
    FinancialHistoryConfig,
};
use rstructor::{GeminiClient, GeminiModel, MediaFile};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use tokio::sync::mpsc;

fn load_documents() -> Result<Vec<DocumentReference>, Box<dyn Error>> {
    let uris = std::env::var("GEMINI_FILE_URIS")?;
    let names = std::env::var("GEMINI_FILE_NAMES").ok();
    let name_list: Vec<String> = names
        .map(|value| value.split(',').map(|s| s.trim().to_string()).collect())
        .unwrap_or_default();

    let documents = uris
        .split(',')
        .enumerate()
        .map(|(index, uri)| {
            let name = name_list
                .get(index)
                .cloned()
                .unwrap_or_else(|| format!("Document {}", index + 1));
            DocumentReference::new(MediaFile::new(uri.trim(), "application/pdf"), name)
        })
        .collect::<Vec<_>>();

    if documents.is_empty() {
        return Err("GEMINI_FILE_URIS must include at least one URI".into());
    }

    Ok(documents)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    dotenv().ok();
    let api_key = std::env::var("GEMINI_API_KEY").expect("GEMINI_API_KEY must be set");

    println!("🚀 Starting Observable Agentic Financial Extraction Workflow...\n");

    let client = GeminiClient::new(api_key)?.model(GeminiModel::Gemini25Flash);
    let extractor = FinancialExtractor::new(client.clone());

    let documents = load_documents()?;
    println!("✅ Loaded {} document URIs.\n", documents.len());

    let (tx, mut rx) = mpsc::channel(32);
    let extraction_docs = documents.clone();

    let extraction_handle =
        tokio::spawn(async move { extractor.extract(&extraction_docs, Some(tx)).await });

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

    let mut config = extraction_handle.await??;

    println!("\n✅ Initial Extraction Complete:");
    println!("   Organization: {}", config.organization_name);
    println!("   Fiscal Year End: Month {}", config.fiscal_year_end_month);
    println!("   Balance Sheet Accounts: {}", config.balance_sheet.len());
    println!(
        "   Income Statement Accounts: {}",
        config.income_statement.len()
    );

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

    println!("\n🧠 Running validation and integrity checks...");
    let dense = process_financial_history(&config)?;
    verify_accounting_equation(&config, &dense, 1.0)?;

    if let Some(sample) = config.balance_sheet.first() {
        let account_name = &sample.name;
        let series: &DenseSeries = dense
            .get(account_name)
            .expect("Expected dense series for sample account");
        let mut months: BTreeSet<_> = series.keys().cloned().collect();
        months.iter().take(3).for_each(|date| {
            if let Some(point) = series.get(date) {
                println!("{} {}: {:.2}", account_name, date, point.value);
            }
        });
    }

    // Demonstrate override application
    let mut overrides = financial_history_builder::FinancialHistoryOverrides::default();
    if let Some(first_account) = config.balance_sheet.first() {
        overrides.modifications.push(financial_history_builder::AccountModification::Rename {
            target: first_account.name.clone(),
            new_name: format!("{} (Reviewed)", first_account.name),
        });
    }
    config = overrides.apply(&config);

    let dense_after = process_financial_history(&config)?;
    let _recheck = verify_accounting_equation(&config, &dense_after, 1.0)?;

    println!("\n✅ Overrides applied and validated.");

    let mut grouped: BTreeMap<AccountType, Vec<String>> = BTreeMap::new();
    for acc in &config.balance_sheet {
        grouped.entry(acc.account_type.clone()).or_default().push(acc.name.clone());
    }
    println!("\nBalance Sheet Account Grouping:");
    for (acc_type, names) in grouped {
        println!("  {:?}: {} accounts", acc_type, names.len());
    }

    Ok(())
}
