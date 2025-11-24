use chrono::{Datelike, NaiveDate};
use financial_history_builder::{
    convert_tb_to_config, process_financial_history, verify_accounting_equation, AccountType,
    TrialBalanceRow,
};

fn main() {
    let trial_balance_rows = vec![
        TrialBalanceRow {
            account_name: "Cash".to_string(),
            account_type: AccountType::Asset,
            date: NaiveDate::from_ymd_opt(2023, 3, 31).unwrap(),
            ytd_value: 120_000.0,
            source_doc: "trial_balance_q1.xlsx".to_string(),
        },
        TrialBalanceRow {
            account_name: "Accounts Payable".to_string(),
            account_type: AccountType::Liability,
            date: NaiveDate::from_ymd_opt(2023, 3, 31).unwrap(),
            ytd_value: 30_000.0,
            source_doc: "trial_balance_q1.xlsx".to_string(),
        },
        TrialBalanceRow {
            account_name: "Retained Earnings".to_string(),
            account_type: AccountType::Equity,
            date: NaiveDate::from_ymd_opt(2023, 3, 31).unwrap(),
            ytd_value: 50_000.0,
            source_doc: "trial_balance_q1.xlsx".to_string(),
        },
        TrialBalanceRow {
            account_name: "Revenue".to_string(),
            account_type: AccountType::Revenue,
            date: NaiveDate::from_ymd_opt(2023, 3, 31).unwrap(),
            ytd_value: 90_000.0,
            source_doc: "trial_balance_q1.xlsx".to_string(),
        },
        TrialBalanceRow {
            account_name: "Operating Expenses".to_string(),
            account_type: AccountType::OperatingExpense,
            date: NaiveDate::from_ymd_opt(2023, 3, 31).unwrap(),
            ytd_value: 30_000.0,
            source_doc: "trial_balance_q1.xlsx".to_string(),
        },
    ];

    let config = convert_tb_to_config(&trial_balance_rows, "TB Demo Co".to_string(), 12);
    let dense = process_financial_history(&config).expect("engine should process trial balance");

    let revenue_q1: f64 = dense
        .get("Revenue")
        .unwrap()
        .iter()
        .filter(|(date, _)| date.month() <= 3)
        .map(|(_, point)| point.value)
        .sum();

    let opex_q1: f64 = dense
        .get("Operating Expenses")
        .unwrap()
        .iter()
        .filter(|(date, _)| date.month() <= 3)
        .map(|(_, point)| point.value)
        .sum();

    println!("Revenue Q1 total (should match YTD): {:.2}", revenue_q1);
    println!(
        "Operating Expenses Q1 total (should match YTD): {:.2}",
        opex_q1
    );

    verify_accounting_equation(&config, &dense, 1.0).expect("Accounting equation should verify");

    println!("Trust layer sample:");
    if let Some(point) = dense
        .get("Revenue")
        .and_then(|series| series.values().next())
    {
        println!(
            " - First revenue month origin: {:?}, source: {:?}",
            point.origin, point.source_doc
        );
    }
}
