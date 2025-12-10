use crate::error::{FinancialHistoryError, Result};
use crate::llm::{extract_first_json_object, prompts, Content, GeminiClient, RemoteDocument};
use crate::overrides::{AccountModification, FinancialHistoryOverrides};
use crate::schema::FinancialHistoryConfig;
use log::{info, warn};
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
2. *Liabilities:* The raw data missed "GST Payable" and "Shareholder Current Account". I must create these. For GST Payable, I'll estimate based on the company's revenue/expense levels (approximately $5,000 seems reasonable for a business this size).
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

    /// Generates overrides using a 2-step process (Draft -> Review).
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
        // --- STEP 1: GENERATE DRAFT ---
        info!("Forecasting Agent: Step 1 - Generating Draft Overrides...");
        let draft_overrides = self
            .generate_draft_overrides(current_config, documents, user_instruction)
            .await?;

        // --- STEP 2: CFO REVIEW & REFINE ---
        info!("Forecasting Agent: Step 2 - CFO Review & Refinement...");
        let final_overrides = self
            .review_and_refine(current_config, &draft_overrides, documents, user_instruction)
            .await?;

        Ok(final_overrides)
    }

    /// Step 1: The "Junior Analyst" Logic - Generates draft overrides
    async fn generate_draft_overrides(
        &self,
        current_config: &FinancialHistoryConfig,
        documents: &[RemoteDocument],
        user_instruction: Option<&str>,
    ) -> Result<FinancialHistoryOverrides> {
        let schema_json_value = FinancialHistoryOverrides::get_gemini_response_schema()?;
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

**🚨 CRITICAL: Check for Existing Accounts First!**
Before adding ANY account to `new_balance_sheet_accounts`, verify it doesn't already exist in the raw data. Only add accounts that are genuinely missing.

**Consider adding these structural accounts if they're missing (check each one):**
- **GST Payable:** (Liability) Almost every business has this. **ESTIMATE VALUES:** If the document doesn't explicitly state GST, estimate a reasonable closing balance (e.g. roughly 10-15% of an average month's revenue/expenses) or use a placeholder like $2,000. Do NOT leave as 0.0 unless the company is clearly exempt.
- **Accounts Receivable:** (Asset) If there is Revenue, there is typically AR. **ESTIMATE:** If missing, estimate a value based on ~1 month of revenue.
- **Accounts Payable:** (Liability) If there are Expenses, there is typically AP. **ESTIMATE:** If missing, estimate a value based on ~1 month of expenses.
- **Current Year Earnings:** (Equity) This account holds the current period's profit/loss before transfer to Retained Earnings. Create it with a value of 0.0 if missing.
- **Shareholder Current Account:** (Equity/Liability) If "Shareholder Salaries" or drawings appear, consider adding this account.
- **Income Tax Payable/Provision:** (Liability) Distinct from GST. If the business is profitable, consider adding this.
- **Accumulated Depreciation:** (Contra-Asset) If Fixed Assets exist, create matching Accumulated Depreciation accounts (e.g., "Accumulated Depreciation - Plant & Equipment"). Estimate 30-50% of the fixed asset value if no data is available.
- **Intangible Assets:** (Asset) Consider whether the business has Goodwill, Brand/Trademarks, Software Licenses, Customer Relationships, etc. If there's evidence of acquisition or intangibles in the documents, add these accounts with reasonable estimated values.
- **Other Structural Accounts:** Think broadly about what other accounts this specific business might need based on the industry, business model, and available data.

**Think holistically:** Don't just add the accounts listed above. Analyze the business and consider what other structural accounts make sense for this particular company.

### 3. The "Naming Convention" Rule
Rename accounts to be short, professional, and clear.
- "Ministry of Education - 20 Hours ECE Funding" -> `Government Funding` or `MOE Funding`
- "Wages & Temporary Staff Expenses" -> `Wages`
- "Light, Power & Heating" -> `Utilities` (or keep original if it matches the user's preference)
- "Telephone & Internet" -> `Comms` or `Telephone & Internet`

### 4. Balancing Account Selection (HIGHEST PRIORITY)
**🚨 CRITICAL - READ CAREFULLY:**
You MUST designate EXACTLY ONE account as the balancing account by setting `is_balancing_account: true`.

**MANDATORY SELECTION PRIORITY (follow this order strictly):**
1. **FIRST CHOICE (99% of cases):** "Cash at Bank" or "Cash" - Look for any account with "Cash" in the name
2. **SECOND CHOICE:** "Bank Account" or any liquid asset account
3. **LAST RESORT ONLY:** Retained Earnings (only if absolutely no cash-type account exists)

**Why Cash is REQUIRED as the balancing account:**
- Cash is the natural balancing point in any business
- The balancing account will be automatically adjusted to maintain Assets = Liabilities + Equity
- Using Retained Earnings as the plug creates artificial equity fluctuations
- Cash fluctuations are real and expected in forecasting

**Action:** Review ALL balance sheet accounts (both existing and new). Find the cash account. Set `is_balancing_account: true` ONLY on that account. Set `is_balancing_account: false` on ALL other accounts (especially Retained Earnings).

### 5. Debt Structure
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
             ## YOUR TASK\n\
             1. Review the `balance_sheet` in the raw data. Identify all individual fixed assets and MERGE them into summary accounts.\n\
             2. **BEFORE adding any new accounts:** Check if they already exist in the raw data! Only add accounts that are genuinely missing.\n\
             3. Review what structural accounts might be missing (e.g., GST, AR, AP, Current Year Earnings, Accumulated Depreciation, Intangible Assets, etc.) and ADD them with realistic estimated values based on the business data.\n\
             4. Think holistically: what other accounts does this specific business need that aren't listed above?\n\
             5. Rename P&L lines to be cleaner and more professional.\n\
             6. **CRITICAL:** Set `is_balancing_account: true` on the CASH account (\"Cash at Bank\", \"Cash\", etc.). Do NOT use Retained Earnings as the balancing account unless there is absolutely no cash account.",
            current_state,
            user_instruction.unwrap_or("Clean up fixed assets and ensure all standard trading accounts exist.")
        );

        self.call_llm_with_retry(
            &system_prompt,
            &user_prompt,
            documents,
            Some(schema_json_value),
            "Forecasting_Draft",
        )
        .await
    }

    /// Step 2: The "CFO" Logic - Reviews and refines the draft
    async fn review_and_refine(
        &self,
        raw_config: &FinancialHistoryConfig,
        draft: &FinancialHistoryOverrides,
        documents: &[RemoteDocument],
        user_instruction: Option<&str>,
    ) -> Result<FinancialHistoryOverrides> {
        let schema_json_value = FinancialHistoryOverrides::get_gemini_response_schema()?;

        let raw_json = serde_json::to_string_pretty(raw_config)?;
        let draft_json = serde_json::to_string_pretty(draft)?;

        let system_prompt = prompts::SYSTEM_PROMPT_FORECAST_REVIEW;

        let user_prompt = format!(
            "## 1. RAW EXTRACTED DATA\n```json\n{}\n```\n\n\
             ## 2. JUNIOR ANALYST DRAFT OVERRIDES\n```json\n{}\n```\n\n\
             ## 3. USER INSTRUCTION (If Any)\n{}\n\n\
             ## YOUR TASK (CFO REVIEW)\n\
             Review the Draft Overrides against the Raw Data and Documents. Your job is to catch mistakes and ensure financial completeness.\n\n\
             **CRITICAL CHECKS (in order of priority):**\n\
             1. **DUPLICATE CHECK:** Did the draft add accounts that already exist in the raw data? Remove duplicate additions immediately.\n\
             2. **BALANCING ACCOUNT:** Did the draft set Retained Earnings (or any equity account) as the balancing account? **FIX THIS!** Change it to the Cash account.\n\
             3. **STRUCTURAL COMPLETENESS:** Consider what accounts might be missing (e.g., GST/VAT, AR, AP, Current Year Earnings, Income Tax Provision, Accumulated Depreciation, Intangible Assets like Goodwill, industry-specific accounts, etc.). Add any that are genuinely needed with realistic estimates.\n\
             4. **THINK HOLISTICALLY:** Beyond the standard list, what other accounts does THIS specific business need based on industry, business model, and available data?\n\
             5. **FIXED ASSETS:** Verify all granular assets were merged into clean pools.\n\
             6. **CATEGORY STANDARDIZATION:** Ensure category names are professional (e.g., 'Current Assets', 'Non-Current Liabilities').\n\n\
             Output the FINAL, corrected overrides JSON that supersedes the draft.",
            raw_json,
            draft_json,
            user_instruction.unwrap_or("Ensure full financial integrity.")
        );

        self.call_llm_with_retry(
            system_prompt,
            &user_prompt,
            documents,
            Some(schema_json_value),
            "Forecasting_Review",
        )
        .await
    }

    /// Helper for robust LLM calls with retry logic
    async fn call_llm_with_retry(
        &self,
        system_prompt: &str,
        user_prompt: &str,
        documents: &[RemoteDocument],
        schema: Option<Value>,
        label: &str,
    ) -> Result<FinancialHistoryOverrides> {
        let mut last_error: Option<String> = None;
        let max_retries = 3;

        for attempt in 1..=max_retries {
            let mut prompt_with_context = user_prompt.to_string();
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
                    system_prompt,
                    messages,
                    schema.clone(),
                    "application/json",
                    &format!("{}_attempt_{}", label, attempt),
                )
                .await
            {
                Ok(response) => {
                    let cleaned_json = extract_first_json_object(&response);
                    match serde_json::from_str::<FinancialHistoryOverrides>(&cleaned_json) {
                        Ok(overrides) => return Ok(overrides),
                        Err(e) => {
                            warn!("{} attempt {} failed to parse: {}", label, attempt, e);
                            if attempt == max_retries {
                                // Last-ditch salvage: try to coerce partial JSON into overrides
                                if let Ok(value) = serde_json::from_str::<Value>(&cleaned_json) {
                                    if let Some(overrides) = salvage_overrides_from_value(&value) {
                                        warn!("{} salvaged from partial JSON after parse failure.", label);
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
                    warn!("{} attempt {} API failed: {}", label, attempt, e);
                    if attempt == max_retries {
                        return Err(e);
                    }
                    last_error = Some(e.to_string());
                }
            }

            // Exponential-ish backoff between retries
            sleep(Duration::from_secs(2 * attempt as u64)).await;
        }

        Err(FinancialHistoryError::ExtractionFailed(format!(
            "{} exhausted retries. Last error: {:?}",
            label, last_error
        )))
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
