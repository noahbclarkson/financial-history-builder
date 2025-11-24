use crate::error::Result;
use crate::llm::client::GeminiClient;
use crate::llm::types::{Content, FileData, Part, RemoteDocument};

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
    /// * `model` - The model to use (e.g., "gemini-1.5-pro")
    /// * `prompt` - The user's question or instruction
    /// * `documents` - Array of uploaded RemoteDocument handles
    pub async fn ask(
        &self,
        model: &str,
        prompt: &str,
        documents: &[RemoteDocument],
    ) -> Result<String> {
        // Combine the text prompt and the file references into a single user message.
        let mut parts: Vec<Part> = documents
            .iter()
            .map(|doc| Part::FileData {
                file_data: FileData {
                    mime_type: doc.mime_type.clone(),
                    file_uri: doc.uri.clone(),
                },
            })
            .collect();

        parts.push(Part::Text {
            text: prompt.to_string(),
        });

        let messages = vec![Content {
            role: "user".to_string(),
            parts,
        }];

        // Passing `None` for response_schema tells Gemini to return free-form text.
        let response = self
            .client
            .generate_content(
                model,
                "You are a helpful assistant analyzing the provided documents.",
                messages,
                None,
                "text/plain", // Model doesn't accept text/markdown; ask for plain text but format in markdown.
            )
            .await?;

        Ok(response)
    }
}
