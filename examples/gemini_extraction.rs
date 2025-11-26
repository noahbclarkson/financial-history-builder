use async_openai::{
    config::OpenAIConfig,
    types::{
        ChatCompletionRequestMessage, ChatCompletionRequestSystemMessage,
        ChatCompletionRequestUserMessage, CreateChatCompletionRequestArgs, ResponseFormat,
        ResponseFormatJsonSchema,
    },
    Client,
};
use dotenv::dotenv;
use financial_history_builder::{
    process_financial_history, verify_accounting_equation, DataOrigin, FinancialHistoryConfig,
};
use std::error::Error;
use std::fs::File;
use std::io::Write;

const MOCK_FINANCIAL_DOC: &str = r#"
ACME Retail Corporation
Financial Statements - Year Ended Dec 31, 2023

INCOME STATEMENT
Revenue: $3,500,000 (2023 Total), $2,800,000 (2022 Total)
  - Q4 2023 Specific: $1,500,000 (Period value)
Cost of Sales: $2,100,000 (2023), $1,680,000 (2022)
  - Mid-year 2023 YTD: $510,000 (Through June 30)
Expenses:
  Rent: $120,000 per year (Fixed)
  Salaries: $650,000 (2023), $580,000 (2022)
  Marketing: $280,000 (2023), $180,000 (2022)
  Utilities: $45,000 (2023), $38,000 (2022)

BALANCE SHEET (As of Dec 31)
Assets:
  Cash: $185,000 (2023), $142,000 (2022)
    - June 30, 2023: $165,000
  Inventory: $680,000 (2023), $550,000 (2022)
    - June 30, 2023: $620,000
  Equipment: $450,000 (2023), $450,000 (2022)
Liabilities:
  Accounts Payable: $285,000 (2023), $220,000 (2022)
    - June 30, 2023: $250,000
  Bank Loan: $450,000 (2023), $450,000 (2022)
Equity:
  Share Capital: $500,000 (constant)
  Retained Earnings: $500,000 (2023), $280,000 (2022)
    - June 30, 2023: $380,000
"#;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    dotenv().ok();
    println!("🚀 Starting Gemini Financial History Builder Example...");

    // USE THE NEW HELPER FUNCTION HERE
    let schema_json = FinancialHistoryConfig::get_gemini_response_schema()?;

    println!("📋 Generated JSON Schema for LLM structured output.");

    let api_key = std::env::var("GEMINI_API_KEY").expect("GEMINI_API_KEY must be set");
    let base_url = "https://generativelanguage.googleapis.com/v1beta/openai";

    let config = OpenAIConfig::new()
        .with_api_base(base_url)
        .with_api_key(api_key);

    let client = Client::with_config(config);

    let system_prompt = r#"You are a Financial Data Extraction Engine.

Extract financial data and output JSON matching the provided schema.

TRUST LAYER - SOURCE TRACKING:
- Populate `source` on snapshots/constraints with `{ document, text }`.
- If extracting from a table, map the numeric value but leave `text` null to save tokens.

THE NEW FINANCIAL MODEL:
1. BALANCE SHEET (Point-in-time snapshots):
   - Use `balance_sheet` array
   - Each account has `snapshots` with `{date, value}`
   - Dates are when the balance was observed (e.g., 2023-12-31)
   - **CRITICAL**: ALWAYS provide an opening balance at the start of the earliest fiscal year
     * If only year-end balances are mentioned, CREATE an opening balance by using the previous year's ending balance
     * If no previous year exists, duplicate the first mentioned balance as the opening balance
     * Example: If you see "Cash Dec 31, 2023: $185k" and "Cash Dec 31, 2022: $142k", provide:
       - Opening: Jan 31, 2022 = $142,000 (start of FY 2022)
       - Closing: Dec 31, 2022 = $142,000
       - Mid-year: Jun 30, 2023 = $165,000 (if mentioned)
       - Closing: Dec 31, 2023 = $185,000
   - Extract ALL snapshots including mid-year figures
   - Choose interpolation method: Linear, Step, or Curve

2. INCOME STATEMENT (Period totals):
   - Use `income_statement` array
   - Each account has `constraints` with `period` string + `value` (e.g., \"2023-01\" or \"2023-01:2023-12\")
   - PROVIDE ALL OVERLAPPING PERIODS YOU SEE:
     * Monthly values (\"2023-01\")
     * Quarterly totals (\"2023-01:2023-03\")
     * Half-year totals (\"2023-01:2023-06\")
     * Annual totals (\"2023-01:2023-12\")
   - Example: If you see \"Q4 was $1.5M\" and \"Full year was $3.5M\", provide BOTH:
     * Constraint: period \"2023-10:2023-12\" = $1,500,000
     * Constraint: period \"2023-01:2023-12\" = $3,500,000
   - Example: If you see \"YTD June was $510k\" and \"Full year was $2.1M\", provide BOTH:
     * Constraint: period \"2023-01:2023-06\" = $510,000
     * Constraint: period \"2023-01:2023-12\" = $2,100,000
   - Choose seasonality profile: Flat, RetailPeak, SummerHigh, SaasGrowth

IMPORTANT RULES:
1. Exactly ONE balance sheet account MUST have is_balancing_account: true (usually Cash)
2. Extract organization name and fiscal year end month (1-12)
3. Account types: Asset, Liability, Equity, Revenue, CostOfSales, OperatingExpense, OtherIncome
4. Balance Sheet snapshot dates should be YYYY-MM-DD (month-end preferred); Income Statement uses `period` strings as described above
5. For balance sheet: Extract opening AND closing balances if both years are mentioned

OUTPUT:
Generate valid JSON matching the FinancialHistoryConfig schema."#;

    println!("🤖 Sending request to Gemini 2.5 Pro...");

    let request = CreateChatCompletionRequestArgs::default()
        .model("gemini-2.5-flash-preview-09-2025")
        .messages(vec![
            ChatCompletionRequestMessage::System(ChatCompletionRequestSystemMessage {
                content: system_prompt.into(),
                ..Default::default()
            }),
            ChatCompletionRequestMessage::User(ChatCompletionRequestUserMessage {
                content: MOCK_FINANCIAL_DOC.into(),
                ..Default::default()
            }),
        ])
        .response_format(ResponseFormat::JsonSchema {
            json_schema: ResponseFormatJsonSchema {
                name: "financial_history".into(),
                schema: Some(schema_json),
                strict: Some(true),
                description: None,
            },
        })
        .build()?;

    let response = client
        .chat()
        .create(request)
        .await
        .map_err(|e| format!("API error: {}", e))?;

    println!("📥 Received response from Gemini.");

    let content = response
        .choices
        .first()
        .and_then(|choice| choice.message.content.as_ref())
        .ok_or_else(|| -> Box<dyn Error> { "No content in response".into() })?;

    println!("📝 Raw JSON response:\n{}\n", content);

    std::fs::write("gemini_raw_response.txt", content)?;

    println!("🔄 Parsing structured JSON output...");
    let config: FinancialHistoryConfig =
        serde_json::from_str(content).map_err(|e| -> Box<dyn Error> {
            eprintln!("❌ JSON Parse Error: {}", e);
            format!("JSON parse error: {}", e).into()
        })?;

    println!(
        "✅ Successfully parsed into Rust structs: {}",
        config.organization_name
    );

    println!("⚙️  Running Financial History Engine (Interpolation + Balancing)...");
    let dense_data = process_financial_history(&config)
        .map_err(|e| format!("Financial processing error: {}", e))?;

    match verify_accounting_equation(&config, &dense_data, 1.0) {
        Ok(_) => println!("⚖️  Accounting Equation Balanced (Assets = Liab + Equity)"),
        Err(e) => eprintln!("⚠️  Balance Warning: {}", e),
    }

    if let Some((account_name, series)) = dense_data
        .iter()
        .find(|(name, _)| name.to_lowercase().contains("revenue"))
    {
        if let Some((date, point)) = series.iter().next() {
            let origin_label = match point.origin {
                DataOrigin::Anchor => "Anchor",
                DataOrigin::Interpolated => "Interpolated",
                DataOrigin::Allocated => "Allocated",
                DataOrigin::BalancingPlug => "Balancing Plug",
            };
            let source_doc = point
                .source
                .as_ref()
                .map(|s| s.document_name.as_str())
                .unwrap_or("Unknown source document");
            println!(
                "🔎 User hovers over {} {}: Shown as '{}' from '{}'",
                date.format("%b %Y"),
                account_name,
                origin_label,
                source_doc
            );
        }
    }

    let filename = "gemini_output.csv";
    let mut file = File::create(filename)?;

    let account_names: Vec<String> = dense_data.keys().cloned().collect();
    write!(file, "Date")?;
    for name in &account_names {
        write!(file, ",{}", name)?;
    }
    writeln!(file)?;

    let mut dates: Vec<chrono::NaiveDate> = dense_data
        .values()
        .flat_map(|s| s.keys())
        .copied()
        .collect();
    dates.sort();
    dates.dedup();

    for date in dates {
        write!(file, "{}", date)?;
        for name in &account_names {
            let val = dense_data
                .get(name)
                .and_then(|s| s.get(&date))
                .map(|point| point.value)
                .unwrap_or(0.0);
            write!(file, ",{:.2}", val)?;
        }
        writeln!(file)?;
    }

    println!("💾 Dense financial history saved to {}", filename);

    Ok(())
}
