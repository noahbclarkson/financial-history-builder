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

        // --- Step 0: Set Token Limit ---
        // Hardcoded to 65536 as requested for Gemini 2.5 Flash/Pro with structured outputs
        let output_token_limit = Some(65536);

        // --- Step 1: Build Document Manifest ---
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
               - `original_text`: ONLY required if the row label differs from the account name, OR the value came from narrative text, OR the value was inferred.\n\
            2. **Fiscal Year**: Determine the correct fiscal year end month.\n\
            3. **Output**: Return ONLY valid JSON matching the schema.",
            doc_manifest
        );

        let mut messages = vec![Content::user_with_files(user_instructions, documents)];

        // Generate Schema string for potential error correction later (debugging context)
        let target_schema_str = FinancialHistoryConfig::schema_as_json()
            .unwrap_or_else(|_| "Could not generate schema".to_string());

        // Generate GEMINI COMPATIBLE Schema Value for the API call
        // This removes $schema, definitions, and inlines all $refs
        let target_schema_value = FinancialHistoryConfig::get_gemini_response_schema()
            .map_err(|e| FinancialHistoryError::ExtractionFailed(format!("Schema generation error: {}", e)))?;

        let raw_json_response = self
            .client
            .generate_content(
                &self.model,
                &self.system_prompt,
                messages.clone(),
                Some(target_schema_value),
                "application/json",
                output_token_limit,
            )
            .await?;

        let _ = self
            .send_event(&progress, ExtractionEvent::ProcessingResponse)
            .await;

        // Clean potential markdown blocks from the initial response
        let cleaned_json = clean_json_output(&raw_json_response);

        // Parse into untyped Value first. If this fails, it's a syntax error (invalid JSON).
        // --- Step 3: Parse with Auto-Repair for Truncation ---
        let mut current_json_value: serde_json::Value = match serde_json::from_str(&cleaned_json) {
            Ok(v) => v,
            Err(e) => {
                if e.is_eof() || e.to_string().contains("EOF") {
                    let _ = self.send_event(&progress, ExtractionEvent::CorrectionNeeded {
                        reason: "JSON truncated. Applying auto-repair...".to_string()
                    }).await;

                    let repaired_json = try_repair_truncated_json(&cleaned_json);

                    serde_json::from_str(&repaired_json).map_err(|e2| {
                        FinancialHistoryError::ExtractionFailed(format!(
                            "Failed to parse even after auto-repair. Orig Error: {}. Repair Error: {}", e, e2
                        ))
                    })?
                } else {
                    return Err(FinancialHistoryError::ExtractionFailed(format!("Initial JSON Syntax Error: {}", e)));
                }
            }
        };

        // --- Step 3: Validation & Correction Loop ---
        let max_retries = 3;

        for attempt in 1..=max_retries {
            let _ = self
                .send_event(&progress, ExtractionEvent::Validating { attempt })
                .await;

            // 1. Attempt Deserialization (Schema Check)
            // We move this INSIDE the loop to catch structural errors
            let config_result: serde_json::Result<FinancialHistoryConfig> =
                serde_json::from_value(current_json_value.clone());

            let error_to_fix: Option<String>;
            let mut current_config: Option<FinancialHistoryConfig> = None;
            let mut is_schema_error = false;

            match config_result {
                Err(e) => {
                    // CASE A: Deserialization failed. The JSON does not match the Rust struct.
                    is_schema_error = true;
                    error_to_fix = Some(format!(
                        "JSON SCHEMA ERROR: The JSON provided does not match the required schema.\n\
                        Error Details: {}\n\
                        Required Schema: {}", 
                        e, target_schema_str
                    ));
                }
                Ok(cfg) => {
                    // CASE B: Deserialization succeeded. Check Math & Logic.
                    current_config = Some(cfg.clone());
                    
                    // Check Math
                    let math_error = match process_financial_history(&cfg) {
                        Ok(dense_data) => {
                            match verify_accounting_equation(&cfg, &dense_data, 1.0) {
                                Ok(_) => None, // Math is good!
                                Err(e) => Some(format!("Accounting Equation Violation: {}", e)),
                            }
                        }
                        Err(e) => Some(format!("Structural/Logic Error: {}", e)),
                    };

                    // Check Sources (if math is ok)
                    error_to_fix = if math_error.is_none() {
                        self.check_missing_sources(&cfg)
                    } else {
                        math_error
                    };
                }
            }

            // 2. Decide: Success or Patch?
            if let Some(error_msg) = error_to_fix {
                let _ = self
                    .send_event(
                        &progress,
                        ExtractionEvent::CorrectionNeeded {
                            reason: if is_schema_error { 
                                "Schema mismatch detected".to_string() 
                            } else { 
                                "Validation/Math logic error detected".to_string() 
                            },
                        },
                    )
                    .await;
                
                let _ = self
                    .send_event(&progress, ExtractionEvent::Patching { attempt })
                    .await;

                // Apply the patch to the untyped `current_json_value`
                self.apply_patch(&mut messages, &mut current_json_value, &error_msg, output_token_limit)
                    .await?;
                
                // Loop continues -> will try to deserialize `current_json_value` again next iteration
            } else {
                // Success!
                let _ = self.send_event(&progress, ExtractionEvent::Success).await;
                return Ok(current_config.unwrap());
            }
        }

        let msg = "Max retries exceeded. The model could not resolve errors.";
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
                Please patch the JSON to add source metadata.",
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
        max_tokens: Option<u32>,
    ) -> Result<()> {
        let patch_prompt = format!(
            "The JSON you provided failed validation:\n\n{}\n\n\
            TASK: Return a JSON Patch (RFC 6902) array to fix this. \
            Do NOT return the full JSON. Return ONLY the patch array.\n\
            Example: [{{ \"op\": \"replace\", \"path\": \"/path/to/field\", \"value\": \"fixed_value\" }}]",
            error_msg
        );

        // We push the *model's own bad JSON* to the history so it knows what it wrote.
        // We convert the current state to string to give the model context of what it needs to fix.
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
                max_tokens,
            )
            .await?;

        let cleaned_patch = clean_json_output(&patch_str);
        
        // Handle potential empty response or invalid patch syntax
        let patch: Patch = serde_json::from_str(&cleaned_patch).map_err(|e| {
             FinancialHistoryError::ExtractionFailed(format!(
                 "AI generated invalid JSON Patch syntax: {}. Response was: {}", 
                 e, cleaned_patch
             ))
        })?;

        json_patch::patch(current_json, &patch)?;

        Ok(())
    }
}

fn try_repair_truncated_json(json_str: &str) -> String {
    let mut balance_stack = Vec::new();
    let mut in_string = false;
    let mut escape = false;

    for c in json_str.chars() {
        if escape {
            escape = false;
            continue;
        }
        if c == '\\' {
            escape = true;
            continue;
        }
        if c == '"' {
            in_string = !in_string;
            continue;
        }
        if !in_string {
            match c {
                '{' => balance_stack.push('}'),
                '[' => balance_stack.push(']'),
                '}' => {
                    if Some(&'}') == balance_stack.last() {
                        balance_stack.pop();
                    }
                }
                ']' => {
                    if Some(&']') == balance_stack.last() {
                        balance_stack.pop();
                    }
                }
                _ => {}
            }
        }
    }

    let mut repaired = json_str.to_string();
    if in_string {
        repaired.push('"');
    }

    while let Some(closing_char) = balance_stack.pop() {
        repaired.push(closing_char);
    }

    repaired
}

fn clean_json_output(raw: &str) -> String {
    let trimmed = raw.trim();
    if let Some(start) = trimmed.find('[') {
        if let Some(end) = trimmed.rfind(']') {
            return trimmed[start..=end].to_string();
        }
    }
    if let Some(start) = trimmed.find('{') {
        if let Some(end) = trimmed.rfind('}') {
            return trimmed[start..=end].to_string();
        }
    }
    trimmed.to_string()
}