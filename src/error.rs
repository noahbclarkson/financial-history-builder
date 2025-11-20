use thiserror::Error;

#[derive(Error, Debug)]
pub enum FinancialHistoryError {
    #[error("Invalid anchor point: {0}")]
    InvalidAnchor(String),

    #[error("No anchors provided for account: {0}")]
    NoAnchors(String),

    #[error("Invalid noise factor {0}: must be between 0.0 and 1.0")]
    InvalidNoiseFactor(f64),

    #[error("Invalid fiscal year end month {0}: must be between 1 and 12")]
    InvalidFiscalYearEndMonth(u32),

    #[error("Custom seasonality profile has invalid weights: {0}")]
    InvalidSeasonalityWeights(String),

    #[error("Accounting equation violation on {date}: Assets ({assets}) != Liabilities ({liabilities}) + Equity ({equity})")]
    AccountingEquationViolation {
        date: String,
        assets: f64,
        liabilities: f64,
        equity: f64,
    },

    #[error("Interpolation error: {0}")]
    InterpolationError(String),

    #[error("Date calculation error: {0}")]
    DateError(String),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, FinancialHistoryError>;
