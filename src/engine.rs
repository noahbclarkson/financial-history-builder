use crate::error::Result;
use crate::schema::*;
use crate::seasonality::{get_profile_weights, rotate_weights_for_fiscal_year};
use crate::utils::get_month_ends_in_period;
use crate::{DataOrigin, DenseSeries, DerivationDetails, MonthlyDataPoint};
use chrono::{Datelike, NaiveDate};
use rand::thread_rng;
use rand_distr::{Distribution, Normal};
use splines::{Interpolation, Key, Spline};
use std::collections::BTreeMap;

pub struct Densifier {
    fiscal_year_end_month: u32,
}

// Internal struct to track state during solving
struct MonthSlot {
    weight: f64,
    locked: bool,
    value: f64,
    origin: DataOrigin,
    source: Option<SourceMetadata>,
    derivation_logic: String,
    original_period_info: Option<(f64, NaiveDate, NaiveDate)>,
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
                let t = s.date.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp() as f64;
                Key::new(t, s.value, interpolation)
            })
            .collect();

        let spline = Spline::from_vec(keys);

        let start = snapshots.first().unwrap().date;
        let end = snapshots.last().unwrap().date;
        let dates = get_month_ends_in_period(start, end);

        let mut series = BTreeMap::new();
        let mut rng = thread_rng();
        let noise_factor = account.noise_factor;

        for date in dates {
            let t = date.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp() as f64;

            let exact_match = snapshots.iter().find(|s| s.date == date);

            let (value, origin, source, derivation) = if let Some(snap) = exact_match {
                (
                    snap.value,
                    DataOrigin::Anchor,
                    snap.source.clone(),
                    DerivationDetails {
                        original_period_value: None,
                        period_start: None,
                        period_end: None,
                        logic: "Exact snapshot match from document".to_string(),
                    },
                )
            } else {
                let mut val = spline.clamped_sample(t).unwrap_or(0.0);
                if noise_factor > 0.0 {
                    let normal = Normal::new(0.0, noise_factor).unwrap();
                    val *= 1.0 + normal.sample(&mut rng);
                }
                (
                    val,
                    DataOrigin::Interpolated,
                    None,
                    DerivationDetails {
                        original_period_value: None,
                        period_start: None,
                        period_end: None,
                        logic: format!("Interpolated using {:?} method", account.method),
                    },
                )
            };

            series.insert(
                date,
                MonthlyDataPoint {
                    value,
                    origin,
                    source,
                    derivation,
                },
            );
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
        let global_end = account
            .constraints
            .iter()
            .map(|c| c.end_date)
            .max()
            .unwrap();

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
                    origin: DataOrigin::Interpolated,
                    source: None,
                    derivation_logic: "Implied zero (no coverage)".to_string(),
                    original_period_info: None,
                },
            );
        }

        let mut constraints = account.constraints.clone();
        constraints.sort_by_key(|c| (c.end_date - c.start_date).num_days());

        let mut rng = thread_rng();
        let noise = account.noise_factor;

        for constraint in constraints {
            let constraint_dates =
                get_month_ends_in_period(constraint.start_date, constraint.end_date);

            // Identify single-month constraints explicitly
            let is_single_month = constraint.start_date.year() == constraint.end_date.year()
                && constraint.start_date.month() == constraint.end_date.month();

            let valid_dates: Vec<NaiveDate> = constraint_dates
                .into_iter()
                .filter(|d| grid.contains_key(d))
                .collect();

            if valid_dates.is_empty() {
                continue;
            }

            // 1. Calculate what has already been filled by smaller constraints
            let locked_sum: f64 = valid_dates
                .iter()
                .filter(|d| grid.get(d).unwrap().locked)
                .map(|d| grid.get(d).unwrap().value)
                .sum();

            // 2. Determine what's left for this period
            let remaining_value = constraint.value - locked_sum;

            // 3. Identify months that still need values
            let unlocked_dates: Vec<NaiveDate> = valid_dates
                .into_iter()
                .filter(|d| !grid.get(d).unwrap().locked)
                .collect();

            if unlocked_dates.is_empty() {
                continue;
            }

            // 4. Distribute based on seasonality weights
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

                // Apply noise
                let val = if noise > 0.0 {
                    let normal = Normal::new(0.0, noise).unwrap();
                    base_alloc * (1.0 + normal.sample(&mut rng))
                } else {
                    base_alloc
                };

                allocations.push(val);
                raw_alloc_sum += val;
            }

            // Re-normalize to ensure sum matches constraint exactly
            let correction = if raw_alloc_sum != 0.0 {
                remaining_value / raw_alloc_sum
            } else {
                0.0
            };

            // 5. Update the Grid with Rich Metadata
            for (i, date) in unlocked_dates.iter().enumerate() {
                let final_val = allocations[i] * correction;

                if let Some(slot) = grid.get_mut(date) {
                    slot.value = final_val;
                    slot.locked = true;
                    slot.source = constraint.source.clone();

                    if is_single_month {
                        slot.origin = DataOrigin::Anchor;
                        slot.derivation_logic = "Direct monthly match".to_string();
                        slot.original_period_info = None; // It's not derived, it IS the value
                    } else {
                        slot.origin = DataOrigin::Allocated;
                        let period_type = if (constraint.end_date.ordinal()
                            - constraint.start_date.ordinal())
                            > 360
                        {
                            "Annual"
                        } else {
                            "Period"
                        };
                        slot.derivation_logic = format!(
                            "Allocated portion of {} total (Seasonality: {:?})",
                            period_type, account.seasonality_profile
                        );
                        slot.original_period_info =
                            Some((constraint.value, constraint.start_date, constraint.end_date));
                    }
                }
            }
        }

        let result: DenseSeries = grid
            .into_iter()
            .map(|(k, v)| {
                (
                    k,
                    MonthlyDataPoint {
                        value: v.value,
                        origin: v.origin,
                        source: v.source,
                        derivation: DerivationDetails {
                            original_period_value: v.original_period_info.map(|x| x.0),
                            period_start: v.original_period_info.map(|x| x.1),
                            period_end: v.original_period_info.map(|x| x.2),
                            logic: v.derivation_logic,
                        },
                    },
                )
            })
            .collect();
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

pub fn process_config(config: &FinancialHistoryConfig) -> Result<BTreeMap<String, DenseSeries>> {
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
                    source: None,
                },
                PeriodConstraint {
                    start_date: NaiveDate::from_ymd_opt(2023, 1, 1).unwrap(),
                    end_date: NaiveDate::from_ymd_opt(2023, 3, 31).unwrap(),
                    value: 13000.0,
                    source: None,
                },
                PeriodConstraint {
                    start_date: NaiveDate::from_ymd_opt(2023, 1, 1).unwrap(),
                    end_date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                    value: 50000.0,
                    source: None,
                },
            ],
            noise_factor: 0.0,
        };

        let densifier = Densifier::new(12);
        let series = densifier.densify_income_statement(&account).unwrap();

        let feb_val = series
            .get(&NaiveDate::from_ymd_opt(2023, 2, 28).unwrap())
            .unwrap()
            .value;
        assert!(
            (feb_val - 5000.0).abs() < 0.01,
            "Feb should be exactly 5000"
        );

        let jan_val = series
            .get(&NaiveDate::from_ymd_opt(2023, 1, 31).unwrap())
            .unwrap()
            .value;
        let mar_val = series
            .get(&NaiveDate::from_ymd_opt(2023, 3, 31).unwrap())
            .unwrap()
            .value;
        let q1_sum = jan_val + feb_val + mar_val;
        assert!(
            (q1_sum - 13000.0).abs() < 0.01,
            "Q1 should sum to 13000, got {}",
            q1_sum
        );

        let year_sum: f64 = series.values().map(|p| p.value).sum();
        assert!(
            (year_sum - 50000.0).abs() < 0.01,
            "Year should sum to 50000, got {}",
            year_sum
        );

        let apr_dec_sum: f64 = series
            .iter()
            .filter(|(date, _)| date.month() >= 4)
            .map(|(_, val)| val.value)
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
                    source: None,
                },
                BalanceSheetSnapshot {
                    date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                    value: 200000.0,
                    source: None,
                },
            ],
            is_balancing_account: false,
            noise_factor: 0.0,
        };

        let densifier = Densifier::new(12);
        let series = densifier.densify_balance_sheet(&account).unwrap();

        assert_eq!(series.len(), 12);

        let first = series
            .get(&NaiveDate::from_ymd_opt(2023, 1, 31).unwrap())
            .unwrap()
            .value;
        assert!((first - 100000.0).abs() < 0.01);

        let last = series
            .get(&NaiveDate::from_ymd_opt(2023, 12, 31).unwrap())
            .unwrap()
            .value;
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
                        source: None,
                    },
                    BalanceSheetSnapshot {
                        date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                        value: 75000.0,
                        source: None,
                    },
                ],
                is_balancing_account: true,
                noise_factor: 0.0,
            }],
            income_statement: vec![IncomeStatementAccount {
                name: "Revenue".to_string(),
                account_type: AccountType::Revenue,
                seasonality_profile: SeasonalityProfileId::Flat,
                constraints: vec![PeriodConstraint {
                    start_date: NaiveDate::from_ymd_opt(2023, 1, 1).unwrap(),
                    end_date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                    value: 120000.0,
                    source: None,
                }],
                noise_factor: 0.0,
            }],
        };

        let result = process_config(&config).unwrap();

        assert!(result.contains_key("Cash"));
        assert!(result.contains_key("Revenue"));

        let revenue_sum: f64 = result
            .get("Revenue")
            .unwrap()
            .values()
            .map(|p| p.value)
            .sum();
        assert!((revenue_sum - 120000.0).abs() < 0.01);
    }
}
