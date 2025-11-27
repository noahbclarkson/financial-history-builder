use crate::schema::{
    AccountType, BalanceSheetAccount, BalanceSheetSnapshot, FinancialHistoryConfig,
    IncomeStatementAccount, InterpolationMethod, PeriodConstraint, SeasonalityProfileId,
    SourceMetadata,
};
use chrono::{Datelike, NaiveDate};
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct TrialBalanceRow {
    pub account_name: String,
    pub account_type: AccountType,
    pub date: NaiveDate,
    pub ytd_value: f64,
    pub source_doc: String,
}

pub fn convert_tb_to_config(
    rows: &[TrialBalanceRow],
    organization_name: String,
    fiscal_year_end_month: u32,
) -> FinancialHistoryConfig {
    let mut balance_sheet_map: BTreeMap<String, BalanceSheetAccount> = BTreeMap::new();
    let mut income_statement_map: BTreeMap<String, IncomeStatementAccount> = BTreeMap::new();

    for row in rows {
        match row.account_type {
            AccountType::Asset | AccountType::Liability | AccountType::Equity => {
                let account = balance_sheet_map
                    .entry(row.account_name.clone())
                    .or_insert_with(|| BalanceSheetAccount {
                        name: row.account_name.clone(),
                        category: None,
                        account_type: row.account_type.clone(),
                        method: InterpolationMethod::Linear,
                        snapshots: Vec::new(),
                        is_balancing_account: false,
                        noise_factor: 0.0,
                    });

                account.snapshots.push(BalanceSheetSnapshot {
                    date: row.date,
                    value: row.ytd_value,
                    source: Some(SourceMetadata {
                        document_name: row.source_doc.clone(),
                        original_text: None,
                    }),
                });
            }
            _ => {
                let account = income_statement_map
                    .entry(row.account_name.clone())
                    .or_insert_with(|| IncomeStatementAccount {
                        name: row.account_name.clone(),
                        category: None,
                        account_type: row.account_type.clone(),
                        seasonality_profile: SeasonalityProfileId::Flat,
                        constraints: Vec::new(),
                        noise_factor: 0.0,
                    });

                let fiscal_year_start =
                    NaiveDate::from_ymd_opt(row.date.year(), 1, 1).unwrap_or(row.date);

                let period_str = format!(
                    "{}:{}",
                    fiscal_year_start.format("%Y-%m"),
                    row.date.format("%Y-%m")
                );

                account.constraints.push(PeriodConstraint {
                    period: period_str,
                    value: row.ytd_value,
                    source: Some(SourceMetadata {
                        document_name: row.source_doc.clone(),
                        original_text: None,
                    }),
                });
            }
        }
    }

    FinancialHistoryConfig {
        organization_name,
        fiscal_year_end_month,
        balance_sheet: balance_sheet_map.into_values().collect(),
        income_statement: income_statement_map.into_values().collect(),
    }
}
