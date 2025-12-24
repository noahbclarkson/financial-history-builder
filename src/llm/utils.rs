use std::collections::HashMap;

use gemini_rust::{FileHandle, Part};
use gemini_structured_output::FileManager;

use crate::error::Result;

pub fn build_prompt_parts(prompt: &str, documents: &[FileHandle]) -> Result<Vec<Part>> {
    let mut parts = Vec::with_capacity(documents.len() + 1);
    parts.push(Part::Text {
        text: prompt.to_string(),
        thought: None,
        thought_signature: None,
    });
    for handle in documents {
        parts.push(FileManager::as_part(handle)?);
    }
    Ok(parts)
}

pub fn document_display_name(handle: &FileHandle) -> String {
    handle
        .get_file_meta()
        .display_name
        .clone()
        .unwrap_or_else(|| handle.name().to_string())
}

pub fn create_document_manifest(
    documents: &[FileHandle],
) -> (String, HashMap<String, String>) {
    let mut manifest = String::from(
        "═══════════════════════════════════════════════════════════════════\n\
         📂 DOCUMENT MANIFEST\n\
         ═══════════════════════════════════════════════════════════════════\n\n",
    );
    let mut id_map = HashMap::new();

    for (i, doc) in documents.iter().enumerate() {
        let id = i.to_string();
        let display_name = document_display_name(doc);
        manifest.push_str(&format!(
            "  Document ID: {}  →  \"{}\"\n",
            id, display_name
        ));
        id_map.insert(id, display_name);
    }

    manifest.push_str(
        "\n═══════════════════════════════════════════════════════════════════\n\
         ⚠️  CRITICAL INSTRUCTION ⚠️\n\
         ═══════════════════════════════════════════════════════════════════\n\
         In ALL `source.document` fields, use ONLY the Document ID number.\n\n\
         ✅ CORRECT:   \"document\": \"0\"\n\
         ✅ CORRECT:   \"document\": \"1\"\n\
         ❌ WRONG:     \"document\": \"2023_Annual_Report.pdf\"\n\
         ❌ WRONG:     \"document\": \"Financial Statements.pdf\"\n\n\
         Do NOT use filenames. Use ONLY the numeric ID from the manifest above.\n\
         ═══════════════════════════════════════════════════════════════════\n\n",
    );

    (manifest, id_map)
}
