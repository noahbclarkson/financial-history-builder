use chrono::{Datelike, NaiveDate};
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
    println!("📊 Intra-Year Constraint Demo\n");
    println!(
        "This demonstrates how Income Statement accounts handle overlapping period constraints."
    );
    println!("When multiple constraints exist, they're solved hierarchically from smallest to largest.\n");

    let config = FinancialHistoryConfig {
        organization_name: "Demo Corp".to_string(),
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
            name: "Salaries".to_string(),
            account_type: AccountType::OperatingExpense,
            seasonality_profile: SeasonalityProfileId::Flat,
            constraints: vec![
                PeriodConstraint {
                    period: period_range(2023, 1, 2023, 6),
                    value: 300000.0,
                    source: None,
                },
                PeriodConstraint {
                    period: period_range(2023, 1, 2023, 12),
                    value: 600000.0,
                    source: None,
                },
            ],
            noise_factor: 0.0,
        }],
    };

    println!("📋 Configuration:");
    println!("  Salaries constraints:");
    println!("    - Jan 1 to Jun 30: $300,000");
    println!("    - Jan 1 to Dec 31: $600,000 (full year)");
    println!("\n🔄 Expected behavior:");
    println!("  Jan-Jun: Distribute $300,000 across 6 months = $50,000/month");
    println!("  Jul-Dec: Distribute ($600k - $300k) = $300,000 across 6 months = $50,000/month");
    println!("  Total: $600,000\n");

    match process_financial_history(&config) {
        Ok(dense_data) => {
            if let Some(salaries) = dense_data.get("Salaries") {
                println!("✅ Results:\n");

                let mut jan_jun_total = 0.0;
                let mut jul_dec_total = 0.0;

                for (date, value) in salaries {
                    println!("  {}: ${:>10.2}", date, value.value);

                    if date.month() <= 6 {
                        jan_jun_total += value.value;
                    } else {
                        jul_dec_total += value.value;
                    }
                }

                let total = jan_jun_total + jul_dec_total;

                println!("\n📊 Summary:");
                println!("  Jan-Jun Total: ${:>12.2}", jan_jun_total);
                println!("  Jul-Dec Total: ${:>12.2}", jul_dec_total);
                println!("  Annual Total:  ${:>12.2}", total);

                println!("\n✅ Verification:");
                println!(
                    "  Jan-Jun matches $300k: {}",
                    (jan_jun_total - 300000.0).abs() < 1.0
                );
                println!(
                    "  Jul-Dec matches $300k: {}",
                    (jul_dec_total - 300000.0).abs() < 1.0
                );
                println!("  Total matches $600k: {}", (total - 600000.0).abs() < 1.0);
            }
        }
        Err(e) => {
            eprintln!("❌ Error: {}", e);
        }
    }
}
