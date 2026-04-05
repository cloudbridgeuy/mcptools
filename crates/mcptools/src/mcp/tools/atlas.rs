use serde::Deserialize;

use super::{CallToolResult, Content, JsonRpcError};

pub async fn handle_atlas_tree_view(
    arguments: Option<serde_json::Value>,
    _global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    #[derive(Deserialize)]
    struct Args {
        path: Option<String>,
        depth: Option<u32>,
    }

    let args: Args = parse_args(arguments)?;
    let (_root, _config, db) = open_atlas()?;

    let path = args.path.map(std::path::PathBuf::from);
    let entries = crate::atlas::data::atlas_tree_data(&db, path.as_deref(), args.depth)
        .map_err(to_internal_error)?;

    let output = mcptools_core::atlas::format_tree(&entries, true);
    call_tool_result_text(&output)
}

pub async fn handle_atlas_peek(
    arguments: Option<serde_json::Value>,
    _global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    #[derive(Deserialize)]
    struct Args {
        path: String,
    }

    let args: Args = parse_args(arguments)?;
    let (_root, _config, db) = open_atlas()?;

    let peek_result = crate::atlas::data::atlas_peek_data(&db, std::path::Path::new(&args.path))
        .map_err(to_internal_error)?;

    let output = match peek_result {
        crate::atlas::db::PeekResult::File(peek) => mcptools_core::atlas::format_peek(&peek, true),
        crate::atlas::db::PeekResult::Directory(dir_peek) => {
            mcptools_core::atlas::format_directory_peek(&dir_peek, true)
        }
    };

    call_tool_result_text(&output)
}

pub async fn handle_atlas_status(
    _arguments: Option<serde_json::Value>,
    _global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    let (root, config, db) = open_atlas()?;

    let status =
        crate::atlas::data::atlas_status_data(&db, &config, &root).map_err(to_internal_error)?;

    let output = mcptools_core::atlas::format_status(&status, true);
    call_tool_result_text(&output)
}

fn parse_args<T: serde::de::DeserializeOwned>(
    arguments: Option<serde_json::Value>,
) -> Result<T, JsonRpcError> {
    serde_json::from_value(arguments.unwrap_or(serde_json::Value::Null)).map_err(|e| JsonRpcError {
        code: -32602,
        message: format!("Invalid arguments: {e}"),
        data: None,
    })
}

fn open_atlas() -> Result<
    (
        std::path::PathBuf,
        mcptools_core::atlas::AtlasConfig,
        crate::atlas::db::Database,
    ),
    JsonRpcError,
> {
    let root = crate::atlas::cli::index::find_git_root().map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Atlas error: {e}"),
        data: None,
    })?;
    let config = crate::atlas::config::load_config(&root).map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Atlas config error: {e}"),
        data: None,
    })?;
    let db = crate::atlas::db::Database::open(&config.db_path.resolve(&root)).map_err(|e| {
        JsonRpcError {
            code: -32603,
            message: format!("Atlas database error: {e}"),
            data: None,
        }
    })?;
    Ok((root, config, db))
}

fn to_internal_error(e: color_eyre::eyre::Report) -> JsonRpcError {
    JsonRpcError {
        code: -32603,
        message: format!("Tool execution error: {e}"),
        data: None,
    }
}

fn call_tool_result_text(text: &str) -> Result<serde_json::Value, JsonRpcError> {
    let result = CallToolResult {
        content: vec![Content::Text {
            text: text.to_string(),
        }],
        is_error: None,
    };
    serde_json::to_value(result).map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Internal error: {e}"),
        data: None,
    })
}
