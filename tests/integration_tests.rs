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
                .map(|point| point.value)
                .unwrap_or(0.0);
            write!(file, ",{:.2}", value)?;
        }
        writeln!(file)?;
    }

    Ok(())
}

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

#[test]
fn test_comprehensive_retail_business() {
    let config = FinancialHistoryConfig {
        organization_name: "Retail Haven Inc".to_string(),
        fiscal_year_end_month: 12,
        balance_sheet: vec![
            BalanceSheetAccount {
                name: "Cash at Bank".to_string(),
                account_type: AccountType::Asset,
                method: InterpolationMethod::Curve,
                snapshots: vec![
                    BalanceSheetSnapshot {
                        date: NaiveDate::from_ymd_opt(2022, 1, 31).unwrap(),
                        value: 150_000.0,
                        source: None,
                    },
                    BalanceSheetSnapshot {
                        date: NaiveDate::from_ymd_opt(2022, 12, 31).unwrap(),
                        value: 180_000.0,
                        source: None,
                    },
                    BalanceSheetSnapshot {
                        date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                        value: 250_000.0,
                        source: None,
                    },
                ],
                is_balancing_account: true,
                noise_factor: 0.03,
            },
            BalanceSheetAccount {
                name: "Inventory".to_string(),
                account_type: AccountType::Asset,
                method: InterpolationMethod::Linear,
                snapshots: vec![
                    BalanceSheetSnapshot {
                        date: NaiveDate::from_ymd_opt(2022, 1, 31).unwrap(),
                        value: 200_000.0,
                        source: None,
                    },
                    BalanceSheetSnapshot {
                        date: NaiveDate::from_ymd_opt(2022, 12, 31).unwrap(),
                        value: 240_000.0,
                        source: None,
                    },
                    BalanceSheetSnapshot {
                        date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                        value: 300_000.0,
                        source: None,
                    },
                ],
                is_balancing_account: false,
                noise_factor: 0.05,
            },
            BalanceSheetAccount {
                name: "Accounts Receivable".to_string(),
                account_type: AccountType::Asset,
                method: InterpolationMethod::Linear,
                snapshots: vec![
                    BalanceSheetSnapshot {
                        date: NaiveDate::from_ymd_opt(2022, 1, 31).unwrap(),
                        value: 80_000.0,
                        source: None,
                    },
                    BalanceSheetSnapshot {
                        date: NaiveDate::from_ymd_opt(2022, 12, 31).unwrap(),
                        value: 100_000.0,
                        source: None,
                    },
                    BalanceSheetSnapshot {
                        date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                        value: 130_000.0,
                        source: None,
                    },
                ],
                is_balancing_account: false,
                noise_factor: 0.04,
            },
            BalanceSheetAccount {
                name: "Equipment".to_string(),
                account_type: AccountType::Asset,
                method: InterpolationMethod::Step,
                snapshots: vec![
                    BalanceSheetSnapshot {
                        date: NaiveDate::from_ymd_opt(2022, 1, 31).unwrap(),
                        value: 100_000.0,
                        source: None,
                    },
                    BalanceSheetSnapshot {
                        date: NaiveDate::from_ymd_opt(2022, 12, 31).unwrap(),
                        value: 95_000.0,
                        source: None,
                    },
                    BalanceSheetSnapshot {
                        date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                        value: 90_000.0,
                        source: None,
                    },
                ],
                is_balancing_account: false,
                noise_factor: 0.0,
            },
            BalanceSheetAccount {
                name: "Accounts Payable".to_string(),
                account_type: AccountType::Liability,
                method: InterpolationMethod::Linear,
                snapshots: vec![
                    BalanceSheetSnapshot {
                        date: NaiveDate::from_ymd_opt(2022, 1, 31).unwrap(),
                        value: 60_000.0,
                        source: None,
                    },
                    BalanceSheetSnapshot {
                        date: NaiveDate::from_ymd_opt(2022, 12, 31).unwrap(),
                        value: 75_000.0,
                        source: None,
                    },
                    BalanceSheetSnapshot {
                        date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                        value: 95_000.0,
                        source: None,
                    },
                ],
                is_balancing_account: false,
                noise_factor: 0.03,
            },
            BalanceSheetAccount {
                name: "Bank Loan".to_string(),
                account_type: AccountType::Liability,
                method: InterpolationMethod::Linear,
                snapshots: vec![
                    BalanceSheetSnapshot {
                        date: NaiveDate::from_ymd_opt(2022, 1, 31).unwrap(),
                        value: 200_000.0,
                        source: None,
                    },
                    BalanceSheetSnapshot {
                        date: NaiveDate::from_ymd_opt(2022, 12, 31).unwrap(),
                        value: 180_000.0,
                        source: None,
                    },
                    BalanceSheetSnapshot {
                        date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                        value: 160_000.0,
                        source: None,
                    },
                ],
                is_balancing_account: false,
                noise_factor: 0.0,
            },
            BalanceSheetAccount {
                name: "Share Capital".to_string(),
                account_type: AccountType::Equity,
                method: InterpolationMethod::Step,
                snapshots: vec![
                    BalanceSheetSnapshot {
                        date: NaiveDate::from_ymd_opt(2022, 1, 31).unwrap(),
                        value: 250_000.0,
                        source: None,
                    },
                    BalanceSheetSnapshot {
                        date: NaiveDate::from_ymd_opt(2022, 12, 31).unwrap(),
                        value: 250_000.0,
                        source: None,
                    },
                    BalanceSheetSnapshot {
                        date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                        value: 250_000.0,
                        source: None,
                    },
                ],
                is_balancing_account: false,
                noise_factor: 0.0,
            },
        ],
        income_statement: vec![
            IncomeStatementAccount {
                name: "Sales Revenue".to_string(),
                account_type: AccountType::Revenue,
                seasonality_profile: SeasonalityProfileId::RetailPeak,
                constraints: vec![
                    PeriodConstraint {
                        period: period_range(2022, 1, 2022, 12),
                        value: 2_400_000.0,
                        source: None,
                    },
                    PeriodConstraint {
                        period: period_range(2023, 1, 2023, 12),
                        value: 3_000_000.0,
                        source: None,
                    },
                ],
                noise_factor: 0.05,
            },
            IncomeStatementAccount {
                name: "Cost of Goods Sold".to_string(),
                account_type: AccountType::CostOfSales,
                seasonality_profile: SeasonalityProfileId::RetailPeak,
                constraints: vec![
                    PeriodConstraint {
                        period: period_range(2022, 1, 2022, 12),
                        value: 1_440_000.0,
                        source: None,
                    },
                    PeriodConstraint {
                        period: period_range(2023, 1, 2023, 12),
                        value: 1_800_000.0,
                        source: None,
                    },
                ],
                noise_factor: 0.04,
            },
            IncomeStatementAccount {
                name: "Store Rent".to_string(),
                account_type: AccountType::OperatingExpense,
                seasonality_profile: SeasonalityProfileId::Flat,
                constraints: vec![
                    PeriodConstraint {
                        period: period_range(2022, 1, 2022, 12),
                        value: 120_000.0,
                        source: None,
                    },
                    PeriodConstraint {
                        period: period_range(2023, 1, 2023, 12),
                        value: 132_000.0,
                        source: None,
                    },
                ],
                noise_factor: 0.0,
            },
            IncomeStatementAccount {
                name: "Salaries & Wages".to_string(),
                account_type: AccountType::OperatingExpense,
                seasonality_profile: SeasonalityProfileId::Flat,
                constraints: vec![
                    PeriodConstraint {
                        period: period_range(2022, 1, 2022, 12),
                        value: 480_000.0,
                        source: None,
                    },
                    PeriodConstraint {
                        period: period_range(2023, 1, 2023, 12),
                        value: 540_000.0,
                        source: None,
                    },
                ],
                noise_factor: 0.02,
            },
            IncomeStatementAccount {
                name: "Marketing Expenses".to_string(),
                account_type: AccountType::OperatingExpense,
                seasonality_profile: SeasonalityProfileId::RetailPeak,
                constraints: vec![
                    PeriodConstraint {
                        period: period_range(2022, 1, 2022, 12),
                        value: 144_000.0,
                        source: None,
                    },
                    PeriodConstraint {
                        period: period_range(2023, 1, 2023, 12),
                        value: 180_000.0,
                        source: None,
                    },
                ],
                noise_factor: 0.08,
            },
        ],
    };

    let dense = process_financial_history(&config).unwrap();

    export_to_csv(&dense, "test_retail_business.csv").unwrap();

    let sales_total_2022: f64 = dense
        .get("Sales Revenue")
        .unwrap()
        .iter()
        .filter(|(date, _)| date.year() == 2022)
        .map(|(_, point)| point.value)
        .sum();

    assert!((sales_total_2022 - 2_400_000.0).abs() < 1.0);

    let verification = verify_accounting_equation(&config, &dense, 1.0);
    assert!(verification.is_ok());

    println!("✓ Retail business test passed - output: test_retail_business.csv");
}

#[test]
fn test_saas_startup() {
    let config = FinancialHistoryConfig {
        organization_name: "CloudTech SaaS Inc".to_string(),
        fiscal_year_end_month: 12,
        balance_sheet: vec![
            BalanceSheetAccount {
                name: "Cash".to_string(),
                account_type: AccountType::Asset,
                method: InterpolationMethod::Linear,
                snapshots: vec![
                    BalanceSheetSnapshot {
                        date: NaiveDate::from_ymd_opt(2022, 1, 31).unwrap(),
                        value: 500_000.0,
                        source: None,
                    },
                    BalanceSheetSnapshot {
                        date: NaiveDate::from_ymd_opt(2022, 12, 31).unwrap(),
                        value: 350_000.0,
                        source: None,
                    },
                    BalanceSheetSnapshot {
                        date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                        value: 200_000.0,
                        source: None,
                    },
                ],
                is_balancing_account: true,
                noise_factor: 0.04,
            },
            BalanceSheetAccount {
                name: "Accounts Receivable".to_string(),
                account_type: AccountType::Asset,
                method: InterpolationMethod::Linear,
                snapshots: vec![
                    BalanceSheetSnapshot {
                        date: NaiveDate::from_ymd_opt(2022, 1, 31).unwrap(),
                        value: 50_000.0,
                        source: None,
                    },
                    BalanceSheetSnapshot {
                        date: NaiveDate::from_ymd_opt(2022, 12, 31).unwrap(),
                        value: 75_000.0,
                        source: None,
                    },
                    BalanceSheetSnapshot {
                        date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                        value: 125_000.0,
                        source: None,
                    },
                ],
                is_balancing_account: false,
                noise_factor: 0.05,
            },
            BalanceSheetAccount {
                name: "Accounts Payable".to_string(),
                account_type: AccountType::Liability,
                method: InterpolationMethod::Linear,
                snapshots: vec![
                    BalanceSheetSnapshot {
                        date: NaiveDate::from_ymd_opt(2022, 1, 31).unwrap(),
                        value: 40_000.0,
                        source: None,
                    },
                    BalanceSheetSnapshot {
                        date: NaiveDate::from_ymd_opt(2022, 12, 31).unwrap(),
                        value: 55_000.0,
                        source: None,
                    },
                    BalanceSheetSnapshot {
                        date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                        value: 75_000.0,
                        source: None,
                    },
                ],
                is_balancing_account: false,
                noise_factor: 0.03,
            },
            BalanceSheetAccount {
                name: "Deferred Revenue".to_string(),
                account_type: AccountType::Liability,
                method: InterpolationMethod::Curve,
                snapshots: vec![
                    BalanceSheetSnapshot {
                        date: NaiveDate::from_ymd_opt(2022, 1, 31).unwrap(),
                        value: 100_000.0,
                        source: None,
                    },
                    BalanceSheetSnapshot {
                        date: NaiveDate::from_ymd_opt(2022, 12, 31).unwrap(),
                        value: 150_000.0,
                        source: None,
                    },
                    BalanceSheetSnapshot {
                        date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                        value: 250_000.0,
                        source: None,
                    },
                ],
                is_balancing_account: false,
                noise_factor: 0.04,
            },
            BalanceSheetAccount {
                name: "Share Capital".to_string(),
                account_type: AccountType::Equity,
                method: InterpolationMethod::Step,
                snapshots: vec![
                    BalanceSheetSnapshot {
                        date: NaiveDate::from_ymd_opt(2022, 1, 31).unwrap(),
                        value: 1_000_000.0,
                        source: None,
                    },
                    BalanceSheetSnapshot {
                        date: NaiveDate::from_ymd_opt(2023, 1, 31).unwrap(),
                        value: 1_500_000.0,
                        source: None,
                    },
                ],
                is_balancing_account: false,
                noise_factor: 0.0,
            },
        ],
        income_statement: vec![
            IncomeStatementAccount {
                name: "Subscription Revenue".to_string(),
                account_type: AccountType::Revenue,
                seasonality_profile: SeasonalityProfileId::SaasGrowth,
                constraints: vec![
                    PeriodConstraint {
                        period: period_range(2022, 1, 2022, 12),
                        value: 600_000.0,
                        source: None,
                    },
                    PeriodConstraint {
                        period: period_range(2023, 1, 2023, 12),
                        value: 1_200_000.0,
                        source: None,
                    },
                ],
                noise_factor: 0.03,
            },
            IncomeStatementAccount {
                name: "Professional Services".to_string(),
                account_type: AccountType::Revenue,
                seasonality_profile: SeasonalityProfileId::Flat,
                constraints: vec![
                    PeriodConstraint {
                        period: period_range(2022, 1, 2022, 12),
                        value: 150_000.0,
                        source: None,
                    },
                    PeriodConstraint {
                        period: period_range(2023, 1, 2023, 12),
                        value: 300_000.0,
                        source: None,
                    },
                ],
                noise_factor: 0.06,
            },
            IncomeStatementAccount {
                name: "Cloud Infrastructure Costs".to_string(),
                account_type: AccountType::CostOfSales,
                seasonality_profile: SeasonalityProfileId::SaasGrowth,
                constraints: vec![
                    PeriodConstraint {
                        period: period_range(2022, 1, 2022, 12),
                        value: 120_000.0,
                        source: None,
                    },
                    PeriodConstraint {
                        period: period_range(2023, 1, 2023, 12),
                        value: 240_000.0,
                        source: None,
                    },
                ],
                noise_factor: 0.02,
            },
            IncomeStatementAccount {
                name: "Engineering Salaries".to_string(),
                account_type: AccountType::OperatingExpense,
                seasonality_profile: SeasonalityProfileId::Flat,
                constraints: vec![
                    PeriodConstraint {
                        period: period_range(2022, 1, 2022, 12),
                        value: 720_000.0,
                        source: None,
                    },
                    PeriodConstraint {
                        period: period_range(2023, 1, 2023, 12),
                        value: 960_000.0,
                        source: None,
                    },
                ],
                noise_factor: 0.01,
            },
            IncomeStatementAccount {
                name: "Sales & Marketing".to_string(),
                account_type: AccountType::OperatingExpense,
                seasonality_profile: SeasonalityProfileId::Flat,
                constraints: vec![
                    PeriodConstraint {
                        period: period_range(2022, 1, 2022, 12),
                        value: 300_000.0,
                        source: None,
                    },
                    PeriodConstraint {
                        period: period_range(2023, 1, 2023, 12),
                        value: 480_000.0,
                        source: None,
                    },
                ],
                noise_factor: 0.07,
            },
            IncomeStatementAccount {
                name: "Office & Admin".to_string(),
                account_type: AccountType::OperatingExpense,
                seasonality_profile: SeasonalityProfileId::Flat,
                constraints: vec![
                    PeriodConstraint {
                        period: period_range(2022, 1, 2022, 12),
                        value: 60_000.0,
                        source: None,
                    },
                    PeriodConstraint {
                        period: period_range(2023, 1, 2023, 12),
                        value: 72_000.0,
                        source: None,
                    },
                ],
                noise_factor: 0.03,
            },
        ],
    };

    let dense = process_financial_history(&config).unwrap();

    export_to_csv(&dense, "test_saas_startup.csv").unwrap();

    let verification = verify_accounting_equation(&config, &dense, 1.0);
    assert!(verification.is_ok());

    println!("✓ SaaS startup test passed - output: test_saas_startup.csv");
}

#[test]
fn test_hospitality_business() {
    let config = FinancialHistoryConfig {
        organization_name: "Seaside Resort Ltd".to_string(),
        fiscal_year_end_month: 12,
        balance_sheet: vec![
            BalanceSheetAccount {
                name: "Cash".to_string(),
                account_type: AccountType::Asset,
                method: InterpolationMethod::Curve,
                snapshots: vec![
                    BalanceSheetSnapshot {
                        date: NaiveDate::from_ymd_opt(2023, 1, 31).unwrap(),
                        value: 200_000.0,
                        source: None,
                    },
                    BalanceSheetSnapshot {
                        date: NaiveDate::from_ymd_opt(2023, 8, 31).unwrap(),
                        value: 400_000.0,
                        source: None,
                    },
                    BalanceSheetSnapshot {
                        date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                        value: 280_000.0,
                        source: None,
                    },
                ],
                is_balancing_account: true,
                noise_factor: 0.05,
            },
            BalanceSheetAccount {
                name: "Property & Equipment".to_string(),
                account_type: AccountType::Asset,
                method: InterpolationMethod::Linear,
                snapshots: vec![
                    BalanceSheetSnapshot {
                        date: NaiveDate::from_ymd_opt(2023, 1, 31).unwrap(),
                        value: 2_000_000.0,
                        source: None,
                    },
                    BalanceSheetSnapshot {
                        date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                        value: 1_900_000.0,
                        source: None,
                    },
                ],
                is_balancing_account: false,
                noise_factor: 0.0,
            },
            BalanceSheetAccount {
                name: "Trade Payables".to_string(),
                account_type: AccountType::Liability,
                method: InterpolationMethod::Linear,
                snapshots: vec![
                    BalanceSheetSnapshot {
                        date: NaiveDate::from_ymd_opt(2023, 1, 31).unwrap(),
                        value: 80_000.0,
                        source: None,
                    },
                    BalanceSheetSnapshot {
                        date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                        value: 100_000.0,
                        source: None,
                    },
                ],
                is_balancing_account: false,
                noise_factor: 0.04,
            },
            BalanceSheetAccount {
                name: "Mortgage".to_string(),
                account_type: AccountType::Liability,
                method: InterpolationMethod::Linear,
                snapshots: vec![
                    BalanceSheetSnapshot {
                        date: NaiveDate::from_ymd_opt(2023, 1, 31).unwrap(),
                        value: 1_500_000.0,
                        source: None,
                    },
                    BalanceSheetSnapshot {
                        date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                        value: 1_450_000.0,
                        source: None,
                    },
                ],
                is_balancing_account: false,
                noise_factor: 0.0,
            },
            BalanceSheetAccount {
                name: "Owner's Equity".to_string(),
                account_type: AccountType::Equity,
                method: InterpolationMethod::Step,
                snapshots: vec![BalanceSheetSnapshot {
                    date: NaiveDate::from_ymd_opt(2023, 1, 31).unwrap(),
                    value: 600_000.0,
                    source: None,
                }],
                is_balancing_account: false,
                noise_factor: 0.0,
            },
        ],
        income_statement: vec![
            IncomeStatementAccount {
                name: "Room Revenue".to_string(),
                account_type: AccountType::Revenue,
                seasonality_profile: SeasonalityProfileId::SummerHigh,
                constraints: vec![PeriodConstraint {
                    period: period_range(2023, 1, 2023, 12),
                    value: 1_800_000.0,
                    source: None,
                }],
                noise_factor: 0.06,
            },
            IncomeStatementAccount {
                name: "Food & Beverage Revenue".to_string(),
                account_type: AccountType::Revenue,
                seasonality_profile: SeasonalityProfileId::SummerHigh,
                constraints: vec![PeriodConstraint {
                    period: period_range(2023, 1, 2023, 12),
                    value: 600_000.0,
                    source: None,
                }],
                noise_factor: 0.07,
            },
            IncomeStatementAccount {
                name: "F&B Cost of Sales".to_string(),
                account_type: AccountType::CostOfSales,
                seasonality_profile: SeasonalityProfileId::SummerHigh,
                constraints: vec![PeriodConstraint {
                    period: period_range(2023, 1, 2023, 12),
                    value: 210_000.0,
                    source: None,
                }],
                noise_factor: 0.04,
            },
            IncomeStatementAccount {
                name: "Staff Wages".to_string(),
                account_type: AccountType::OperatingExpense,
                seasonality_profile: SeasonalityProfileId::SummerHigh,
                constraints: vec![PeriodConstraint {
                    period: period_range(2023, 1, 2023, 12),
                    value: 720_000.0,
                    source: None,
                }],
                noise_factor: 0.03,
            },
            IncomeStatementAccount {
                name: "Utilities".to_string(),
                account_type: AccountType::OperatingExpense,
                seasonality_profile: SeasonalityProfileId::SummerHigh,
                constraints: vec![PeriodConstraint {
                    period: period_range(2023, 1, 2023, 12),
                    value: 120_000.0,
                    source: None,
                }],
                noise_factor: 0.05,
            },
            IncomeStatementAccount {
                name: "Property Lease".to_string(),
                account_type: AccountType::OperatingExpense,
                seasonality_profile: SeasonalityProfileId::Flat,
                constraints: vec![PeriodConstraint {
                    period: period_range(2023, 1, 2023, 12),
                    value: 240_000.0,
                    source: None,
                }],
                noise_factor: 0.0,
            },
        ],
    };

    let dense = process_financial_history(&config).unwrap();

    export_to_csv(&dense, "test_hospitality_business.csv").unwrap();

    let verification = verify_accounting_equation(&config, &dense, 1.0);
    assert!(verification.is_ok());

    println!("✓ Hospitality business test passed - output: test_hospitality_business.csv");
}

#[test]
fn test_schema_generation() {
    let schema_json = FinancialHistoryConfig::schema_as_json().unwrap();

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
    let config = FinancialHistoryConfig {
        organization_name: "Tech Startup Inc".to_string(),
        fiscal_year_end_month: 12,
        balance_sheet: vec![
            BalanceSheetAccount {
                name: "Cash at Bank".to_string(),
                account_type: AccountType::Asset,
                method: InterpolationMethod::Linear,
                snapshots: vec![BalanceSheetSnapshot {
                    date: NaiveDate::from_ymd_opt(2023, 1, 31).unwrap(),
                    value: 100_000.0,
                    source: None,
                }],
                is_balancing_account: true,
                noise_factor: 0.0,
            },
            BalanceSheetAccount {
                name: "Accounts Receivable".to_string(),
                account_type: AccountType::Asset,
                method: InterpolationMethod::Linear,
                snapshots: vec![
                    BalanceSheetSnapshot {
                        date: NaiveDate::from_ymd_opt(2023, 1, 31).unwrap(),
                        value: 50_000.0,
                        source: None,
                    },
                    BalanceSheetSnapshot {
                        date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                        value: 75_000.0,
                        source: None,
                    },
                ],
                is_balancing_account: false,
                noise_factor: 0.02,
            },
            BalanceSheetAccount {
                name: "Accounts Payable".to_string(),
                account_type: AccountType::Liability,
                method: InterpolationMethod::Linear,
                snapshots: vec![
                    BalanceSheetSnapshot {
                        date: NaiveDate::from_ymd_opt(2023, 1, 31).unwrap(),
                        value: 30_000.0,
                        source: None,
                    },
                    BalanceSheetSnapshot {
                        date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                        value: 40_000.0,
                        source: None,
                    },
                ],
                is_balancing_account: false,
                noise_factor: 0.01,
            },
            BalanceSheetAccount {
                name: "Share Capital".to_string(),
                account_type: AccountType::Equity,
                method: InterpolationMethod::Step,
                snapshots: vec![
                    BalanceSheetSnapshot {
                        date: NaiveDate::from_ymd_opt(2023, 1, 31).unwrap(),
                        value: 100_000.0,
                        source: None,
                    },
                    BalanceSheetSnapshot {
                        date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                        value: 100_000.0,
                        source: None,
                    },
                ],
                is_balancing_account: false,
                noise_factor: 0.0,
            },
        ],
        income_statement: vec![],
    };

    let dense = process_financial_history(&config).unwrap();

    export_to_csv(&dense, "test_designated_balancing_account.csv").unwrap();

    assert!(dense.contains_key("Cash at Bank"));
    assert!(dense.contains_key("Accounts Receivable"));
    assert!(dense.contains_key("Accounts Payable"));
    assert!(dense.contains_key("Share Capital"));

    assert!(!dense.contains_key("Balancing Equity Adjustment"));

    let chart = ChartOfAccounts::from_dense_data(&config, &dense);

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
fn test_retained_earnings_integrity_check() {
    let config = FinancialHistoryConfig {
        organization_name: "Integrity Check Co".to_string(),
        fiscal_year_end_month: 12,
        balance_sheet: vec![
            BalanceSheetAccount {
                name: "Cash".to_string(),
                account_type: AccountType::Asset,
                method: InterpolationMethod::Linear,
                snapshots: vec![
                    BalanceSheetSnapshot {
                        date: NaiveDate::from_ymd_opt(2023, 1, 31).unwrap(),
                        value: 100_000.0,
                        source: None,
                    },
                    BalanceSheetSnapshot {
                        date: NaiveDate::from_ymd_opt(2023, 2, 28).unwrap(),
                        value: 100_000.0,
                        source: None,
                    },
                ],
                is_balancing_account: true,
                noise_factor: 0.0,
            },
            BalanceSheetAccount {
                name: "Retained Earnings".to_string(),
                account_type: AccountType::Equity,
                method: InterpolationMethod::Step,
                snapshots: vec![
                    BalanceSheetSnapshot {
                        date: NaiveDate::from_ymd_opt(2023, 1, 31).unwrap(),
                        value: 500_000.0,
                        source: None,
                    },
                    BalanceSheetSnapshot {
                        date: NaiveDate::from_ymd_opt(2023, 2, 28).unwrap(),
                        value: 500_000.0,
                        source: None,
                    },
                ],
                is_balancing_account: false,
                noise_factor: 0.0,
            },
        ],
        income_statement: vec![IncomeStatementAccount {
            name: "Revenue".to_string(),
            account_type: AccountType::Revenue,
            seasonality_profile: SeasonalityProfileId::Flat,
            constraints: vec![PeriodConstraint {
                period: period_range(2023, 2, 2023, 2),
                value: 100_000.0,
                source: None,
            }],
            noise_factor: 0.0,
        }],
    };

    let mut dense = process_config(&config).unwrap();
    let verification = enforce_accounting_equation(&config, &mut dense).unwrap();

    assert!(
        verification
            .warnings
            .iter()
            .any(|w| w.contains("Retained earnings")),
        "Expected retained earnings roll-forward warning"
    );
}

#[test]
fn test_hierarchical_constraints() {
    let config = FinancialHistoryConfig {
        organization_name: "Mixed Mode Corp".to_string(),
        fiscal_year_end_month: 12,
        balance_sheet: vec![BalanceSheetAccount {
            name: "Cash".to_string(),
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

    let jan = sales
        .get(&NaiveDate::from_ymd_opt(2023, 1, 31).unwrap())
        .unwrap()
        .value;
    assert!(
        (jan - 10_000.0).abs() < 0.01,
        "Jan should be $10k, got {}",
        jan
    );

    let feb = sales
        .get(&NaiveDate::from_ymd_opt(2023, 2, 28).unwrap())
        .unwrap()
        .value;
    assert_eq!(feb, 0.0, "Feb should be exactly $0");

    let mar = sales
        .get(&NaiveDate::from_ymd_opt(2023, 3, 31).unwrap())
        .unwrap()
        .value;
    assert!(
        (mar - 15_000.0).abs() < 0.01,
        "Mar should be $15k, got {}",
        mar
    );

    println!("✓ Hierarchical constraints test passed");
}

#[test]
fn test_quarterly_constraints() {
    let config = FinancialHistoryConfig {
        organization_name: "Quarterly Corp".to_string(),
        fiscal_year_end_month: 12,
        balance_sheet: vec![BalanceSheetAccount {
            name: "Cash".to_string(),
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
            name: "Sales A".to_string(),
            account_type: AccountType::Revenue,
            seasonality_profile: SeasonalityProfileId::Flat,
            constraints: vec![
                PeriodConstraint {
                    period: period_range(2023, 1, 2023, 6),
                    value: 50_000.0,
                    source: None,
                },
                PeriodConstraint {
                    period: period_range(2023, 7, 2023, 9),
                    value: 15_000.0,
                    source: None,
                },
            ],
            noise_factor: 0.0,
        }],
    };

    let dense = process_financial_history(&config).unwrap();
    let sales = dense.get("Sales A").unwrap();

    let jul = sales
        .get(&NaiveDate::from_ymd_opt(2023, 7, 31).unwrap())
        .unwrap()
        .value;
    let aug = sales
        .get(&NaiveDate::from_ymd_opt(2023, 8, 31).unwrap())
        .unwrap()
        .value;
    let sep = sales
        .get(&NaiveDate::from_ymd_opt(2023, 9, 30).unwrap())
        .unwrap()
        .value;

    assert!(
        (jul - 5000.0).abs() < 0.1,
        "Jul should be ~$5k, got {}",
        jul
    );
    assert!(
        (aug - 5000.0).abs() < 0.1,
        "Aug should be ~$5k, got {}",
        aug
    );
    assert!(
        (sep - 5000.0).abs() < 0.1,
        "Sep should be ~$5k, got {}",
        sep
    );

    println!("✓ Quarterly constraints test passed");
}
