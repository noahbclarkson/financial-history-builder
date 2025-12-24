# financial-history-builder

[![Crates.io](https://img.shields.io/crates/v/financial-history-builder.svg)](https://crates.io/crates/financial-history-builder)
[![Documentation](https://docs.rs/financial-history-builder/badge.svg)](https://docs.rs/financial-history-builder)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Build Status](https://img.shields.io/badge/build-passing-brightgreen.svg)]()

**A rigorous mathematical engine for converting sparse financial data (annual reports, PDF extractions) into dense, mathematically consistent monthly time series.**

---

## 🎯 The Problem

When extracting financial data from documents (via LLMs or OCR), you often get **sparse** data:

* *"Revenue was \$1.2M in 2023"* (A single number for 12 months)
* *"Cash on Dec 31 was \$50k"* (A single snapshot)
* *"Q1 Expenses were \$30k"* (A period total)

To build a financial forecast, you need **dense** data: a specific value for every single month.

## 💡 The Solution

`financial-history-builder` fills the gaps between these numbers using two distinct mathematical models, applies industry-specific seasonality, adds realistic variance (noise), and ensures the accounting equation (`Assets = Liabilities + Equity`) balances perfectly every month.

### Key Features

* **Hierarchical Constraint Solving:** Handles overlapping periods (e.g., specific Q1 data + total Annual data) by locking detailed data first and distributing the remainder.
* **Smart Interpolation:** Generates monthly balance sheet positions using Linear, Step, or Catmull-Rom Spline interpolation.
* **Seasonality Profiles:** Applies realistic curves (Retail Peak, SaaS Growth, Summer High) to distribute revenue/expenses accurately.
* **Accounting Integrity:** Automatically enforces $Assets = Liabilities + Equity$ by calculating a balancing plug (usually Cash or Equity).
* **LLM Ready:** Generates strict JSON Schemas (`schemars`) to force AI models (Gemini, GPT-4) to output data in the exact format the engine requires.

---

## 📦 Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
financial-history-builder = "0.1.0"
```

---

## 📐 Mathematical Models

This library treats the **Income Statement** and **Balance Sheet** as fundamentally different mathematical objects.

### 1. Income Statement: The "Bucket Filling" Model

Income statement items (Revenue, Expenses) are **flows**. The engine uses a **Hierarchical Constraint Solver**.

Given a set of constraints $C = \{c_1, c_2, ...\}$ where each constraint has a time range $[t_{start}, t_{end}]$ and a value $V$:

1. **Sort Constraints:** Smallest time ranges (e.g., "February") are processed first. Largest (e.g., "2023 Full Year") are processed last.
2. **Locking:** When a small period is solved, those months are "locked."
3. **Distribution:** For a larger period, the engine calculates the **Remaining Value**:
    $$V_{remaining} = V_{total} - \sum V_{locked}$$
4. **Seasonality Weighting:** The remaining value is distributed among the *unlocked* months based on the account's Seasonality Profile ($w_m$):
    $$Value_m = V_{remaining} \times \frac{w_m}{\sum_{i \in Unlocked} w_i}$$

> **Example:**
>
> * Annual Revenue: \$120,000
> * Known February Revenue: \$2,000
> * **Result:** February is locked at \$2,000. The remaining \$118,000 is distributed across the other 11 months according to the seasonality curve.

### 2. Balance Sheet: The "Curve Fitting" Model

Balance sheet items (Assets, Liabilities) are **stocks** (snapshots). The engine uses **Interpolation**.

Given a set of snapshots $S = \{(t_1, v_1), (t_2, v_2), ...\}$:

* **Linear:** $f(t) = mt + c$ (Steady growth/decline)
* **Step:** $f(t) = v_i$ for $t_i \le t < t_{i+1}$ (Fixed costs, share capital)
* **Curve:** Uses **Catmull-Rom Splines** to create smooth, organic transitions between points, ensuring the line passes through every snapshot exactly.

### 3. The Accounting Equation Enforcer

For every generated month $t$:

$$Assets_t = Liabilities_t + Equity_t$$

The engine sums all known accounts. It then calculates the discrepancy and adjusts the designated `is_balancing_account` (usually "Cash at Bank" or "Retained Earnings") to force the equation to zero.

---

## 🚀 Usage Example

### Basic Rust Implementation

```rust
use financial_history_builder::*;
use chrono::NaiveDate;

fn main() -> Result<()> {
    // 1. Define the configuration
    let config = FinancialHistoryConfig {
        organization_name: "ACME SaaS Inc".to_string(),
        fiscal_year_end_month: 12,
        // Balance Sheet: Snapshots
        balance_sheet: vec![
            BalanceSheetAccount {
                name: "Cash at Bank".to_string(),
                account_type: AccountType::Asset,
                method: InterpolationMethod::Linear,
                snapshots: vec![
                    BalanceSheetSnapshot {
                        date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                        value: 150_000.0,
                        source: None,
                    }
                ],
                is_balancing_account: true, // <--- Auto-calculates this
                noise_factor: 0.0,
            },
        ],
        // Income Statement: Period Constraints
        income_statement: vec![
            IncomeStatementAccount {
                name: "Subscription Revenue".to_string(),
                account_type: AccountType::Revenue,
                seasonality_profile: SeasonalityProfileId::SaasGrowth,
                constraints: vec![
                    // Constraint 1: Full Year
                    PeriodConstraint {
                        period: "2023-01:2023-12".to_string(),
                        value: 1_200_000.0,
                        source: None,
                    },
                    // Constraint 2: Specific Q4 bump
                    PeriodConstraint {
                        period: "2023-10:2023-12".to_string(),
                        value: 400_000.0, 
                        source: None,
                    }
                ],
                noise_factor: 0.05, // Add 5% random noise
            },
        ],
    };

    // 2. Process into dense time series
    let dense_data = process_financial_history(&config)?;

    // 3. Access data (BTreeMap<String, BTreeMap<NaiveDate, f64>>)
    let revenue = dense_data.get("Subscription Revenue").unwrap();
    
    println!("Monthly Revenue:");
    for (date, value) in revenue {
        println!("{}: ${:.2}", date, value);
    }

    Ok(())
}
```

---

## 🤖 AI & LLM Integration

This library ships with a `llm` module that uses `gemini-structured-output` for strongly typed extraction, refinement, and forecasting setup.

### 1. Typed Extraction with Gemini Structured Output

```rust
use financial_history_builder::llm::FinancialExtractor;
use gemini_structured_output::prelude::{Model, StructuredClientBuilder};

# async fn example() -> Result<(), Box<dyn std::error::Error>> {
let client = StructuredClientBuilder::new("your-api-key")
    .with_model(Model::Gemini25Flash)
    .build()?;

let extractor = FinancialExtractor::new(client.clone());
let docs = vec![client.file_manager.upload_and_wait("examples/documents/report.pdf").await?];

let config = extractor.extract(&docs, None).await?;
println!("Extracted {} accounts", config.balance_sheet.len());
# Ok(())
# }
```

### 2. Optional Schema Introspection

You can still generate a JSON schema for auditing or debugging:

```rust
use financial_history_builder::FinancialHistoryConfig;

fn main() {
    println!("{}", FinancialHistoryConfig::schema_as_json().unwrap());
}
```

---

## ⚙️ Configuration Options

### Seasonality Profiles

When defining Income Statement accounts, you can select a profile to control how annual totals are distributed:

| Profile      | Description                                                         |
| :----------- | :------------------------------------------------------------------ |
| `Flat`       | Even distribution (8.33% per month). Used for rent, fixed salaries. |
| `RetailPeak` | Low Jan-Nov, massive spike (30%+) in December.                      |
| `SummerHigh` | High Q2/Q3, low Q1/Q4. Good for tourism/hospitality.                |
| `SaasGrowth` | Back-loaded linear growth. Month 12 is higher than Month 1.         |
| `Custom`     | Provide your own `Vec<f64>` of 12 weights summing to 1.0.           |

### Interpolation Methods

For Balance Sheet accounts:

| Method   | Description                                                                      |
| :------- | :------------------------------------------------------------------------------- |
| `Linear` | Straight line between points. Good for loans, receivables.                       |
| `Step`   | Holds value constant until the next snapshot. Good for Share Capital.            |
| `Curve`  | Catmull-Rom spline. Good for organic accounts (e.g., Retained Earnings history). |

### Noise Factors

To make synthetic monthly data look realistic, you can inject Gaussian noise.

* `0.0`: No noise (Rent).
* `0.02`: Low noise (Salaries).
* `0.05 - 0.10`: High noise (Marketing spend, incidental expenses).

*Note: The engine automatically re-normalizes after adding noise to ensure the sum still exactly matches the anchor constraint.*

---

## 🏗️ Directory Structure

```text
financial-history-builder
├── examples
│   ├── gemini_extraction.rs   # Full AI extraction workflow
│   ├── intra_year_demo.rs     # Demonstrates overlapping period logic
│   └── debug_test.rs          # Simple manual test
├── src
│   ├── balancer.rs            # Accounting equation logic
│   ├── engine.rs              # Core mathematical densifier
│   ├── seasonality.rs         # Seasonality weight definitions
│   └── schema.rs              # Structs and JSON Schema generation
├── GEMINI_PROMPT_EXAMPLE.md   # Prompt engineering guide
└── Cargo.toml
```

---

**Built for financial modeling.** Ensures that even when data is invented (interpolated), it remains mathematically and accountingly sound.
