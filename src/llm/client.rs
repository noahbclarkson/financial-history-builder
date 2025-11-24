use crate::error::{FinancialHistoryError, Result};
use crate::llm::types::*;
use reqwest::Client;
use serde_json::json;
use std::path::Path;
use std::time::Duration;
use tokio::fs;
use tokio::time::sleep;

const GEMINI_BASE_URL: &str = "https://generativelanguage.googleapis.com/v1beta";
const GEMINI_UPLOAD_URL: &str = "https://generativelanguage.googleapis.com/upload/v1beta/files";

#[derive(Clone)]
pub struct GeminiClient {
    client: Client,
    api_key: String,
    base_url: String,
}

impl GeminiClient {
    pub fn new(api_key: String) -> Self {
        Self {
            client: Client::new(),
            api_key,
            base_url: GEMINI_BASE_URL.to_string(),
        }
    }

    pub async fn upload_document(&self, path: &Path) -> Result<RemoteDocument> {
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| {
                FinancialHistoryError::ExtractionFailed("Invalid file name".to_string())
            })?;

        let file_size = fs::metadata(path).await?.len();
        let mime_type = mime_guess::from_path(path)
            .first_or_octet_stream()
            .to_string();
        let file_bytes = fs::read(path).await?;

        let start_url = format!("{}?key={}", GEMINI_UPLOAD_URL, self.api_key);
        let metadata = json!({ "file": { "display_name": file_name } });

        let init_res = self
            .client
            .post(&start_url)
            .header("X-Goog-Upload-Protocol", "resumable")
            .header("X-Goog-Upload-Command", "start")
            .header("X-Goog-Upload-Header-Content-Length", file_size.to_string())
            .header("X-Goog-Upload-Header-Content-Type", &mime_type)
            .header("Content-Type", "application/json")
            .json(&metadata)
            .send()
            .await?;

        let init_status = init_res.status();
        if !init_status.is_success() {
            let error_text = init_res.text().await?;
            return Err(FinancialHistoryError::ExtractionFailed(format!(
                "Upload init failed (status {}): {}",
                init_status, error_text
            )));
        }

        let upload_url = init_res
            .headers()
            .get("x-goog-upload-url")
            .ok_or_else(|| {
                FinancialHistoryError::ExtractionFailed("No upload URL in headers".to_string())
            })?
            .to_str()
            .map_err(|e| FinancialHistoryError::ExtractionFailed(e.to_string()))?
            .to_string();

        let upload_res = self
            .client
            .post(&upload_url)
            .header("Content-Length", file_size.to_string())
            .header("X-Goog-Upload-Offset", "0")
            .header("X-Goog-Upload-Command", "upload, finalize")
            .body(file_bytes)
            .send()
            .await?;

        let upload_status = upload_res.status();
        if !upload_status.is_success() {
            let error_text = upload_res.text().await?;
            return Err(FinancialHistoryError::ExtractionFailed(format!(
                "File upload failed (status {}): {}",
                upload_status, error_text
            )));
        }

        let upload_body: serde_json::Value = upload_res.json().await?;
        let file_obj = upload_body.get("file").ok_or_else(|| {
            FinancialHistoryError::ExtractionFailed("Upload response missing 'file'".to_string())
        })?;

        let uri = file_obj
            .get("uri")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                FinancialHistoryError::ExtractionFailed("Upload response missing uri".to_string())
            })?
            .to_string();

        let name = file_obj
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                FinancialHistoryError::ExtractionFailed("Upload response missing name".to_string())
            })?
            .to_string();

        let mut state = file_obj
            .get("state")
            .and_then(|v| v.as_str())
            .unwrap_or("PROCESSING")
            .to_string();

        while state != "ACTIVE" {
            let check_url = format!("{}/{}?key={}", self.base_url, name, self.api_key);
            let check_res = self.client.get(&check_url).send().await?;
            let check_json: serde_json::Value = check_res.json().await?;
            let file_obj = check_json.get("file").unwrap_or(&check_json);
            state = file_obj
                .get("state")
                .and_then(|v| v.as_str())
                .unwrap_or("PROCESSING")
                .to_string();

            match state.as_str() {
                "ACTIVE" => break,
                "FAILED" => {
                    return Err(FinancialHistoryError::ExtractionFailed(
                        "Google failed to process the file".to_string(),
                    ))
                }
                _ => sleep(Duration::from_secs(2)).await,
            }
        }

        Ok(RemoteDocument {
            uri,
            name,
            display_name: file_name.to_string(),
            mime_type,
            state,
        })
    }

    pub(crate) async fn generate_content(
        &self,
        model: &str,
        system_prompt: &str,
        messages: Vec<Content>,
        response_schema: Option<serde_json::Value>,
    ) -> Result<String> {
        let url = format!(
            "{}/models/{}:generateContent?key={}",
            self.base_url, model, self.api_key
        );

        let system_content = Some(Content {
            role: "user".to_string(),
            parts: vec![Part::Text {
                text: system_prompt.to_string(),
            }],
        });

        let payload = GenerateContentRequest {
            contents: messages,
            system_instruction: system_content,
            generation_config: GenerationConfig {
                response_mime_type: "application/json".to_string(),
                response_schema,
            },
        };

        let res = self.client.post(&url).json(&payload).send().await?;
        let status = res.status();

        if !status.is_success() {
            let err_text = res.text().await?;
            return Err(FinancialHistoryError::ExtractionFailed(format!(
                "Gemini API Error (status {}): {}",
                status, err_text
            )));
        }

        let body: GenerateContentResponse = res.json().await?;

        let text = body
            .candidates
            .ok_or_else(|| {
                FinancialHistoryError::ExtractionFailed("No candidates returned".to_string())
            })?
            .first()
            .ok_or_else(|| {
                FinancialHistoryError::ExtractionFailed("Empty candidates list".to_string())
            })?
            .content
            .parts
            .first()
            .ok_or_else(|| {
                FinancialHistoryError::ExtractionFailed("No parts in content".to_string())
            })?
            .clone();

        match text {
            Part::Text { text } => Ok(text),
            _ => Err(FinancialHistoryError::ExtractionFailed(
                "Model returned non-text content".to_string(),
            )),
        }
    }
}
