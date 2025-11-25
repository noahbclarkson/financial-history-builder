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

    /// Programmatically fetch model metadata (token limits, etc.)
    pub async fn get_model_info(&self, model_name: &str) -> Result<ModelMetadata> {
        let model_path = if model_name.starts_with("models/") {
            model_name.to_string()
        } else {
            format!("models/{}", model_name)
        };

        let url = format!("{}/{}?key={}", self.base_url, model_path, self.api_key);

        let res = self.client.get(&url).send().await?;

        if !res.status().is_success() {
            let err = res.text().await?;
            return Err(FinancialHistoryError::ExtractionFailed(format!(
                "Failed to fetch model info: {}",
                err
            )));
        }

        let metadata: ModelMetadata = res.json().await?;
        Ok(metadata)
    }

    /// Upload a file from a local path (CLI/Desktop use case)
    pub async fn upload_document(&self, path: &Path) -> Result<RemoteDocument> {
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| {
                FinancialHistoryError::ExtractionFailed("Invalid file name".to_string())
            })?
            .to_string();

        let mime_type = mime_guess::from_path(path)
            .first_or_octet_stream()
            .to_string();

        let file_bytes = fs::read(path).await?;

        self.perform_resumable_upload(&file_name, &mime_type, file_bytes)
            .await
    }

    /// Upload a file from memory bytes (Server/API use case).
    /// Useful when handling multipart uploads in web frameworks (Axum/Actix)
    /// where you have the bytes in memory but no file on disk.
    pub async fn upload_document_from_bytes(
        &self,
        filename: &str,
        mime_type: &str,
        data: Vec<u8>,
    ) -> Result<RemoteDocument> {
        self.perform_resumable_upload(filename, mime_type, data)
            .await
    }

    /// Shared internal logic for Google's Resumable Upload Protocol
    async fn perform_resumable_upload(
        &self,
        display_name: &str,
        mime_type: &str,
        file_bytes: Vec<u8>,
    ) -> Result<RemoteDocument> {
        let file_size = file_bytes.len();

        // 1. Initiate Upload
        let start_url = format!("{}?key={}", GEMINI_UPLOAD_URL, self.api_key);
        let metadata = json!({ "file": { "display_name": display_name } });

        let init_res = self
            .client
            .post(&start_url)
            .header("X-Goog-Upload-Protocol", "resumable")
            .header("X-Goog-Upload-Command", "start")
            .header("X-Goog-Upload-Header-Content-Length", file_size.to_string())
            .header("X-Goog-Upload-Header-Content-Type", mime_type)
            .header("Content-Type", "application/json")
            .json(&metadata)
            .send()
            .await?;

        if !init_res.status().is_success() {
            let error_text = init_res.text().await?;
            return Err(FinancialHistoryError::ExtractionFailed(format!(
                "Upload init failed: {}",
                error_text
            )));
        }

        let upload_url = init_res
            .headers()
            .get("x-goog-upload-url")
            .ok_or_else(|| {
                FinancialHistoryError::ExtractionFailed(
                    "No x-goog-upload-url header returned".to_string(),
                )
            })?
            .to_str()
            .map_err(|e| FinancialHistoryError::ExtractionFailed(e.to_string()))?
            .to_string();

        // 2. Upload Bytes
        let upload_res = self
            .client
            .post(&upload_url)
            .header("Content-Length", file_size.to_string())
            .header("X-Goog-Upload-Offset", "0")
            .header("X-Goog-Upload-Command", "upload, finalize")
            .body(file_bytes)
            .send()
            .await?;

        if !upload_res.status().is_success() {
            let error_text = upload_res.text().await?;
            return Err(FinancialHistoryError::ExtractionFailed(format!(
                "File upload failed: {}",
                error_text
            )));
        }

        let upload_body: serde_json::Value = upload_res.json().await?;
        let file_obj = upload_body.get("file").ok_or_else(|| {
            FinancialHistoryError::ExtractionFailed(
                "Upload response missing 'file' object".to_string(),
            )
        })?;

        let uri = file_obj
            .get("uri")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let name = file_obj
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let mut state = file_obj
            .get("state")
            .and_then(|v| v.as_str())
            .unwrap_or("PROCESSING")
            .to_string();

        // 3. Poll for Active State
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
                        "Google processing failed".to_string(),
                    ))
                }
                _ => sleep(Duration::from_secs(1)).await,
            }
        }

        Ok(RemoteDocument {
            uri,
            name,
            display_name: display_name.to_string(),
            mime_type: mime_type.to_string(),
            state,
        })
    }

    pub(crate) async fn generate_content(
        &self,
        model: &str,
        system_prompt: &str,
        messages: Vec<Content>,
        response_schema: Option<serde_json::Value>,
        response_mime_type: &str,
        max_output_tokens: Option<u32>,
        debug_label: &str,
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
                response_mime_type: response_mime_type.to_string(),
                response_schema,
                max_output_tokens,
            },
        };

        let res = self.client.post(&url).json(&payload).send().await?;

        if !res.status().is_success() {
            let status = res.status();
            let err_text = res.text().await.unwrap_or_default();
            return Err(FinancialHistoryError::ExtractionFailed(format!(
                "API Request Failed ({}): {}",
                status, err_text
            )));
        }

        // Always capture the raw body so we can dump it if decoding fails.
        let raw_body = res.text().await?;
        let body: GenerateContentResponse = serde_json::from_str(&raw_body).map_err(|e| {
            // Sanitize label for filename
            let safe_label: String = debug_label
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
                .collect();
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let filename = format!("debug_raw_model_response_{}_{}.json", safe_label, timestamp);
            let _ = std::fs::write(&filename, &raw_body);

            FinancialHistoryError::ExtractionFailed(format!(
                "Failed to decode model response: {}. Raw response dumped to {}",
                e, filename
            ))
        })?;

        // 1. Check for prompt blocking (Safety)
        if let Some(feedback) = body.prompt_feedback {
            if let Some(reason) = feedback.block_reason {
                return Err(FinancialHistoryError::ExtractionFailed(format!(
                    "Prompt blocked by safety settings. Reason: {}",
                    reason
                )));
            }
        }

        // 2. Check candidates
        let candidates = body.candidates.ok_or_else(|| {
            FinancialHistoryError::ExtractionFailed(
                "No candidates returned (Prompt filtered?)".to_string(),
            )
        })?;

        let first_candidate = candidates.first().ok_or_else(|| {
            FinancialHistoryError::ExtractionFailed("Empty candidate list".to_string())
        })?;

        // 3. Check finish reason
        if let Some(reason) = &first_candidate.finish_reason {
            if reason != "STOP" {
                println!("⚠️  Finish Reason: {}", reason);
            }
            if reason == "SAFETY" || reason == "RECITATION" {
                return Err(FinancialHistoryError::ExtractionFailed(format!(
                    "Generation stopped due to: {}",
                    reason
                )));
            }
            if reason == "MAX_TOKENS" {
                // Dump the truncated response for debugging
                if let Some(content) = &first_candidate.content {
                    if let Some(Part::Text { text }) = content.parts.first() {
                        let timestamp = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_secs();
                        let filename = format!("debug_max_tokens_truncated_{}.json", timestamp);
                        let _ = std::fs::write(&filename, text);
                        eprintln!(
                            "❌ MAX_TOKENS error: Truncated response dumped to {}",
                            filename
                        );
                    }
                }
                return Err(FinancialHistoryError::ExtractionFailed(
                    "MAX_TOKENS: Response was truncated. The output is likely incomplete and invalid JSON. \
                    Try reducing the scope of the request or increasing max_output_tokens.".to_string()
                ));
            }
        }

        // 4. Extract text
        let content = first_candidate.content.as_ref().ok_or_else(|| {
            FinancialHistoryError::ExtractionFailed(
                "Candidate has no content (Safety block)".to_string(),
            )
        })?;

        let part = content.parts.first().ok_or_else(|| {
            FinancialHistoryError::ExtractionFailed("No parts in content".to_string())
        })?;

        match part {
            Part::Text { text } => Ok(text.clone()),
            _ => Err(FinancialHistoryError::ExtractionFailed(
                "Model returned non-text content".to_string(),
            )),
        }
    }
}
