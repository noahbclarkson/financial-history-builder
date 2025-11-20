use crate::error::Result;
use crate::schema::*;
use crate::seasonality::{get_profile_weights, rotate_weights_for_fiscal_year};
use crate::utils::get_month_ends_in_period;
use chrono::{Datelike, NaiveDate};
use rand::thread_rng;
use rand_distr::{Distribution, Normal};
use splines::{Interpolation, Key, Spline};
use std::collections::BTreeMap;

pub type DenseSeries = BTreeMap<NaiveDate, f64>;

pub struct Densifier {
    fiscal_year_end_month: u32,
}

struct MonthSlot {
    weight: f64,
    locked: bool,
    value: f64,
}

impl Densifier {
    pub fn new(fiscal_year_end_month: u32) -> Self {
        Self {
            fiscal_year_end_month,
        }
    }

    pub fn densify_balance_sheet(&self, account: &BalanceSheetAccount) -> Result<DenseSeries> {
        if account.snapshots.is_empty() {
            return Ok(BTreeMap::new());
        }

        let mut snapshots = account.snapshots.clone();
        snapshots.sort_by_key(|s| s.date);

        let interpolation = match account.method {
            InterpolationMethod::Step => Interpolation::Step(0.0),
            InterpolationMethod::Curve => Interpolation::CatmullRom,
            InterpolationMethod::Linear => Interpolation::Linear,
        };

        let keys: Vec<Key<f64, f64>> = snapshots
            .iter()
            .map(|s| {
                let t = s
                    .date
                    .and_hms_opt(0, 0, 0)
                    .unwrap()
                    .and_utc()
                    .timestamp() as f64;
                Key::new(t, s.value, interpolation)
            })
            .collect();

        let spline = Spline::from_vec(keys);

        let start = snapshots.first().unwrap().date;
        let end = snapshots.last().unwrap().date;
        let dates = get_month_ends_in_period(start, end);

        let mut series = BTreeMap::new();
        let mut rng = thread_rng();
        let noise_factor = account.noise_factor.unwrap_or(0.0);

        for date in dates {
            let t = date
                .and_hms_opt(0, 0, 0)
                .unwrap()
                .and_utc()
                .timestamp() as f64;

            let exact_match = snapshots.iter().find(|s| s.date == date);

            let value = if let Some(snap) = exact_match {
                snap.value
            } else {
                let mut val = spline.clamped_sample(t).unwrap_or(0.0);
                if noise_factor > 0.0 {
                    let normal = Normal::new(0.0, noise_factor).unwrap();
                    val *= 1.0 + normal.sample(&mut rng);
                }
                val
            };

            series.insert(date, value);
        }

        Ok(series)
    }

    pub fn densify_income_statement(
        &self,
        account: &IncomeStatementAccount,
    ) -> Result<DenseSeries> {
        if account.constraints.is_empty() {
            return Ok(BTreeMap::new());
        }

        let global_start = account
            .constraints
            .iter()
            .map(|c| c.start_date)
            .min()
            .unwrap();
        let global_end = account.constraints.iter().map(|c| c.end_date).max().unwrap();

        let all_dates = get_month_ends_in_period(global_start, global_end);

        let calendar_weights = self.get_calendar_weights(&account.seasonality_profile)?;

        let mut grid: BTreeMap<NaiveDate, MonthSlot> = BTreeMap::new();
        for date in &all_dates {
            let month_idx = date.month0() as usize;
            grid.insert(
                *date,
                MonthSlot {
                    weight: calendar_weights[month_idx],
                    locked: false,
                    value: 0.0,
                },
            );
        }

        let mut constraints = account.constraints.clone();
        constraints.sort_by_key(|c| (c.end_date - c.start_date).num_days());

        let mut rng = thread_rng();
        let noise = account.noise_factor.unwrap_or(0.0);

        for constraint in constraints {
            let constraint_dates = get_month_ends_in_period(constraint.start_date, constraint.end_date);

            let valid_dates: Vec<NaiveDate> = constraint_dates
                .into_iter()
                .filter(|d| grid.contains_key(d))
                .collect();

            if valid_dates.is_empty() {
                continue;
            }

            let locked_sum: f64 = valid_dates
                .iter()
                .filter(|d| grid.get(d).unwrap().locked)
                .map(|d| grid.get(d).unwrap().value)
                .sum();

            let remaining_value = constraint.value - locked_sum;

            let unlocked_dates: Vec<NaiveDate> = valid_dates
                .into_iter()
                .filter(|d| !grid.get(d).unwrap().locked)
                .collect();

            if unlocked_dates.is_empty() {
                continue;
            }

            let total_weight: f64 = unlocked_dates
                .iter()
                .map(|d| grid.get(d).unwrap().weight)
                .sum();

            let mut allocations = Vec::new();
            let mut raw_alloc_sum = 0.0;

            for date in &unlocked_dates {
                let slot = grid.get(date).unwrap();
                let relative_weight = if total_weight == 0.0 {
                    1.0 / unlocked_dates.len() as f64
                } else {
                    slot.weight / total_weight
                };

                let base_alloc = remaining_value * relative_weight;

                let val = if noise > 0.0 {
                    let normal = Normal::new(0.0, noise).unwrap();
                    base_alloc * (1.0 + normal.sample(&mut rng))
                } else {
                    base_alloc
                };

                allocations.push(val);
                raw_alloc_sum += val;
            }

            let correction = if raw_alloc_sum != 0.0 {
                remaining_value / raw_alloc_sum
            } else {
                0.0
            };

            for (i, date) in unlocked_dates.iter().enumerate() {
                let final_val = allocations[i] * correction;
                if let Some(slot) = grid.get_mut(date) {
                    slot.value = final_val;
                    slot.locked = true;
                }
            }
        }

        let result: DenseSeries = grid.into_iter().map(|(k, v)| (k, v.value)).collect();
        Ok(result)
    }

    fn get_calendar_weights(&self, profile: &SeasonalityProfileId) -> Result<Vec<f64>> {
        let base_weights = get_profile_weights(profile)?;
        let fy_weights = rotate_weights_for_fiscal_year(&base_weights, self.fiscal_year_end_month);
        Ok(self.align_weights_to_calendar(&fy_weights))
    }

    fn align_weights_to_calendar(&self, fy_weights: &[f64]) -> Vec<f64> {
        let mut calendar = vec![0.0; 12];
        let fy_start_month = if self.fiscal_year_end_month == 12 {
            1
        } else {
            self.fiscal_year_end_month + 1
        };

        for (fy_idx, &weight) in fy_weights.iter().enumerate() {
            let cal_idx = (fy_start_month as usize - 1 + fy_idx) % 12;
            calendar[cal_idx] = weight;
        }
        calendar
    }
}

pub fn process_config(
    config: &FinancialHistoryConfig,
) -> Result<BTreeMap<String, DenseSeries>> {
    let densifier = Densifier::new(config.fiscal_year_end_month);
    let mut data = BTreeMap::new();

    for account in &config.balance_sheet {
        let series = densifier.densify_balance_sheet(account)?;
        data.insert(account.name.clone(), series);
    }

    for account in &config.income_statement {
        let series = densifier.densify_income_statement(account)?;
        data.insert(account.name.clone(), series);
    }

    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hierarchical_constraint_solving() {
        let account = IncomeStatementAccount {
            name: "Revenue".to_string(),
            account_type: AccountType::Revenue,
            seasonality_profile: SeasonalityProfileId::Flat,
            constraints: vec![
                PeriodConstraint {
                    start_date: NaiveDate::from_ymd_opt(2023, 2, 1).unwrap(),
                    end_date: NaiveDate::from_ymd_opt(2023, 2, 28).unwrap(),
                    value: 5000.0,
                },
                PeriodConstraint {
                    start_date: NaiveDate::from_ymd_opt(2023, 1, 1).unwrap(),
                    end_date: NaiveDate::from_ymd_opt(2023, 3, 31).unwrap(),
                    value: 13000.0,
                },
                PeriodConstraint {
                    start_date: NaiveDate::from_ymd_opt(2023, 1, 1).unwrap(),
                    end_date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                    value: 50000.0,
                },
            ],
            noise_factor: None,
        };

        let densifier = Densifier::new(12);
        let series = densifier.densify_income_statement(&account).unwrap();

        let feb_val = series.get(&NaiveDate::from_ymd_opt(2023, 2, 28).unwrap()).unwrap();
        assert!((feb_val - 5000.0).abs() < 0.01, "Feb should be exactly 5000");

        let jan_val = series.get(&NaiveDate::from_ymd_opt(2023, 1, 31).unwrap()).unwrap();
        let mar_val = series.get(&NaiveDate::from_ymd_opt(2023, 3, 31).unwrap()).unwrap();
        let q1_sum = jan_val + feb_val + mar_val;
        assert!(
            (q1_sum - 13000.0).abs() < 0.01,
            "Q1 should sum to 13000, got {}",
            q1_sum
        );

        let year_sum: f64 = series.values().sum();
        assert!(
            (year_sum - 50000.0).abs() < 0.01,
            "Year should sum to 50000, got {}",
            year_sum
        );

        let apr_dec_sum: f64 = series
            .iter()
            .filter(|(date, _)| date.month() >= 4)
            .map(|(_, val)| val)
            .sum();
        let expected_apr_dec = 50000.0 - 13000.0;
        assert!(
            (apr_dec_sum - expected_apr_dec).abs() < 0.01,
            "Apr-Dec should be 37000, got {}",
            apr_dec_sum
        );
    }

    #[test]
    fn test_balance_sheet_interpolation() {
        let account = BalanceSheetAccount {
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
                    value: 200000.0,
                },
            ],
            is_balancing_account: false,
            noise_factor: None,
        };

        let densifier = Densifier::new(12);
        let series = densifier.densify_balance_sheet(&account).unwrap();

        assert_eq!(series.len(), 12);

        let first = series
            .get(&NaiveDate::from_ymd_opt(2023, 1, 31).unwrap())
            .unwrap();
        assert!((first - 100000.0).abs() < 0.01);

        let last = series
            .get(&NaiveDate::from_ymd_opt(2023, 12, 31).unwrap())
            .unwrap();
        assert!((last - 200000.0).abs() < 0.01);
    }

    #[test]
    fn test_process_config() {
        let config = FinancialHistoryConfig {
            organization_name: "Test Corp".to_string(),
            fiscal_year_end_month: 12,
            balance_sheet: vec![BalanceSheetAccount {
                name: "Cash".to_string(),
                account_type: AccountType::Asset,
                method: InterpolationMethod::Linear,
                snapshots: vec![
                    BalanceSheetSnapshot {
                        date: NaiveDate::from_ymd_opt(2023, 1, 31).unwrap(),
                        value: 50000.0,
                    },
                    BalanceSheetSnapshot {
                        date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                        value: 75000.0,
                    },
                ],
                is_balancing_account: true,
                noise_factor: None,
            }],
            income_statement: vec![IncomeStatementAccount {
                name: "Revenue".to_string(),
                account_type: AccountType::Revenue,
                seasonality_profile: SeasonalityProfileId::Flat,
                constraints: vec![PeriodConstraint {
                    start_date: NaiveDate::from_ymd_opt(2023, 1, 1).unwrap(),
                    end_date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                    value: 120000.0,
                }],
                noise_factor: None,
            }],
        };

        let result = process_config(&config).unwrap();

        assert!(result.contains_key("Cash"));
        assert!(result.contains_key("Revenue"));

        let revenue_sum: f64 = result.get("Revenue").unwrap().values().sum();
        assert!((revenue_sum - 120000.0).abs() < 0.01);
    }
}
