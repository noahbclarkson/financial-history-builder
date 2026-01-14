use crate::schema::{
    AccountType, BalanceSheetAccount, BalanceSheetSnapshot, FinancialHistoryConfig,
    IncomeStatementAccount, PeriodConstraint,
};
use chrono::NaiveDate;
use rstructor::{Instructor, SchemaType};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The master container for all strategic adjustments.
/// This struct is serialized to JSON Schema and passed to the LLM.
#[derive(Debug, Clone, Serialize, Deserialize, Instructor, Default)]
pub struct FinancialHistoryOverrides {
    #[llm(
        description = "List of NEW Balance Sheet accounts to create. Use this to populate missing items found in the documents (e.g., GST Payable, Loans, Fixed Assets) that were missed in the initial extraction."
    )]
    #[serde(default)]
    pub new_balance_sheet_accounts: Vec<BalanceSheetAccount>,

    #[llm(
        description = "List of NEW Income Statement accounts to create. Use this to split aggregated accounts or add revenue streams found in narrative text."
    )]
    #[serde(default)]
    pub new_income_statement_accounts: Vec<IncomeStatementAccount>,

    #[llm(
        description = "Ordered list of modifications to apply to accounts. These are applied AFTER new accounts are added."
    )]
    #[serde(default)]
    pub modifications: Vec<AccountModification>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Instructor)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum AccountModification {
    /// Rename an account (e.g., 'Telco' -> 'Telephone & Internet').
    Rename {
        #[llm(description = "The exact current name of the account.")]
        target: String,
        #[llm(description = "The new name.")]
        new_name: String,
    },

    /// Merge multiple accounts into one.
    /// - BS: Sums snapshots on matching dates.
    /// - IS: Collects all period constraints into the target.
    Merge {
        #[llm(description = "List of account names to merge FROM. These will be deleted.")]
        sources: Vec<String>,
        #[llm(
            description = "The account name to merge INTO. If it doesn't exist, it will be created using properties from the first source."
        )]
        target_name: String,
    },

    /// Change the category, account type, or balancing account flag.
    UpdateMetadata {
        #[llm(description = "The account name.")]
        target: String,
        #[llm(description = "New category string (optional).")]
        new_category: Option<String>,
        #[llm(description = "New account type (optional).")]
        new_type: Option<AccountType>,
        #[llm(
            description = "Set whether this account is the balancing account (optional). CRITICAL: Only ONE account should have this set to true. Use this to change the balancing account from Retained Earnings to Cash."
        )]
        new_is_balancing_account: Option<bool>,
    },

    /// Delete an account entirely.
    Delete { target: String },

    /// Multiply all values by a factor (e.g., -1.0 to flip sign).
    ScaleValues { target: String, factor: f64 },

    /// Manually set/override a specific value.
    /// - BS: Adds/Replaces a snapshot at the date.
    /// - IS: Adds a constraint for the period.
    SetValue {
        target: String,
        #[llm(
            description = "YYYY-MM-DD for BS snapshot, or 'YYYY-MM'/'YYYY-MM:YYYY-MM' for IS constraint."
        )]
        date_or_period: String,
        value: f64,
    },
}

impl FinancialHistoryOverrides {
    /// Applies the overrides to a base configuration, returning a new configuration.
    /// The original config is immutable, preserving the audit trail.
    pub fn apply(&self, base_config: &FinancialHistoryConfig) -> FinancialHistoryConfig {
        let mut config = base_config.clone();

        // 1. Inject New Accounts
        // We append them first so they can be targets of subsequent modifications
        config
            .balance_sheet
            .extend(self.new_balance_sheet_accounts.clone());
        config
            .income_statement
            .extend(self.new_income_statement_accounts.clone());

        // 2. Apply Modifications
        for modification in &self.modifications {
            apply_single_modification(&mut config, modification);
        }

        config
    }

    /// Generates a Gemini-compatible JSON schema (no $ref, $schema, or definitions)
    pub fn get_gemini_response_schema() -> serde_json::Result<serde_json::Value> {
        Ok(Self::schema().to_json())
    }
}

fn apply_single_modification(
    config: &mut FinancialHistoryConfig,
    modification: &AccountModification,
) {
    match modification {
        AccountModification::Rename { target, new_name } => {
            if let Some(acc) = find_bs_mut(config, target) {
                acc.name = new_name.clone();
            } else if let Some(acc) = find_is_mut(config, target) {
                acc.name = new_name.clone();
            }
        }

        AccountModification::Delete { target } => {
            config.balance_sheet.retain(|a| &a.name != target);
            config.income_statement.retain(|a| &a.name != target);
        }

        AccountModification::UpdateMetadata {
            target,
            new_category,
            new_type,
            new_is_balancing_account,
        } => {
            if let Some(acc) = find_bs_mut(config, target) {
                if let Some(c) = new_category {
                    acc.category = Some(c.clone());
                }
                if let Some(t) = new_type {
                    acc.account_type = t.clone();
                }
                if let Some(is_balancing) = new_is_balancing_account {
                    acc.is_balancing_account = *is_balancing;
                }
            } else if let Some(acc) = find_is_mut(config, target) {
                // IS accounts don't currently have a 'category' field in schema, but we update type
                if let Some(t) = new_type {
                    acc.account_type = t.clone();
                }
            }
        }

        AccountModification::ScaleValues { target, factor } => {
            if let Some(acc) = find_bs_mut(config, target) {
                for s in &mut acc.snapshots {
                    s.value *= factor;
                }
            } else if let Some(acc) = find_is_mut(config, target) {
                for c in &mut acc.constraints {
                    c.value *= factor;
                }
            }
        }

        AccountModification::SetValue {
            target,
            date_or_period,
            value,
        } => {
            if let Some(acc) = find_bs_mut(config, target) {
                // Parse date for BS
                if let Ok(date) = NaiveDate::parse_from_str(date_or_period, "%Y-%m-%d") {
                    // Remove existing snapshot at this date if any
                    acc.snapshots.retain(|s| s.date != date);
                    acc.snapshots.push(BalanceSheetSnapshot {
                        date,
                        value: *value,
                        source: None, // Manual override
                    });
                }
            } else if let Some(acc) = find_is_mut(config, target) {
                // IS simply accepts the string period
                acc.constraints.push(PeriodConstraint {
                    period: date_or_period.clone(),
                    value: *value,
                    source: None,
                });
            }
        }

        AccountModification::Merge {
            sources,
            target_name,
        } => {
            // Determine if we are operating on BS or IS based on where the sources exist
            let is_bs = config
                .balance_sheet
                .iter()
                .any(|a| sources.contains(&a.name));
            if is_bs {
                merge_balance_sheet(config, sources, target_name);
            } else {
                merge_income_statement(config, sources, target_name);
            }
        }
    }
}

fn find_bs_mut<'a>(
    config: &'a mut FinancialHistoryConfig,
    name: &str,
) -> Option<&'a mut BalanceSheetAccount> {
    config.balance_sheet.iter_mut().find(|a| a.name == name)
}

fn find_is_mut<'a>(
    config: &'a mut FinancialHistoryConfig,
    name: &str,
) -> Option<&'a mut IncomeStatementAccount> {
    config.income_statement.iter_mut().find(|a| a.name == name)
}

fn merge_balance_sheet(config: &mut FinancialHistoryConfig, sources: &[String], target_name: &str) {
    let mut collected_snapshots = Vec::new();
    let mut properties_template = None;
    let mut indices_to_remove = Vec::new();

    // 1. Collect data
    for (i, acc) in config.balance_sheet.iter().enumerate() {
        if sources.contains(&acc.name) || acc.name == target_name {
            collected_snapshots.extend(acc.snapshots.clone());
            if properties_template.is_none() {
                properties_template = Some(acc.clone());
            }
            indices_to_remove.push(i);
        }
    }

    // 2. Remove old
    // Sort descending to remove safely
    indices_to_remove.sort_by(|a, b| b.cmp(a));
    indices_to_remove.dedup(); // Handle case where target_name is also in sources
    for i in indices_to_remove {
        config.balance_sheet.remove(i);
    }

    // 3. Create merged
    if let Some(mut template) = properties_template {
        template.name = target_name.to_string();

        // Sum snapshots by date
        let mut sums: BTreeMap<NaiveDate, f64> = BTreeMap::new();
        for snap in collected_snapshots {
            *sums.entry(snap.date).or_default() += snap.value;
        }

        template.snapshots = sums
            .into_iter()
            .map(|(date, value)| BalanceSheetSnapshot {
                date,
                value,
                source: None,
            })
            .collect();

        config.balance_sheet.push(template);
    }
}

fn merge_income_statement(
    config: &mut FinancialHistoryConfig,
    sources: &[String],
    target_name: &str,
) {
    let mut collected_constraints = Vec::new();
    let mut properties_template = None;
    let mut indices_to_remove = Vec::new();

    for (i, acc) in config.income_statement.iter().enumerate() {
        if sources.contains(&acc.name) || acc.name == target_name {
            collected_constraints.extend(acc.constraints.clone());
            if properties_template.is_none() {
                properties_template = Some(acc.clone());
            }
            indices_to_remove.push(i);
        }
    }

    indices_to_remove.sort_by(|a, b| b.cmp(a));
    indices_to_remove.dedup();
    for i in indices_to_remove {
        config.income_statement.remove(i);
    }

    if let Some(mut template) = properties_template {
        template.name = target_name.to_string();
        template.constraints = collected_constraints;
        config.income_statement.push(template);
    }
}
