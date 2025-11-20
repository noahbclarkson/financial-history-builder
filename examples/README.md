# Gemini Financial Extraction Example

This example demonstrates how to use the Financial History Builder crate with Google's Gemini 2.5 Pro API to extract structured financial data from unstructured text.

## Overview

The example shows the complete workflow:

1. **Schema Generation**: Automatically generates JSON Schema from Rust structs using `schemars`
2. **API Integration**: Calls Gemini 2.5 Pro via the OpenAI-compatible endpoint with structured output
3. **Type-Safe Parsing**: Deserializes the LLM response directly into Rust structs
4. **Mathematical Processing**: Runs the financial history engine (interpolation + balancing)
5. **Verification**: Validates accounting equation (Assets = Liabilities + Equity)
6. **Output**: Generates CSV file with dense monthly time series

## Prerequisites

1. **Gemini API Key**: Get your API key from [Google AI Studio](https://aistudio.google.com/app/apikey)
2. **Rust**: Ensure you have Rust installed (1.70 or later)

## Setup

### 1. Set Environment Variable

**Linux/macOS:**
```bash
export GEMINI_API_KEY="your_gemini_api_key_here"
```

**Windows (PowerShell):**
```powershell
$env:GEMINI_API_KEY="your_gemini_api_key_here"
```

**Or use a `.env` file:**
Create a `.env` file in the project root:
```
GEMINI_API_KEY=your_gemini_api_key_here
```

### 2. Run the Example

```bash
cargo run --example gemini_extraction
```

## What It Does

### Input
The example uses a mock financial document (simulating OCR output from a PDF):

```
ACME Retail Corporation
Financial Statements - Year Ended Dec 31, 2023

INCOME STATEMENT
Revenue: $3,500,000 (2023), $2,800,000 (2022)
Cost of Sales: $2,100,000 (2023), $1,680,000 (2022)
...
```

### Process

1. **Schema Generation**: The code generates a strict JSON Schema from `SparseFinancialHistory`
2. **LLM Call**: Sends the document to Gemini 2.5 Pro with:
   - System prompt instructing extraction rules
   - The financial document
   - The JSON Schema for structured output
3. **Parsing**: Deserializes the JSON response into Rust structs
4. **Densification**: Converts sparse anchor points to dense monthly time series
5. **Balancing**: Enforces accounting equation by adjusting the balancing account

### Output

**Console:**
```
🚀 Starting Gemini Financial History Builder Example...
📋 Generated JSON Schema for LLM strict output.
🤖 Sending request to Gemini 2.5 Pro...
📥 Received JSON from Gemini.
✅ Successfully parsed into Rust structs: ACME Retail Corporation
⚙️  Running Financial History Engine (Interpolation + Balancing)...
⚖️  Accounting Equation Balanced (Assets = Liab + Equity)
💾 Dense financial history saved to gemini_output.csv
```

**File:** `gemini_output.csv`
- Contains monthly time series for all accounts
- Each row is a month-end date
- Each column is an account
- Values are mathematically consistent and balanced

## Key Features

### Type Safety
The schema passed to Gemini is generated from the same Rust structs used for parsing. This creates a type-safe bridge:

```rust
let schema = schemars::schema_for!(SparseFinancialHistory);
// ... send to Gemini ...
let history: SparseFinancialHistory = serde_json::from_str(content)?;
```

### Structured Output
Uses Gemini's structured output feature via the OpenAI-compatible endpoint to ensure valid JSON:

```rust
.response_format(ResponseFormat::JsonSchema {
    json_schema: ResponseFormatJsonSchema {
        name: "financial_history".into(),
        schema: Some(schema_json),
        strict: Some(true),  // Force strict adherence to schema
        description: None,
    }
})
```

This eliminates the need for markdown parsing - Gemini returns clean JSON that matches your Rust structs exactly.

### Mathematical Integrity
The crate's engine ensures:
- Flow accounts (P&L): Monthly values sum exactly to annual total
- Stock accounts (Balance Sheet): Smooth interpolation between anchor points
- Accounting equation: Assets = Liabilities + Equity at all times

## Customization

### Using Your Own Documents

Replace `MOCK_FINANCIAL_DOC` with actual text from:
- OCR output (Tesseract, Google Cloud Vision, etc.)
- PDF extraction (pdf2text, pdfplumber)
- Any unstructured financial text

### Adjusting the Prompt

Modify `system_prompt` to:
- Change classification rules (e.g., which accounts use seasonal interpolation)
- Adjust balancing account selection logic
- Add domain-specific knowledge

Example:
```rust
let system_prompt = r#"
You are a Financial History Extraction Engine for SaaS companies.
Revenue should use SaasGrowth seasonality profile.
Mark "Cash at Bank" as the balancing account.
Extract MRR, churn, and ARR if mentioned.
...
"#;
```

### Model Selection

To use a different Gemini model, change the model parameter:

```rust
.model("gemini-2.0-flash")     // Faster, cheaper
.model("gemini-2.5-pro")       // More capable (default)
```

## Troubleshooting

### Error: `GEMINI_API_KEY must be set`
Set the environment variable or create a `.env` file.

### Error: API returns 400
- Check that your API key is valid
- Ensure you have Gemini API access enabled
- Verify the base URL is correct for your region

### Error: Deserialization failed
- The LLM might have hallucinated invalid fields
- Check the raw JSON response (add debug logging)
- Adjust the system prompt for stricter adherence

### Error: Accounting equation violation
- This means the extracted data is inconsistent
- The balancing account will be adjusted to force balance
- Review the `gemini_output.csv` to inspect the generated plug values

## Production Use

For production systems:

1. **Error Handling**: Add retry logic and fallbacks
2. **Validation**: Implement business logic validation on extracted data
3. **Logging**: Add structured logging for debugging
4. **Rate Limiting**: Implement rate limiting for API calls
5. **Caching**: Cache results to avoid redundant API calls
6. **Monitoring**: Track extraction accuracy and API costs

## License

MIT
