use super::{CallToolResult, Content, JsonRpcError};
use serde::Deserialize;

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

const INVALID_PARAMS: i32 = -32602;
const INTERNAL_ERROR: i32 = -32603;

fn parse_args<T: serde::de::DeserializeOwned>(
    arguments: Option<serde_json::Value>,
) -> Result<T, JsonRpcError> {
    serde_json::from_value(arguments.unwrap_or(serde_json::Value::Null)).map_err(|e| JsonRpcError {
        code: INVALID_PARAMS,
        message: format!("Invalid arguments: {e}"),
        data: None,
    })
}

fn internal_err(message: String) -> JsonRpcError {
    JsonRpcError {
        code: INTERNAL_ERROR,
        message,
        data: None,
    }
}

fn to_text_result(value: &impl serde::Serialize) -> Result<serde_json::Value, JsonRpcError> {
    let json = serde_json::to_string_pretty(value)
        .map_err(|e| internal_err(format!("Serialization error: {e}")))?;

    serde_json::to_value(CallToolResult {
        content: vec![Content::Text { text: json }],
        is_error: None,
    })
    .map_err(|e| internal_err(format!("Internal error: {e}")))
}

async fn run_blocking<T, F>(f: F) -> Result<T, JsonRpcError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| internal_err(format!("Task join error: {e}")))?
        .map_err(internal_err)
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

pub async fn handle_pdf_toc(
    arguments: Option<serde_json::Value>,
    _global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    #[derive(Deserialize)]
    struct Args {
        path: String,
    }

    let args: Args = parse_args(arguments)?;

    let tree = run_blocking(move || {
        let bytes = std::fs::read(&args.path).map_err(|e| format!("Failed to read file: {e}"))?;
        pdf::parse(&bytes).map_err(|e| format!("PDF error: {e}"))
    })
    .await?;

    to_text_result(&tree)
}

pub async fn handle_pdf_read(
    arguments: Option<serde_json::Value>,
    _global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    #[derive(Deserialize)]
    struct Args {
        path: String,
        #[serde(rename = "sectionId")]
        section_id: String,
    }

    let args: Args = parse_args(arguments)?;

    let content = run_blocking(move || {
        let bytes = std::fs::read(&args.path).map_err(|e| format!("Failed to read file: {e}"))?;
        let id = pdf::SectionId::parse(&args.section_id)
            .map_err(|e| format!("Invalid section ID: {e}"))?;
        pdf::read_section(&bytes, &id).map_err(|e| format!("PDF error: {e}"))
    })
    .await?;

    to_text_result(&content)
}

pub async fn handle_pdf_image(
    arguments: Option<serde_json::Value>,
    _global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    #[derive(Deserialize)]
    struct Args {
        path: String,
        #[serde(rename = "imageId")]
        image_id: String,
    }

    let args: Args = parse_args(arguments)?;

    let result = run_blocking(move || {
        let bytes = std::fs::read(&args.path).map_err(|e| format!("Failed to read file: {e}"))?;
        let id = pdf::ImageId::new(&args.image_id);
        let img = pdf::get_image(&bytes, &id).map_err(|e| format!("PDF error: {e}"))?;
        use base64::Engine;
        Ok(serde_json::json!({
            "id": args.image_id,
            "format": format!("{}", img.format),
            "data": base64::engine::general_purpose::STANDARD.encode(&img.bytes),
            "size": img.bytes.len(),
        }))
    })
    .await?;

    to_text_result(&result)
}

pub async fn handle_pdf_info(
    arguments: Option<serde_json::Value>,
    _global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    #[derive(Deserialize)]
    struct Args {
        path: String,
    }

    let args: Args = parse_args(arguments)?;

    let metadata = run_blocking(move || {
        let bytes = std::fs::read(&args.path).map_err(|e| format!("Failed to read file: {e}"))?;
        pdf::info(&bytes).map_err(|e| format!("PDF error: {e}"))
    })
    .await?;

    to_text_result(&metadata)
}
