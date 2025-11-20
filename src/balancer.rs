use crate::engine::DenseSeries;
use crate::error::{FinancialHistoryError, Result};
use crate::schema::{AccountType, FinancialHistoryConfig};
use chrono::NaiveDate;
use std::collections::{BTreeMap, HashSet};

pub struct AccountingBalancer<'a> {
    config: &'a FinancialHistoryConfig,
}

impl<'a> AccountingBalancer<'a> {
    pub fn new(config: &'a FinancialHistoryConfig) -> Self {
        Self { config }
    }

    pub fn enforce_accounting_equation(
        &self,
        dense_data: &mut BTreeMap<String, DenseSeries>,
    ) -> Result<()> {
        let plug_account_name = self.find_or_create_plug_account(dense_data)?;
        let plug_type = self.get_account_type(&plug_account_name);

        let all_dates = self.collect_all_dates(dense_data);

        for date in all_dates {
            let (assets, liabilities, equity) = self.calculate_balances(dense_data, &plug_account_name, date);

            let required_plug = match plug_type {
                AccountType::Asset => liabilities + equity - assets,
                _ => assets - liabilities - equity,
            };

            dense_data
                .entry(plug_account_name.clone())
                .or_default()
                .insert(date, required_plug);
        }

        Ok(())
    }

    pub fn verify_accounting_equation(
        &self,
        dense_data: &BTreeMap<String, DenseSeries>,
        tolerance: f64,
    ) -> Result<()> {
        let all_dates = self.collect_all_dates(dense_data);

        for date in all_dates {
            let (assets, liabilities, equity) = self.calculate_balances(dense_data, "", date);

            let left_side = assets;
            let right_side = liabilities + equity;
            let difference = (left_side - right_side).abs();

            if difference > tolerance {
                return Err(FinancialHistoryError::AccountingEquationViolation {
                    date,
                    assets,
                    liabilities,
                    equity,
                    difference,
                });
            }
        }

        Ok(())
    }

    fn find_or_create_plug_account(
        &self,
        dense_data: &BTreeMap<String, DenseSeries>,
    ) -> Result<String> {
        // 1. Explicit configuration - user designated balancing account
        for account in &self.config.balance_sheet {
            if account.is_balancing_account {
                return Ok(account.name.clone());
            }
        }

        // 2. Explicit Equity type with "retained" or "adjustment" in name
        for account in &self.config.balance_sheet {
            if account.account_type == AccountType::Equity
                && (account.name.to_lowercase().contains("retained")
                    || account.name.to_lowercase().contains("adjustment"))
            {
                return Ok(account.name.clone());
            }
        }

        // 3. Fallback: Any Equity account (by type, not name)
        for account in &self.config.balance_sheet {
            if account.account_type == AccountType::Equity {
                return Ok(account.name.clone());
            }
        }

        // 4. String matching fallback (for generated accounts not in original config)
        for name in dense_data.keys() {
            if name.to_lowercase().contains("equity") {
                return Ok(name.clone());
            }
        }

        // 5. Create new if absolutely nothing found
        Ok("Balancing Equity Adjustment".to_string())
    }

    fn collect_all_dates(&self, dense_data: &BTreeMap<String, DenseSeries>) -> Vec<NaiveDate> {
        let mut dates: HashSet<NaiveDate> = HashSet::new();

        for series in dense_data.values() {
            for &date in series.keys() {
                dates.insert(date);
            }
        }

        let mut dates_vec: Vec<NaiveDate> = dates.into_iter().collect();
        dates_vec.sort();
        dates_vec
    }

    fn calculate_balances(
        &self,
        dense_data: &BTreeMap<String, DenseSeries>,
        plug_account_name: &str,
        date: NaiveDate,
    ) -> (f64, f64, f64) {
        let mut assets = 0.0;
        let mut liabilities = 0.0;
        let mut equity = 0.0;

        for (name, series) in dense_data.iter() {
            if name == plug_account_name {
                continue;
            }

            if let Some(&value) = series.get(&date) {
                if let Some(account) = self.config.balance_sheet.iter().find(|a| a.name == *name) {
                    match account.account_type {
                        AccountType::Asset => assets += value,
                        AccountType::Liability => liabilities += value,
                        AccountType::Equity => equity += value,
                        _ => {}
                    }
                } else {
                    let name_lower = name.to_lowercase();
                    if name_lower.contains("equity")
                        || name_lower.contains("capital")
                        || name_lower.contains("retained")
                        || name_lower.contains("adjustment")
                    {
                        equity += value;
                    }
                }
            }
        }

        (assets, liabilities, equity)
    }

    fn get_account_type(&self, name: &str) -> AccountType {
        if let Some(account) = self.config.balance_sheet.iter().find(|a| a.name == name) {
            return account.account_type.clone();
        }

        AccountType::Equity
    }
}

pub fn enforce_accounting_equation(
    config: &FinancialHistoryConfig,
    dense_data: &mut BTreeMap<String, DenseSeries>,
) -> Result<()> {
    let balancer = AccountingBalancer::new(config);
    balancer.enforce_accounting_equation(dense_data)
}

pub fn verify_accounting_equation(
    config: &FinancialHistoryConfig,
    dense_data: &BTreeMap<String, DenseSeries>,
    tolerance: f64,
) -> Result<()> {
    let balancer = AccountingBalancer::new(config);
    balancer.verify_accounting_equation(dense_data, tolerance)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{BalanceSheetAccount, BalanceSheetSnapshot, InterpolationMethod};

    #[test]
    fn test_enforce_accounting_equation() {
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
                    is_balancing_account: false,
                    noise_factor: None,
                },
                BalanceSheetAccount {
                    name: "Loan".to_string(),
                    account_type: AccountType::Liability,
                    method: InterpolationMethod::Linear,
                    snapshots: vec![BalanceSheetSnapshot {
                        date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                        value: 5000.0,
                    }],
                    is_balancing_account: false,
                    noise_factor: None,
                },
                BalanceSheetAccount {
                    name: "Retained Earnings".to_string(),
                    account_type: AccountType::Equity,
                    method: InterpolationMethod::Linear,
                    snapshots: vec![],
                    is_balancing_account: false,
                    noise_factor: None,
                },
            ],
            income_statement: vec![],
        };

        let mut dense_data = BTreeMap::new();

        let mut cash_series = BTreeMap::new();
        cash_series.insert(NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(), 10000.0);
        dense_data.insert("Cash".to_string(), cash_series);

        let mut loan_series = BTreeMap::new();
        loan_series.insert(NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(), 5000.0);
        dense_data.insert("Loan".to_string(), loan_series);

        enforce_accounting_equation(&config, &mut dense_data).unwrap();

        let result = verify_accounting_equation(&config, &dense_data, 0.01);
        assert!(result.is_ok());
    }

    #[test]
    fn test_accounting_equation_violation() {
        let config = FinancialHistoryConfig {
            organization_name: "Test Corp".to_string(),
            fiscal_year_end_month: 12,
            balance_sheet: vec![
                BalanceSheetAccount {
                    name: "Cash".to_string(),
                    account_type: AccountType::Asset,
                    method: InterpolationMethod::Linear,
                    snapshots: vec![],
                    is_balancing_account: false,
                    noise_factor: None,
                },
                BalanceSheetAccount {
                    name: "Loan".to_string(),
                    account_type: AccountType::Liability,
                    method: InterpolationMethod::Linear,
                    snapshots: vec![],
                    is_balancing_account: false,
                    noise_factor: None,
                },
            ],
            income_statement: vec![],
        };

        let mut dense_data = BTreeMap::new();

        let mut cash_series = BTreeMap::new();
        cash_series.insert(NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(), 10000.0);
        dense_data.insert("Cash".to_string(), cash_series);

        let mut loan_series = BTreeMap::new();
        loan_series.insert(NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(), 3000.0);
        dense_data.insert("Loan".to_string(), loan_series);

        let result = verify_accounting_equation(&config, &dense_data, 0.01);
        assert!(result.is_err());
    }
}
