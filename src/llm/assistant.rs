use rstructor::{GeminiClient, LLMClient};

use crate::error::Result;
use crate::llm::types::{DocumentReference, MarkdownResponse};
use crate::llm::utils::document_media;

pub struct DocumentAssistant {
    client: GeminiClient,
}

impl DocumentAssistant {
    pub fn new(client: GeminiClient) -> Self {
        Self { client }
    }

    /// Ask a question about a specific set of documents.
    ///
    /// # Arguments
    /// * `prompt` - The user's question or instruction
    /// * `documents` - Gemini file handles to include as context
    pub async fn ask(&self, prompt: &str, documents: &[DocumentReference]) -> Result<String> {
        let media = document_media(documents);
        let full_prompt = format!(
            "You are a helpful assistant analyzing the provided documents.\n\n{}",
            prompt
        );
        let response: MarkdownResponse = self
            .client
            .materialize_with_media(&full_prompt, &media)
            .await?;

        Ok(response.markdown)
    }
}
