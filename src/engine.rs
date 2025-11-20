use crate::error::{FinancialHistoryError, Result};
use crate::schema::*;
use crate::seasonality::{get_profile_weights, rotate_weights_for_fiscal_year};
use crate::utils::{fiscal_year_start, get_month_ends_in_period, next_month_end};
use chrono::{Datelike, NaiveDate};
use rand::thread_rng;
use rand_distr::{Distribution, Normal};
use splines::{Interpolation, Key, Spline};
use std::collections::BTreeMap;

pub type DenseSeries = BTreeMap<NaiveDate, f64>;

pub struct Densifier {
    fiscal_year_end_month: u32,
}

impl Densifier {
    pub fn new(fiscal_year_end_month: u32) -> Self {
        Self {
            fiscal_year_end_month,
        }
    }

    pub fn densify(&self, account: &SparseAccount) -> Result<DenseSeries> {
        if account.anchors.is_empty() {
            return Err(FinancialHistoryError::NoAnchors(account.name.clone()));
        }

        if let Some(noise) = account.noise_factor {
            if !(0.0..=1.0).contains(&noise) {
                return Err(FinancialHistoryError::InvalidNoiseFactor(noise));
            }
        }

        let mut anchors = account.anchors.clone();
        anchors.sort_by_key(|a| a.date);

        match account.behavior {
            AccountBehavior::Flow => self.densify_flow(account, &anchors),
            AccountBehavior::Stock => self.densify_stock(account, &anchors),
        }
    }

    fn densify_flow(&self, account: &SparseAccount, anchors: &[AnchorPoint]) -> Result<DenseSeries> {
        let mut series = BTreeMap::new();
        let noise_factor = account.noise_factor.unwrap_or(0.0);

        // Group anchors by fiscal year to detect intra-year data
        let mut anchors_by_fy: std::collections::HashMap<i32, Vec<&AnchorPoint>> = std::collections::HashMap::new();

        for anchor in anchors {
            let fy_end = crate::utils::get_fiscal_year_end_for_date(anchor.date, self.fiscal_year_end_month);
            anchors_by_fy.entry(fy_end.year()).or_insert_with(Vec::new).push(anchor);
        }

        // Process each fiscal year
        for (_fy_year, mut fy_anchors) in anchors_by_fy {
            // Sort anchors chronologically within the fiscal year
            fy_anchors.sort_by_key(|a| a.date);

            // If only one anchor
            if fy_anchors.len() == 1 {
                let anchor = fy_anchors[0];
                let period_end = anchor.date;

                let (period_start, period_value) = match anchor.anchor_type {
                    AnchorType::Cumulative => {
                        // Cumulative: distribute across the entire FY up to this date
                        let fy_end = crate::utils::get_fiscal_year_end_for_date(period_end, self.fiscal_year_end_month);
                        (fiscal_year_start(fy_end), anchor.value)
                    }
                    AnchorType::Period => {
                        // Period: value is for just this month
                        (period_end, anchor.value)
                    }
                };

                let month_ends = get_month_ends_in_period(period_start, period_end);
                let num_months = month_ends.len();

                let monthly_values = if num_months > 0 {
                    match &account.interpolation {
                        InterpolationMethod::Seasonal { profile_id } => {
                            if num_months == 12 {
                                // Full year - use standard distribution
                                self.distribute_with_seasonality(
                                    period_value,
                                    profile_id,
                                    noise_factor,
                                )?
                            } else {
                                // Partial year - use period distribution
                                self.distribute_period_with_seasonality(
                                    period_value,
                                    profile_id,
                                    noise_factor,
                                    &month_ends,
                                )?
                            }
                        }
                        InterpolationMethod::Linear => {
                            if num_months == 12 {
                                self.distribute_linear(period_value, noise_factor)
                            } else {
                                self.distribute_step_for_months(period_value, noise_factor, num_months)
                            }
                        }
                        InterpolationMethod::Step | InterpolationMethod::Curve => {
                            if num_months == 12 {
                                self.distribute_step(period_value, noise_factor)
                            } else {
                                self.distribute_step_for_months(period_value, noise_factor, num_months)
                            }
                        }
                    }
                } else {
                    vec![]
                };

                for (i, &date) in month_ends.iter().enumerate() {
                    if i < monthly_values.len() {
                        series.insert(date, monthly_values[i]);
                    }
                }
            } else {
                // Multiple anchors in same FY - treat as cumulative YTD values
                let last_anchor_date = fy_anchors.last().unwrap().date;
                let fy_end = crate::utils::get_fiscal_year_end_for_date(last_anchor_date, self.fiscal_year_end_month);
                let fy_start = fiscal_year_start(fy_end);

                let mut prev_cumulative = 0.0;
                let mut prev_date_opt: Option<NaiveDate> = None;

                for anchor in fy_anchors {
                    // Handle AnchorType to calculate the period value
                    let period_value = match anchor.anchor_type {
                        AnchorType::Cumulative => anchor.value - prev_cumulative,
                        AnchorType::Period => anchor.value,
                    };

                    // Update running cumulative total for next iteration
                    match anchor.anchor_type {
                        AnchorType::Cumulative => prev_cumulative = anchor.value,
                        AnchorType::Period => prev_cumulative += anchor.value,
                    }

                    let period_start = match prev_date_opt {
                        None => fy_start,  // First anchor defaults to FY start
                        Some(prev_date) => next_month_end(prev_date),  // Subsequent: start after previous anchor
                    };
                    let period_end = anchor.date;

                    // Get months in this sub-period
                    let month_ends_full = get_month_ends_in_period(period_start, period_end);

                    // Decide which months to fill:
                    // - For the very first Period anchor, scope only to its own month to avoid backfilling earlier months.
                    // - For other anchors, prefer months that are not already populated; if all are filled, fall back to the full span.
                    let target_months: Vec<NaiveDate> = if anchor.anchor_type == AnchorType::Period && prev_date_opt.is_none() {
                        vec![period_end]
                    } else {
                        let unfilled: Vec<NaiveDate> = month_ends_full
                            .iter()
                            .copied()
                            .filter(|d| !series.contains_key(d))
                            .collect();
                        if unfilled.is_empty() {
                            month_ends_full.clone()
                        } else {
                            unfilled
                        }
                    };

                    let num_months = target_months.len();

                    if num_months > 0 {
                        // Distribute period value across months
                        let monthly_values = match &account.interpolation {
                            InterpolationMethod::Seasonal { profile_id } => {
                                // Use improved sub-period seasonality distribution
                                self.distribute_period_with_seasonality(
                                    period_value,
                                    profile_id,
                                    noise_factor,
                                    &target_months,
                                )?
                            }
                            InterpolationMethod::Step => {
                                // Even distribution for Step
                                self.distribute_step_for_months(period_value, noise_factor, num_months)
                            }
                            InterpolationMethod::Linear | InterpolationMethod::Curve => {
                                // Even distribution for Linear
                                self.distribute_step_for_months(period_value, noise_factor, num_months)
                            }
                        };

                        for (i, &date) in target_months.iter().enumerate() {
                            if i < monthly_values.len() {
                                series.insert(date, monthly_values[i]);
                            }
                        }
                    }

                    prev_date_opt = Some(anchor.date);
                }
            }
        }

        Ok(series)
    }

    // Helper for distributing across a specific number of months (not full year)
    fn distribute_step_for_months(&self, total: f64, noise_factor: f64, num_months: usize) -> Vec<f64> {
        let mut rng = thread_rng();
        let monthly_base = total / num_months as f64;
        let mut values = Vec::with_capacity(num_months);
        let mut raw_sum = 0.0;

        for _ in 0..num_months {
            let noise = if noise_factor > 0.0 {
                let normal = Normal::new(0.0, noise_factor).unwrap();
                let pct = normal.sample(&mut rng);
                monthly_base * pct
            } else {
                0.0
            };

            let value = monthly_base + noise;
            values.push(value);
            raw_sum += value;
        }

        let correction_ratio = if raw_sum != 0.0 {
            total / raw_sum
        } else {
            1.0
        };

        values.iter().map(|v| v * correction_ratio).collect()
    }

    // Helper for seasonal distribution across partial year
    fn distribute_period_with_seasonality(
        &self,
        total: f64,
        profile_id: &SeasonalityProfileId,
        noise_factor: f64,
        dates: &[NaiveDate],
    ) -> Result<Vec<f64>> {
        // Get full year weights
        let base_weights = get_profile_weights(profile_id)?;

        // Map each date to its fiscal month index and extract corresponding weights
        let mut period_weights = Vec::new();
        let mut weight_sum = 0.0;

        for date in dates {
            // Calculate which month of the fiscal year this date represents (0-11)
            let fy_month_index = crate::utils::get_fiscal_month_index(date.month(), self.fiscal_year_end_month);
            let w = base_weights[fy_month_index];
            period_weights.push(w);
            weight_sum += w;
        }

        // Normalize weights for this specific sub-period
        if weight_sum == 0.0 {
            // Fallback to even distribution if weights define 0 (unlikely)
            return Ok(self.distribute_step_for_months(total, noise_factor, dates.len()));
        }

        let mut rng = thread_rng();
        let mut values = Vec::with_capacity(dates.len());
        let mut raw_values_sum = 0.0;

        for w in period_weights {
            let normalized_weight = w / weight_sum;
            let base_val = total * normalized_weight;

            // Apply noise
            let val = if noise_factor > 0.0 {
                let normal = Normal::new(0.0, noise_factor).unwrap();
                base_val * (1.0 + normal.sample(&mut rng))
            } else {
                base_val
            };

            values.push(val);
            raw_values_sum += val;
        }

        // Mathematical correction to ensure sum equals exactly `total`
        let correction = if raw_values_sum != 0.0 {
            total / raw_values_sum
        } else {
            1.0
        };

        let corrected_values: Vec<f64> = values.iter().map(|v| v * correction).collect();

        Ok(corrected_values)
    }

    fn distribute_with_seasonality(
        &self,
        total: f64,
        profile_id: &SeasonalityProfileId,
        noise_factor: f64,
    ) -> Result<Vec<f64>> {
        let base_weights = get_profile_weights(profile_id)?;
        let weights = rotate_weights_for_fiscal_year(&base_weights, self.fiscal_year_end_month);

        let mut rng = thread_rng();
        let mut monthly_values = Vec::with_capacity(12);
        let mut raw_sum = 0.0;

        for &weight in &weights {
            let base_value = total * weight;

            let noise = if noise_factor > 0.0 {
                let normal = Normal::new(0.0, noise_factor).unwrap();
                let pct = normal.sample(&mut rng);
                base_value * pct
            } else {
                0.0
            };

            let value = base_value + noise;
            monthly_values.push(value);
            raw_sum += value;
        }

        let correction_ratio = if raw_sum != 0.0 {
            total / raw_sum
        } else {
            1.0
        };

        let corrected: Vec<f64> = monthly_values
            .iter()
            .map(|v| v * correction_ratio)
            .collect();

        Ok(corrected)
    }

    fn distribute_step(&self, total: f64, noise_factor: f64) -> Vec<f64> {
        let mut rng = thread_rng();
        let monthly_base = total / 12.0;
        let mut values = Vec::with_capacity(12);
        let mut raw_sum = 0.0;

        for _ in 0..12 {
            let noise = if noise_factor > 0.0 {
                let normal = Normal::new(0.0, noise_factor).unwrap();
                let pct = normal.sample(&mut rng);
                monthly_base * pct
            } else {
                0.0
            };

            let value = monthly_base + noise;
            values.push(value);
            raw_sum += value;
        }

        let correction_ratio = if raw_sum != 0.0 {
            total / raw_sum
        } else {
            1.0
        };

        values.iter().map(|v| v * correction_ratio).collect()
    }

    fn distribute_linear(&self, total: f64, noise_factor: f64) -> Vec<f64> {
        self.distribute_step(total, noise_factor)
    }

    fn densify_stock(&self, account: &SparseAccount, anchors: &[AnchorPoint]) -> Result<DenseSeries> {
        let mut series = BTreeMap::new();

        if anchors.is_empty() {
            return Ok(series);
        }

        let interpolation_type = match account.interpolation {
            InterpolationMethod::Step => Interpolation::Step(0.0),
            InterpolationMethod::Curve => Interpolation::CatmullRom,
            InterpolationMethod::Seasonal { .. } => Interpolation::Linear,
            InterpolationMethod::Linear => Interpolation::Linear,
        };

        let keys: Vec<Key<f64, f64>> = anchors
            .iter()
            .map(|a| {
                let t = a.date.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp() as f64;
                Key::new(t, a.value, interpolation_type)
            })
            .collect();

        let spline = Spline::from_vec(keys);

        let start_date = anchors.first().unwrap().date;
        let end_date = anchors.last().unwrap().date;

        let month_ends = get_month_ends_in_period(start_date, end_date);

        let noise_factor = account.noise_factor.unwrap_or(0.0);
        let mut rng = thread_rng();

        for date in month_ends {
            let t = date.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp() as f64;
            let mut val = spline.clamped_sample(t).unwrap_or(0.0);

            let is_anchor = anchors.iter().any(|a| a.date == date);

            if !is_anchor && noise_factor > 0.0 {
                let normal = Normal::new(0.0, noise_factor).unwrap();
                let noise_pct = normal.sample(&mut rng);
                val *= 1.0 + noise_pct;
            } else if is_anchor {
                val = anchors.iter().find(|a| a.date == date).unwrap().value;
            }

            series.insert(date, val);
        }

        Ok(series)
    }
}

pub fn densify_all_accounts(
    history: &SparseFinancialHistory,
) -> Result<BTreeMap<String, DenseSeries>> {
    let densifier = Densifier::new(history.fiscal_year_end_month);
    let mut result = BTreeMap::new();

    for account in &history.accounts {
        let series = densifier.densify(account)?;
        result.insert(account.name.clone(), series);
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_densify_flow_flat() {
        let account = SparseAccount {
            name: "Test Revenue".to_string(),
            account_type: AccountType::Revenue,
            behavior: AccountBehavior::Flow,
            interpolation: InterpolationMethod::Seasonal {
                profile_id: SeasonalityProfileId::Flat,
            },
            noise_factor: None,
            anchors: vec![AnchorPoint {
                date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                value: 120000.0,
                anchor_type: AnchorType::Cumulative,
            }],
            is_balancing_account: false,
        };

        let densifier = Densifier::new(12);
        let series = densifier.densify(&account).unwrap();

        let sum: f64 = series.values().sum();
        assert!((sum - 120000.0).abs() < 0.01);

        assert_eq!(series.len(), 12);
    }

    #[test]
    fn test_densify_flow_with_noise() {
        let account = SparseAccount {
            name: "Test Revenue".to_string(),
            account_type: AccountType::Revenue,
            behavior: AccountBehavior::Flow,
            interpolation: InterpolationMethod::Seasonal {
                profile_id: SeasonalityProfileId::Flat,
            },
            noise_factor: Some(0.05),
            anchors: vec![AnchorPoint {
                date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                value: 120000.0,
                anchor_type: AnchorType::Cumulative,
            }],
            is_balancing_account: false,
        };

        let densifier = Densifier::new(12);
        let series = densifier.densify(&account).unwrap();

        let sum: f64 = series.values().sum();
        assert!((sum - 120000.0).abs() < 0.01);
    }

    #[test]
    fn test_densify_stock_linear() {
        let account = SparseAccount {
            name: "Test Asset".to_string(),
            account_type: AccountType::Asset,
            behavior: AccountBehavior::Stock,
            interpolation: InterpolationMethod::Linear,
            noise_factor: None,
            anchors: vec![
                AnchorPoint {
                    date: NaiveDate::from_ymd_opt(2023, 1, 31).unwrap(),
                    value: 10000.0,
                    anchor_type: AnchorType::Cumulative,
                },
                AnchorPoint {
                    date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                    value: 20000.0,
                    anchor_type: AnchorType::Cumulative,
                },
            ],
                    is_balancing_account: false,
                };

        let densifier = Densifier::new(12);
        let series = densifier.densify(&account).unwrap();

        assert_eq!(series.len(), 12);

        let first = series.get(&NaiveDate::from_ymd_opt(2023, 1, 31).unwrap()).unwrap();
        assert!((first - 10000.0).abs() < 0.01);

        let last = series.get(&NaiveDate::from_ymd_opt(2023, 12, 31).unwrap()).unwrap();
        assert!((last - 20000.0).abs() < 0.01);
    }

    #[test]
    fn test_no_anchors_error() {
        let account = SparseAccount {
            name: "Test".to_string(),
            account_type: AccountType::Revenue,
            behavior: AccountBehavior::Flow,
            interpolation: InterpolationMethod::Linear,
            noise_factor: None,
            anchors: vec![],
                    is_balancing_account: false,
                };

        let densifier = Densifier::new(12);
        let result = densifier.densify(&account);
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_noise_factor() {
        let account = SparseAccount {
            name: "Test".to_string(),
            account_type: AccountType::Revenue,
            behavior: AccountBehavior::Flow,
            interpolation: InterpolationMethod::Linear,
            noise_factor: Some(1.5),
            anchors: vec![AnchorPoint {
                date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                value: 100000.0,
                anchor_type: AnchorType::Cumulative,
            }],
                    is_balancing_account: false,
                };

        let densifier = Densifier::new(12);
        let result = densifier.densify(&account);
        assert!(result.is_err());
    }

    #[test]
    fn test_intra_year_ytd_cumulative() {
        let account = SparseAccount {
            name: "Salaries".to_string(),
            account_type: AccountType::OperatingExpense,
            behavior: AccountBehavior::Flow,
            interpolation: InterpolationMethod::Linear,
            noise_factor: None,
            anchors: vec![
                AnchorPoint {
                    date: NaiveDate::from_ymd_opt(2023, 6, 30).unwrap(),
                    value: 300000.0,
                    anchor_type: AnchorType::Cumulative,
                },
                AnchorPoint {
                    date: NaiveDate::from_ymd_opt(2023, 12, 31).unwrap(),
                    value: 600000.0,
                    anchor_type: AnchorType::Cumulative,
                },
            ],
            is_balancing_account: false,
        };

        let densifier = Densifier::new(12);
        let series = densifier.densify(&account).unwrap();

        assert_eq!(series.len(), 12);

        let jan_jun_sum: f64 = series
            .iter()
            .filter(|(date, _)| date.month() <= 6)
            .map(|(_, val)| val)
            .sum();

        assert!((jan_jun_sum - 300000.0).abs() < 1.0,
                "Jan-Jun should sum to 300k YTD, got {}", jan_jun_sum);

        let jul_dec_sum: f64 = series
            .iter()
            .filter(|(date, _)| date.month() > 6)
            .map(|(_, val)| val)
            .sum();

        assert!((jul_dec_sum - 300000.0).abs() < 1.0,
                "Jul-Dec should sum to 300k (600k - 300k), got {}", jul_dec_sum);

        let total_sum: f64 = series.values().sum();
        assert!((total_sum - 600000.0).abs() < 1.0,
                "Total should sum to 600k, got {}", total_sum);
    }
}
