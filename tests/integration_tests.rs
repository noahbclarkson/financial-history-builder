use chrono::{Datelike, NaiveDate};
use financial_history_builder::*;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::Write;

fn export_to_csv(
    dense_data: &BTreeMap<String, DenseSeries>,
    filename: &str,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut file = File::create(filename)?;

    let mut all_dates: Vec<NaiveDate> = dense_data
        .values()
        .flat_map(|series| series.keys())
        .copied()
        .collect();
    all_dates.sort();
    all_dates.dedup();

    let account_names: Vec<String> = dense_data.keys().cloned().collect();

    write!(file, "Date")?;
    for name in &account_names {
        write!(file, ",{}", name)?;
    }
    writeln!(file)?;

    for date in &all_dates {
        write!(file, "{}", date.format("%Y-%m-%d"))?;
        for name in &account_names {
            let value = dense_data
                .get(name)
                .and_then(|series| series.get(date))
                .copied()
                .unwrap_or(0.0);
            write!(file, ",{:.2}", value)?;
        }
        writeln!(file)?;
    }

    Ok(())
}

#[test]
fn test_comprehensive_retail_business() {
    let history = SparseFinancialHistory {
        organization_name: "Retail Haven Inc".to_string(),
        fiscal_year_end_month: 12,
        accounts: vec![
            SparseAccount {
                name: "Sales Revenue".to_string(),
                account_type: AccountType::Revenue,
                behavior: AccountBehavior::Flow,
                interpolation: InterpolationMethod::Seasonal {
                    profile_id: SeasonalityProfileId::RetailPeak,
                },
                noise_factor: Some(0.05),
                anchors: vec![
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2022, 12, 31).unwrap(),
                        value: 2_400_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                        value: 3_000_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                ],
            is_balancing_account: false,
            },
            SparseAccount {
                name: "Cost of Goods Sold".to_string(),
                account_type: AccountType::CostOfSales,
                behavior: AccountBehavior::Flow,
                interpolation: InterpolationMethod::Seasonal {
                    profile_id: SeasonalityProfileId::RetailPeak,
                },
                noise_factor: Some(0.04),
                anchors: vec![
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2022, 12, 31).unwrap(),
                        value: 1_440_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                        value: 1_800_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                ],
            is_balancing_account: false,
            },
            SparseAccount {
                name: "Store Rent".to_string(),
                account_type: AccountType::OperatingExpense,
                behavior: AccountBehavior::Flow,
                interpolation: InterpolationMethod::Step,
                noise_factor: None,
                anchors: vec![
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2022, 12, 31).unwrap(),
                        value: 120_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                        value: 132_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                ],
                is_balancing_account: false,
            },
            SparseAccount {
                name: "Salaries & Wages".to_string(),
                account_type: AccountType::OperatingExpense,
                behavior: AccountBehavior::Flow,
                interpolation: InterpolationMethod::Linear,
                noise_factor: Some(0.02),
                anchors: vec![
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2022, 12, 31).unwrap(),
                        value: 480_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                        value: 540_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                ],
                is_balancing_account: false,
            },
            SparseAccount {
                name: "Marketing Expenses".to_string(),
                account_type: AccountType::OperatingExpense,
                behavior: AccountBehavior::Flow,
                interpolation: InterpolationMethod::Seasonal {
                    profile_id: SeasonalityProfileId::RetailPeak,
                },
                noise_factor: Some(0.08),
                anchors: vec![
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2022, 12, 31).unwrap(),
                        value: 144_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                        value: 180_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                ],
            is_balancing_account: false,
            },
            SparseAccount {
                name: "Cash at Bank".to_string(),
                account_type: AccountType::Asset,
                behavior: AccountBehavior::Stock,
                interpolation: InterpolationMethod::Curve,
                noise_factor: Some(0.03),
                anchors: vec![
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2022, 1, 31).unwrap(),
                        value: 150_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2022, 12, 31).unwrap(),
                        value: 180_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                        value: 250_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                ],
                is_balancing_account: false,
            },
            SparseAccount {
                name: "Inventory".to_string(),
                account_type: AccountType::Asset,
                behavior: AccountBehavior::Stock,
                interpolation: InterpolationMethod::Linear,
                noise_factor: Some(0.05),
                anchors: vec![
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2022, 1, 31).unwrap(),
                        value: 200_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2022, 12, 31).unwrap(),
                        value: 240_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                        value: 300_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                ],
                is_balancing_account: false,
            },
            SparseAccount {
                name: "Accounts Receivable".to_string(),
                account_type: AccountType::Asset,
                behavior: AccountBehavior::Stock,
                interpolation: InterpolationMethod::Linear,
                noise_factor: Some(0.04),
                anchors: vec![
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2022, 1, 31).unwrap(),
                        value: 80_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2022, 12, 31).unwrap(),
                        value: 100_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                        value: 130_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                ],
                is_balancing_account: false,
            },
            SparseAccount {
                name: "Equipment".to_string(),
                account_type: AccountType::Asset,
                behavior: AccountBehavior::Stock,
                interpolation: InterpolationMethod::Step,
                noise_factor: None,
                anchors: vec![
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2022, 1, 31).unwrap(),
                        value: 100_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2022, 12, 31).unwrap(),
                        value: 95_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                        value: 90_000.0,
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
                noise_factor: Some(0.03),
                anchors: vec![
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2022, 1, 31).unwrap(),
                        value: 60_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2022, 12, 31).unwrap(),
                        value: 75_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                        value: 95_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                ],
                is_balancing_account: false,
            },
            SparseAccount {
                name: "Bank Loan".to_string(),
                account_type: AccountType::Liability,
                behavior: AccountBehavior::Stock,
                interpolation: InterpolationMethod::Linear,
                noise_factor: None,
                anchors: vec![
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2022, 1, 31).unwrap(),
                        value: 200_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2022, 12, 31).unwrap(),
                        value: 180_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                        value: 160_000.0,
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
                        date: NaiveDate::from_ymd_opt(2022, 1, 31).unwrap(),
                        value: 250_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2022, 12, 31).unwrap(),
                        value: 250_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                        value: 250_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                ],
                is_balancing_account: false,
            },
        ],
    };

    let dense = process_financial_history(&history).unwrap();

    export_to_csv(&dense, "test_retail_business.csv").unwrap();

    let sales_total_2022: f64 = dense
        .get("Sales Revenue")
        .unwrap()
        .iter()
        .filter(|(date, _)| date.year() == 2022)
        .map(|(_, value)| value)
        .sum();

    assert!((sales_total_2022 - 2_400_000.0).abs() < 1.0);

    let verification = verify_accounting_equation(&history, &dense, 1.0);
    assert!(verification.is_ok());

    println!("✓ Retail business test passed - output: test_retail_business.csv");
}

#[test]
fn test_saas_startup() {
    let history = SparseFinancialHistory {
        organization_name: "CloudTech SaaS Inc".to_string(),
        fiscal_year_end_month: 12,
        accounts: vec![
            SparseAccount {
                name: "Subscription Revenue".to_string(),
                account_type: AccountType::Revenue,
                behavior: AccountBehavior::Flow,
                interpolation: InterpolationMethod::Seasonal {
                    profile_id: SeasonalityProfileId::SaasGrowth,
                },
                noise_factor: Some(0.03),
                anchors: vec![
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2022, 12, 31).unwrap(),
                        value: 600_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                        value: 1_200_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                ],
            is_balancing_account: false,
            },
            SparseAccount {
                name: "Professional Services".to_string(),
                account_type: AccountType::Revenue,
                behavior: AccountBehavior::Flow,
                interpolation: InterpolationMethod::Linear,
                noise_factor: Some(0.06),
                anchors: vec![
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2022, 12, 31).unwrap(),
                        value: 150_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                        value: 300_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                ],
                is_balancing_account: false,
            },
            SparseAccount {
                name: "Cloud Infrastructure Costs".to_string(),
                account_type: AccountType::CostOfSales,
                behavior: AccountBehavior::Flow,
                interpolation: InterpolationMethod::Seasonal {
                    profile_id: SeasonalityProfileId::SaasGrowth,
                },
                noise_factor: Some(0.02),
                anchors: vec![
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2022, 12, 31).unwrap(),
                        value: 120_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                        value: 240_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                ],
            is_balancing_account: false,
            },
            SparseAccount {
                name: "Engineering Salaries".to_string(),
                account_type: AccountType::OperatingExpense,
                behavior: AccountBehavior::Flow,
                interpolation: InterpolationMethod::Step,
                noise_factor: Some(0.01),
                anchors: vec![
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2022, 12, 31).unwrap(),
                        value: 720_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                        value: 960_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                ],
                is_balancing_account: false,
            },
            SparseAccount {
                name: "Sales & Marketing".to_string(),
                account_type: AccountType::OperatingExpense,
                behavior: AccountBehavior::Flow,
                interpolation: InterpolationMethod::Linear,
                noise_factor: Some(0.07),
                anchors: vec![
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2022, 12, 31).unwrap(),
                        value: 300_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                        value: 480_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                ],
                is_balancing_account: false,
            },
            SparseAccount {
                name: "Office & Admin".to_string(),
                account_type: AccountType::OperatingExpense,
                behavior: AccountBehavior::Flow,
                interpolation: InterpolationMethod::Step,
                noise_factor: Some(0.03),
                anchors: vec![
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2022, 12, 31).unwrap(),
                        value: 60_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                        value: 72_000.0,
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
                noise_factor: Some(0.04),
                anchors: vec![
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2022, 1, 31).unwrap(),
                        value: 500_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2022, 12, 31).unwrap(),
                        value: 350_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                        value: 200_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                ],
                is_balancing_account: false,
            },
            SparseAccount {
                name: "Accounts Receivable".to_string(),
                account_type: AccountType::Asset,
                behavior: AccountBehavior::Stock,
                interpolation: InterpolationMethod::Linear,
                noise_factor: Some(0.05),
                anchors: vec![
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2022, 1, 31).unwrap(),
                        value: 50_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2022, 12, 31).unwrap(),
                        value: 75_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                        value: 125_000.0,
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
                noise_factor: Some(0.03),
                anchors: vec![
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2022, 1, 31).unwrap(),
                        value: 40_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2022, 12, 31).unwrap(),
                        value: 55_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                        value: 75_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                ],
                is_balancing_account: false,
            },
            SparseAccount {
                name: "Deferred Revenue".to_string(),
                account_type: AccountType::Liability,
                behavior: AccountBehavior::Stock,
                interpolation: InterpolationMethod::Curve,
                noise_factor: Some(0.04),
                anchors: vec![
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2022, 1, 31).unwrap(),
                        value: 100_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2022, 12, 31).unwrap(),
                        value: 150_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                        value: 250_000.0,
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
                        date: NaiveDate::from_ymd_opt(2022, 1, 31).unwrap(),
                        value: 1_000_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2023, 1, 31).unwrap(),
                        value: 1_500_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                ],
                is_balancing_account: false,
            },
        ],
    };

    let dense = process_financial_history(&history).unwrap();

    export_to_csv(&dense, "test_saas_startup.csv").unwrap();

    let verification = verify_accounting_equation(&history, &dense, 1.0);
    assert!(verification.is_ok());

    println!("✓ SaaS startup test passed - output: test_saas_startup.csv");
}

#[test]
fn test_hospitality_business() {
    let history = SparseFinancialHistory {
        organization_name: "Seaside Resort Ltd".to_string(),
        fiscal_year_end_month: 12,
        accounts: vec![
            SparseAccount {
                name: "Room Revenue".to_string(),
                account_type: AccountType::Revenue,
                behavior: AccountBehavior::Flow,
                interpolation: InterpolationMethod::Seasonal {
                    profile_id: SeasonalityProfileId::SummerHigh,
                },
                noise_factor: Some(0.06),
                anchors: vec![
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                        value: 1_800_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                ],
            is_balancing_account: false,
            },
            SparseAccount {
                name: "Food & Beverage Revenue".to_string(),
                account_type: AccountType::Revenue,
                behavior: AccountBehavior::Flow,
                interpolation: InterpolationMethod::Seasonal {
                    profile_id: SeasonalityProfileId::SummerHigh,
                },
                noise_factor: Some(0.07),
                anchors: vec![
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                        value: 600_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                ],
            is_balancing_account: false,
            },
            SparseAccount {
                name: "F&B Cost of Sales".to_string(),
                account_type: AccountType::CostOfSales,
                behavior: AccountBehavior::Flow,
                interpolation: InterpolationMethod::Seasonal {
                    profile_id: SeasonalityProfileId::SummerHigh,
                },
                noise_factor: Some(0.04),
                anchors: vec![
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                        value: 210_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                ],
            is_balancing_account: false,
            },
            SparseAccount {
                name: "Staff Wages".to_string(),
                account_type: AccountType::OperatingExpense,
                behavior: AccountBehavior::Flow,
                interpolation: InterpolationMethod::Seasonal {
                    profile_id: SeasonalityProfileId::SummerHigh,
                },
                noise_factor: Some(0.03),
                anchors: vec![
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                        value: 720_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                ],
            is_balancing_account: false,
            },
            SparseAccount {
                name: "Utilities".to_string(),
                account_type: AccountType::OperatingExpense,
                behavior: AccountBehavior::Flow,
                interpolation: InterpolationMethod::Seasonal {
                    profile_id: SeasonalityProfileId::SummerHigh,
                },
                noise_factor: Some(0.05),
                anchors: vec![
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                        value: 120_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                ],
            is_balancing_account: false,
            },
            SparseAccount {
                name: "Property Lease".to_string(),
                account_type: AccountType::OperatingExpense,
                behavior: AccountBehavior::Flow,
                interpolation: InterpolationMethod::Step,
                noise_factor: None,
                anchors: vec![
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                        value: 240_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                ],
                is_balancing_account: false,
            },
            SparseAccount {
                name: "Cash".to_string(),
                account_type: AccountType::Asset,
                behavior: AccountBehavior::Stock,
                interpolation: InterpolationMethod::Curve,
                noise_factor: Some(0.05),
                anchors: vec![
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2023, 1, 31).unwrap(),
                        value: 200_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2023, 8, 31).unwrap(),
                        value: 400_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                        value: 280_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                ],
                is_balancing_account: false,
            },
            SparseAccount {
                name: "Property & Equipment".to_string(),
                account_type: AccountType::Asset,
                behavior: AccountBehavior::Stock,
                interpolation: InterpolationMethod::Linear,
                noise_factor: None,
                anchors: vec![
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2023, 1, 31).unwrap(),
                        value: 2_000_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                        value: 1_900_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                ],
                is_balancing_account: false,
            },
            SparseAccount {
                name: "Trade Payables".to_string(),
                account_type: AccountType::Liability,
                behavior: AccountBehavior::Stock,
                interpolation: InterpolationMethod::Linear,
                noise_factor: Some(0.04),
                anchors: vec![
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2023, 1, 31).unwrap(),
                        value: 80_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                        value: 100_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                ],
                is_balancing_account: false,
            },
            SparseAccount {
                name: "Mortgage".to_string(),
                account_type: AccountType::Liability,
                behavior: AccountBehavior::Stock,
                interpolation: InterpolationMethod::Linear,
                noise_factor: None,
                anchors: vec![
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2023, 1, 31).unwrap(),
                        value: 1_500_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                        value: 1_450_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                ],
                is_balancing_account: false,
            },
            SparseAccount {
                name: "Owner's Equity".to_string(),
                account_type: AccountType::Equity,
                behavior: AccountBehavior::Stock,
                interpolation: InterpolationMethod::Step,
                noise_factor: None,
                anchors: vec![
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2023, 1, 31).unwrap(),
                        value: 600_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                ],
                is_balancing_account: false,
            },
        ],
    };

    let dense = process_financial_history(&history).unwrap();

    export_to_csv(&dense, "test_hospitality_business.csv").unwrap();

    let verification = verify_accounting_equation(&history, &dense, 1.0);
    assert!(verification.is_ok());

    println!("✓ Hospitality business test passed - output: test_hospitality_business.csv");
}

#[test]
fn test_custom_seasonality() {
    let custom_pattern = vec![
        0.05, 0.05, 0.10, 0.15, 0.10, 0.05, 0.05, 0.10, 0.15, 0.10, 0.05, 0.05,
    ];

    let history = SparseFinancialHistory {
        organization_name: "Custom Pattern Corp".to_string(),
        fiscal_year_end_month: 6,
        accounts: vec![
            SparseAccount {
                name: "Custom Revenue".to_string(),
                account_type: AccountType::Revenue,
                behavior: AccountBehavior::Flow,
                interpolation: InterpolationMethod::Seasonal {
                    profile_id: SeasonalityProfileId::Custom(custom_pattern),
                },
                noise_factor: Some(0.03),
                anchors: vec![
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2023, 6, 30).unwrap(),
                        value: 1_200_000.0,
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
                        date: NaiveDate::from_ymd_opt(2022, 7, 31).unwrap(),
                        value: 100_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2023, 6, 30).unwrap(),
                        value: 150_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                ],
                is_balancing_account: false,
            },
            SparseAccount {
                name: "Liabilities".to_string(),
                account_type: AccountType::Liability,
                behavior: AccountBehavior::Stock,
                interpolation: InterpolationMethod::Step,
                noise_factor: None,
                anchors: vec![
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2022, 7, 31).unwrap(),
                        value: 50_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                ],
                is_balancing_account: false,
            },
            SparseAccount {
                name: "Equity".to_string(),
                account_type: AccountType::Equity,
                behavior: AccountBehavior::Stock,
                interpolation: InterpolationMethod::Step,
                noise_factor: None,
                anchors: vec![
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2022, 7, 31).unwrap(),
                        value: 50_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                ],
                is_balancing_account: false,
            },
        ],
    };

    let dense = process_financial_history(&history).unwrap();

    export_to_csv(&dense, "test_custom_seasonality.csv").unwrap();

    let revenue_total: f64 = dense.get("Custom Revenue").unwrap().values().sum();
    assert!((revenue_total - 1_200_000.0).abs() < 1.0);

    println!("✓ Custom seasonality test passed - output: test_custom_seasonality.csv");
}

#[test]
fn test_schema_generation() {
    let schema_json = SparseFinancialHistory::schema_as_json().unwrap();

    let mut file = File::create("schema_output.json").unwrap();
    file.write_all(schema_json.as_bytes()).unwrap();

    assert!(schema_json.contains("organization_name"));
    assert!(schema_json.contains("fiscal_year_end_month"));
    assert!(schema_json.contains("AccountType"));
    assert!(schema_json.contains("SeasonalityProfileId"));
    assert!(schema_json.contains("is_balancing_account"));

    println!("✓ Schema generation test passed - output: schema_output.json");
}

#[test]
fn test_designated_balancing_account() {
    let history = SparseFinancialHistory {
        organization_name: "Tech Startup Inc".to_string(),
        fiscal_year_end_month: 12,
        accounts: vec![
            SparseAccount {
                name: "Cash at Bank".to_string(),
                account_type: AccountType::Asset,
                behavior: AccountBehavior::Stock,
                interpolation: InterpolationMethod::Linear,
                noise_factor: None,
                anchors: vec![
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2023, 1, 31).unwrap(),
                        value: 100_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                ],
                is_balancing_account: true,
            },
            SparseAccount {
                name: "Accounts Receivable".to_string(),
                account_type: AccountType::Asset,
                behavior: AccountBehavior::Stock,
                interpolation: InterpolationMethod::Linear,
                noise_factor: Some(0.02),
                anchors: vec![
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2023, 1, 31).unwrap(),
                        value: 50_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                        value: 75_000.0,
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
                        value: 30_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                        value: 40_000.0,
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
                        value: 100_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                        value: 100_000.0,
                    anchor_type: AnchorType::Cumulative,
                    },
                ],
                is_balancing_account: false,
            },
        ],
    };

    let dense = process_financial_history(&history).unwrap();

    export_to_csv(&dense, "test_designated_balancing_account.csv").unwrap();

    assert!(dense.contains_key("Cash at Bank"));
    assert!(dense.contains_key("Accounts Receivable"));
    assert!(dense.contains_key("Accounts Payable"));
    assert!(dense.contains_key("Share Capital"));

    assert!(!dense.contains_key("Balancing Equity Adjustment"));

    let chart = ChartOfAccounts::from_dense_data(&history, &dense);

    assert_eq!(chart.total_accounts(), 4);

    let balancing_account = chart.get_balancing_account();
    assert!(balancing_account.is_some());
    assert_eq!(balancing_account.unwrap().name, "Cash at Bank");

    let mut file = File::create("test_chart_of_accounts.json").unwrap();
    file.write_all(chart.to_json().unwrap().as_bytes()).unwrap();

    let mut file = File::create("test_chart_of_accounts.csv").unwrap();
    file.write_all(chart.to_csv().as_bytes()).unwrap();

    let mut file = File::create("test_chart_of_accounts.md").unwrap();
    file.write_all(chart.to_markdown().as_bytes()).unwrap();

    println!("✓ Designated balancing account test passed");
    println!("  - Output: test_designated_balancing_account.csv");
    println!("  - Chart of Accounts: test_chart_of_accounts.json");
    println!("  - Chart of Accounts: test_chart_of_accounts.csv");
    println!("  - Chart of Accounts: test_chart_of_accounts.md");
}

#[test]
fn test_mixed_period_and_cumulative() {
    // Scenario:
    // 1. Jan Sales: $10k (Period)
    // 2. Feb Sales: $0 (Period - "We had no sales in Feb")
    // 3. Q1 Total: $25k (Cumulative YTD at March 31) -> Implies March was $15k

    let history = SparseFinancialHistory {
        organization_name: "Mixed Mode Corp".to_string(),
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
                        anchor_type: AnchorType::Cumulative, // $25k YTD
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

    // Check Jan
    let jan = sales.get(&NaiveDate::from_ymd_opt(2023, 1, 31).unwrap()).unwrap();
    assert!((jan - 10_000.0).abs() < 0.01, "Jan should be $10k, got {}", jan);

    // Check Feb (Should be exactly 0)
    let feb = sales.get(&NaiveDate::from_ymd_opt(2023, 2, 28).unwrap()).unwrap();
    assert_eq!(*feb, 0.0, "Feb should be exactly $0");

    // Check March
    // Cumulative is 25k. Jan(10) + Feb(0) = 10k.
    // So March Period Value = 25k - 10k = 15k.
    let mar = sales.get(&NaiveDate::from_ymd_opt(2023, 3, 31).unwrap()).unwrap();
    assert!((mar - 15_000.0).abs() < 0.01, "Mar should be $15k, got {}", mar);

    println!("✓ Mixed Period and Cumulative anchor types test passed");
}

#[test]
fn test_discrete_quarterly_input() {
    // Scenario: "In Q3 we made $15,000 in Sales A"
    // Anchor at Sep 30 with value 15,000 and type Period.
    // Assumes previous anchor or start of year bounds it.

    let history = SparseFinancialHistory {
        organization_name: "Quarterly Corp".to_string(),
        fiscal_year_end_month: 12,
        accounts: vec![
            SparseAccount {
                name: "Sales A".to_string(),
                account_type: AccountType::Revenue,
                behavior: AccountBehavior::Flow,
                interpolation: InterpolationMethod::Linear, // Even spread
                noise_factor: None,
                anchors: vec![
                    // Anchor for H1 (Jan-Jun)
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2023, 6, 30).unwrap(),
                        value: 50_000.0,
                        anchor_type: AnchorType::Cumulative,
                    },
                    // Anchor for Q3 (Jul-Sep) - Discrete
                    AnchorPoint {
                        date: NaiveDate::from_ymd_opt(2023, 9, 30).unwrap(),
                        value: 15_000.0,
                        anchor_type: AnchorType::Period,
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
    let sales = dense.get("Sales A").unwrap();

    // Check Q3 (Jul, Aug, Sep)
    // Total 15k spread linearly over 3 months = 5k/month
    let jul = sales.get(&NaiveDate::from_ymd_opt(2023, 7, 31).unwrap()).unwrap();
    let aug = sales.get(&NaiveDate::from_ymd_opt(2023, 8, 31).unwrap()).unwrap();
    let sep = sales.get(&NaiveDate::from_ymd_opt(2023, 9, 30).unwrap()).unwrap();

    assert!((jul - 5000.0).abs() < 0.1, "Jul should be ~$5k, got {}", jul);
    assert!((aug - 5000.0).abs() < 0.1, "Aug should be ~$5k, got {}", aug);
    assert!((sep - 5000.0).abs() < 0.1, "Sep should be ~$5k, got {}", sep);

    println!("✓ Discrete quarterly input test passed");
}
