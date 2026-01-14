use dotenv::dotenv;
use financial_history_builder::llm::{DocumentAssistant, DocumentReference};
use rstructor::{GeminiClient, GeminiModel, MediaFile};
use std::error::Error;
use std::io::{self, Write};

fn load_documents() -> Result<Vec<DocumentReference>, Box<dyn Error>> {
    let uris = std::env::var("GEMINI_FILE_URIS")?;
    let names = std::env::var("GEMINI_FILE_NAMES").ok();
    let name_list: Vec<String> = names
        .map(|value| value.split(',').map(|s| s.trim().to_string()).collect())
        .unwrap_or_default();

    let documents = uris
        .split(',')
        .enumerate()
        .map(|(index, uri)| {
            let name = name_list
                .get(index)
                .cloned()
                .unwrap_or_else(|| format!("Document {}", index + 1));
            DocumentReference::new(MediaFile::new(uri.trim(), "application/pdf"), name)
        })
        .collect::<Vec<_>>();

    if documents.is_empty() {
        return Err("GEMINI_FILE_URIS must include at least one URI".into());
    }

    Ok(documents)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    dotenv().ok();
    let api_key = std::env::var("GEMINI_API_KEY").expect("GEMINI_API_KEY must be set");

    println!("💬 Starting Document Chat...\n");

    let client = GeminiClient::new(api_key)?.model(GeminiModel::Gemini25Flash);
    let assistant = DocumentAssistant::new(client);

    let documents = load_documents()?;
    println!("✅ Loaded {} document URIs.\n", documents.len());

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

        match assistant.ask(prompt, &documents).await {
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
