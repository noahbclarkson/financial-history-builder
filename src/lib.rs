//! # Financial History Builder
//!
//! A library for converting sparse financial data (extracted from PDFs/documents via LLM)
//! into dense monthly time series with mathematical integrity.
//!
//! ## Core Concepts
//!
//! - **Sparse Data**: A few data points (e.g., annual totals, quarterly snapshots) with metadata
//! - **Dense Data**: Complete monthly time series with realistic variation
//! - **Balance Sheet Accounts**: Point-in-time snapshots (Assets, Liabilities, Equity) that are interpolated
//! - **Income Statement Accounts**: Period constraints (Revenue, Expenses) that are solved hierarchically
//! - **Accounting Integrity**: Enforces Assets = Liabilities + Equity at all times
//!
//! ## Example
//!
//! ```rust
//! use financial_history_builder::*;
//! use chrono::NaiveDate;
//!
//! let config = FinancialHistoryConfig {
//!     organization_name: "ACME Corp".to_string(),
//!     fiscal_year_end_month: 12,
//!     balance_sheet: vec![
//!         BalanceSheetAccount {
//!             name: "Cash".to_string(),
//!             account_type: AccountType::Asset,
//!             method: InterpolationMethod::Linear,
//!             snapshots: vec![
//!                 BalanceSheetSnapshot {
//!                     date: NaiveDate::from_ymd_opt(2023, 1, 31).unwrap(),
//!                     value: 50000.0,
//!                 },
//!                 BalanceSheetSnapshot {
//!                     date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
//!                     value: 75000.0,
//!                 },
//!             ],
//!             is_balancing_account: true,
//!             noise_factor: Some(0.02),
//!         },
//!     ],
//!     income_statement: vec![
//!         IncomeStatementAccount {
//!             name: "Revenue".to_string(),
//!             account_type: AccountType::Revenue,
//!             seasonality_profile: SeasonalityProfileId::Flat,
//!             constraints: vec![
//!                 PeriodConstraint {
//!                     start_date: NaiveDate::from_ymd_opt(2023, 1, 1).unwrap(),
//!                     end_date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
//!                     value: 1_200_000.0,
//!                 },
//!             ],
//!             noise_factor: Some(0.05),
//!         },
//!     ],
//! };
//!
//! let dense = process_financial_history(&config).unwrap();
//! ```

pub mod balancer;
pub mod chart_of_accounts;
pub mod engine;
pub mod error;
pub mod schema;
pub mod seasonality;
pub mod utils;

pub use balancer::{enforce_accounting_equation, verify_accounting_equation, AccountingBalancer};
pub use chart_of_accounts::{AccountEntry, ChartOfAccounts};
pub use engine::{process_config, DenseSeries, Densifier};
pub use error::{FinancialHistoryError, Result};
pub use schema::*;
pub use seasonality::{get_profile_weights, rotate_weights_for_fiscal_year};
pub use utils::*;

use chrono::NaiveDate;
use std::collections::BTreeMap;

pub struct FinancialHistoryProcessor;

impl FinancialHistoryProcessor {
    pub fn process(config: &FinancialHistoryConfig) -> Result<BTreeMap<String, DenseSeries>> {
        validate_fiscal_year_end_month(config.fiscal_year_end_month)?;

        let mut dense_data = process_config(config)?;

        enforce_accounting_equation_new(config, &mut dense_data)?;

        Ok(dense_data)
    }

    pub fn process_with_verification(
        config: &FinancialHistoryConfig,
        tolerance: f64,
    ) -> Result<BTreeMap<String, DenseSeries>> {
        let dense_data = Self::process(config)?;

        verify_accounting_equation_new(config, &dense_data, tolerance)?;

        Ok(dense_data)
    }
}

pub fn process_financial_history(
    config: &FinancialHistoryConfig,
) -> Result<BTreeMap<String, DenseSeries>> {
    FinancialHistoryProcessor::process(config)
}

pub fn process_with_verification(
    config: &FinancialHistoryConfig,
    tolerance: f64,
) -> Result<BTreeMap<String, DenseSeries>> {
    FinancialHistoryProcessor::process_with_verification(config, tolerance)
}

fn enforce_accounting_equation_new(
    config: &FinancialHistoryConfig,
    dense_data: &mut BTreeMap<String, DenseSeries>,
) -> Result<()> {
    let balancing_account = config
        .balance_sheet
        .iter()
        .find(|acc| acc.is_balancing_account);

    if balancing_account.is_none() {
        return Ok(());
    }

    let balancing_name = &balancing_account.unwrap().name;

    let all_dates: std::collections::BTreeSet<NaiveDate> = dense_data
        .values()
        .flat_map(|series| series.keys())
        .copied()
        .collect();

    for date in all_dates {
        let mut assets = 0.0;
        let mut liabilities = 0.0;
        let mut equity = 0.0;

        for account in &config.balance_sheet {
            if let Some(series) = dense_data.get(&account.name) {
                if let Some(&value) = series.get(&date) {
                    match account.account_type {
                        AccountType::Asset => {
                            if account.name != *balancing_name {
                                assets += value;
                            }
                        }
                        AccountType::Liability => liabilities += value,
                        AccountType::Equity => equity += value,
                        _ => {}
                    }
                }
            }
        }

        let required_balancing = liabilities + equity - assets;

        if let Some(series) = dense_data.get_mut(balancing_name) {
            series.insert(date, required_balancing);
        }
    }

    Ok(())
}

fn verify_accounting_equation_new(
    config: &FinancialHistoryConfig,
    dense_data: &BTreeMap<String, DenseSeries>,
    tolerance: f64,
) -> Result<()> {
    let all_dates: std::collections::BTreeSet<NaiveDate> = dense_data
        .values()
        .flat_map(|series| series.keys())
        .copied()
        .collect();

    for date in all_dates {
        let mut assets = 0.0;
        let mut liabilities = 0.0;
        let mut equity = 0.0;

        for account in &config.balance_sheet {
            if let Some(series) = dense_data.get(&account.name) {
                if let Some(&value) = series.get(&date) {
                    match account.account_type {
                        AccountType::Asset => assets += value,
                        AccountType::Liability => liabilities += value,
                        AccountType::Equity => equity += value,
                        _ => {}
                    }
                }
            }
        }

        let difference = (assets - (liabilities + equity)).abs();
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn test_end_to_end_processing() {
        let config = FinancialHistoryConfig {
            organization_name: "Test Company".to_string(),
            fiscal_year_end_month: 12,
            balance_sheet: vec![
                BalanceSheetAccount {
                    name: "Cash".to_string(),
                    account_type: AccountType::Asset,
                    method: InterpolationMethod::Linear,
                    snapshots: vec![
                        BalanceSheetSnapshot {
                            date: NaiveDate::from_ymd_opt(2023, 1, 31).unwrap(),
                            value: 50000.0,
                        },
                        BalanceSheetSnapshot {
                            date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                            value: 75000.0,
                        },
                    ],
                    is_balancing_account: true,
                    noise_factor: Some(0.02),
                },
                BalanceSheetAccount {
                    name: "Accounts Payable".to_string(),
                    account_type: AccountType::Liability,
                    method: InterpolationMethod::Linear,
                    snapshots: vec![
                        BalanceSheetSnapshot {
                            date: NaiveDate::from_ymd_opt(2023, 1, 31).unwrap(),
                            value: 20000.0,
                        },
                        BalanceSheetSnapshot {
                            date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                            value: 25000.0,
                        },
                    ],
                    is_balancing_account: false,
                    noise_factor: Some(0.01),
                },
                BalanceSheetAccount {
                    name: "Share Capital".to_string(),
                    account_type: AccountType::Equity,
                    method: InterpolationMethod::Step,
                    snapshots: vec![
                        BalanceSheetSnapshot {
                            date: NaiveDate::from_ymd_opt(2023, 1, 31).unwrap(),
                            value: 30000.0,
                        },
                        BalanceSheetSnapshot {
                            date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                            value: 30000.0,
                        },
                    ],
                    is_balancing_account: false,
                    noise_factor: None,
                },
            ],
            income_statement: vec![],
        };

        let result = process_financial_history(&config);
        assert!(result.is_ok());

        let dense = result.unwrap();
        assert!(dense.contains_key("Cash"));
        assert!(dense.contains_key("Accounts Payable"));
        assert!(dense.contains_key("Share Capital"));

        let verification = verify_accounting_equation_new(&config, &dense, 10.0);
        if let Err(e) = &verification {
            println!("Verification error: {:?}", e);
        }
        assert!(verification.is_ok());
    }

    #[test]
    fn test_revenue_flow_account() {
        let config = FinancialHistoryConfig {
            organization_name: "Revenue Test".to_string(),
            fiscal_year_end_month: 12,
            balance_sheet: vec![],
            income_statement: vec![IncomeStatementAccount {
                name: "Sales".to_string(),
                account_type: AccountType::Revenue,
                seasonality_profile: SeasonalityProfileId::RetailPeak,
                constraints: vec![PeriodConstraint {
                    start_date: NaiveDate::from_ymd_opt(2023, 1, 1).unwrap(),
                    end_date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                    value: 1_200_000.0,
                }],
                noise_factor: None,
            }],
        };

        let result = process_config(&config);
        assert!(result.is_ok());

        let dense = result.unwrap();
        let sales = dense.get("Sales").unwrap();

        let total: f64 = sales.values().sum();
        assert!((total - 1_200_000.0).abs() < 0.01);

        assert_eq!(sales.len(), 12);
    }

    #[test]
    fn test_hierarchical_constraints() {
        let config = FinancialHistoryConfig {
            organization_name: "Constraint Test".to_string(),
            fiscal_year_end_month: 12,
            balance_sheet: vec![],
            income_statement: vec![IncomeStatementAccount {
                name: "Revenue".to_string(),
                account_type: AccountType::Revenue,
                seasonality_profile: SeasonalityProfileId::Flat,
                constraints: vec![
                    PeriodConstraint {
                        start_date: NaiveDate::from_ymd_opt(2023, 2, 1).unwrap(),
                        end_date: NaiveDate::from_ymd_opt(2023, 2, 28).unwrap(),
                        value: 5000.0,
                    },
                    PeriodConstraint {
                        start_date: NaiveDate::from_ymd_opt(2023, 1, 1).unwrap(),
                        end_date: NaiveDate::from_ymd_opt(2023, 3, 31).unwrap(),
                        value: 13000.0,
                    },
                    PeriodConstraint {
                        start_date: NaiveDate::from_ymd_opt(2023, 1, 1).unwrap(),
                        end_date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                        value: 50000.0,
                    },
                ],
                noise_factor: None,
            }],
        };

        let result = process_config(&config);
        assert!(result.is_ok());

        let dense = result.unwrap();
        let revenue = dense.get("Revenue").unwrap();

        let total: f64 = revenue.values().sum();
        assert!(
            (total - 50000.0).abs() < 0.01,
            "Total should be 50000, got {}",
            total
        );

        let feb = revenue
            .get(&NaiveDate::from_ymd_opt(2023, 2, 28).unwrap())
            .unwrap();
        assert!(
            (feb - 5000.0).abs() < 0.01,
            "Feb should be 5000, got {}",
            feb
        );
    }
}
