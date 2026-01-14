# Financial History Builder Examples

These examples use `gemini-structured-output` to produce strongly typed extraction and refinement outputs.

## Prerequisites

1. **Gemini API Key**: Get your API key from [Google AI Studio](https://aistudio.google.com/app/apikey)
2. **Rust**: Ensure you have Rust installed (1.70 or later)

## Setup

Set `GEMINI_API_KEY` in your environment:

```bash
export GEMINI_API_KEY="your_gemini_api_key_here"
```

Or create a `.env` file in the project root:

```
GEMINI_API_KEY=your_gemini_api_key_here
```

## Run Examples

```bash
cargo run --example gemini_pdf_example
cargo run --example chat_with_docs
cargo run --example forecasting_workflow
```

Each example expects PDFs in `examples/documents/`.
