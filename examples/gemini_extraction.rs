use async_openai::{
    config::OpenAIConfig,
    types::{
        ChatCompletionRequestMessage, ChatCompletionRequestSystemMessage,
        ChatCompletionRequestUserMessage, CreateChatCompletionRequestArgs,
        ResponseFormat, ResponseFormatJsonSchema,
    },
    Client,
};
use dotenv::dotenv;
use financial_history_builder::{
    process_financial_history, verify_accounting_equation, SparseFinancialHistory,
};
use std::error::Error;
use std::fs::File;
use std::io::Write;

// 1. Define the Input Document (Simulated OCR output)
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

    // 2. Generate JSON Schema from Rust Structs
    let schema = schemars::schema_for!(SparseFinancialHistory);
    let schema_json = serde_json::to_value(&schema)?;

    println!("📋 Generated JSON Schema for LLM structured output.");

    // 3. Initialize Gemini Client via OpenAI Shim
    let api_key = std::env::var("GEMINI_API_KEY").expect("GEMINI_API_KEY must be set");
    let base_url = "https://generativelanguage.googleapis.com/v1beta/openai";

    let config = OpenAIConfig::new()
        .with_api_base(base_url)
        .with_api_key(api_key);

    let client = Client::with_config(config);

    // 4. Construct the system and user prompts
    let system_prompt = r#"You are a Financial History Extraction Engine.
Extract accounts, classify them, and determine appropriate interpolation methods.

INTERPOLATION FORMAT (CRITICAL):
- For Seasonal interpolation, use: {"method": "Seasonal", "profile_id": "RetailPeak"}
- For Linear interpolation, use: {"method": "Linear"}
- For Step interpolation, use: {"method": "Step"}
- For Curve interpolation, use: {"method": "Curve"}

Available profile_id values: Flat, RetailPeak, SummerHigh, SaasGrowth, Custom

IMPORTANT FOR INTERPOLATION:
- For businesses that look like standard retail, default to 'Seasonal' with 'RetailPeak'.
- However, if the business seems steady, prefer 'Linear'.
- 'RetailPeak' puts 30% of revenue in December. Only use this if the text implies strong holiday sales.

ACCOUNT CLASSIFICATION:
- Revenue accounts: Use Seasonal interpolation with RetailPeak profile
- Expense accounts (fixed like rent): Use Step interpolation
- Balance sheet accounts: Use Linear interpolation

ANCHOR POINT EXTRACTION (CRITICAL):

**For Flow Accounts (P&L - Revenue, Expenses):**
You must specify the `anchor_type` for each data point.
1. **Cumulative (Default)**: Use for annual totals or Year-to-Date (YTD) figures.
   - Example: "Revenue YTD June: $510k" → { "value": 510000, "anchor_type": "Cumulative", "date": "2023-06-30" }
   - Example: "Annual Revenue $3.5M" → { "value": 3500000, "anchor_type": "Cumulative", "date": "2023-12-31" }
2. **Period**: Use for values representing a specific slice of time (single quarter/month only).
   - Example: "Q4 Revenue was $1.5M" → { "value": 1500000, "anchor_type": "Period", "date": "2023-12-31" }
3. You may mix Period and Cumulative anchors in the same fiscal year on different dates. Example: "Q1 revenue was 20,000 and full year was 55,000" → Period anchor at 2023-03-31 for 20000; Cumulative anchor at 2023-12-31 for 55000.
   Always provide both (partial period + later cumulative/period) so the engine can spread Q1 across Jan–Mar and the remainder across later months.
4. If you provide a Period anchor without any earlier anchor, it will apply only to that month. To spread a quarter-sized Period across multiple months, also provide the preceding anchor (e.g., cumulative through the prior month).
5. Do not place a Period and a Cumulative anchor on the same date.

**For Stock Accounts (Balance Sheet - Assets, Liabilities, Equity):**
- Extract ALL date snapshots including mid-year figures
- Each anchor is the balance AT that specific date
- `anchor_type` defaults to "Cumulative" (snapshot balance)
- Always include an opening balance at the start of the earliest fiscal year mentioned; if only a year-end balance is given, duplicate it at the fiscal year start month-end (e.g., 2022-01-31). Use month-end dates.

IMPORTANT RULES:
1. Exactly ONE account (usually "Cash" or "Cash at Bank") MUST have is_balancing_account: true
2. All other accounts MUST have is_balancing_account: false
3. Extract the organization name and fiscal year end month (1-12)
4. For each account, extract ALL anchor points with dates and values
5. Dates should be in format YYYY-MM-DD (end of month)
6. Account types: Asset, Liability, Equity, Revenue, CostOfSales, OperatingExpense, OtherIncome
7. Behavior: Flow for P&L accounts, Stock for balance sheet accounts

Output valid JSON matching the provided schema."#;

    // 5. Call Gemini 2.5 Pro with structured output
    println!("🤖 Sending request to Gemini 2.5 Pro...");

    let request = CreateChatCompletionRequestArgs::default()
        .model("gemini-2.5-pro")
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

    let response = client.chat().create(request).await
        .map_err(|e| format!("API error: {}", e))?;

    println!("📥 Received response from Gemini.");

    // Extract the structured JSON content
    let content = response
        .choices
        .first()
        .and_then(|choice| choice.message.content.as_ref())
        .ok_or_else(|| -> Box<dyn Error> { "No content in response".into() })?;

    println!("📝 Raw JSON response:\n{}\n", content);

    // Save for debugging
    std::fs::write("gemini_raw_response.txt", content)?;

    // 6. Deserialize directly - structured output returns clean JSON
    println!("🔄 Parsing structured JSON output...");
    let history: SparseFinancialHistory = serde_json::from_str(content)
        .map_err(|e| -> Box<dyn Error> {
            eprintln!("❌ JSON Parse Error: {}", e);
            format!("JSON parse error: {}", e).into()
        })?;

    println!("✅ Successfully parsed into Rust structs: {}", history.organization_name);

    // 7. Run the Math Engine (Densify & Balance)
    println!("⚙️  Running Financial History Engine (Interpolation + Balancing)...");
    let dense_data = process_financial_history(&history)
        .map_err(|e| format!("Financial processing error: {}", e))?;

    // 8. Verify Accounting Integrity
    match verify_accounting_equation(&history, &dense_data, 1.0) {
        Ok(_) => println!("⚖️  Accounting Equation Balanced (Assets = Liab + Equity)"),
        Err(e) => eprintln!("⚠️  Balance Warning: {}", e),
    }

    // 9. Output to CSV for verification
    let filename = "gemini_output.csv";
    let mut file = File::create(filename)?;

    // Header
    let account_names: Vec<String> = dense_data.keys().cloned().collect();
    write!(file, "Date")?;
    for name in &account_names {
        write!(file, ",{}", name)?;
    }
    writeln!(file)?;

    // Data Rows (get all dates)
    let mut dates: Vec<chrono::NaiveDate> = dense_data.values()
        .flat_map(|s| s.keys())
        .copied()
        .collect();
    dates.sort();
    dates.dedup();

    for date in dates {
        write!(file, "{}", date)?;
        for name in &account_names {
            let val = dense_data.get(name).and_then(|s| s.get(&date)).unwrap_or(&0.0);
            write!(file, ",{:.2}", val)?;
        }
        writeln!(file)?;
    }

    println!("💾 Dense financial history saved to {}", filename);

    Ok(())
}
