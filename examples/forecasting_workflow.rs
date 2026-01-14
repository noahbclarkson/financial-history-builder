use dotenv::dotenv;
use financial_history_builder::llm::{DocumentReference, FinancialExtractor, ForecastingSetupAgent};
use financial_history_builder::{process_financial_history, AccountType, DenseSeries};
use rstructor::{GeminiClient, GeminiModel, MediaFile};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;

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

    println!("🚀 Financial Forecasting Workflow Demonstration");
    println!("═══════════════════════════════════════════════════════════════\n");

    let client = GeminiClient::new(api_key)?.model(GeminiModel::Gemini25Flash);
    let extractor = FinancialExtractor::new(client.clone());
    let forecaster = ForecastingSetupAgent::new(client.clone());

    let docs = load_documents()?;
    println!("✅ Loaded {} document URIs.\n", docs.len());

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

    let refined_config = overrides.apply(&raw_config);
    let dense = process_financial_history(&refined_config)?;

    println!("═══════════════════════════════════════════════════════════════");
    println!("✅ PHASE 3: Forecast-Ready Data");
    println!("═══════════════════════════════════════════════════════════════\n");

    let mut grouped: BTreeMap<AccountType, Vec<String>> = BTreeMap::new();
    for acc in &refined_config.balance_sheet {
        grouped.entry(acc.account_type.clone()).or_default().push(acc.name.clone());
    }
    println!("Balance Sheet Account Grouping:");
    for (acc_type, names) in grouped {
        println!("  {:?}: {} accounts", acc_type, names.len());
    }

    if let Some(first_account) = refined_config.balance_sheet.first() {
        let series: &DenseSeries = dense
            .get(&first_account.name)
            .expect("Expected dense series for sample account");
        let months: BTreeSet<_> = series.keys().cloned().collect();
        println!("\nSample Densified Values for {}:", first_account.name);
        for date in months.iter().take(3) {
            if let Some(point) = series.get(date) {
                println!("  {}: {:.2}", date, point.value);
            }
        }
    }

    Ok(())
}
