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
//!                     period: "2023-01:2023-12".to_string(),
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
pub mod overrides;
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
pub use overrides::*;
pub use schema::*;
pub use seasonality::{get_profile_weights, rotate_weights_for_fiscal_year};
pub use utils::*;

use chrono::NaiveDate;
use log::{debug, info};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DataOrigin {
    /// Exact match from a source document (e.g., "Cash on Dec 31")
    Anchor,
    /// Mathematically derived from surrounding points (Balance Sheet)
    Interpolated,
    /// Distributed from a larger time period (Income Statement)
    /// e.g., A monthly value derived from an Annual Total
    Allocated,
    /// Generated to force Assets = Liabilities + Equity
    BalancingPlug,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DerivationDetails {
    /// If allocated from a period (e.g. Annual), what was the total?
    pub original_period_value: Option<f64>,
    /// Start of the constraint period this was derived from
    pub period_start: Option<NaiveDate>,
    /// End of the constraint period
    pub period_end: Option<NaiveDate>,
    /// Human readable explanation (e.g. "Allocated from Annual Total")
    pub logic: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonthlyDataPoint {
    pub value: f64,
    pub origin: DataOrigin,
    /// The specific document and text snippet this came from (if applicable)
    pub source: Option<SourceMetadata>,
    /// How we calculated this specific number
    pub derivation: DerivationDetails,
}

pub type DenseSeries = BTreeMap<NaiveDate, MonthlyDataPoint>;

pub struct FinancialHistoryProcessor;

impl FinancialHistoryProcessor {
    pub fn process(config: &FinancialHistoryConfig) -> Result<BTreeMap<String, DenseSeries>> {
        validate_config_integrity(config)?;
        validate_fiscal_year_end_month(config.fiscal_year_end_month)?;

        info!(
            "Processing financial history for organization: {}",
            config.organization_name
        );
        debug!(
            "Configuration contains {} balance sheet accounts and {} income statement accounts",
            config.balance_sheet.len(),
            config.income_statement.len()
        );

        let mut dense_data = process_config(config)?;

        let verification = enforce_accounting_equation_new(config, &mut dense_data)?;

        if !verification.warnings.is_empty() {
            for warning in verification.warnings {
                debug!("Balancing adjustment details: {}", warning);
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
            let (start, end) =
                constraint
                    .resolve_dates()
                    .map_err(|e| FinancialHistoryError::ValidationError {
                        account: account.name.clone(),
                        details: format!(
                            "Constraint #{} has invalid period format '{}': {}",
                            idx, constraint.period, e
                        ),
                    })?;

            if end < start {
                return Err(FinancialHistoryError::ValidationError {
                    account: account.name.clone(),
                    details: format!(
                        "Constraint #{} period '{}' results in end_date {} before start_date {}.",
                        idx, constraint.period, end, start
                    ),
                });
            }
        }
    }

    for account in &config.balance_sheet {
        if account.noise_factor < 0.0 || account.noise_factor > 1.0 {
            return Err(FinancialHistoryError::InvalidNoiseFactor(
                account.noise_factor,
            ));
        }
    }

    for account in &config.income_statement {
        if account.noise_factor < 0.0 || account.noise_factor > 1.0 {
            return Err(FinancialHistoryError::InvalidNoiseFactor(
                account.noise_factor,
            ));
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
                    category: None,
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
                    category: None,
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
                    category: None,
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
                    period: "2023-01:2023-12".to_string(),
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
                        period: "2023-02".to_string(),
                        value: 5000.0,
                        source: None,
                    },
                    PeriodConstraint {
                        period: "2023-01:2023-03".to_string(),
                        value: 13000.0,
                        source: None,
                    },
                    PeriodConstraint {
                        period: "2023-01:2023-12".to_string(),
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
