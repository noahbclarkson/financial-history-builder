use crate::error::{FinancialHistoryError, Result};
use crate::schema::SeasonalityProfileId;

pub fn get_profile_weights(profile: &SeasonalityProfileId) -> Result<Vec<f64>> {
    let weights = match profile {
        SeasonalityProfileId::Flat => {
            vec![
                1.0 / 12.0,
                1.0 / 12.0,
                1.0 / 12.0,
                1.0 / 12.0,
                1.0 / 12.0,
                1.0 / 12.0,
                1.0 / 12.0,
                1.0 / 12.0,
                1.0 / 12.0,
                1.0 / 12.0,
                1.0 / 12.0,
                1.0 / 12.0,
            ]
        }

        SeasonalityProfileId::RetailPeak => {
            vec![
                0.045, 0.045, 0.045, 0.055, 0.055, 0.060, 0.065, 0.070, 0.075, 0.080, 0.105,
                0.300,
            ]
        }

        SeasonalityProfileId::SummerHigh => {
            vec![
                0.05, 0.05, 0.05, 0.12, 0.12, 0.12, 0.12, 0.12, 0.07, 0.07, 0.07, 0.04,
            ]
        }

        SeasonalityProfileId::SaasGrowth => {
            let mut weights = Vec::new();
            let base = 0.06;
            let increment = 0.04 / 11.0;
            for i in 0..12 {
                weights.push(base + (i as f64 * increment));
            }
            normalize_weights(&weights)
        }

        SeasonalityProfileId::Custom(ref custom_weights) => {
            validate_custom_weights(custom_weights)?;
            custom_weights.clone()
        }
    };

    Ok(weights)
}

fn validate_custom_weights(weights: &[f64]) -> Result<()> {
    if weights.len() != 12 {
        return Err(FinancialHistoryError::InvalidSeasonalityWeights(
            format!("Expected 12 weights, got {}", weights.len()),
        ));
    }

    if weights.iter().any(|&w| w < 0.0) {
        return Err(FinancialHistoryError::InvalidSeasonalityWeights(
            "All weights must be non-negative".to_string(),
        ));
    }

    let sum: f64 = weights.iter().sum();
    if (sum - 1.0).abs() > 0.01 {
        return Err(FinancialHistoryError::InvalidSeasonalityWeights(
            format!("Weights must sum to 1.0 (got {})", sum),
        ));
    }

    Ok(())
}

fn normalize_weights(weights: &[f64]) -> Vec<f64> {
    let sum: f64 = weights.iter().sum();
    if sum == 0.0 {
        return weights.to_vec();
    }
    weights.iter().map(|w| w / sum).collect()
}

pub fn rotate_weights_for_fiscal_year(
    weights: &[f64],
    fiscal_year_end_month: u32,
) -> Vec<f64> {
    if fiscal_year_end_month == 12 {
        return weights.to_vec();
    }

    let rotation = fiscal_year_end_month as usize;
    let mut rotated = Vec::with_capacity(12);

    for i in 0..12 {
        let idx = (i + rotation) % 12;
        rotated.push(weights[idx]);
    }

    rotated
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_flat_profile() {
        let weights = get_profile_weights(&SeasonalityProfileId::Flat).unwrap();
        assert_eq!(weights.len(), 12);
        let sum: f64 = weights.iter().sum();
        assert!((sum - 1.0).abs() < 1e-10);
        for w in weights {
            assert!((w - 1.0 / 12.0).abs() < 1e-10);
        }
    }

    #[test]
    fn test_retail_peak_profile() {
        let weights = get_profile_weights(&SeasonalityProfileId::RetailPeak).unwrap();
        assert_eq!(weights.len(), 12);
        let sum: f64 = weights.iter().sum();
        assert!((sum - 1.0).abs() < 1e-10);
        assert!(weights[11] > 0.25);
    }

    #[test]
    fn test_summer_high_profile() {
        let weights = get_profile_weights(&SeasonalityProfileId::SummerHigh).unwrap();
        assert_eq!(weights.len(), 12);
        let sum: f64 = weights.iter().sum();
        assert!((sum - 1.0).abs() < 1e-10);
        assert!(weights[3] > 0.1);
        assert!(weights[4] > 0.1);
    }

    #[test]
    fn test_saas_growth_profile() {
        let weights = get_profile_weights(&SeasonalityProfileId::SaasGrowth).unwrap();
        assert_eq!(weights.len(), 12);
        let sum: f64 = weights.iter().sum();
        assert!((sum - 1.0).abs() < 1e-6);
        assert!(weights[11] > weights[0]);
    }

    #[test]
    fn test_custom_valid() {
        let custom = vec![
            0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.05, 0.05, 0.05, 0.05, 0.05, 0.15,
        ];
        let weights = get_profile_weights(&SeasonalityProfileId::Custom(custom)).unwrap();
        assert_eq!(weights.len(), 12);
    }

    #[test]
    fn test_custom_invalid_length() {
        let custom = vec![0.5, 0.5];
        let result = get_profile_weights(&SeasonalityProfileId::Custom(custom));
        assert!(result.is_err());
    }

    #[test]
    fn test_custom_invalid_sum() {
        let custom = vec![0.1; 12];
        let result = get_profile_weights(&SeasonalityProfileId::Custom(custom));
        assert!(result.is_err());
    }

    #[test]
    fn test_rotate_weights() {
        let weights = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0];
        let rotated = rotate_weights_for_fiscal_year(&weights, 6);
        assert_eq!(rotated[0], 7.0);
        assert_eq!(rotated[6], 1.0);
        assert_eq!(rotated[11], 6.0);
    }

    #[test]
    fn test_rotate_weights_no_rotation() {
        let weights = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0];
        let rotated = rotate_weights_for_fiscal_year(&weights, 12);
        assert_eq!(rotated, weights);
    }
}
