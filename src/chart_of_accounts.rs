use crate::engine::DenseSeries;
use crate::schema::{AccountType, FinancialHistoryConfig};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountEntry {
    pub name: String,
    pub account_type: AccountType,
    pub is_balancing_account: bool,
    pub code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChartOfAccounts {
    pub organization_name: String,
    pub fiscal_year_end_month: u32,
    pub assets: Vec<AccountEntry>,
    pub liabilities: Vec<AccountEntry>,
    pub equity: Vec<AccountEntry>,
    pub revenue: Vec<AccountEntry>,
    pub cost_of_sales: Vec<AccountEntry>,
    pub operating_expenses: Vec<AccountEntry>,
    pub other_income: Vec<AccountEntry>,
}

impl ChartOfAccounts {
    pub fn from_config(config: &FinancialHistoryConfig) -> Self {
        let mut assets = Vec::new();
        let mut liabilities = Vec::new();
        let mut equity = Vec::new();
        let mut revenue = Vec::new();
        let mut cost_of_sales = Vec::new();
        let mut operating_expenses = Vec::new();
        let mut other_income = Vec::new();

        for account in &config.balance_sheet {
            let entry = AccountEntry {
                name: account.name.clone(),
                account_type: account.account_type.clone(),
                is_balancing_account: account.is_balancing_account,
                code: None,
            };

            match account.account_type {
                AccountType::Asset => assets.push(entry),
                AccountType::Liability => liabilities.push(entry),
                AccountType::Equity => equity.push(entry),
                _ => {}
            }
        }

        for account in &config.income_statement {
            let entry = AccountEntry {
                name: account.name.clone(),
                account_type: account.account_type.clone(),
                is_balancing_account: false,
                code: None,
            };

            match account.account_type {
                AccountType::Revenue => revenue.push(entry),
                AccountType::CostOfSales => cost_of_sales.push(entry),
                AccountType::OperatingExpense => operating_expenses.push(entry),
                AccountType::OtherIncome => other_income.push(entry),
                _ => {}
            }
        }

        assets.sort_by(|a, b| a.name.cmp(&b.name));
        liabilities.sort_by(|a, b| a.name.cmp(&b.name));
        equity.sort_by(|a, b| a.name.cmp(&b.name));
        revenue.sort_by(|a, b| a.name.cmp(&b.name));
        cost_of_sales.sort_by(|a, b| a.name.cmp(&b.name));
        operating_expenses.sort_by(|a, b| a.name.cmp(&b.name));
        other_income.sort_by(|a, b| a.name.cmp(&b.name));

        Self {
            organization_name: config.organization_name.clone(),
            fiscal_year_end_month: config.fiscal_year_end_month,
            assets,
            liabilities,
            equity,
            revenue,
            cost_of_sales,
            operating_expenses,
            other_income,
        }
    }

    pub fn from_dense_data(
        config: &FinancialHistoryConfig,
        dense_data: &BTreeMap<String, DenseSeries>,
    ) -> Self {
        let mut chart = Self::from_config(config);

        for account_name in dense_data.keys() {
            let is_in_balance_sheet = config.balance_sheet.iter().any(|a| a.name == *account_name);
            let is_in_income_statement = config.income_statement.iter().any(|a| a.name == *account_name);

            if !is_in_balance_sheet && !is_in_income_statement {
                let entry = AccountEntry {
                    name: account_name.clone(),
                    account_type: AccountType::Equity,
                    is_balancing_account: true,
                    code: None,
                };
                chart.equity.push(entry);
            }
        }

        chart.equity.sort_by(|a, b| a.name.cmp(&b.name));

        chart
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    pub fn to_csv(&self) -> String {
        let mut output = String::new();
        output.push_str("Section,Account Name,Account Type,Is Balancing Account\n");

        for account in &self.assets {
            output.push_str(&format!(
                "Assets,{},{:?},{}\n",
                account.name,
                account.account_type,
                account.is_balancing_account
            ));
        }

        for account in &self.liabilities {
            output.push_str(&format!(
                "Liabilities,{},{:?},{}\n",
                account.name,
                account.account_type,
                account.is_balancing_account
            ));
        }

        for account in &self.equity {
            output.push_str(&format!(
                "Equity,{},{:?},{}\n",
                account.name,
                account.account_type,
                account.is_balancing_account
            ));
        }

        for account in &self.revenue {
            output.push_str(&format!(
                "Revenue,{},{:?},{}\n",
                account.name,
                account.account_type,
                account.is_balancing_account
            ));
        }

        for account in &self.cost_of_sales {
            output.push_str(&format!(
                "Cost of Sales,{},{:?},{}\n",
                account.name,
                account.account_type,
                account.is_balancing_account
            ));
        }

        for account in &self.operating_expenses {
            output.push_str(&format!(
                "Operating Expenses,{},{:?},{}\n",
                account.name,
                account.account_type,
                account.is_balancing_account
            ));
        }

        for account in &self.other_income {
            output.push_str(&format!(
                "Other Income,{},{:?},{}\n",
                account.name,
                account.account_type,
                account.is_balancing_account
            ));
        }

        output
    }

    pub fn to_markdown(&self) -> String {
        let mut output = String::new();

        output.push_str(&format!("# Chart of Accounts - {}\n\n", self.organization_name));
        output.push_str(&format!(
            "**Fiscal Year End:** Month {}\n\n",
            self.fiscal_year_end_month
        ));

        output.push_str("## Balance Sheet\n\n");

        output.push_str("### Assets\n\n");
        for account in &self.assets {
            let balancing_marker = if account.is_balancing_account {
                " 🔄 **[BALANCING]**"
            } else {
                ""
            };
            output.push_str(&format!("- {}{}\n", account.name, balancing_marker));
        }
        output.push('\n');

        output.push_str("### Liabilities\n\n");
        for account in &self.liabilities {
            let balancing_marker = if account.is_balancing_account {
                " 🔄 **[BALANCING]**"
            } else {
                ""
            };
            output.push_str(&format!("- {}{}\n", account.name, balancing_marker));
        }
        output.push('\n');

        output.push_str("### Equity\n\n");
        for account in &self.equity {
            let balancing_marker = if account.is_balancing_account {
                " 🔄 **[BALANCING]**"
            } else {
                ""
            };
            output.push_str(&format!("- {}{}\n", account.name, balancing_marker));
        }
        output.push('\n');

        output.push_str("## Income Statement\n\n");

        output.push_str("### Revenue\n\n");
        for account in &self.revenue {
            output.push_str(&format!("- {}\n", account.name));
        }
        output.push('\n');

        output.push_str("### Cost of Sales\n\n");
        for account in &self.cost_of_sales {
            output.push_str(&format!("- {}\n", account.name));
        }
        output.push('\n');

        output.push_str("### Operating Expenses\n\n");
        for account in &self.operating_expenses {
            output.push_str(&format!("- {}\n", account.name));
        }
        output.push('\n');

        output.push_str("### Other Income\n\n");
        for account in &self.other_income {
            output.push_str(&format!("- {}\n", account.name));
        }
        output.push('\n');

        output
    }

    pub fn total_accounts(&self) -> usize {
        self.assets.len()
            + self.liabilities.len()
            + self.equity.len()
            + self.revenue.len()
            + self.cost_of_sales.len()
            + self.operating_expenses.len()
            + self.other_income.len()
    }

    pub fn get_balancing_account(&self) -> Option<&AccountEntry> {
        self.assets
            .iter()
            .chain(self.liabilities.iter())
            .chain(self.equity.iter())
            .chain(self.revenue.iter())
            .chain(self.cost_of_sales.iter())
            .chain(self.operating_expenses.iter())
            .chain(self.other_income.iter())
            .find(|a| a.is_balancing_account)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{BalanceSheetAccount, BalanceSheetSnapshot, IncomeStatementAccount, InterpolationMethod, PeriodConstraint, SeasonalityProfileId};
    use chrono::NaiveDate;

    #[test]
    fn test_chart_of_accounts_creation() {
        let config = FinancialHistoryConfig {
            organization_name: "Test Corp".to_string(),
            fiscal_year_end_month: 12,
            balance_sheet: vec![
                BalanceSheetAccount {
                    name: "Cash".to_string(),
                    account_type: AccountType::Asset,
                    method: InterpolationMethod::Linear,
                    snapshots: vec![BalanceSheetSnapshot {
                        date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                        value: 10000.0,
                    }],
                    is_balancing_account: true,
                    noise_factor: None,
                },
            ],
            income_statement: vec![
                IncomeStatementAccount {
                    name: "Revenue".to_string(),
                    account_type: AccountType::Revenue,
                    seasonality_profile: SeasonalityProfileId::Flat,
                    constraints: vec![PeriodConstraint {
                        start_date: NaiveDate::from_ymd_opt(2023, 1, 1).unwrap(),
                        end_date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                        value: 100000.0,
                    }],
                    noise_factor: None,
                },
            ],
        };

        let chart = ChartOfAccounts::from_config(&config);

        assert_eq!(chart.assets.len(), 1);
        assert_eq!(chart.revenue.len(), 1);
        assert_eq!(chart.total_accounts(), 2);
        assert_eq!(chart.organization_name, "Test Corp");

        let balancing = chart.get_balancing_account();
        assert!(balancing.is_some());
        assert_eq!(balancing.unwrap().name, "Cash");
    }

    #[test]
    fn test_chart_to_markdown() {
        let config = FinancialHistoryConfig {
            organization_name: "Test Corp".to_string(),
            fiscal_year_end_month: 12,
            balance_sheet: vec![BalanceSheetAccount {
                name: "Cash".to_string(),
                account_type: AccountType::Asset,
                method: InterpolationMethod::Linear,
                snapshots: vec![BalanceSheetSnapshot {
                    date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                    value: 10000.0,
                }],
                is_balancing_account: true,
                noise_factor: None,
            }],
            income_statement: vec![],
        };

        let chart = ChartOfAccounts::from_config(&config);
        let markdown = chart.to_markdown();

        assert!(markdown.contains("# Chart of Accounts - Test Corp"));
        assert!(markdown.contains("Cash"));
        assert!(markdown.contains("[BALANCING]"));
    }

    #[test]
    fn test_chart_to_csv() {
        let config = FinancialHistoryConfig {
            organization_name: "Test Corp".to_string(),
            fiscal_year_end_month: 12,
            balance_sheet: vec![BalanceSheetAccount {
                name: "Cash".to_string(),
                account_type: AccountType::Asset,
                method: InterpolationMethod::Linear,
                snapshots: vec![BalanceSheetSnapshot {
                    date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                    value: 10000.0,
                }],
                is_balancing_account: true,
                noise_factor: None,
            }],
            income_statement: vec![],
        };

        let chart = ChartOfAccounts::from_config(&config);
        let csv = chart.to_csv();

        assert!(csv.contains("Section,Account Name"));
        assert!(csv.contains("Assets,Cash"));
        assert!(csv.contains("true"));
    }
}
