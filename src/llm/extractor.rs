use crate::error::{FinancialHistoryError, Result};
use crate::llm::{client::GeminiClient, types::*};
use crate::{process_financial_history, verify_accounting_equation, FinancialHistoryConfig};
use json_patch::Patch;
use tokio::sync::mpsc::Sender;

pub struct FinancialExtractor {
    client: GeminiClient,
    model: String,
    system_prompt: String,
}

impl FinancialExtractor {
    pub fn new(client: GeminiClient, model: impl Into<String>) -> Self {
        let default_prompt = include_str!("../../GEMINI_PROMPT_EXAMPLE.md").to_string();
        Self {
            client,
            model: model.into(),
            system_prompt: default_prompt,
        }
    }

    /// Allow the user to load a specific prompt file (e.g., for different industries)
    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = prompt.into();
        self
    }

    pub async fn extract(
        &self,
        documents: &[RemoteDocument],
        progress: Option<Sender<ExtractionEvent>>,
    ) -> Result<FinancialHistoryConfig> {
        let _ = self.send_event(&progress, ExtractionEvent::Starting).await;

        // --- Step 1: Build Document Manifest ---
        // This tells the AI: "The first file you see is named X, the second is Y..."
        let mut doc_manifest = String::from(
            "\n### 📂 DOCUMENT MANIFEST\n\
            You have received the following files. You MUST use these EXACT filenames in the `source.document_name` field:\n",
        );
        for (i, doc) in documents.iter().enumerate() {
            doc_manifest.push_str(&format!("{}. \"{}\"\n", i + 1, doc.display_name));
        }

        // --- Step 2: Initial Drafting ---
        let _ = self
            .send_event(&progress, ExtractionEvent::DraftingResponse)
            .await;

        let user_instructions = format!(
            "Extract financial history from the attached files.\n\
            {}\n\
            CRITICAL INSTRUCTIONS:\n\
            1. **Source Tracking**: Every numeric value MUST have a `source` populated.\n\
               - `document_name`: Must match a name from the Document Manifest exactly.\n\
               - `original_text`: ONLY required if the row label differs from the account name, OR the value came from narrative text, OR the value was inferred. If the account name matches the table row label exactly, you may omit `original_text`.\n\
            2. **Fiscal Year**: Determine the correct fiscal year end month.\n\
            3. **Overlapping Periods**: Do not calculate values. Only extract what is explicitly written (e.g. if Q1 and Full Year are both present, extract both).\n\
            4. **Output**: Return ONLY valid JSON matching the schema.",
            doc_manifest
        );

        let mut messages = vec![Content::user_with_files(user_instructions, documents)];

        let raw_json = self
            .client
            .generate_content(
                &self.model,
                &self.system_prompt,
                messages.clone(),
                None,
                "application/json",
            )
            .await?;

        let _ = self
            .send_event(&progress, ExtractionEvent::ProcessingResponse)
            .await;

        let mut current_json_value: serde_json::Value =
            serde_json::from_str(&raw_json).map_err(|e| {
                FinancialHistoryError::ExtractionFailed(format!("Initial JSON parse failed: {}", e))
            })?;

        let mut current_config: FinancialHistoryConfig =
            serde_json::from_value(current_json_value.clone())?;

        // --- Step 2: Validation & Correction Loop ---
        let max_retries = 3;

        for attempt in 1..=max_retries {
            let _ = self
                .send_event(&progress, ExtractionEvent::Validating { attempt })
                .await;

            // Check 1: Math & Logic
            let validation_error = match process_financial_history(&current_config) {
                Ok(dense_data) => {
                    match verify_accounting_equation(&current_config, &dense_data, 1.0) {
                        Ok(_) => None, // Math is good!
                        Err(e) => Some(format!("Accounting Equation Violation: {}", e)),
                    }
                }
                Err(e) => Some(format!("Structural/Logic Error: {}", e)),
            };

            // Check 2: Missing Sources (The "Trust" Layer)
            // Only run this if math is okay, to avoid overwhelming the model
            let source_error = if validation_error.is_none() {
                self.check_missing_sources(&current_config)
            } else {
                None
            };

            // Decide if we need to patch
            let error_to_fix = validation_error.or(source_error);

            if let Some(error_msg) = error_to_fix {
                let _ = self
                    .send_event(
                        &progress,
                        ExtractionEvent::CorrectionNeeded {
                            reason: error_msg.clone(),
                        },
                    )
                    .await;
                let _ = self
                    .send_event(&progress, ExtractionEvent::Patching { attempt })
                    .await;

                self.apply_patch(&mut messages, &mut current_json_value, &error_msg)
                    .await?;

                // Re-hydrate config from patched JSON
                current_config = serde_json::from_value(current_json_value.clone())?;
            } else {
                // No errors found!
                let _ = self.send_event(&progress, ExtractionEvent::Success).await;
                return Ok(current_config);
            }
        }

        let msg = "Max retries exceeded. The model could not resolve validation errors.";
        let _ = self
            .send_event(
                &progress,
                ExtractionEvent::Failed {
                    reason: msg.to_string(),
                },
            )
            .await;
        Err(FinancialHistoryError::ExtractionFailed(msg.into()))
    }

    async fn send_event(&self, sender: &Option<Sender<ExtractionEvent>>, event: ExtractionEvent) {
        if let Some(tx) = sender {
            let _ = tx.send(event).await;
        }
    }

    fn check_missing_sources(&self, config: &FinancialHistoryConfig) -> Option<String> {
        let mut missing_count = 0;
        let mut missing_details = Vec::new();

        for acc in &config.balance_sheet {
            for (idx, snap) in acc.snapshots.iter().enumerate() {
                if snap.source.is_none() {
                    missing_count += 1;
                    missing_details.push(format!(
                        "Balance Sheet account '{}', snapshot {} (date: {})",
                        acc.name, idx, snap.date
                    ));
                }
            }
        }
        for acc in &config.income_statement {
            for (idx, cons) in acc.constraints.iter().enumerate() {
                if cons.source.is_none() {
                    missing_count += 1;
                    missing_details.push(format!(
                        "Income Statement account '{}', constraint {} (period: {} to {})",
                        acc.name, idx, cons.start_date, cons.end_date
                    ));
                }
            }
        }

        if missing_count > 0 {
            let details_summary = if missing_details.len() <= 5 {
                missing_details.join("\n  - ")
            } else {
                format!(
                    "{}\n  - ... and {} more",
                    missing_details[..5].join("\n  - "),
                    missing_details.len() - 5
                )
            };

            Some(format!(
                "Validation Failed: {} data points are missing the 'source' field entirely. \
                Every value MUST have a source with at minimum the 'document_name' field populated. \
                Missing sources in:\n  - {}\n\
                Please patch the JSON to add source metadata with the correct document_name from the Document Manifest.",
                missing_count, details_summary
            ))
        } else {
            None
        }
    }

    async fn apply_patch(
        &self,
        history: &mut Vec<Content>,
        current_json: &mut serde_json::Value,
        error_msg: &str,
    ) -> Result<()> {
        let patch_prompt = format!(
            "The JSON you provided failed validation:\n\nERROR: {}\n\n\
            TASK: Return a JSON Patch (RFC 6902) array to fix this. \
            Do NOT return the full JSON. Return ONLY the patch array.\n\
            Example: [{{ \"op\": \"replace\", \"path\": \"/path/to/field\", \"value\": \"fixed_value\" }}]",
            error_msg
        );

        // We push the *model's own bad JSON* to the history so it knows what it wrote
        history.push(Content::model(current_json.to_string()));
        history.push(Content::user(patch_prompt));

        let patch_str = self
            .client
            .generate_content(
                &self.model,
                "You are a JSON Repair Agent.",
                history.clone(),
                None,
                "application/json",
            )
            .await?;

        let cleaned_patch = clean_json_output(&patch_str);
        let patch: Patch = serde_json::from_str(&cleaned_patch)?;

        json_patch::patch(current_json, &patch)?;

        Ok(())
    }
}

fn clean_json_output(raw: &str) -> String {
    if let Some(start) = raw.find('[') {
        if let Some(end) = raw.rfind(']') {
            return raw[start..=end].to_string();
        }
    }
    if let Some(start) = raw.find('{') {
        if let Some(end) = raw.rfind('}') {
            return raw[start..=end].to_string();
        }
    }
    raw.trim().to_string()
}
