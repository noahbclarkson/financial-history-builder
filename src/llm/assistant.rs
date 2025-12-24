use gemini_rust::FileHandle;
use gemini_structured_output::StructuredClient;

use crate::error::Result;
use crate::llm::types::MarkdownResponse;
use crate::llm::utils::build_prompt_parts;

pub struct DocumentAssistant {
    client: StructuredClient,
}

impl DocumentAssistant {
    pub fn new(client: StructuredClient) -> Self {
        Self { client }
    }

    /// Ask a question about a specific set of documents.
    ///
    /// # Arguments
    /// * `prompt` - The user's question or instruction
    /// * `documents` - Gemini file handles to include as context
    pub async fn ask(&self, prompt: &str, documents: &[FileHandle]) -> Result<String> {
        let parts = build_prompt_parts(prompt, documents)?;
        let outcome = self
            .client
            .request::<MarkdownResponse>()
            .system("You are a helpful assistant analyzing the provided documents.")
            .user_parts(parts)
            .execute()
            .await?;

        Ok(outcome.value.markdown)
    }
}
