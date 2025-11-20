use financial_history_builder::*;
use chrono::NaiveDate;

fn main() {
    let config = FinancialHistoryConfig {
        organization_name: "Debug".to_string(),
        fiscal_year_end_month: 12,
        balance_sheet: vec![
            BalanceSheetAccount {
                name: "Cash".to_string(),
                account_type: AccountType::Asset,
                method: InterpolationMethod::Linear,
                snapshots: vec![
                    BalanceSheetSnapshot {
                        date: NaiveDate::from_ymd_opt(2023, 1, 31).unwrap(),
                        value: 100000.0,
                    },
                    BalanceSheetSnapshot {
                        date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                        value: 100000.0,
                    },
                ],
                is_balancing_account: true,
                noise_factor: None,
            },
        ],
        income_statement: vec![
            IncomeStatementAccount {
                name: "Sales".to_string(),
                account_type: AccountType::Revenue,
                seasonality_profile: SeasonalityProfileId::Flat,
                constraints: vec![
                    PeriodConstraint {
                        start_date: NaiveDate::from_ymd_opt(2023, 1, 1).unwrap(),
                        end_date: NaiveDate::from_ymd_opt(2023, 1, 31).unwrap(),
                        value: 10_000.0,
                    },
                    PeriodConstraint {
                        start_date: NaiveDate::from_ymd_opt(2023, 2, 1).unwrap(),
                        end_date: NaiveDate::from_ymd_opt(2023, 2, 28).unwrap(),
                        value: 0.0,
                    },
                    PeriodConstraint {
                        start_date: NaiveDate::from_ymd_opt(2023, 1, 1).unwrap(),
                        end_date: NaiveDate::from_ymd_opt(2023, 3, 31).unwrap(),
                        value: 25_000.0,
                    },
                ],
                noise_factor: None,
            },
        ],
    };

    let dense = process_financial_history(&config).unwrap();
    let sales = dense.get("Sales").unwrap();

    println!("\nSales breakdown:");
    for (date, value) in sales.iter().take(6) {
        println!("{}: ${:.2}", date, value);
    }

    let jan = sales.get(&NaiveDate::from_ymd_opt(2023, 1, 31).unwrap()).unwrap_or(&0.0);
    let feb = sales.get(&NaiveDate::from_ymd_opt(2023, 2, 28).unwrap()).unwrap_or(&0.0);
    let mar = sales.get(&NaiveDate::from_ymd_opt(2023, 3, 31).unwrap()).unwrap_or(&0.0);

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
