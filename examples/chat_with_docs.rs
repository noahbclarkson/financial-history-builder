use dotenv::dotenv;
use financial_history_builder::llm::{DocumentAssistant, GeminiClient, RemoteDocument};
use futures::future;
use std::error::Error;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use tokio::fs;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    dotenv().ok();
    let api_key = std::env::var("GEMINI_API_KEY").expect("GEMINI_API_KEY must be set");

    println!("💬 Starting Document Chat...\n");

    // 1. Setup Client
    let client = GeminiClient::new(api_key);
    let assistant = DocumentAssistant::new(client.clone());

    // 2. Discovery & Upload (Same as previous examples)
    let doc_dir = Path::new("examples").join("documents");
    let mut dir_stream = fs::read_dir(&doc_dir).await?;
    let mut pdf_paths: Vec<PathBuf> = Vec::new();
    while let Ok(Some(entry)) = dir_stream.next_entry().await {
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "pdf") {
            pdf_paths.push(path);
        }
    }

    if pdf_paths.is_empty() {
        println!("⚠️  No PDF files found in {:?}. Please add some.", doc_dir);
        return Ok(());
    }

    println!("☁️  Uploading {} documents...", pdf_paths.len());
    let upload_futures: Vec<_> = pdf_paths
        .iter()
        .map(|path| client.upload_document(path))
        .collect();

    let documents: Vec<RemoteDocument> = future::try_join_all(upload_futures).await?;
    println!("✅ Documents active.\n");

    // 3. Interactive Loop
    let model = "gemini-2.5-flash-preview-09-2025"; // Or gemini-1.5-pro

    println!("🤖 Ready! Ask questions about your documents (type 'quit' to exit).");
    println!("------------------------------------------------------------------");

    loop {
        print!("> ");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let prompt = input.trim();

        if prompt.eq_ignore_ascii_case("quit") || prompt.eq_ignore_ascii_case("exit") {
            break;
        }

        if prompt.is_empty() {
            continue;
        }

        println!("\nThinking...");

        // The Magic Call
        match assistant.ask(model, prompt, &documents).await {
            Ok(response) => {
                println!("\n{}\n", response);
                println!("------------------------------------------------------------------");
            }
            Err(e) => {
                eprintln!("❌ Error: {}", e);
            }
        }
    }

    Ok(())
}
