use crate::error::{FinancialHistoryError, Result};
use crate::llm::{client::GeminiClient, types::*};
use crate::{process_financial_history, verify_accounting_equation, FinancialHistoryConfig};
use json_patch::Patch;

pub struct FinancialExtractor {
    client: GeminiClient,
    model: String,
}

impl FinancialExtractor {
    pub fn new(client: GeminiClient, model: impl Into<String>) -> Self {
        Self {
            client,
            model: model.into(),
        }
    }

    pub async fn extract(&self, documents: &[RemoteDocument]) -> Result<FinancialHistoryConfig> {
        let system_prompt = include_str!("../../GEMINI_PROMPT_EXAMPLE.md");

        let user_prompt = "Extract the financial history from the attached PDF(s). \
            Pay specific attention to overlapping periods (e.g. Q1 vs Full Year) and ensure \
            the Balance Sheet has explicit opening balances for the first period. Opening Balance \
            accuracy is critical for downstream charts—if the PDF is missing it, derive it from the \
            earliest balance sheet totals so Assets = Liabilities + Equity and call this out in notes. \
            Please try to extract all months including from monthly reports. \
            Return ONLY raw JSON matching the FinancialHistoryConfig schema described in the system prompt (no markdown/backticks/explanation).".to_string();

        let mut messages = vec![Content::user_with_files(user_prompt, documents)];

        let raw_json = self
            .client
            .generate_content(&self.model, system_prompt, messages.clone(), None)
            .await?;

        let mut current_json_value: serde_json::Value = serde_json::from_str(&raw_json)?;
        let mut current_config: FinancialHistoryConfig =
            serde_json::from_value(current_json_value.clone())?;

        let max_retries = 3;

        for attempt in 0..max_retries {
            match process_financial_history(&current_config) {
                Ok(dense_data) => {
                    match verify_accounting_equation(&current_config, &dense_data, 1.0) {
                        Ok(_) => {
                            return Ok(current_config);
                        }
                        Err(e) => {
                            println!("Attempt {}: Balance error: {}", attempt, e);
                            self.request_patch(&mut messages, &mut current_json_value, &e.to_string())
                                .await?;
                        }
                    }
                }
                Err(e) => {
                    println!("Attempt {}: Validation error: {}", attempt, e);
                    self.request_patch(&mut messages, &mut current_json_value, &e.to_string())
                        .await?;
                }
            }

            current_config = serde_json::from_value(current_json_value.clone())?;
        }

        Err(FinancialHistoryError::ExtractionFailed(
            "Max retries exceeded".into(),
        ))
    }

    async fn request_patch(
        &self,
        history: &mut Vec<Content>,
        current_json: &mut serde_json::Value,
        error_msg: &str,
    ) -> Result<()> {
        history.push(Content::model(current_json.to_string()));

        let patch_prompt = format!(
            "The JSON you provided failed validation with this error:\n\n{}\n\n\
            Do NOT return the full JSON. Return a JSON Patch (RFC 6902) array of operations \
            to fix this specific error. \
            Example: [{{ \"op\": \"replace\", \"path\": \"/income_statement/0/constraints/1/end_date\", \"value\": \"2025-04-30\" }}]",
            error_msg
        );

        history.push(Content::user(patch_prompt));

        let patch_str = self
            .client
            .generate_content(
                &self.model,
                "You are a JSON Repair Agent.",
                history.clone(),
                None,
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
