use crate::error::{FinancialHistoryError, Result};
use chrono::{Datelike, Days, NaiveDate};

pub fn next_month_end(date: NaiveDate) -> NaiveDate {
    let year = if date.month() == 12 {
        date.year() + 1
    } else {
        date.year()
    };

    let month = if date.month() == 12 {
        1
    } else {
        date.month() + 1
    };

    last_day_of_month(year, month)
}

pub fn prev_month_end(date: NaiveDate) -> NaiveDate {
    let year = if date.month() == 1 {
        date.year() - 1
    } else {
        date.year()
    };

    let month = if date.month() == 1 {
        12
    } else {
        date.month() - 1
    };

    last_day_of_month(year, month)
}

pub fn last_day_of_month(year: i32, month: u32) -> NaiveDate {
    let next_month = if month == 12 { 1 } else { month + 1 };
    let next_year = if month == 12 { year + 1 } else { year };

    NaiveDate::from_ymd_opt(next_year, next_month, 1)
        .unwrap()
        .checked_sub_days(Days::new(1))
        .unwrap()
}

pub fn fiscal_year_start(fiscal_year_end: NaiveDate) -> NaiveDate {
    let year = fiscal_year_end.year() - 1;
    let month = fiscal_year_end.month();

    let start_month = if month == 12 { 1 } else { month + 1 };
    let start_year = if month == 12 { year + 1 } else { year };

    last_day_of_month(start_year, start_month)
}

pub fn get_month_ends_in_period(start: NaiveDate, end: NaiveDate) -> Vec<NaiveDate> {
    let mut dates = Vec::new();

    let start_month_end = last_day_of_month(start.year(), start.month());

    let mut current = start_month_end;
    while current <= end {
        if current >= start {
            dates.push(current);
        }
        current = next_month_end(current);
    }

    dates
}

pub fn validate_fiscal_year_end_month(month: u32) -> Result<()> {
    if !(1..=12).contains(&month) {
        return Err(FinancialHistoryError::InvalidFiscalYearEndMonth(month));
    }
    Ok(())
}

pub fn get_fiscal_year_end_date(year: i32, fiscal_month: u32) -> NaiveDate {
    last_day_of_month(year, fiscal_month)
}

pub fn months_between(start: NaiveDate, end: NaiveDate) -> i32 {
    let year_diff = end.year() - start.year();
    let month_diff = end.month() as i32 - start.month() as i32;
    year_diff * 12 + month_diff
}

/// Get the fiscal year end date for a given date
/// Returns the fiscal year end date that this date belongs to
pub fn get_fiscal_year_end_for_date(date: NaiveDate, fiscal_month: u32) -> NaiveDate {
    let current_month = date.month();
    let current_year = date.year();

    // If we're past the fiscal year end month, the FY end is in the current year
    // Otherwise, it's in the next year
    if current_month <= fiscal_month {
        last_day_of_month(current_year, fiscal_month)
    } else {
        last_day_of_month(current_year + 1, fiscal_month)
    }
}

/// Returns the 0-based index of the month within the fiscal year.
/// This is used to map calendar months to seasonality weight indices.
///
/// # Examples
/// - If FY ends in Dec (12): Jan=0, Feb=1, ..., Dec=11
/// - If FY ends in June (6): July=0, Aug=1, ..., June=11
pub fn get_fiscal_month_index(calendar_month: u32, fiscal_year_end_month: u32) -> usize {
    let fy_start_month = if fiscal_year_end_month == 12 {
        1
    } else {
        fiscal_year_end_month + 1
    };

    if calendar_month >= fy_start_month {
        (calendar_month - fy_start_month) as usize
    } else {
        (calendar_month + 12 - fy_start_month) as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_next_month_end() {
        let date = NaiveDate::from_ymd_opt(2023, 1, 31).unwrap();
        let next = next_month_end(date);
        assert_eq!(next, NaiveDate::from_ymd_opt(2023, 2, 28).unwrap());

        let date = NaiveDate::from_ymd_opt(2023, 12, 31).unwrap();
        let next = next_month_end(date);
        assert_eq!(next, NaiveDate::from_ymd_opt(2024, 1, 31).unwrap());
    }

    #[test]
    fn test_prev_month_end() {
        let date = NaiveDate::from_ymd_opt(2023, 2, 28).unwrap();
        let prev = prev_month_end(date);
        assert_eq!(prev, NaiveDate::from_ymd_opt(2023, 1, 31).unwrap());

        let date = NaiveDate::from_ymd_opt(2023, 1, 31).unwrap();
        let prev = prev_month_end(date);
        assert_eq!(prev, NaiveDate::from_ymd_opt(2022, 12, 31).unwrap());
    }

    #[test]
    fn test_last_day_of_month() {
        assert_eq!(
            last_day_of_month(2023, 2),
            NaiveDate::from_ymd_opt(2023, 2, 28).unwrap()
        );
        assert_eq!(
            last_day_of_month(2024, 2),
            NaiveDate::from_ymd_opt(2024, 2, 29).unwrap()
        );
        assert_eq!(
            last_day_of_month(2023, 4),
            NaiveDate::from_ymd_opt(2023, 4, 30).unwrap()
        );
    }

    #[test]
    fn test_fiscal_year_start() {
        let fy_end = NaiveDate::from_ymd_opt(2023, 12, 31).unwrap();
        let fy_start = fiscal_year_start(fy_end);
        assert_eq!(fy_start, NaiveDate::from_ymd_opt(2023, 1, 31).unwrap());

        let fy_end = NaiveDate::from_ymd_opt(2023, 6, 30).unwrap();
        let fy_start = fiscal_year_start(fy_end);
        assert_eq!(fy_start, NaiveDate::from_ymd_opt(2022, 7, 31).unwrap());
    }

    #[test]
    fn test_fiscal_month_index() {
        // Standard calendar year (Ends Dec)
        assert_eq!(get_fiscal_month_index(1, 12), 0); // Jan
        assert_eq!(get_fiscal_month_index(6, 12), 5); // June
        assert_eq!(get_fiscal_month_index(12, 12), 11); // Dec

        // US Gov Year (Ends Sept / Month 9). Start Oct (10).
        assert_eq!(get_fiscal_month_index(10, 9), 0); // Oct is month 0
        assert_eq!(get_fiscal_month_index(1, 9), 3); // Jan is month 3
        assert_eq!(get_fiscal_month_index(9, 9), 11); // Sept is month 11

        // June year end (FY starts July)
        assert_eq!(get_fiscal_month_index(7, 6), 0); // July is month 0
        assert_eq!(get_fiscal_month_index(12, 6), 5); // Dec is month 5
        assert_eq!(get_fiscal_month_index(6, 6), 11); // June is month 11
    }

    #[test]
    fn test_parse_period_string_month_and_range() {
        let (start, end) = parse_period_string("2023-02").unwrap();
        assert_eq!(start, NaiveDate::from_ymd_opt(2023, 2, 1).unwrap());
        assert_eq!(end, NaiveDate::from_ymd_opt(2023, 2, 28).unwrap());

        let (start, end) = parse_period_string("2023-01:2023-03").unwrap();
        assert_eq!(start, NaiveDate::from_ymd_opt(2023, 1, 1).unwrap());
        assert_eq!(end, NaiveDate::from_ymd_opt(2023, 3, 31).unwrap());
    }
}

/// Parses a period string in the format "YYYY-MM" or "YYYY-MM:YYYY-MM"
/// Returns (start_date, end_date)
pub fn parse_period_string(period: &str) -> Result<(NaiveDate, NaiveDate)> {
    let parts: Vec<&str> = period.split(':').collect();

    match parts.len() {
        1 => {
            // Single month: "2023-01"
            let start_str = format!("{}-01", parts[0].trim());
            let start_date = NaiveDate::parse_from_str(&start_str, "%Y-%m-%d").map_err(|_| {
                FinancialHistoryError::DateError(format!(
                    "Invalid date format in period: {}. Expected YYYY-MM",
                    parts[0]
                ))
            })?;

            let end_date = last_day_of_month(start_date.year(), start_date.month());
            Ok((start_date, end_date))
        }
        2 => {
            // Range: "2023-01:2023-03"
            let start_str = format!("{}-01", parts[0].trim());
            let start_date = NaiveDate::parse_from_str(&start_str, "%Y-%m-%d").map_err(|_| {
                FinancialHistoryError::DateError(format!(
                    "Invalid start date format in period: {}. Expected YYYY-MM",
                    parts[0]
                ))
            })?;

            let end_str = format!("{}-01", parts[1].trim());
            let end_start_ref = NaiveDate::parse_from_str(&end_str, "%Y-%m-%d").map_err(|_| {
                FinancialHistoryError::DateError(format!(
                    "Invalid end date format in period: {}. Expected YYYY-MM",
                    parts[1]
                ))
            })?;

            let end_date = last_day_of_month(end_start_ref.year(), end_start_ref.month());
            Ok((start_date, end_date))
        }
        _ => Err(FinancialHistoryError::DateError(format!(
            "Invalid period format: {}. Expected 'YYYY-MM' or 'YYYY-MM:YYYY-MM'",
            period
        ))),
    }
}
