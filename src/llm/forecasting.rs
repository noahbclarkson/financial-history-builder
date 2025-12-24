use gemini_rust::FileHandle;
use gemini_structured_output::StructuredClient;
use log::info;

use crate::error::Result;
use crate::llm::utils::build_prompt_parts;
use crate::overrides::FinancialHistoryOverrides;
use crate::schema::FinancialHistoryConfig;

// Gold-standard example to nudge the model toward aggressive aggregation and NZ/AU structure.
const FORECASTING_EXAMPLE: &str = r#"
### EXAMPLE SCENARIO
**Raw Input:** A balance sheet containing 45 individual asset lines (e.g., "Playground Mat", "Dishwasher", "Samsung TV") and a P&L with "Ministry of Education - 20 Hours" and "Ministry of Education - Subsidy".
**Goal:** A clean 3-way forecast structure.

**Correct "Thought Process":**
1. *Fixed Assets:* A forecast doesn't need to depreciate a specific "Dishwasher". It needs a "Plant & Equipment" pool. I will merge all 45 lines into 3 categories: "Fixed Assets - Plant", "Fixed Assets - Furniture", and "Fixed Assets - Improvements".
2. *Liabilities:* The raw data missed "GST Payable" and "Shareholder Current Account". I must create these. For GST Payable, I'll estimate based on the company's revenue/expense levels (approximately $5,000 seems reasonable for a business this size).
3. *Naming:* "Ministry of Education - 20 Hours..." is too long. Rename to "Government Funding".
4. *Balancing Account:* The raw data is missing a "Cash at Bank" account, so I'll add it with `is_balancing_account: true`. This is where the accounting equation will be balanced. (Note: If Cash already existed in the raw data, I would use an `UpdateMetadata` modification instead to set its `is_balancing_account` flag to true.)

**Correct Output (JSON):**
{
  "new_balance_sheet_accounts": [
    {
      "name": "Cash at Bank",
      "category": "Current Assets",
      "account_type": "Asset",
      "method": "Curve",
      "snapshots": [{ "date": "2024-03-31", "value": 15000.0, "source": null }],
      "is_balancing_account": true
    },
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
    client: StructuredClient,
}

impl ForecastingSetupAgent {
    pub fn new(client: StructuredClient) -> Self {
        Self { client }
    }

    /// Generates overrides using a 2-step process (Draft -> Review).
    pub async fn generate_overrides(
        &self,
        current_config: &FinancialHistoryConfig,
        documents: &[FileHandle],
        user_instruction: Option<&str>,
    ) -> Result<FinancialHistoryOverrides> {
        info!("Forecasting Agent: Step 1 - Generating Draft Overrides...");
        let draft_overrides = self
            .generate_draft_overrides(current_config, documents, user_instruction)
            .await?;

        info!("Forecasting Agent: Step 2 - CFO Review & Refinement...");
        let final_overrides = self
            .review_and_refine(current_config, &draft_overrides, documents, user_instruction)
            .await?;

        Ok(final_overrides)
    }

    async fn generate_draft_overrides(
        &self,
        current_config: &FinancialHistoryConfig,
        documents: &[FileHandle],
        user_instruction: Option<&str>,
    ) -> Result<FinancialHistoryOverrides> {
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

**⛔ ABSOLUTE PROHIBITION:**
**DO NOT set Retained Earnings as the balancing account!**
**DO NOT set ANY equity account as the balancing account!**
**The balancing account MUST be Cash or a liquid asset!**

**MANDATORY SELECTION PRIORITY (follow this order strictly):**
1. **FIRST CHOICE (99% of cases):** "Cash at Bank" or "Cash" - Look for any account with "Cash" in the name
2. **SECOND CHOICE:** "Bank Account" or any liquid asset account
3. **LAST RESORT ONLY:** Retained Earnings (only if absolutely no cash-type account exists)

**Why Cash is REQUIRED as the balancing account:**
- Cash is the natural balancing point in any business
- The balancing account will be automatically adjusted to maintain Assets = Liabilities + Equity
- Using Retained Earnings as the plug creates artificial equity fluctuations
- Cash fluctuations are real and expected in forecasting

**Implementation Steps:**
1. **Check if Cash exists in raw data:** Look for "Cash", "Cash at Bank", "Bank Account", etc. in the existing balance sheet accounts
2. **If Cash EXISTS:** Do NOT add it to `new_balance_sheet_accounts`. Instead:
   - Add an `UpdateMetadata` modification to set `new_is_balancing_account: true` on the Cash account
   - Example: `{{"action": "update_metadata", "target": "Cash at Bank", "new_is_balancing_account": true}}`
   - **NEVER add a duplicate Cash account!**
3. **If Cash DOES NOT EXIST:** Add it to `new_balance_sheet_accounts` with `is_balancing_account: true` (see example below)
4. **If Retained Earnings (or any equity account) is currently the balancing account:** Add an `UpdateMetadata` modification to set `new_is_balancing_account: false` on it
   - Example: `{{"action": "update_metadata", "target": "Retained Earnings", "new_is_balancing_account": false}}`

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

        let parts = build_prompt_parts(&user_prompt, documents)?;
        let outcome = self
            .client
            .request::<FinancialHistoryOverrides>()
            .system(system_prompt)
            .user_parts(parts)
            .execute()
            .await?;

        Ok(outcome.value)
    }

    async fn review_and_refine(
        &self,
        raw_config: &FinancialHistoryConfig,
        draft: &FinancialHistoryOverrides,
        documents: &[FileHandle],
        user_instruction: Option<&str>,
    ) -> Result<FinancialHistoryOverrides> {
        let raw_json = serde_json::to_string_pretty(raw_config)?;
        let draft_json = serde_json::to_string_pretty(draft)?;

        let system_prompt = r#"
You are a CFO reviewing the proposed overrides for a 3-way forecast.
Your task is to refine the draft for correctness, completeness, and professional structure.
Focus on ensuring:
- One and only one balancing account (Cash preferred)
- No duplicate or conflicting modifications
- Reasonable estimates for missing accounts
- Clear, concise account names
Return the final overrides as JSON matching the FinancialHistoryOverrides schema.
"#;

        let user_prompt = format!(
            "## RAW CONFIG\n```json\n{}\n```\n\n\
             ## DRAFT OVERRIDES\n```json\n{}\n```\n\n\
             ## USER INSTRUCTION\n{}\n\n\
             Please review and refine the draft overrides to produce the final set.",
            raw_json,
            draft_json,
            user_instruction.unwrap_or("No additional instructions.")
        );

        let parts = build_prompt_parts(&user_prompt, documents)?;
        let outcome = self
            .client
            .request::<FinancialHistoryOverrides>()
            .system(system_prompt)
            .user_parts(parts)
            .execute()
            .await?;

        Ok(outcome.value)
    }
}
