# Financial History Builder

A Rust library for converting sparse financial data (extracted from PDFs/documents via LLM) into dense monthly time series with mathematical integrity and automatic accounting equation balancing.

## 🎯 Purpose

This crate acts as a bridge between unstructured text/PDF data (via AI/LLM) and the rigorous mathematical requirements of financial forecasting engines. It transforms a few annual data points into complete monthly time series while:

- ✅ Maintaining mathematical accuracy (sums match exactly)
- ✅ Adding realistic variance through controlled noise
- ✅ Enforcing accounting integrity (Assets = Liabilities + Equity)
- ✅ Applying industry-specific seasonality patterns

## 🚀 Features

- **Smart Interpolation**: Multiple methods (linear, step, curve, seasonal) for different account types
- **Seasonality Profiles**: Pre-built patterns for retail, SaaS, hospitality, and custom patterns
- **Accounting Enforcement**: Automatic balancing to ensure Assets = Liabilities + Equity
- **Realistic Noise**: Configurable variance to make synthetic data look organic
- **JSON Schema**: Detailed schemars integration for AI/LLM consumption
- **Flow vs Stock**: Proper handling of P&L (period totals) vs Balance Sheet (point-in-time) accounts

## 📦 Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
financial-history-builder = "0.1.0"
```

## 🔧 Basic Usage

```rust
use financial_history_builder::*;
use chrono::NaiveDate;

fn main() -> Result<()> {
    // Define sparse financial history
    let history = SparseFinancialHistory {
        organization_name: "ACME Corp".to_string(),
        fiscal_year_end_month: 12,
        accounts: vec![
            SparseAccount {
                name: "Sales Revenue".to_string(),
                account_type: AccountType::Revenue,
                behavior: AccountBehavior::Flow,
                interpolation: InterpolationMethod::Seasonal {
                    profile_id: SeasonalityProfileId::RetailPeak,
                },
                noise_factor: Some(0.05),
                anchors: vec![
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                        value: 1_200_000.0,
                    },
                ],
            },
        ],
    };

    // Process into dense monthly data
    let dense_data = process_financial_history(&history)?;

    // Verify accounting equation holds
    verify_accounting_equation(&history, &dense_data, 1.0)?;

    Ok(())
}
```

## 📊 Core Concepts

### Account Types

- **Revenue**: Income from sales/services (Income Statement, credit)
- **CostOfSales**: Direct production costs (Income Statement, debit)
- **OperatingExpense**: Operating costs like rent, salaries (Income Statement, debit)
- **OtherIncome**: Non-operating income (Income Statement, credit)
- **Asset**: Resources owned (Balance Sheet, debit)
- **Liability**: Obligations owed (Balance Sheet, credit)
- **Equity**: Owner's interest (Balance Sheet, credit)

### Account Behaviors

- **Flow**: Represents activity over a period (e.g., Revenue, Expenses)
  - Anchor value = total for the period
  - Distributed across months based on interpolation method

- **Stock**: Represents snapshot at a point in time (e.g., Cash, Accounts Receivable)
  - Anchor value = balance on that specific date
  - Interpolated between anchor dates

### Interpolation Methods

1. **Linear**: Steady progression between points
   - Use for: General growth, gradual changes

2. **Step**: Value holds constant until changed
   - Use for: Fixed costs (rent, insurance, subscriptions)

3. **Curve**: Smooth Catmull-Rom interpolation
   - Use for: Organic balance sheet changes

4. **Seasonal**: Distribute based on predefined patterns
   - `Flat`: Even 8.33% per month
   - `RetailPeak`: Low Jan-Nov, 30%+ spike in December
   - `SummerHigh`: Tourism pattern, high Q2/Q3
   - `SaasGrowth`: Back-loaded growth (month 1: 6%, month 12: 10%)
   - `Custom([f64; 12])`: Your own monthly weights (must sum to 1.0)

### Noise Factors

Add realistic variation to synthetic data:

- `0.0` - No noise (fixed costs like rent)
- `0.01-0.02` - Very stable (balance sheet accounts, fixed salaries)
- `0.03-0.05` - Normal variation (most revenues and variable expenses)
- `0.06-0.10` - High variation (marketing, seasonal items)

## 🧪 Testing

Run the comprehensive test suite:

```bash
cargo test
```

This will:
- Run all unit tests
- Execute integration tests for retail, SaaS, and hospitality businesses
- Generate CSV outputs for inspection:
  - `test_retail_business.csv`
  - `test_saas_startup.csv`
  - `test_hospitality_business.csv`
  - `test_custom_seasonality.csv`
- Generate `schema_output.json` with the complete JSON schema

## 🤖 AI Integration

This library is designed to work seamlessly with LLMs like Gemini 2.5 Pro. See [`GEMINI_PROMPT_EXAMPLE.md`](GEMINI_PROMPT_EXAMPLE.md) for a comprehensive prompt that instructs an AI to extract financial data from documents and output in the correct JSON format.

### Generate JSON Schema for LLM

```rust
use financial_history_builder::SparseFinancialHistory;

// Get the JSON schema
let schema_json = SparseFinancialHistory::schema_as_json().unwrap();
println!("{}", schema_json);
```

This schema can be provided to the LLM to ensure it outputs data in the exact format this library expects.

## 📈 Accounting Integrity

The library automatically enforces the fundamental accounting equation:

```
Assets = Liabilities + Equity
```

When you call `process_financial_history()`, it will:

1. Densify all accounts into monthly time series
2. Calculate Assets, Liabilities, and existing Equity for each month
3. Create a "Balancing Equity Adjustment" account to ensure the equation holds
4. Verify the equation is balanced within tolerance

## ⚠️ Important Notes

1. **Flow accounts** must have anchors that represent period **end dates** (e.g., 2023-12-31 for FY2023)
2. **Stock accounts** must have anchors that represent **snapshot dates** (e.g., 2023-12-31 for Dec 31 balance)
3. The sum of monthly flow values will **exactly** match the annual anchor (accounting integrity)
4. Stock accounts will **exactly** match anchor values on anchor dates
5. Noise is added proportionally and then re-normalized to maintain exact sums
6. The accounting equation is automatically balanced via an equity adjustment account

## 📄 License

MIT

---

**Built with ❤️ for the financial technology community**
