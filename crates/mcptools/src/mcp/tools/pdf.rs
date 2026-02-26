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

/// Parse an optional section ID string.
fn parse_section_id(s: Option<&str>) -> Result<Option<pdf::SectionId>, String> {
    s.map(|id| pdf::SectionId::parse(id).map_err(|e| format!("Invalid section ID: {e}")))
        .transpose()
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
        section_id: Option<String>,
    }

    let args: Args = parse_args(arguments)?;

    let content = run_blocking(move || {
        let bytes = std::fs::read(&args.path).map_err(|e| format!("Failed to read file: {e}"))?;
        let id = parse_section_id(args.section_id.as_deref())?;
        pdf::read_section(&bytes, id.as_ref()).map_err(|e| format!("PDF error: {e}"))
    })
    .await?;

    to_text_result(&content)
}

pub async fn handle_pdf_peek(
    arguments: Option<serde_json::Value>,
    _global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    #[derive(Deserialize)]
    struct Args {
        path: String,
        #[serde(rename = "sectionId")]
        section_id: Option<String>,
        position: Option<String>,
        limit: Option<usize>,
    }

    let args: Args = parse_args(arguments)?;

    let content = run_blocking(move || {
        let bytes = std::fs::read(&args.path).map_err(|e| format!("Failed to read file: {e}"))?;
        let position: pdf::PeekPosition =
            args.position
                .as_deref()
                .unwrap_or("beginning")
                .parse()
                .map_err(|e: pdf::InvalidPeekPosition| format!("Invalid position: {e}"))?;
        let limit = args.limit.unwrap_or(500);
        let id = parse_section_id(args.section_id.as_deref())?;

        pdf::peek_section(&bytes, id.as_ref(), position, limit)
            .map_err(|e| format!("PDF error: {e}"))
    })
    .await?;

    to_text_result(&content)
}

pub async fn handle_pdf_images(
    arguments: Option<serde_json::Value>,
    _global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    #[derive(Deserialize)]
    struct Args {
        path: String,
        #[serde(rename = "sectionId")]
        section_id: Option<String>,
    }

    let args: Args = parse_args(arguments)?;

    let images = run_blocking(move || {
        let bytes = std::fs::read(&args.path).map_err(|e| format!("Failed to read file: {e}"))?;
        let id = parse_section_id(args.section_id.as_deref())?;
        pdf::list_section_images(&bytes, id.as_ref()).map_err(|e| format!("PDF error: {e}"))
    })
    .await?;

    to_text_result(&images)
}

pub async fn handle_pdf_image(
    arguments: Option<serde_json::Value>,
    _global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    #[derive(Deserialize)]
    struct Args {
        path: String,
        #[serde(rename = "imageId")]
        image_id: Option<String>,
        #[serde(rename = "sectionId")]
        section_id: Option<String>,
        random: Option<bool>,
    }

    let args: Args = parse_args(arguments)?;
    let random = args.random.unwrap_or(false);

    if args.image_id.is_some() && random {
        return Err(internal_err(
            "Cannot specify both imageId and random".to_string(),
        ));
    }
    if args.image_id.is_none() && !random {
        return Err(internal_err(
            "Either provide imageId or set random to true".to_string(),
        ));
    }

    let result = run_blocking(move || {
        let bytes = std::fs::read(&args.path).map_err(|e| format!("Failed to read file: {e}"))?;

        let image_id = if let Some(id_str) = args.image_id {
            pdf::ImageId::new(id_str)
        } else {
            let section_id = parse_section_id(args.section_id.as_deref())?;
            let images = pdf::list_section_images(&bytes, section_id.as_ref())
                .map_err(|e| format!("PDF error: {e}"))?;
            if images.is_empty() {
                return Err("No images found".to_string());
            }
            use rand::Rng;
            let idx = rand::thread_rng().gen_range(0..images.len());
            images[idx].id.clone()
        };

        let img = pdf::get_image(&bytes, &image_id).map_err(|e| format!("PDF error: {e}"))?;
        use base64::Engine;
        Ok(serde_json::json!({
            "id": image_id.as_str(),
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
