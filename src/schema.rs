use chrono::NaiveDate;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub enum AccountType {
    #[schemars(description = "Revenue from sales of goods or services (Income Statement, credit balance)")]
    Revenue,

    #[schemars(description = "Direct costs attributable to production of goods sold (Income Statement, debit balance)")]
    CostOfSales,

    #[schemars(
        description = "Operating expenses like salaries, rent, marketing, utilities (Income Statement, debit balance)"
    )]
    OperatingExpense,

    #[schemars(
        description = "Non-operating income such as interest income, investment gains (Income Statement, credit balance)"
    )]
    OtherIncome,

    #[schemars(
        description = "Resources owned by the company: cash, accounts receivable, inventory, equipment (Balance Sheet, debit balance)"
    )]
    Asset,

    #[schemars(
        description = "Obligations owed to creditors: accounts payable, loans, accrued expenses (Balance Sheet, credit balance)"
    )]
    Liability,

    #[schemars(
        description = "Owner's residual interest: share capital, retained earnings (Balance Sheet, credit balance)"
    )]
    Equity,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub enum AccountBehavior {
    #[schemars(
        description = "Flow accounts represent activity over a period (e.g., Revenue, Expenses). The anchor value is the TOTAL for the period, which will be distributed across months."
    )]
    Flow,

    #[schemars(
        description = "Stock accounts represent a snapshot at a point in time (e.g., Cash, Accounts Receivable). The anchor value is the balance ON that specific date."
    )]
    Stock,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
#[serde(rename_all = "PascalCase", tag = "method")]
pub enum InterpolationMethod {
    #[schemars(
        description = "Linear interpolation between anchor points. Good for steady, predictable growth in revenues or gradual changes in balance sheet accounts."
    )]
    Linear,

    #[schemars(
        description = "Step function - value remains constant until the next anchor. Ideal for fixed costs like rent, insurance premiums, or subscription fees that don't change month-to-month."
    )]
    Step,

    #[schemars(
        description = "Smooth Catmull-Rom curve interpolation. Best for balance sheet accounts that change organically over time (e.g., gradual inventory buildup, smoothly growing accounts receivable)."
    )]
    Curve,

    #[schemars(
        description = "Distributes the annual total according to a predefined seasonality pattern. Use for revenues/expenses with known cyclical patterns (retail, tourism, SaaS)."
    )]
    Seasonal {
        #[schemars(description = "The seasonality profile to apply")]
        profile_id: SeasonalityProfileId,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub enum SeasonalityProfileId {
    #[schemars(
        description = "Evenly distributed across all 12 months (8.33% per month). Use when there's no known seasonality."
    )]
    Flat,

    #[schemars(
        description = "Retail pattern: Low Jan-Nov (~6% per month), massive spike in December (~40%). Think Black Friday/Christmas sales."
    )]
    RetailPeak,

    #[schemars(
        description = "Summer tourism pattern: Low in Q1 (5% each), high in Q2/Q3 (12% each), moderate Q4 (7% each). For hospitality, travel, outdoor recreation."
    )]
    SummerHigh,

    #[schemars(
        description = "SaaS growth pattern: Back-loaded within the fiscal year, simulating gradual customer acquisition. Starts at 6% in month 1, ramps to 10% by month 12."
    )]
    SaasGrowth,

    #[schemars(
        description = "Custom 12-value array representing the percentage weight for each month (must sum to 1.0). Month 1 is the first month after the fiscal year end."
    )]
    Custom(
        #[schemars(
            description = "Array of 12 decimal values representing monthly weights (must sum to 1.0)"
        )]
        Vec<f64>,
    ),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub enum AnchorType {
    #[schemars(description = "The value represents the cumulative total from the start of the fiscal year up to this date (Default for Flow accounts).")]
    Cumulative,

    #[schemars(description = "The value represents the specific amount generated ONLY in the period since the previous anchor point.")]
    Period,
}

impl Default for AnchorType {
    fn default() -> Self {
        Self::Cumulative
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AnchorPoint {
    #[schemars(
        description = "Date in YYYY-MM-DD format. For Flow accounts, this is the END of the period (e.g., 2023-12-31 for FY2023). For Stock accounts, this is the snapshot date."
    )]
    pub date: NaiveDate,

    #[schemars(
        description = "The monetary value at this anchor point. For Flow accounts with Cumulative type, this is the YTD total. For Period type, this is the specific period amount. For Stock accounts, this is the balance on the date. Can be negative for certain account types."
    )]
    pub value: f64,

    #[serde(default)]
    #[schemars(description = "Defines if the value is a Cumulative YTD total or a specific Period amount. Only applies to Flow accounts (P&L). Stock accounts always use point-in-time balances. Defaults to Cumulative.")]
    pub anchor_type: AnchorType,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SparseAccount {
    #[schemars(
        description = "The account name as it appears in financial statements (e.g., 'Sales Revenue', 'Office Rent', 'Cash at Bank')"
    )]
    pub name: String,

    #[schemars(description = "Classification of the account for financial statement presentation")]
    pub account_type: AccountType,

    #[schemars(description = "Whether this account is a flow (period activity) or stock (point-in-time balance)")]
    pub behavior: AccountBehavior,

    #[schemars(description = "How to interpolate values between anchor points")]
    pub interpolation: InterpolationMethod,

    #[schemars(
        description = "Optional variance to add realistic noise. Range: 0.0 (no noise) to 0.1 (10% random variation). Use 0.0 for fixed costs like rent. Use 0.03-0.05 for variable revenues/expenses. Use 0.01-0.02 for balance sheet accounts."
    )]
    pub noise_factor: Option<f64>,

    #[schemars(
        description = "Array of known data points. Must have at least one anchor. For Flow accounts, anchors typically represent fiscal year ends. For Stock accounts, they represent balance sheet dates."
    )]
    pub anchors: Vec<AnchorPoint>,

    #[serde(default)]
    #[schemars(
        description = "If true, this account will be used as the balancing/plug account to enforce the accounting equation (Assets = Liabilities + Equity). Typically set for Cash or a Retained Earnings account. Only ONE account should have this flag set to true. If no account is marked, a 'Balancing Equity Adjustment' account will be created automatically."
    )]
    pub is_balancing_account: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SparseFinancialHistory {
    #[schemars(description = "The legal name of the organization/business")]
    pub organization_name: String,

    #[schemars(
        description = "The month when the fiscal year ends (1 = January, 12 = December). For calendar year companies, use 12. For July-June fiscal year, use 6."
    )]
    pub fiscal_year_end_month: u32,

    #[schemars(
        description = "Array of all financial accounts with their sparse data points. Should include both Income Statement (flow) and Balance Sheet (stock) accounts."
    )]
    pub accounts: Vec<SparseAccount>,
}

impl SparseFinancialHistory {
    pub fn generate_json_schema() -> schemars::schema::RootSchema {
        schemars::schema_for!(SparseFinancialHistory)
    }

    pub fn schema_as_json() -> Result<String, serde_json::Error> {
        let schema = Self::generate_json_schema();
        serde_json::to_string_pretty(&schema)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_generation() {
        let schema_json = SparseFinancialHistory::schema_as_json().unwrap();
        assert!(schema_json.contains("organization_name"));
        assert!(schema_json.contains("fiscal_year_end_month"));
        assert!(schema_json.contains("accounts"));
        println!("Generated schema:\n{}", schema_json);
    }

    #[test]
    fn test_serialization() {
        let history = SparseFinancialHistory {
            organization_name: "Test Corp".to_string(),
            fiscal_year_end_month: 12,
            accounts: vec![SparseAccount {
                name: "Revenue".to_string(),
                account_type: AccountType::Revenue,
                behavior: AccountBehavior::Flow,
                interpolation: InterpolationMethod::Linear,
                noise_factor: Some(0.05),
                anchors: vec![AnchorPoint {
                    date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                    value: 100000.0,
                    anchor_type: AnchorType::Cumulative,
                }],
                    is_balancing_account: false,
                }],
        };

        let json = serde_json::to_string_pretty(&history).unwrap();
        assert!(json.contains("Test Corp"));

        let deserialized: SparseFinancialHistory = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.organization_name, "Test Corp");
    }
}
