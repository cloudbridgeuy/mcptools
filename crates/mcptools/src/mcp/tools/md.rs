use super::JsonRpcError;
use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Deserialize, JsonSchema)]
pub struct MdFetchArgs {
    #[serde(default)]
    pub fields: Option<Vec<String>>,
    pub url: String,
    #[serde(default)]
    pub timeout: Option<u64>,
    #[serde(default)]
    pub raw_html: Option<bool>,
    #[serde(default)]
    pub selector: Option<String>,
    #[serde(default)]
    pub strategy: Option<String>,
    #[serde(default)]
    pub index: Option<usize>,
    #[serde(default)]
    pub offset: Option<usize>,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub page: Option<usize>,
}

#[derive(Deserialize, JsonSchema)]
pub struct MdTocArgs {
    pub url: String,
    #[serde(default)]
    pub timeout: Option<u64>,
    #[serde(default)]
    pub selector: Option<String>,
    #[serde(default)]
    pub strategy: Option<String>,
    #[serde(default)]
    pub index: Option<usize>,
    #[serde(default)]
    pub output: Option<String>,
}

fn to_strategy(strategy: Option<String>) -> Result<crate::md::SelectionStrategy, JsonRpcError> {
    match strategy.as_deref() {
        None | Some("first") => Ok(crate::md::SelectionStrategy::First),
        Some("last") => Ok(crate::md::SelectionStrategy::Last),
        Some("all") => Ok(crate::md::SelectionStrategy::All),
        Some("n") => Ok(crate::md::SelectionStrategy::N),
        Some(other) => Err(JsonRpcError {
            code: -32602,
            message: format!("Invalid strategy: '{other}'. Must be 'first', 'last', 'all', or 'n'"),
            data: None,
        }),
    }
}

fn invalid_strategy_index() -> JsonRpcError {
    JsonRpcError {
        code: -32602,
        message: "Strategy 'n' requires 'index' parameter".to_string(),
        data: None,
    }
}

pub async fn handle_md_fetch(
    arguments: Option<serde_json::Value>,
    _global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    let args: MdFetchArgs = serde_json::from_value(arguments.unwrap_or(serde_json::Value::Null))
        .map_err(|e| JsonRpcError {
            code: -32602,
            message: format!("Invalid arguments: {e}"),
            data: None,
        })?;

    let strategy = to_strategy(args.strategy)?;
    if matches!(strategy, crate::md::SelectionStrategy::N) && args.index.is_none() {
        return Err(invalid_strategy_index());
    }
    let fields = args.fields;

    let fetch_data = tokio::task::spawn_blocking(move || {
        crate::md::fetch_and_convert_data(crate::md::FetchConfig {
            url: args.url,
            timeout: args.timeout.unwrap_or(30),
            raw_html: args.raw_html.unwrap_or(false),
            selector: args.selector,
            strategy,
            index: args.index,
            offset: args.offset.unwrap_or(0),
            limit: args.limit.unwrap_or(1000),
            page: args.page.unwrap_or(1),
            paginated: true,
        })
    })
    .await
    .map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Task join error: {e}"),
        data: None,
    })?
    .map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("Tool execution error: {e}"),
        data: None,
    })?;

    super::to_dual_result_projected(fetch_data, fields.as_deref())
}

pub async fn handle_md_toc(
    arguments: Option<serde_json::Value>,
    _global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    let args: MdTocArgs = serde_json::from_value(arguments.unwrap_or(serde_json::Value::Null))
        .map_err(|e| JsonRpcError {
            code: -32602,
            message: format!("Invalid arguments: {e}"),
            data: None,
        })?;

    let strategy = to_strategy(args.strategy)?;
    if matches!(strategy, crate::md::SelectionStrategy::N) && args.index.is_none() {
        return Err(invalid_strategy_index());
    }

    let output_format = match args.output.as_deref() {
        Some("markdown") => crate::md::toc::OutputFormat::Markdown,
        Some("json") => crate::md::toc::OutputFormat::Json,
        Some("indented") | None => crate::md::toc::OutputFormat::Indented,
        Some(other) => {
            return Err(JsonRpcError {
                code: -32602,
                message: format!(
                    "Invalid output format: '{other}'. Must be 'indented', 'markdown', or 'json'"
                ),
                data: None,
            });
        }
    };

    let toc_options = crate::md::TocOptions {
        url: args.url,
        timeout: args.timeout.unwrap_or(30),
        selector: args.selector,
        strategy,
        index: args.index,
        output: output_format,
        json: false,
    };

    let toc_data =
        tokio::task::spawn_blocking(move || crate::md::toc::extract_toc_data(toc_options))
            .await
            .map_err(|e| JsonRpcError {
                code: -32603,
                message: format!("Task join error: {e}"),
                data: None,
            })?
            .map_err(|e| JsonRpcError {
                code: -32603,
                message: format!("Tool execution error: {e}"),
                data: None,
            })?;

    super::to_dual_result(toc_data)
}
