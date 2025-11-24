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
//! ```rust,ignore
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
//!                     source: None,
//!                 },
//!                 BalanceSheetSnapshot {
//!                     date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
//!                     value: 75000.0,
//!                     source: None,
//!                 },
//!             ],
//!             is_balancing_account: true,
//!             noise_factor: 0.02,
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
//!                     source: None,
//!                 },
//!             ],
//!             noise_factor: 0.05,
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
pub mod ingestion;
pub mod schema;
pub mod seasonality;
pub mod utils;

#[cfg(feature = "gemini")]
pub mod llm;

pub use balancer::{
    enforce_accounting_equation, verify_accounting_equation, AccountingBalancer, VerificationResult,
};
pub use chart_of_accounts::{AccountEntry, ChartOfAccounts};
pub use engine::{process_config, Densifier};
pub use error::{FinancialHistoryError, Result};
pub use ingestion::*;
pub use schema::*;
pub use seasonality::{get_profile_weights, rotate_weights_for_fiscal_year};
pub use utils::*;

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DataOrigin {
    Anchor,
    Interpolated,
    BalancingPlug,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonthlyDataPoint {
    pub value: f64,
    pub origin: DataOrigin,
    pub source_doc: Option<String>,
}

pub type DenseSeries = BTreeMap<NaiveDate, MonthlyDataPoint>;

pub struct FinancialHistoryProcessor;

impl FinancialHistoryProcessor {
    pub fn process(config: &FinancialHistoryConfig) -> Result<BTreeMap<String, DenseSeries>> {
        validate_config_integrity(config)?;
        validate_fiscal_year_end_month(config.fiscal_year_end_month)?;

        let mut dense_data = process_config(config)?;

        let verification = enforce_accounting_equation_new(config, &mut dense_data)?;

        if !verification.warnings.is_empty() {
            for warning in verification.warnings {
                eprintln!("Warning: {}", warning);
            }
        }

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

fn validate_config_integrity(config: &FinancialHistoryConfig) -> Result<()> {
    for account in &config.income_statement {
        for (idx, constraint) in account.constraints.iter().enumerate() {
            if constraint.end_date < constraint.start_date {
                return Err(FinancialHistoryError::ValidationError {
                    account: account.name.clone(),
                    details: format!(
                        "Constraint #{} has end_date {} which is before start_date {}. Dates must be chronological.",
                        idx, constraint.end_date, constraint.start_date
                    ),
                });
            }
        }
    }

    for account in &config.balance_sheet {
        if account.noise_factor < 0.0 || account.noise_factor > 1.0 {
            return Err(FinancialHistoryError::InvalidNoiseFactor(account.noise_factor));
        }
    }

    for account in &config.income_statement {
        if account.noise_factor < 0.0 || account.noise_factor > 1.0 {
            return Err(FinancialHistoryError::InvalidNoiseFactor(account.noise_factor));
        }
    }

    Ok(())
}

fn enforce_accounting_equation_new(
    config: &FinancialHistoryConfig,
    dense_data: &mut BTreeMap<String, DenseSeries>,
) -> Result<crate::balancer::VerificationResult> {
    let balancer = AccountingBalancer::new(config);
    balancer.enforce_accounting_equation(dense_data)
}

fn verify_accounting_equation_new(
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
                },
                BalanceSheetAccount {
                    name: "Accounts Payable".to_string(),
                    account_type: AccountType::Liability,
                    method: InterpolationMethod::Linear,
                    snapshots: vec![
                        BalanceSheetSnapshot {
                            date: NaiveDate::from_ymd_opt(2023, 1, 31).unwrap(),
                            value: 20000.0,
                            source: None,
                        },
                        BalanceSheetSnapshot {
                            date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                            value: 25000.0,
                            source: None,
                        },
                    ],
                    is_balancing_account: false,
                    noise_factor: 0.01,
                },
                BalanceSheetAccount {
                    name: "Share Capital".to_string(),
                    account_type: AccountType::Equity,
                    method: InterpolationMethod::Step,
                    snapshots: vec![
                        BalanceSheetSnapshot {
                            date: NaiveDate::from_ymd_opt(2023, 1, 31).unwrap(),
                            value: 30000.0,
                            source: None,
                        },
                        BalanceSheetSnapshot {
                            date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                            value: 30000.0,
                            source: None,
                        },
                    ],
                    is_balancing_account: false,
                    noise_factor: 0.0,
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
                    source: None,
                }],
                noise_factor: 0.0,
            }],
        };

        let result = process_config(&config);
        assert!(result.is_ok());

        let dense = result.unwrap();
        let sales = dense.get("Sales").unwrap();

        let total: f64 = sales.values().map(|p| p.value).sum();
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
                        source: None,
                    },
                    PeriodConstraint {
                        start_date: NaiveDate::from_ymd_opt(2023, 1, 1).unwrap(),
                        end_date: NaiveDate::from_ymd_opt(2023, 3, 31).unwrap(),
                        value: 13000.0,
                        source: None,
                    },
                    PeriodConstraint {
                        start_date: NaiveDate::from_ymd_opt(2023, 1, 1).unwrap(),
                        end_date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                        value: 50000.0,
                        source: None,
                    },
                ],
                noise_factor: 0.0,
            }],
        };

        let result = process_config(&config);
        assert!(result.is_ok());

        let dense = result.unwrap();
        let revenue = dense.get("Revenue").unwrap();

        let total: f64 = revenue.values().map(|p| p.value).sum();
        assert!(
            (total - 50000.0).abs() < 0.01,
            "Total should be 50000, got {}",
            total
        );

        let feb = revenue
            .get(&NaiveDate::from_ymd_opt(2023, 2, 28).unwrap())
            .unwrap()
            .value;
        assert!(
            (feb - 5000.0).abs() < 0.01,
            "Feb should be 5000, got {}",
            feb
        );
    }
}
