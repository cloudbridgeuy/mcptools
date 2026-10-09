use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::JsonRpcError;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AtlasTextOutput {
    pub format: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub freshness: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AtlasBoundedOutput {
    #[serde(flatten)]
    pub view: AtlasTextOutput,
    pub bounds: crate::atlas::query::Bounds,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AtlasPeekOutput {
    #[serde(flatten)]
    pub view: AtlasBoundedOutput,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub children_bounds: Option<crate::atlas::query::Bounds>,
}

#[derive(Deserialize, JsonSchema)]
pub struct AtlasTreeViewArgs {
    pub path: Option<String>,
    pub depth: Option<u32>,
    pub limit: Option<usize>,
    pub context_chars: Option<usize>,
}

#[derive(Deserialize, JsonSchema)]
pub struct AtlasPeekArgs {
    pub path: String,
    pub limit: Option<usize>,
    pub context_chars: Option<usize>,
}

#[derive(Deserialize, JsonSchema)]
pub struct AtlasQueryArgs {
    pub query: String,
    pub kind: Option<crate::atlas::query::QueryKind>,
    pub limit: Option<usize>,
    pub excerpts: Option<bool>,
    pub context_lines: Option<usize>,
    pub context_chars: Option<usize>,
}

pub async fn handle_atlas_query(
    arguments: Option<serde_json::Value>,
    _global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    let args: AtlasQueryArgs = parse_args(arguments)?;
    let limit = args.limit.unwrap_or(crate::atlas::query::DEFAULT_LIMIT);
    let context_lines = args
        .context_lines
        .unwrap_or(crate::atlas::query::DEFAULT_CONTEXT_LINES);
    let context_chars = args
        .context_chars
        .unwrap_or(crate::atlas::query::DEFAULT_CONTEXT_CHARS);
    crate::atlas::query::validate_query(&args.query, limit, context_lines)
        .map_err(to_argument_error)?;
    crate::atlas::query::validate_context(context_chars).map_err(to_argument_error)?;
    let (root, config, db) = open_atlas()?;
    let freshness =
        crate::atlas::data::query_freshness(&db, &config, &root).map_err(to_internal_error)?;
    let result = crate::atlas::query::query_data(
        &db,
        &root,
        crate::atlas::query::QueryRequest {
            query: &args.query,
            kind: args.kind.unwrap_or_default(),
            limit,
            context_lines: if args.excerpts.unwrap_or(false) {
                context_lines
            } else {
                0
            },
            context_chars,
        },
        freshness.clone(),
    )
    .map_err(to_internal_error)?;
    super::to_dual_result(AtlasBoundedOutput {
        view: AtlasTextOutput {
            format: "query".into(),
            text: serde_json::to_string(&result)
                .map_err(|error| to_internal_error(error.into()))?,
            freshness: Some(freshness),
        },
        bounds: result.bounds,
    })
}

#[derive(Deserialize, JsonSchema)]
pub struct AtlasStatusArgs {}

pub async fn handle_atlas_tree_view(
    arguments: Option<serde_json::Value>,
    _global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    let args: AtlasTreeViewArgs = parse_args(arguments)?;
    let path = args.path.map(std::path::PathBuf::from).unwrap_or_default();
    let depth = args.depth.unwrap_or(100);
    let limit = args.limit.unwrap_or(crate::atlas::query::DEFAULT_LIMIT);
    let context_chars = args
        .context_chars
        .unwrap_or(crate::atlas::query::DEFAULT_CONTEXT_CHARS);
    crate::atlas::query::validate_tree(&path, depth, limit, context_chars)
        .map_err(to_argument_error)?;
    let (root, config, db) = open_atlas()?;
    let freshness =
        crate::atlas::data::query_freshness(&db, &config, &root).map_err(to_internal_error)?;
    let (entries, bounds) = crate::atlas::query::tree_data(&db, &path, depth, limit, context_chars)
        .map_err(to_internal_error)?;

    let output = mcptools_core::atlas::format_tree(&entries, true);
    super::to_dual_result(AtlasBoundedOutput {
        view: AtlasTextOutput {
            format: "tree".to_string(),
            text: output,
            freshness: Some(freshness),
        },
        bounds,
    })
}

pub async fn handle_atlas_peek(
    arguments: Option<serde_json::Value>,
    _global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    let args: AtlasPeekArgs = parse_args(arguments)?;
    let limit = args.limit.unwrap_or(crate::atlas::query::DEFAULT_LIMIT);
    crate::atlas::query::validate_limit(limit).map_err(to_argument_error)?;
    let context_chars = args
        .context_chars
        .unwrap_or(crate::atlas::query::DEFAULT_CONTEXT_CHARS);
    crate::atlas::query::validate_context(context_chars).map_err(to_argument_error)?;
    let path = crate::atlas::query::normalize_path(std::path::Path::new(&args.path))
        .map_err(to_argument_error)?;
    let (root, config, db) = open_atlas()?;
    let freshness =
        crate::atlas::data::query_freshness(&db, &config, &root).map_err(to_internal_error)?;
    let (peek_result, bounds, children_bounds) =
        crate::atlas::query::peek_data(&db, &path, limit, context_chars)
            .map_err(to_internal_error)?;

    let output = match peek_result {
        crate::atlas::db::PeekResult::File(peek) => mcptools_core::atlas::format_peek(&peek, true),
        crate::atlas::db::PeekResult::Directory(dir_peek) => {
            mcptools_core::atlas::format_directory_peek(&dir_peek, true)
        }
    };

    super::to_dual_result(AtlasPeekOutput {
        view: AtlasBoundedOutput {
            view: AtlasTextOutput {
                format: "peek".to_string(),
                text: output,
                freshness: Some(freshness),
            },
            bounds,
        },
        children_bounds,
    })
}

pub async fn handle_atlas_status(
    arguments: Option<serde_json::Value>,
    _global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    let _: AtlasStatusArgs = parse_args(Some(arguments.unwrap_or_else(|| serde_json::json!({}))))?;
    let root = crate::atlas::cli::index::find_git_root().map_err(to_internal_error)?;
    let status = crate::atlas::data::repository_status(&root).map_err(to_internal_error)?;

    let output = mcptools_core::atlas::format_status(&status, true);
    super::to_dual_result(AtlasTextOutput {
        format: "status".to_string(),
        text: output,
        freshness: Some(format!(
            "structural={}; semantic={}",
            serde_json::to_value(status.structural)
                .map_err(|error| to_internal_error(error.into()))?
                .as_str()
                .unwrap_or("unknown"),
            status.enrichment_state
        )),
    })
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
    crate::atlas::data::open_navigation().map_err(to_internal_error)
}

fn to_argument_error(e: color_eyre::eyre::Report) -> JsonRpcError {
    JsonRpcError {
        code: -32602,
        message: format!("Invalid arguments: {e}"),
        data: None,
    }
}

fn to_internal_error(e: color_eyre::eyre::Report) -> JsonRpcError {
    JsonRpcError {
        code: -32603,
        message: format!("Tool execution error: {e}"),
        data: None,
    }
}
