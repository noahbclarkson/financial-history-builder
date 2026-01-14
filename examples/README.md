# Financial History Builder Examples

These examples use `rstructor` to produce strongly typed extraction and refinement outputs.

## Prerequisites

1. **Gemini API Key**: Get your API key from [Google AI Studio](https://aistudio.google.com/app/apikey)
2. **Rust**: Ensure you have Rust installed (1.70 or later)

## Setup

Set `GEMINI_API_KEY` and `GEMINI_FILE_URIS` in your environment:

```bash
export GEMINI_API_KEY="your_gemini_api_key_here"
export GEMINI_FILE_URIS="https://generativelanguage.googleapis.com/v1beta/files/FILE_NAME_1,https://generativelanguage.googleapis.com/v1beta/files/FILE_NAME_2"
```

Or create a `.env` file in the project root:

```
GEMINI_API_KEY=your_gemini_api_key_here
GEMINI_FILE_URIS=https://generativelanguage.googleapis.com/v1beta/files/FILE_NAME_1,https://generativelanguage.googleapis.com/v1beta/files/FILE_NAME_2
```

## Run Examples

```bash
cargo run --example gemini_pdf_example
cargo run --example chat_with_docs
cargo run --example forecasting_workflow
```

Each example expects Gemini file URIs for the PDFs listed in `GEMINI_FILE_URIS`.
