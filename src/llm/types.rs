use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExtractionEvent {
    Starting,
    Uploading { filename: String },
    Step1Discovery,
    Step2Extraction,
    Step3Assembly,
    DraftingResponse,
    ProcessingResponse,
    Validating { attempt: usize },
    CorrectionNeeded { reason: String },
    Retry { attempt: usize, error: String },
    Success,
    Failed { reason: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MarkdownResponse {
    pub markdown: String,
}
