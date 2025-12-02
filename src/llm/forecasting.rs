use crate::error::{FinancialHistoryError, Result};
use crate::llm::{extract_first_json_object, Content, GeminiClient, RemoteDocument};
use crate::overrides::{AccountModification, FinancialHistoryOverrides};
use crate::schema::FinancialHistoryConfig;
use schemars::schema_for;
use serde_json::Value;
use std::time::Duration;
use tokio::time::sleep;

// Gold-standard example to nudge the model toward aggressive aggregation and NZ/AU structure.
const FORECASTING_EXAMPLE: &str = r#"
### EXAMPLE SCENARIO
**Raw Input:** A balance sheet containing 45 individual asset lines (e.g., "Playground Mat", "Dishwasher", "Samsung TV") and a P&L with "Ministry of Education - 20 Hours" and "Ministry of Education - Subsidy".
**Goal:** A clean 3-way forecast structure.

**Correct "Thought Process":**
1. *Fixed Assets:* A forecast doesn't need to depreciate a specific "Dishwasher". It needs a "Plant & Equipment" pool. I will merge all 45 lines into 3 categories: "Fixed Assets - Plant", "Fixed Assets - Furniture", and "Fixed Assets - Improvements".
2. *Liabilities:* The raw data missed "GST Payable" and "Shareholder Current Account". I must create these.
3. *Naming:* "Ministry of Education - 20 Hours..." is too long. Rename to "Government Funding".

**Correct Output (JSON):**
{
  "new_balance_sheet_accounts": [
    {
      "name": "GST Payable",
      "category": "Current Liabilities",
      "account_type": "Liability",
      "method": "Linear",
      "snapshots": [{ "date": "2024-03-31", "value": 5000.0, "source": null }],
      "is_balancing_account": false
    },
    {
      "name": "Shareholder Current Account",
      "category": "Current Liabilities",
      "account_type": "Liability",
      "method": "Linear",
      "snapshots": [{ "date": "2024-03-31", "value": 0.0, "source": null }],
      "is_balancing_account": false
    }
  ],
  "modifications": [
    {
      "action": "merge",
      "target_name": "Fixed Assets - Plant & Equipment",
      "sources": ["Dishwasher", "Playground Mat", "Samsung TV", "Cots", "Microwave"]
    },
    {
      "action": "merge",
      "target_name": "Government Funding",
      "sources": ["Ministry of Education - 20 Hours", "Ministry of Education - Subsidy"]
    },
    {
      "action": "rename",
      "target": "Wages & Temporary Staff Expenses",
      "new_name": "Wages"
    }
  ]
}
"#;

pub struct ForecastingSetupAgent {
    client: GeminiClient,
    model: String,
}

impl ForecastingSetupAgent {
    pub fn new(client: GeminiClient, model: impl Into<String>) -> Self {
        Self {
            client,
            model: model.into(),
        }
    }

    /// Generates overrides to prepare the extracted history for forecasting.
    ///
    /// # Arguments
    /// * `current_config` - The raw extraction result.
    /// * `documents` - The source documents (for context on missing items).
    /// * `user_instruction` - Optional specific requests (e.g., "Merge cleaning and rubbish").
    pub async fn generate_overrides(
        &self,
        current_config: &FinancialHistoryConfig,
        documents: &[RemoteDocument],
        user_instruction: Option<&str>,
    ) -> Result<FinancialHistoryOverrides> {
        // Schema and current state for the model.
        let schema_json = schema_for!(FinancialHistoryOverrides);
        let current_state = serde_json::to_string_pretty(current_config)?;

        let system_prompt = format!(
            r#"
You are a Senior Financial Modeler preparing data for a 3-way forecast (P&L, Balance Sheet, Cashflow).
Your goal is to transform "Raw Extracted Data" into "Forecast-Ready Data".

## THE "FORECAST-READY" MENTALITY
Raw data is granular and messy. Forecast data is aggregated and structural.
You must apply the following **Thought Process** to every account:

### 1. The "Fixed Asset Explosion" Rule (HIGHEST PRIORITY)
**Problem:** Raw extracts often contain dozens of small assets (e.g., "Sandpit Cover", "Ipad", "Chair").
**Solution:** You MUST generate `Merge` modifications to collapse these into 3-4 high-level pools.
**Target Accounts:**
- `Fixed Assets - Plant & Equipment` (Tools, machinery, kitchen appliances, playground items)
- `Fixed Assets - Furniture & Fittings` (Chairs, tables, carpets, blinds)
- `Fixed Assets - Office & Computer` (Laptops, websites, phones)
- `Fixed Assets - Motor Vehicles`
- `Leasehold Improvements`
*Never leave individual small assets in the Balance Sheet.*

### 2. The "Invisible Accounts" Rule (NZ/AU Context)
Forecasting requires accounts that may not appear in a simple P&L extract but are logically necessary.
**You must inject these into `new_balance_sheet_accounts`:**
- **GST Payable:** (Liability) Almost every business has this. Create it if missing.
- **Accounts Receivable:** (Asset) If there is Revenue, there is AR.
- **Accounts Payable:** (Liability) If there are Expenses, there is AP.
- **Shareholder Current Account:** (Equity/Liability) If "Shareholder Salaries" appear in P&L, this account MUST exist in BS to offset the entry.
- **Income Tax Payable:** (Liability) distinct from GST.

### 3. The "Naming Convention" Rule
Rename accounts to be short, professional, and clear.
- "Ministry of Education - 20 Hours ECE Funding" -> `Government Funding` or `MOE Funding`
- "Wages & Temporary Staff Expenses" -> `Wages`
- "Light, Power & Heating" -> `Utilities` (or keep original if it matches the user's preference)
- "Telephone & Internet" -> `Comms` or `Telephone & Internet`

### 4. Debt Structure
If you see "Interest" in P&L but no Debt in BS:
- Create `Business Loan` (Liability).
- If the user instruction mentions specific terms (e.g., "30 year loan"), create those specific accounts.

{}

## YOUR OUTPUT
Return a valid JSON object matching the `FinancialHistoryOverrides` schema.
"#,
            FORECASTING_EXAMPLE
        );

        let user_prompt = format!(
            "## CURRENT EXTRACTED DATA\n```json\n{}\n```\n\n\
             ## USER INSTRUCTION\n{}\n\n\
             ## TASK\n\
             1. Review the `balance_sheet` in the raw data. Identify all individual fixed assets and MERGE them into summary accounts.\n\
             2. Check for missing structural accounts (GST, AR, AP, Shareholder Current Account) and ADD them.\n\
             3. Rename P&L lines to be cleaner.",
            current_state,
            user_instruction.unwrap_or("Clean up fixed assets and ensure all standard trading accounts exist.")
        );

        let mut last_error: Option<String> = None;
        let max_retries = 5;
        let schema_json_value = serde_json::to_value(schema_json)?;

        for attempt in 1..=max_retries {
            let mut prompt_with_context = user_prompt.clone();
            if let Some(err) = &last_error {
                prompt_with_context.push_str(&format!(
                    "\n\n## PREVIOUS ATTEMPT ISSUE\n{}\n\
                     Please return STRICT JSON matching the `FinancialHistoryOverrides` schema, \
                     ensuring every modification includes an `action` field.",
                    err
                ));
            }

            let messages = vec![Content::user_with_files(prompt_with_context, documents)];

            match self
                .client
                .generate_content(
                    &self.model,
                    &system_prompt,
                    messages,
                    Some(schema_json_value.clone()),
                    "application/json",
                    "Forecasting_Setup",
                )
                .await
            {
                Ok(response) => {
                    let cleaned_json = extract_first_json_object(&response);
                    match serde_json::from_str::<FinancialHistoryOverrides>(&cleaned_json) {
                        Ok(overrides) => return Ok(overrides),
                        Err(e) => {
                            eprintln!(
                                "⚠️ Forecasting overrides attempt {} failed to parse: {}",
                                attempt, e
                            );
                            if attempt == max_retries {
                                // Last-ditch salvage: try to coerce partial JSON into overrides
                                if let Ok(value) = serde_json::from_str::<Value>(&cleaned_json) {
                                    if let Some(overrides) = salvage_overrides_from_value(&value) {
                                        eprintln!(
                                            "⚠️ Salvaged forecasting overrides from partial JSON after parse failure."
                                        );
                                        return Ok(overrides);
                                    }
                                }
                                return Err(FinancialHistoryError::SerializationError(e));
                            }
                            last_error = Some(format!("Parsing failed: {}", e));
                        }
                    }
                }
                Err(e) => {
                    eprintln!("⚠️ Forecasting overrides attempt {} failed: {}", attempt, e);
                    if attempt == max_retries {
                        return Err(e);
                    }
                    last_error = Some(e.to_string());
                }
            }

            // Exponential-ish backoff between retries
            sleep(Duration::from_secs(2 * attempt as u64)).await;
        }

        Err(FinancialHistoryError::ExtractionFailed(
            "Forecasting overrides generation exhausted retries".to_string(),
        ))
    }
}

fn salvage_overrides_from_value(value: &Value) -> Option<FinancialHistoryOverrides> {
    let mut overrides = FinancialHistoryOverrides::default();

    if let Some(bs_val) = value.get("new_balance_sheet_accounts") {
        if let Ok(bs) = serde_json::from_value(bs_val.clone()) {
            overrides.new_balance_sheet_accounts = bs;
        }
    }

    if let Some(is_val) = value.get("new_income_statement_accounts") {
        if let Ok(is_accs) = serde_json::from_value(is_val.clone()) {
            overrides.new_income_statement_accounts = is_accs;
        }
    }

    if let Some(mods_val) = value.get("modifications").and_then(|v| v.as_array()) {
        for mv in mods_val {
            if let Some(modification) = coerce_modification(mv) {
                overrides.modifications.push(modification);
            }
        }
    }

    Some(overrides)
}

fn coerce_modification(value: &Value) -> Option<AccountModification> {
    let obj = value.as_object()?;
    let action = obj
        .get("action")
        .and_then(|v| v.as_str())
        .map(|s| s.to_lowercase())?;

    match action.as_str() {
        "rename" => {
            let target = obj.get("target")?.as_str()?.to_string();
            let new_name = obj.get("new_name")?.as_str()?.to_string();
            Some(AccountModification::Rename { target, new_name })
        }
        "merge" => {
            let sources: Vec<String> = obj
                .get("sources")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|s| s.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();
            let target_name = obj.get("target_name")?.as_str()?.to_string();
            if sources.is_empty() {
                None
            } else {
                Some(AccountModification::Merge {
                    sources,
                    target_name,
                })
            }
        }
        "update_metadata" | "modify" => {
            let target = obj.get("target")?.as_str()?.to_string();
            let new_category = obj
                .get("new_category")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let new_type = obj
                .get("new_type")
                .and_then(|v| serde_json::from_value(v.clone()).ok());
            Some(AccountModification::UpdateMetadata {
                target,
                new_category,
                new_type,
            })
        }
        "delete" => {
            let target = obj.get("target")?.as_str()?.to_string();
            Some(AccountModification::Delete { target })
        }
        "scale_values" | "scale" => {
            let target = obj.get("target")?.as_str()?.to_string();
            let factor = obj.get("factor")?.as_f64()?;
            Some(AccountModification::ScaleValues { target, factor })
        }
        "set_value" | "set" => {
            let target = obj.get("target")?.as_str()?.to_string();
            let date_or_period = obj.get("date_or_period")?.as_str()?.to_string();
            let value = obj.get("value")?.as_f64()?;
            Some(AccountModification::SetValue {
                target,
                date_or_period,
                value,
            })
        }
        _ => None,
    }
}
