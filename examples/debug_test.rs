use financial_history_builder::*;
use chrono::NaiveDate;

fn main() {
    let history = SparseFinancialHistory {
        organization_name: "Debug".to_string(),
        fiscal_year_end_month: 12,
        accounts: vec![
            SparseAccount {
                name: "Sales".to_string(),
                account_type: AccountType::Revenue,
                behavior: AccountBehavior::Flow,
                interpolation: InterpolationMethod::Linear,
                noise_factor: None,
                anchors: vec![
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2023, 1, 31).unwrap(),
                        value: 10_000.0,
                        anchor_type: AnchorType::Period,
                    },
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2023, 2, 28).unwrap(),
                        value: 0.0,
                        anchor_type: AnchorType::Period,
                    },
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2023, 3, 31).unwrap(),
                        value: 25_000.0,
                        anchor_type: AnchorType::Cumulative,
                    },
                ],
                is_balancing_account: false,
            },
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

    let dense = process_financial_history(&history).unwrap();
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
    println!("Mar: $15,000.00 (25k cumulative - 10k from Jan - 0 from Feb)");

    println!("\nActual:");
    println!("Jan: ${:.2}", jan);
    println!("Feb: ${:.2}", feb);
    println!("Mar: ${:.2}", mar);

    println!("\nQ1 Total: ${:.2}", jan + feb + mar);
}
