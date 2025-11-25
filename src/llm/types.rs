use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteDocument {
    pub uri: String,
    pub name: String,
    pub display_name: String,
    pub mime_type: String,
    pub state: String,
}

impl RemoteDocument {
    pub fn is_active(&self) -> bool {
        self.state == "ACTIVE"
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelMetadata {
    pub name: String,
    pub display_name: Option<String>,
    pub output_token_limit: u32,
}

// --- Content Structures ---

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Content {
    pub role: String,
    pub parts: Vec<Part>,
}

impl Content {
    pub fn user(text: String) -> Self {
        Self {
            role: "user".to_string(),
            parts: vec![Part::Text { text }],
        }
    }

    pub fn model(text: String) -> Self {
        Self {
            role: "model".to_string(),
            parts: vec![Part::Text { text }],
        }
    }

    pub fn user_with_files(text: String, files: &[RemoteDocument]) -> Self {
        let mut parts: Vec<Part> = files
            .iter()
            .map(|doc| Part::FileData {
                file_data: FileData {
                    mime_type: doc.mime_type.clone(),
                    file_uri: doc.uri.clone(),
                },
            })
            .collect();

        parts.push(Part::Text { text });

        Self {
            role: "user".to_string(),
            parts,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(untagged)]
pub enum Part {
    Text {
        text: String,
    },
    FileData {
        #[serde(rename = "fileData")]
        file_data: FileData,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct FileData {
    pub mime_type: String,
    pub file_uri: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GenerateContentRequest {
    pub contents: Vec<Content>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_instruction: Option<Content>,
    pub generation_config: GenerationConfig,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GenerationConfig {
    pub response_mime_type: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_schema: Option<serde_json::Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u32>,
}

// --- API Response Structures ---

#[derive(Deserialize, Debug)]
pub(crate) struct GenerateContentResponse {
    pub candidates: Option<Vec<Candidate>>,
    #[serde(rename = "promptFeedback")]
    pub prompt_feedback: Option<PromptFeedback>,
}

#[derive(Deserialize, Debug)]
pub(crate) struct PromptFeedback {
    #[serde(rename = "blockReason")]
    pub block_reason: Option<String>,
}

#[derive(Deserialize, Debug)]
pub(crate) struct Candidate {
    // Optional because safety filters can return a candidate with no content
    pub content: Option<Content>,
    #[serde(rename = "finishReason")]
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone)]
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
