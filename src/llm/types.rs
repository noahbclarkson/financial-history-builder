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

#[derive(Serialize, Deserialize, Clone)]
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

#[derive(Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum Part {
    Text {
        text: String
    },
    FileData {
        #[serde(rename = "fileData")]
        file_data: FileData
    },
}

#[derive(Serialize, Deserialize, Clone)]
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

    // Explicitly rename to match the API documentation "responseJsonSchema"
    #[serde(rename = "responseJsonSchema")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_schema: Option<serde_json::Value>,
}

#[derive(Deserialize)]
pub(crate) struct GenerateContentResponse {
    pub candidates: Option<Vec<Candidate>>,
}

#[derive(Deserialize)]
pub(crate) struct Candidate {
    pub content: Content,
}

#[derive(Debug, Clone)]
pub enum ExtractionEvent {
    Starting,
    Uploading { filename: String },
    DraftingResponse,
    ProcessingResponse,
    Validating { attempt: usize },
    CorrectionNeeded { reason: String },
    Patching { attempt: usize },
    Success,
    Failed { reason: String },
}
