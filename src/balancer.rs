use crate::error::{FinancialHistoryError, Result};
use crate::schema::{AccountType, FinancialHistoryConfig};
use crate::{DataOrigin, DenseSeries, DerivationDetails, MonthlyDataPoint};
use chrono::NaiveDate;
use std::collections::{BTreeMap, HashSet};

pub struct AccountingBalancer<'a> {
    config: &'a FinancialHistoryConfig,
}

#[derive(Debug, Default, Clone)]
pub struct VerificationResult {
    pub warnings: Vec<String>,
}

impl<'a> AccountingBalancer<'a> {
    pub fn new(config: &'a FinancialHistoryConfig) -> Self {
        Self { config }
    }

    pub fn enforce_accounting_equation(
        &self,
        dense_data: &mut BTreeMap<String, DenseSeries>,
    ) -> Result<VerificationResult> {
        let plug_account_name = self.find_or_create_plug_account(dense_data)?;
        let plug_type = self.get_account_type(&plug_account_name);

        let all_dates = self.collect_all_dates(dense_data);

        for date in all_dates {
            let (assets, liabilities, equity) =
                self.calculate_balances(dense_data, &plug_account_name, date);

            let required_plug = match plug_type {
                AccountType::Asset => liabilities + equity - assets,
                _ => assets - liabilities - equity,
            };

            dense_data
                .entry(plug_account_name.clone())
                .or_default()
                .insert(
                    date,
                    MonthlyDataPoint {
                        value: required_plug,
                        origin: DataOrigin::BalancingPlug,
                        source: None,
                        derivation: DerivationDetails {
                            original_period_value: None,
                            period_start: None,
                            period_end: None,
                            logic: format!(
                                "System generated plug to enforce Assets ({:.2}) = Liab ({:.2}) + Equity ({:.2})",
                                assets, liabilities, equity
                            ),
                        },
                    },
                );
        }

        let warnings = self.check_retained_earnings_rollforward(dense_data);

        Ok(VerificationResult { warnings })
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

            if let Some(point) = series.get(&date) {
                let value = point.value;
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

    fn calculate_net_income(
        &self,
        dense_data: &BTreeMap<String, DenseSeries>,
        date: NaiveDate,
    ) -> f64 {
        let mut revenue = 0.0;
        let mut other_income = 0.0;
        let mut cost_of_sales = 0.0;
        let mut operating_expense = 0.0;

        // New accumulators for specific expense types
        let mut interest = 0.0;
        let mut depreciation = 0.0;
        let mut shareholder_salaries = 0.0;
        let mut income_tax = 0.0;

        for account in &self.config.income_statement {
            if let Some(series) = dense_data.get(&account.name) {
                if let Some(point) = series.get(&date) {
                    match account.account_type {
                        AccountType::Revenue => revenue += point.value,
                        AccountType::OtherIncome => other_income += point.value,
                        AccountType::CostOfSales => cost_of_sales += point.value,
                        AccountType::OperatingExpense => operating_expense += point.value,
                        // Handle new types as expenses
                        AccountType::Interest => interest += point.value,
                        AccountType::Depreciation => depreciation += point.value,
                        AccountType::ShareholderSalaries => shareholder_salaries += point.value,
                        AccountType::IncomeTax => income_tax += point.value,
                        _ => {}
                    }
                }
            }
        }

        // Net Income = (Revenue + Other Income) - (All Expenses)
        revenue + other_income
            - cost_of_sales
            - operating_expense
            - interest
            - depreciation
            - shareholder_salaries
            - income_tax
    }

    fn check_retained_earnings_rollforward(
        &self,
        dense_data: &BTreeMap<String, DenseSeries>,
    ) -> Vec<String> {
        let retained = self
            .config
            .balance_sheet
            .iter()
            .find(|acc| acc.name.to_lowercase().contains("retained earnings"));

        let Some(account) = retained else {
            return Vec::new();
        };

        let Some(series) = dense_data.get(&account.name) else {
            return Vec::new();
        };

        let mut dates: Vec<NaiveDate> = series.keys().copied().collect();
        dates.sort();

        let mut warnings = Vec::new();
        const RE_TOLERANCE: f64 = 1.0;

        for window in dates.windows(2) {
            let prev = window[0];
            let current = window[1];

            if let (Some(prev_point), Some(curr_point)) = (series.get(&prev), series.get(&current))
            {
                let change = curr_point.value - prev_point.value;
                let net_income = self.calculate_net_income(dense_data, current);
                if (change - net_income).abs() > RE_TOLERANCE {
                    warnings.push(format!(
                        "Retained earnings movement mismatch on {}: change {:.2} vs net income {:.2}",
                        current, change, net_income
                    ));
                }
            }
        }

        warnings
    }
}

pub fn enforce_accounting_equation(
    config: &FinancialHistoryConfig,
    dense_data: &mut BTreeMap<String, DenseSeries>,
) -> Result<VerificationResult> {
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
                        source: None,
                    }],
                    is_balancing_account: false,
                    noise_factor: 0.0,
                },
                BalanceSheetAccount {
                    name: "Loan".to_string(),
                    account_type: AccountType::Liability,
                    method: InterpolationMethod::Linear,
                    snapshots: vec![BalanceSheetSnapshot {
                        date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                        value: 5000.0,
                        source: None,
                    }],
                    is_balancing_account: false,
                    noise_factor: 0.0,
                },
                BalanceSheetAccount {
                    name: "Retained Earnings".to_string(),
                    account_type: AccountType::Equity,
                    method: InterpolationMethod::Linear,
                    snapshots: vec![],
                    is_balancing_account: false,
                    noise_factor: 0.0,
                },
            ],
            income_statement: vec![],
        };

        let mut dense_data = BTreeMap::new();

        let mut cash_series = BTreeMap::new();
        cash_series.insert(
            NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
            MonthlyDataPoint {
                value: 10000.0,
                origin: DataOrigin::Anchor,
                source: None,
                derivation: DerivationDetails {
                    original_period_value: None,
                    period_start: None,
                    period_end: None,
                    logic: "Test data".to_string(),
                },
            },
        );
        dense_data.insert("Cash".to_string(), cash_series);

        let mut loan_series = BTreeMap::new();
        loan_series.insert(
            NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
            MonthlyDataPoint {
                value: 5000.0,
                origin: DataOrigin::Anchor,
                source: None,
                derivation: DerivationDetails {
                    original_period_value: None,
                    period_start: None,
                    period_end: None,
                    logic: "Test data".to_string(),
                },
            },
        );
        dense_data.insert("Loan".to_string(), loan_series);

        let verification_result = enforce_accounting_equation(&config, &mut dense_data).unwrap();
        assert!(verification_result.warnings.is_empty());

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
                    noise_factor: 0.0,
                },
                BalanceSheetAccount {
                    name: "Loan".to_string(),
                    account_type: AccountType::Liability,
                    method: InterpolationMethod::Linear,
                    snapshots: vec![],
                    is_balancing_account: false,
                    noise_factor: 0.0,
                },
            ],
            income_statement: vec![],
        };

        let mut dense_data = BTreeMap::new();

        let mut cash_series = BTreeMap::new();
        cash_series.insert(
            NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
            MonthlyDataPoint {
                value: 10000.0,
                origin: DataOrigin::Anchor,
                source: None,
                derivation: DerivationDetails {
                    original_period_value: None,
                    period_start: None,
                    period_end: None,
                    logic: "Test data".to_string(),
                },
            },
        );
        dense_data.insert("Cash".to_string(), cash_series);

        let mut loan_series = BTreeMap::new();
        loan_series.insert(
            NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
            MonthlyDataPoint {
                value: 3000.0,
                origin: DataOrigin::Anchor,
                source: None,
                derivation: DerivationDetails {
                    original_period_value: None,
                    period_start: None,
                    period_end: None,
                    logic: "Test data".to_string(),
                },
            },
        );
        dense_data.insert("Loan".to_string(), loan_series);

        let result = verify_accounting_equation(&config, &dense_data, 0.01);
        assert!(result.is_err());
    }
}
