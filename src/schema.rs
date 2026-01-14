use chrono::NaiveDate;
use rstructor::{Instructor, RStructorError, SchemaType};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use crate::error::Result as FHResult;
use crate::utils::parse_period_string;
use crate::{process_financial_history, verify_accounting_equation};

#[derive(Debug, Clone, Serialize, Deserialize, Instructor)]
pub struct SourceMetadata {
    #[llm(
        description = "The Document ID from the manifest (e.g., \"0\", \"1\", \"2\"). Use ONLY the numeric ID, not the filename."
    )]
    #[serde(rename = "document")]
    pub document_name: String,

    #[llm(
        description = "Optional context about where this value was found. This is serialized as `text`. ONLY required if: (1) the source row/line label differs from the account name, OR (2) the value was extracted from narrative text rather than a labeled table row. If the account name exactly matches the row label in a financial table, you may omit this field."
    )]
    #[serde(rename = "text")]
    pub original_text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Instructor)]
#[serde(rename_all = "PascalCase")]
pub enum AccountType {
    #[llm(
        description = "Revenue from sales of goods or services (Income Statement, credit balance)"
    )]
    Revenue,

    #[llm(
        description = "Direct costs attributable to production of goods sold (Income Statement, debit balance)"
    )]
    CostOfSales,

    #[llm(
        description = "Operating expenses like salaries, rent, marketing, utilities (Income Statement, debit balance)"
    )]
    OperatingExpense,

    #[llm(
        description = "Non-operating income such as interest income, investment gains (Income Statement, credit balance)"
    )]
    OtherIncome,

    #[llm(description = "Interest expense (finance costs) (Income Statement, debit balance)")]
    Interest,

    #[llm(
        description = "Depreciation and Amortisation expense (Income Statement, debit balance)"
    )]
    Depreciation,

    #[llm(
        description = "Shareholder or Director salaries (distinct from standard wages) (Income Statement, debit balance)"
    )]
    ShareholderSalaries,

    #[llm(
        description = "Income Tax Expense (Corporate Tax) (Income Statement, debit balance)"
    )]
    IncomeTax,

    #[llm(
        description = "Resources owned by the company: cash, accounts receivable, inventory, equipment (Balance Sheet, debit balance)"
    )]
    Asset,

    #[llm(
        description = "Obligations owed to creditors: accounts payable, loans, accrued expenses (Balance Sheet, credit balance)"
    )]
    Liability,

    #[llm(
        description = "Owner's residual interest: share capital, retained earnings (Balance Sheet, credit balance)"
    )]
    Equity,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Instructor)]
#[serde(rename_all = "PascalCase")]
pub enum SeasonalityProfileId {
    #[llm(
        description = "Evenly distributed across all 12 months (8.33% per month). Use when there's no known seasonality."
    )]
    Flat,

    #[llm(
        description = "Retail pattern: Low Jan-Nov (~6% per month), massive spike in December (~40%). Think Black Friday/Christmas sales."
    )]
    RetailPeak,

    #[llm(
        description = "Summer tourism pattern: Low in Q1 (5% each), high in Q2/Q3 (12% each), moderate Q4 (7% each). For hospitality, travel, outdoor recreation."
    )]
    SummerHigh,

    #[llm(
        description = "SaaS growth pattern: Back-loaded within the fiscal year, simulating gradual customer acquisition. Starts at 6% in month 1, ramps to 10% by month 12."
    )]
    SaasGrowth,

    #[llm(
        description = "Custom 12-value array representing the percentage weight for each month (must sum to 1.0). Month 1 is the first month after the fiscal year end."
    )]
    Custom(
        #[llm(
            description = "Array of 12 decimal values representing monthly weights (must sum to 1.0)"
        )]
        Vec<f64>,
    ),
}

#[derive(Debug, Clone, Serialize, Deserialize, Instructor)]
pub struct BalanceSheetSnapshot {
    #[llm(description = "The date of the snapshot (e.g., 2023-12-31). Use month-end dates.")]
    pub date: NaiveDate,

    #[llm(
        description = "The value of the account on this specific date (point-in-time balance)"
    )]
    pub value: f64,

    #[serde(default)]
    #[llm(description = "Metadata to trace this value back to the source document.")]
    pub source: Option<SourceMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Instructor)]
#[serde(rename_all = "PascalCase")]
pub enum InterpolationMethod {
    #[llm(
        description = "Draw straight lines between snapshots. Good for accounts that change steadily."
    )]
    Linear,

    #[llm(
        description = "Hold value until it changes. Ideal for accounts that remain constant between snapshots."
    )]
    Step,

    #[llm(
        description = "Smooth curve (Catmull-Rom) between snapshots. Best for organic changes in balance sheet accounts."
    )]
    Curve,
}

#[derive(Debug, Clone, Serialize, Deserialize, Instructor)]
pub struct BalanceSheetAccount {
    #[llm(
        description = "The specific account name. IMPORTANT: Extract LEAF nodes only. DO NOT extract subtotal lines like 'Total Assets', 'Total Liabilities', 'Current Assets', or 'Fixed Assets'. Only extract the specific items listed under them (e.g., 'Cash at Bank', 'Accounts Receivable')."
    )]
    pub name: String,

    #[serde(default)]
    #[llm(
        description = "The specific subcategory header this account appears under in the report (e.g., 'Current Assets', 'Non-Current Liabilities', 'Fixed Assets')."
    )]
    pub category: Option<String>,

    #[llm(description = "The type of account (Asset, Liability, or Equity)")]
    pub account_type: AccountType,

    #[llm(description = "How to interpolate values between snapshots")]
    pub method: InterpolationMethod,

    #[llm(
        description = "Array of known balance sheet snapshots. Must have at least one snapshot. These are point-in-time balances, not cumulative totals."
    )]
    pub snapshots: Vec<BalanceSheetSnapshot>,

    #[serde(default)]
    #[llm(
        description = "If true, this account will be used as the balancing account to enforce the accounting equation (Assets = Liabilities + Equity). Typically set for Cash or Retained Earnings. Only ONE account should have this flag set to true."
    )]
    pub is_balancing_account: bool,

    #[serde(default)]
    #[llm(
        description = "Optional variance to add realistic noise. Range: 0.0 (no noise) to 0.1 (10% random variation). Defaults to 0.0. Use 0.0 for fixed items. Use 0.01-0.02 for stable balance sheet accounts."
    )]
    #[serde(rename = "noise")]
    pub noise_factor: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Instructor)]
pub struct PeriodConstraint {
    #[llm(description = "Time period string. \
        For a SINGLE month, use 'YYYY-MM' (e.g. '2023-03'). \
        For a RANGE, use 'YYYY-MM:YYYY-MM' (e.g. '2023-01:2023-12'). \
        IMPORTANT: Ranges are INCLUSIVE. '2023-03:2023-04' means the sum of March AND April. \
        DO NOT use a range for a single month.")]
    pub period: String,

    #[llm(
        description = "Total value generated during this specific period. If the document lists 'Gross Profit' or 'Net Income', DO NOT extract them. Only extract Revenue and specific Expense categories. You can provide overlapping periods (e.g., a month total AND a quarter total AND a year total). The engine will solve them hierarchically."
    )]
    pub value: f64,

    #[serde(default)]
    #[llm(description = "Metadata to trace this value back to the source document.")]
    pub source: Option<SourceMetadata>,
}

impl PeriodConstraint {
    /// Helper to resolve the string period into actual NaiveDates
    pub fn resolve_dates(&self) -> FHResult<(NaiveDate, NaiveDate)> {
        parse_period_string(&self.period)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Instructor)]
pub struct IncomeStatementAccount {
    #[llm(
        description = "The account name (e.g., 'Revenue', 'Salaries'). DO NOT extract 'Total Operating Expenses', 'Gross Profit', 'Net Income', or 'EBITDA'. Extraction should be granular - extract individual revenue and expense line items only."
    )]
    pub name: String,

    #[llm(
        description = "The type of account (Revenue, CostOfSales, OperatingExpense, or OtherIncome)"
    )]
    pub account_type: AccountType,

    #[llm(
        description = "Defines the shape of the data when filling in gaps between constraints. This determines how the engine distributes values across months."
    )]
    #[serde(rename = "seasonality")]
    pub seasonality_profile: SeasonalityProfileId,

    #[llm(
        description = "List of known totals for specific periods (Months, Quarters, or Years). You can and should provide overlapping periods - the engine will solve them hierarchically. For example, provide both a monthly total AND a quarterly total AND a yearly total if available."
    )]
    pub constraints: Vec<PeriodConstraint>,

    #[serde(default)]
    #[llm(
        description = "Optional variance to add realistic noise. Range: 0.0 (no noise) to 0.1 (10% random variation). Defaults to 0.0. Use 0.0 for fixed costs. Use 0.03-0.05 for normal revenues/expenses."
    )]
    #[serde(rename = "noise")]
    pub noise_factor: f64,
}

// --- Intermediate Schemas for Multi-Step Extraction ---

#[derive(Debug, Clone, Serialize, Deserialize, Instructor)]
pub struct DiscoveryResponse {
    #[llm(description = "The legal name of the organization")]
    pub organization_name: String,

    #[llm(description = "The month when the fiscal year ends (1-12)")]
    pub fiscal_year_end_month: u32,

    #[llm(
        description = "The logical start date for the financial history (YYYY-MM-DD). Pick the start of the earliest fiscal year present in the columns (e.g., if 2022 and 2023 columns exist, use 2022-01-01)."
    )]
    pub forecast_start_date: Option<NaiveDate>,

    #[llm(
        description = "The logical end date for the financial history (YYYY-MM-DD). Usually the date of the latest balance sheet."
    )]
    pub forecast_end_date: Option<NaiveDate>,

    #[llm(
        description = "List of ALL unique Balance Sheet account names found. Leaf nodes only."
    )]
    pub balance_sheet_account_names: Vec<String>,

    #[llm(
        description = "List of ALL unique Income Statement account names found. Leaf nodes only."
    )]
    pub income_statement_account_names: Vec<String>,
}

impl DiscoveryResponse {
    pub fn get_schema() -> serde_json::Result<serde_json::Value> {
        Ok(Self::schema().to_json())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Instructor)]
pub struct BalanceSheetExtractionResponse {
    pub balance_sheet: Vec<BalanceSheetAccount>,
}

impl BalanceSheetExtractionResponse {
    pub fn get_schema() -> serde_json::Result<serde_json::Value> {
        Ok(Self::schema().to_json())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Instructor)]
pub struct IncomeStatementExtractionResponse {
    pub income_statement: Vec<IncomeStatementAccount>,
}

impl IncomeStatementExtractionResponse {
    pub fn get_schema() -> serde_json::Result<serde_json::Value> {
        Ok(Self::schema().to_json())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Instructor)]
#[llm(validate = "validate_financial_history_config")]
pub struct FinancialHistoryConfig {
    #[llm(description = "The legal name of the organization/business")]
    pub organization_name: String,

    #[llm(
        description = "The month when the fiscal year ends (1 = January, 12 = December). For calendar year companies, use 12. For July-June fiscal year, use 6."
    )]
    pub fiscal_year_end_month: u32,

    #[llm(
        description = "Array of Balance Sheet accounts (Assets, Liabilities, Equity) with their snapshots"
    )]
    pub balance_sheet: Vec<BalanceSheetAccount>,

    #[llm(
        description = "Array of Income Statement accounts (Revenue, Expenses) with their period constraints"
    )]
    pub income_statement: Vec<IncomeStatementAccount>,
}

impl FinancialHistoryConfig {
    pub fn get_gemini_response_schema() -> serde_json::Result<serde_json::Value> {
        Ok(Self::schema().to_json())
    }

    pub fn schema_as_json() -> serde_json::Result<String> {
        serde_json::to_string_pretty(&Self::schema().to_json())
    }

    pub fn schema_as_json_value() -> serde_json::Result<serde_json::Value> {
        Ok(Self::schema().to_json())
    }
}

fn validate_financial_history_config(cfg: &FinancialHistoryConfig) -> rstructor::Result<()> {
    for acc in &cfg.balance_sheet {
        for (i, snap) in acc.snapshots.iter().enumerate() {
            if snap.source.is_none() {
                return Err(RStructorError::ValidationError(format!(
                    "Balance Sheet '{}' snapshot #{} missing `source`.",
                    acc.name, i
                )));
            }
        }
    }
    for acc in &cfg.income_statement {
        for (i, cons) in acc.constraints.iter().enumerate() {
            if cons.source.is_none() {
                return Err(RStructorError::ValidationError(format!(
                    "Income Statement '{}' constraint #{} missing `source`.",
                    acc.name, i
                )));
            }
        }
    }

    let mut seen_bs = HashSet::new();
    for acc in &cfg.balance_sheet {
        if !seen_bs.insert(&acc.name) {
            return Err(RStructorError::ValidationError(format!(
                "Duplicate Balance Sheet account detected: '{}'. Account names must be unique.",
                acc.name
            )));
        }
    }

    let mut seen_is = HashSet::new();
    for acc in &cfg.income_statement {
        if !seen_is.insert(&acc.name) {
            return Err(RStructorError::ValidationError(format!(
                "Duplicate Income Statement account detected: '{}'. Account names must be unique.",
                acc.name
            )));
        }
    }

    match process_financial_history(cfg) {
        Ok(dense) => match verify_accounting_equation(cfg, &dense, 1.0) {
            Ok(_) => Ok(()),
            Err(e) => Err(RStructorError::ValidationError(format!(
                "Accounting Equation Violation: {}",
                e
            ))),
        },
        Err(e) => Err(RStructorError::ValidationError(format!(
            "Processing Engine Error: {}",
            e
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_generation() {
        let schema_json = FinancialHistoryConfig::schema_as_json().unwrap();
        assert!(schema_json.contains("organization_name"));
        assert!(schema_json.contains("fiscal_year_end_month"));
        assert!(schema_json.contains("balance_sheet"));
        assert!(schema_json.contains("income_statement"));
        println!("Generated schema:\n{}", schema_json);
    }

    #[test]
    fn test_serialization() {
        let config = FinancialHistoryConfig {
            organization_name: "Test Corp".to_string(),
            fiscal_year_end_month: 12,
            balance_sheet: vec![BalanceSheetAccount {
                name: "Cash".to_string(),
                category: None,
                account_type: AccountType::Asset,
                method: InterpolationMethod::Linear,
                snapshots: vec![
                    BalanceSheetSnapshot {
                        date: NaiveDate::from_ymd_opt(2023, 1, 31).unwrap(),
                        value: 50000.0,
                        source: None,
                    },
                    BalanceSheetSnapshot {
                        date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                        value: 75000.0,
                        source: None,
                    },
                ],
                is_balancing_account: true,
                noise_factor: 0.02,
            }],
            income_statement: vec![IncomeStatementAccount {
                name: "Revenue".to_string(),
                account_type: AccountType::Revenue,
                seasonality_profile: SeasonalityProfileId::Flat,
                constraints: vec![PeriodConstraint {
                    period: "2023-01:2023-12".to_string(),
                    value: 1200000.0,
                    source: None,
                }],
                noise_factor: 0.05,
            }],
        };

        let json = serde_json::to_string_pretty(&config).unwrap();
        assert!(json.contains("Test Corp"));

        let deserialized: FinancialHistoryConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.organization_name, "Test Corp");
    }
}
