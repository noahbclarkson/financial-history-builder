use chrono::NaiveDate;
use financial_history_builder::*;

fn period_range(start_year: i32, start_month: u32, end_year: i32, end_month: u32) -> String {
    if start_year == end_year && start_month == end_month {
        format!("{:04}-{:02}", start_year, start_month)
    } else {
        format!(
            "{:04}-{:02}:{:04}-{:02}",
            start_year, start_month, end_year, end_month
        )
    }
}

fn main() {
    let config = FinancialHistoryConfig {
        organization_name: "Debug".to_string(),
        fiscal_year_end_month: 12,
        balance_sheet: vec![BalanceSheetAccount {
            name: "Cash".to_string(),
            category: None,
            account_type: AccountType::Asset,
            method: InterpolationMethod::Linear,
            snapshots: vec![
                BalanceSheetSnapshot {
                    date: NaiveDate::from_ymd_opt(2023, 1, 31).unwrap(),
                    value: 100000.0,
                    source: None,
                },
                BalanceSheetSnapshot {
                    date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                    value: 100000.0,
                    source: None,
                },
            ],
            is_balancing_account: true,
            noise_factor: 0.0,
        }],
        income_statement: vec![IncomeStatementAccount {
            name: "Sales".to_string(),
            account_type: AccountType::Revenue,
            seasonality_profile: SeasonalityProfileId::Flat,
            constraints: vec![
                PeriodConstraint {
                    period: period_range(2023, 1, 2023, 1),
                    value: 10_000.0,
                    source: None,
                },
                PeriodConstraint {
                    period: period_range(2023, 2, 2023, 2),
                    value: 0.0,
                    source: None,
                },
                PeriodConstraint {
                    period: period_range(2023, 1, 2023, 3),
                    value: 25_000.0,
                    source: None,
                },
            ],
            noise_factor: 0.0,
        }],
    };

    let dense = process_financial_history(&config).unwrap();
    let sales = dense.get("Sales").unwrap();

    println!("\nSales breakdown:");
    for (date, value) in sales.iter().take(6) {
        println!("{}: ${:.2}", date, value.value);
    }

    let jan = sales
        .get(&NaiveDate::from_ymd_opt(2023, 1, 31).unwrap())
        .map(|p| p.value)
        .unwrap_or(0.0);
    let feb = sales
        .get(&NaiveDate::from_ymd_opt(2023, 2, 28).unwrap())
        .map(|p| p.value)
        .unwrap_or(0.0);
    let mar = sales
        .get(&NaiveDate::from_ymd_opt(2023, 3, 31).unwrap())
        .map(|p| p.value)
        .unwrap_or(0.0);

    println!("\nExpected:");
    println!("Jan: $10,000.00");
    println!("Feb: $0.00");
    println!("Mar: $15,000.00 (25k total - 10k Jan - 0 Feb)");

    println!("\nActual:");
    println!("Jan: ${:.2}", jan);
    println!("Feb: ${:.2}", feb);
    println!("Mar: ${:.2}", mar);

    println!("\nQ1 Total: ${:.2}", jan + feb + mar);
}
