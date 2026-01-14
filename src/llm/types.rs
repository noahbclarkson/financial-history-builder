use rstructor::{Instructor, MediaFile};
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

#[derive(Debug, Clone, Serialize, Deserialize, Instructor)]
pub struct MarkdownResponse {
    pub markdown: String,
}

#[derive(Debug, Clone)]
pub struct DocumentReference {
    pub media: MediaFile,
    pub display_name: String,
}

impl DocumentReference {
    #[must_use]
    pub fn new(media: MediaFile, display_name: impl Into<String>) -> Self {
        Self {
            media,
            display_name: display_name.into(),
        }
    }
}
