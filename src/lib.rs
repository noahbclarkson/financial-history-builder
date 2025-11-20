//! # Financial History Builder
//!
//! A library for converting sparse financial data (extracted from PDFs/documents via LLM)
//! into dense monthly time series with mathematical integrity.
//!
//! ## Core Concepts
//!
//! - **Sparse Data**: A few anchor points (e.g., annual totals) with metadata
//! - **Dense Data**: Complete monthly time series with realistic variation
//! - **Flow Accounts**: P&L items where values represent period totals (Revenue, Expenses)
//! - **Stock Accounts**: Balance Sheet items where values represent point-in-time balances (Assets, Liabilities, Equity)
//! - **Accounting Integrity**: Enforces Assets = Liabilities + Equity at all times
//!
//! ## Example
//!
//! ```rust
//! use financial_history_builder::*;
//! use chrono::NaiveDate;
//!
//! let sparse = SparseFinancialHistory {
//!     organization_name: "ACME Corp".to_string(),
//!     fiscal_year_end_month: 12,
//!     accounts: vec![
//!         SparseAccount {
//!             name: "Revenue".to_string(),
//!             account_type: AccountType::Revenue,
//!             behavior: AccountBehavior::Flow,
//!             interpolation: InterpolationMethod::Seasonal {
//!                 profile_id: SeasonalityProfileId::Flat,
//!             },
//!             noise_factor: Some(0.05),
//!             anchors: vec![
//!                 AnchorPoint {
//!                     date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
//!                     value: 1_200_000.0,
//!                     anchor_type: AnchorType::Cumulative,
//!                 },
//!             ],
//!             is_balancing_account: false,
//!         },
//!     ],
//! };
//!
//! let dense = process_financial_history(&sparse).unwrap();
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
pub use engine::{densify_all_accounts, DenseSeries, Densifier};
pub use error::{FinancialHistoryError, Result};
pub use schema::*;
pub use seasonality::{get_profile_weights, rotate_weights_for_fiscal_year};
pub use utils::*;

use std::collections::BTreeMap;

pub struct FinancialHistoryProcessor;

impl FinancialHistoryProcessor {
    pub fn process(history: &SparseFinancialHistory) -> Result<BTreeMap<String, DenseSeries>> {
        validate_fiscal_year_end_month(history.fiscal_year_end_month)?;

        let mut dense_data = densify_all_accounts(history)?;

        enforce_accounting_equation(history, &mut dense_data)?;

        Ok(dense_data)
    }

    pub fn process_with_verification(
        history: &SparseFinancialHistory,
        tolerance: f64,
    ) -> Result<BTreeMap<String, DenseSeries>> {
        let dense_data = Self::process(history)?;

        verify_accounting_equation(history, &dense_data, tolerance)?;

        Ok(dense_data)
    }
}

pub fn process_financial_history(
    history: &SparseFinancialHistory,
) -> Result<BTreeMap<String, DenseSeries>> {
    FinancialHistoryProcessor::process(history)
}

pub fn process_with_verification(
    history: &SparseFinancialHistory,
    tolerance: f64,
) -> Result<BTreeMap<String, DenseSeries>> {
    FinancialHistoryProcessor::process_with_verification(history, tolerance)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn test_end_to_end_processing() {
        let history = SparseFinancialHistory {
            organization_name: "Test Company".to_string(),
            fiscal_year_end_month: 12,
            accounts: vec![
                SparseAccount {
                    name: "Cash".to_string(),
                    account_type: AccountType::Asset,
                    behavior: AccountBehavior::Stock,
                    interpolation: InterpolationMethod::Linear,
                    noise_factor: Some(0.02),
                    anchors: vec![
                        AnchorPoint {
                            date: NaiveDate::from_ymd_opt(2023, 1, 31).unwrap(),
                            value: 50000.0,
                        anchor_type: AnchorType::Cumulative,
                        },
                        AnchorPoint {
                            date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                            value: 75000.0,
                        anchor_type: AnchorType::Cumulative,
                        },
                    ],
                    is_balancing_account: false,
                },
                SparseAccount {
                    name: "Accounts Payable".to_string(),
                    account_type: AccountType::Liability,
                    behavior: AccountBehavior::Stock,
                    interpolation: InterpolationMethod::Linear,
                    noise_factor: Some(0.01),
                    anchors: vec![
                        AnchorPoint {
                            date: NaiveDate::from_ymd_opt(2023, 1, 31).unwrap(),
                            value: 20000.0,
                        anchor_type: AnchorType::Cumulative,
                        },
                        AnchorPoint {
                            date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                            value: 25000.0,
                        anchor_type: AnchorType::Cumulative,
                        },
                    ],
                    is_balancing_account: false,
                },
                SparseAccount {
                    name: "Share Capital".to_string(),
                    account_type: AccountType::Equity,
                    behavior: AccountBehavior::Stock,
                    interpolation: InterpolationMethod::Step,
                    noise_factor: None,
                    anchors: vec![
                        AnchorPoint {
                            date: NaiveDate::from_ymd_opt(2023, 1, 31).unwrap(),
                            value: 30000.0,
                        anchor_type: AnchorType::Cumulative,
                        },
                        AnchorPoint {
                            date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                            value: 30000.0,
                        anchor_type: AnchorType::Cumulative,
                        },
                    ],
                    is_balancing_account: false,
                },
            ],
        };

        let result = process_financial_history(&history);
        assert!(result.is_ok());

        let dense = result.unwrap();
        assert!(dense.contains_key("Cash"));
        assert!(dense.contains_key("Accounts Payable"));
        assert!(dense.contains_key("Share Capital"));

        let verification = verify_accounting_equation(&history, &dense, 10.0);
        if let Err(e) = &verification {
            println!("Verification error: {:?}", e);
        }
        assert!(verification.is_ok());
    }

    #[test]
    fn test_revenue_flow_account() {
        let history = SparseFinancialHistory {
            organization_name: "Revenue Test".to_string(),
            fiscal_year_end_month: 12,
            accounts: vec![SparseAccount {
                name: "Sales".to_string(),
                account_type: AccountType::Revenue,
                behavior: AccountBehavior::Flow,
                interpolation: InterpolationMethod::Seasonal {
                    profile_id: SeasonalityProfileId::RetailPeak,
                },
                noise_factor: None,
                anchors: vec![AnchorPoint {
                    date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                    value: 1_200_000.0,
                    anchor_type: AnchorType::Cumulative,
                }],
                is_balancing_account: false,
            }],
        };

        let result = process_financial_history(&history);
        assert!(result.is_ok());

        let dense = result.unwrap();
        let sales = dense.get("Sales").unwrap();

        let total: f64 = sales.values().sum();
        assert!((total - 1_200_000.0).abs() < 0.01);

        assert_eq!(sales.len(), 12);
    }
}
