use financial_history_builder::*;
use chrono::{Datelike, NaiveDate};

fn main() {
    println!("📊 Intra-Year YTD Anchor Demo\n");
    println!("This demonstrates how Flow accounts handle multiple anchors in the same fiscal year.");
    println!("When multiple anchors exist, they're treated as cumulative YTD values.\n");

    let history = SparseFinancialHistory {
        organization_name: "Demo Corp".to_string(),
        fiscal_year_end_month: 12,
        accounts: vec![
            // Salaries with intra-year YTD anchors
            SparseAccount {
                name: "Salaries".to_string(),
                account_type: AccountType::OperatingExpense,
                behavior: AccountBehavior::Flow,
                interpolation: InterpolationMethod::Linear,
                noise_factor: None,
                anchors: vec![
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2023, 6, 30).unwrap(),
                        value: 300000.0, // YTD through June
                    anchor_type: AnchorType::Cumulative,
                    },
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                        value: 600000.0, // Full year
                    anchor_type: AnchorType::Cumulative,
                    },
                ],
                is_balancing_account: false,
            },
            // Cash as balancing account
            SparseAccount {
                name: "Cash".to_string(),
                account_type: AccountType::Asset,
                behavior: AccountBehavior::Stock,
                interpolation: InterpolationMethod::Linear,
                noise_factor: None,
                anchors: vec![
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2023, 1, 31).unwrap(),
                        value: 100000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                        value: 100000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                ],
                is_balancing_account: true,
            },
        ],
    };

    println!("📋 Configuration:");
    println!("  Salaries anchors:");
    println!("    - 2023-06-30: $300,000 (YTD cumulative)");
    println!("    - 2023-12-31: $600,000 (full year total)");
    println!("\n🔄 Expected behavior:");
    println!("  Jan-Jun: Distribute $300,000 across 6 months = $50,000/month");
    println!("  Jul-Dec: Distribute ($600k - $300k) = $300,000 across 6 months = $50,000/month");
    println!("  Total: $600,000\n");

    match process_financial_history(&history) {
        Ok(dense_data) => {
            if let Some(salaries) = dense_data.get("Salaries") {
                println!("✅ Results:\n");

                let mut jan_jun_total = 0.0;
                let mut jul_dec_total = 0.0;

                for (date, value) in salaries {
                    println!("  {}: ${:>10.2}", date, value);

                    if date.month() <= 6 {
                        jan_jun_total += value;
                    } else {
                        jul_dec_total += value;
                    }
                }

                let total = jan_jun_total + jul_dec_total;

                println!("\n📊 Summary:");
                println!("  Jan-Jun Total: ${:>12.2}", jan_jun_total);
                println!("  Jul-Dec Total: ${:>12.2}", jul_dec_total);
                println!("  Annual Total:  ${:>12.2}", total);

                println!("\n✅ Verification:");
                println!("  Jan-Jun matches $300k YTD: {}", (jan_jun_total - 300000.0).abs() < 1.0);
                println!("  Jul-Dec matches $300k incremental: {}", (jul_dec_total - 300000.0).abs() < 1.0);
                println!("  Total matches $600k: {}", (total - 600000.0).abs() < 1.0);
            }
        }
        Err(e) => {
            eprintln!("❌ Error: {}", e);
        }
    }
}
