use chrono::NaiveDate;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::error::Result as FHResult;
use crate::utils::parse_period_string;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SourceMetadata {
    #[schemars(
        description = "The Document ID from the manifest (e.g., \"0\", \"1\", \"2\"). Use ONLY the numeric ID, not the filename."
    )]
    #[serde(rename = "document")]
    pub document_name: String,

    #[schemars(
        description = "Optional context about where this value was found. This is serialized as `text`. ONLY required if: (1) the source row/line label differs from the account name, OR (2) the value was extracted from narrative text rather than a labeled table row. If the account name exactly matches the row label in a financial table, you may omit this field."
    )]
    #[serde(rename = "text")]
    pub original_text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub enum AccountType {
    #[schemars(
        description = "Revenue from sales of goods or services (Income Statement, credit balance)"
    )]
    Revenue,

    #[schemars(
        description = "Direct costs attributable to production of goods sold (Income Statement, debit balance)"
    )]
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

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct BalanceSheetSnapshot {
    #[schemars(description = "The date of the snapshot (e.g., 2023-12-31). Use month-end dates.")]
    pub date: NaiveDate,

    #[schemars(
        description = "The value of the account on this specific date (point-in-time balance)"
    )]
    pub value: f64,

    #[serde(default)]
    #[schemars(description = "Metadata to trace this value back to the source document.")]
    pub source: Option<SourceMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub enum InterpolationMethod {
    #[schemars(
        description = "Draw straight lines between snapshots. Good for accounts that change steadily."
    )]
    Linear,

    #[schemars(
        description = "Hold value until it changes. Ideal for accounts that remain constant between snapshots."
    )]
    Step,

    #[schemars(
        description = "Smooth curve (Catmull-Rom) between snapshots. Best for organic changes in balance sheet accounts."
    )]
    Curve,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct BalanceSheetAccount {
    #[schemars(
        description = "The specific account name. IMPORTANT: Extract LEAF nodes only. DO NOT extract subtotal lines like 'Total Assets', 'Total Liabilities', 'Current Assets', or 'Fixed Assets'. Only extract the specific items listed under them (e.g., 'Cash at Bank', 'Accounts Receivable')."
    )]
    pub name: String,

    #[schemars(description = "The type of account (Asset, Liability, or Equity)")]
    pub account_type: AccountType,

    #[schemars(description = "How to interpolate values between snapshots")]
    pub method: InterpolationMethod,

    #[schemars(
        description = "Array of known balance sheet snapshots. Must have at least one snapshot. These are point-in-time balances, not cumulative totals."
    )]
    pub snapshots: Vec<BalanceSheetSnapshot>,

    #[serde(default)]
    #[schemars(
        description = "If true, this account will be used as the balancing account to enforce the accounting equation (Assets = Liabilities + Equity). Typically set for Cash or Retained Earnings. Only ONE account should have this flag set to true."
    )]
    pub is_balancing_account: bool,

    #[serde(default)]
    #[schemars(
        description = "Optional variance to add realistic noise. Range: 0.0 (no noise) to 0.1 (10% random variation). Defaults to 0.0. Use 0.0 for fixed items. Use 0.01-0.02 for stable balance sheet accounts."
    )]
    #[serde(rename = "noise")]
    pub noise_factor: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PeriodConstraint {
    #[schemars(
        description = "Time period string. \
        For a SINGLE month, use 'YYYY-MM' (e.g. '2023-03'). \
        For a RANGE, use 'YYYY-MM:YYYY-MM' (e.g. '2023-01:2023-12'). \
        IMPORTANT: Ranges are INCLUSIVE. '2023-03:2023-04' means the sum of March AND April. \
        DO NOT use a range for a single month."
    )]
    pub period: String,

    #[schemars(
        description = "Total value generated during this specific period. If the document lists 'Gross Profit' or 'Net Income', DO NOT extract them. Only extract Revenue and specific Expense categories. You can provide overlapping periods (e.g., a month total AND a quarter total AND a year total). The engine will solve them hierarchically."
    )]
    pub value: f64,

    #[serde(default)]
    #[schemars(description = "Metadata to trace this value back to the source document.")]
    pub source: Option<SourceMetadata>,
}

impl PeriodConstraint {
    /// Helper to resolve the string period into actual NaiveDates
    pub fn resolve_dates(&self) -> FHResult<(NaiveDate, NaiveDate)> {
        parse_period_string(&self.period)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct IncomeStatementAccount {
    #[schemars(
        description = "The account name (e.g., 'Revenue', 'Salaries'). DO NOT extract 'Total Operating Expenses', 'Gross Profit', 'Net Income', or 'EBITDA'. Extraction should be granular - extract individual revenue and expense line items only."
    )]
    pub name: String,

    #[schemars(
        description = "The type of account (Revenue, CostOfSales, OperatingExpense, or OtherIncome)"
    )]
    pub account_type: AccountType,

    #[schemars(
        description = "Defines the shape of the data when filling in gaps between constraints. This determines how the engine distributes values across months."
    )]
    #[serde(rename = "seasonality")]
    pub seasonality_profile: SeasonalityProfileId,

    #[schemars(
        description = "List of known totals for specific periods (Months, Quarters, or Years). You can and should provide overlapping periods - the engine will solve them hierarchically. For example, provide both a monthly total AND a quarterly total AND a yearly total if available."
    )]
    pub constraints: Vec<PeriodConstraint>,

    #[serde(default)]
    #[schemars(
        description = "Optional variance to add realistic noise. Range: 0.0 (no noise) to 0.1 (10% random variation). Defaults to 0.0. Use 0.0 for fixed costs. Use 0.03-0.05 for normal revenues/expenses."
    )]
    #[serde(rename = "noise")]
    pub noise_factor: f64,
}

// --- Intermediate Schemas for Multi-Step Extraction ---

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DiscoveryResponse {
    #[schemars(description = "The legal name of the organization")]
    pub organization_name: String,

    #[schemars(description = "The month when the fiscal year ends (1-12)")]
    pub fiscal_year_end_month: u32,

    #[schemars(
        description = "List of ALL unique Balance Sheet account names found. Leaf nodes only."
    )]
    pub balance_sheet_account_names: Vec<String>,

    #[schemars(
        description = "List of ALL unique Income Statement account names found. Leaf nodes only."
    )]
    pub income_statement_account_names: Vec<String>,
}

impl DiscoveryResponse {
    pub fn get_schema() -> serde_json::Result<serde_json::Value> {
        FinancialHistoryConfig::clean_schema(schemars::schema_for!(DiscoveryResponse))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct BalanceSheetExtractionResponse {
    pub balance_sheet: Vec<BalanceSheetAccount>,
}

impl BalanceSheetExtractionResponse {
    pub fn get_schema() -> serde_json::Result<serde_json::Value> {
        FinancialHistoryConfig::clean_schema(schemars::schema_for!(BalanceSheetExtractionResponse))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct IncomeStatementExtractionResponse {
    pub income_statement: Vec<IncomeStatementAccount>,
}

impl IncomeStatementExtractionResponse {
    pub fn get_schema() -> serde_json::Result<serde_json::Value> {
        FinancialHistoryConfig::clean_schema(schemars::schema_for!(
            IncomeStatementExtractionResponse
        ))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FinancialHistoryConfig {
    #[schemars(description = "The legal name of the organization/business")]
    pub organization_name: String,

    #[schemars(
        description = "The month when the fiscal year ends (1 = January, 12 = December). For calendar year companies, use 12. For July-June fiscal year, use 6."
    )]
    pub fiscal_year_end_month: u32,

    #[schemars(
        description = "Array of Balance Sheet accounts (Assets, Liabilities, Equity) with their snapshots"
    )]
    pub balance_sheet: Vec<BalanceSheetAccount>,

    #[schemars(
        description = "Array of Income Statement accounts (Revenue, Expenses) with their period constraints"
    )]
    pub income_statement: Vec<IncomeStatementAccount>,
}

impl FinancialHistoryConfig {
    pub fn generate_json_schema() -> schemars::schema::RootSchema {
        schemars::schema_for!(FinancialHistoryConfig)
    }

    /// Generates a Gemini-compatible schema.
    /// This performs deep cleaning to remove fields Gemini dislikes ($ref, type arrays, additionalProperties).
    pub fn get_gemini_response_schema() -> serde_json::Result<serde_json::Value> {
        Self::clean_schema(Self::generate_json_schema())
    }

    /// Shared schema cleaning logic that can be reused by other types
    pub fn clean_schema(
        root: schemars::schema::RootSchema,
    ) -> serde_json::Result<serde_json::Value> {
        let mut root_val = serde_json::to_value(root)?;

        // 1. Extract definitions map
        let definitions = root_val
            .get("definitions")
            .cloned()
            .unwrap_or(serde_json::json!({}));

        // 2. Recursively inline references AND clean up Gemini incompatibilities
        process_schema_node(&mut root_val, &definitions);

        // 3. Clean up root-level forbidden keys
        if let serde_json::Value::Object(ref mut map) = root_val {
            map.remove("$schema");
            map.remove("title");
            map.remove("definitions");
            map.remove("$defs");
        }

        Ok(root_val)
    }

    pub fn schema_as_json() -> serde_json::Result<String> {
        let schema = Self::generate_json_schema();
        serde_json::to_string_pretty(&schema)
    }

    pub fn schema_as_json_value() -> serde_json::Result<serde_json::Value> {
        let schema = Self::generate_json_schema();
        serde_json::to_value(schema)
    }
}

/// Main recursive processor
fn process_schema_node(node: &mut serde_json::Value, definitions: &serde_json::Value) {
    match node {
        serde_json::Value::Object(map) => {
            // A. Handle $ref inlining
            if let Some(serde_json::Value::String(ref_path)) = map.get("$ref") {
                let def_name = ref_path.split('/').next_back().unwrap_or_default();
                if let Some(def) = definitions.get(def_name) {
                    let mut inlined_def = def.clone();
                    // Recursive call to process the inlined definition
                    process_schema_node(&mut inlined_def, definitions);
                    *node = inlined_def;
                    return; // The node is replaced, stop processing this branch
                }
            }

            // B. Remove forbidden fields
            map.remove("additionalProperties");
            map.remove("title");
            map.remove("$id");
            map.remove("default"); // Gemini sometimes dislikes default values in schema

            // C. Fix Nullable Types (Option<T>)
            // Change {"type": ["string", "null"]} to {"type": "string", "nullable": true}
            if let Some(serde_json::Value::Array(types)) = map.get("type") {
                if types.len() == 2 && types.contains(&serde_json::json!("null")) {
                    // Find the actual type (e.g., "string" or "object")
                    if let Some(real_type) = types.iter().find(|t| *t != &serde_json::json!("null"))
                    {
                        let real_type_clone = real_type.clone();
                        map.insert("type".to_string(), real_type_clone);
                        map.insert("nullable".to_string(), serde_json::json!(true));
                    }
                } else if types.len() == 1 {
                    // Flatten single-element array {"type": ["object"]} -> {"type": "object"}
                    let single_type = types[0].clone();
                    map.insert("type".to_string(), single_type);
                }
            }

            // D. Recurse into children
            // We must iterate over keys that contain nested schemas
            let keys_to_recurse = vec![
                "properties",
                "items",
                "allOf",
                "anyOf",
                "oneOf",
                "not",
                "if",
                "then",
                "else",
            ];

            for key in keys_to_recurse {
                if let Some(child) = map.get_mut(key) {
                    process_schema_node(child, definitions);
                }
            }

            // Specifically handle properties map values
            if let Some(serde_json::Value::Object(props)) = map.get_mut("properties") {
                for (_, value) in props.iter_mut() {
                    process_schema_node(value, definitions);
                }
            }
        }
        serde_json::Value::Array(arr) => {
            // Recurse into arrays (used in allOf, anyOf, oneOf)
            for item in arr.iter_mut() {
                process_schema_node(item, definitions);
            }
        }
        _ => {}
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
