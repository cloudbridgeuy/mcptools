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
    mcptools_core::md::validate_pagination(args.limit.unwrap_or(1000), args.page.unwrap_or(1))
        .map_err(|message| JsonRpcError {
            code: -32602,
            message,
            data: None,
        })?;

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
        output: output_format.clone(),
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

    toc_result(toc_data, args.output.as_ref().map(|_| &output_format))
}

fn toc_result(
    toc_data: crate::md::toc::TocOutput,
    format: Option<&crate::md::toc::OutputFormat>,
) -> Result<serde_json::Value, JsonRpcError> {
    use crate::md::toc::{format_toc_indented, format_toc_markdown, OutputFormat};

    let text = match format {
        Some(OutputFormat::Indented) => Some(format_toc_indented(&toc_data.entries)),
        Some(OutputFormat::Markdown) => Some(format_toc_markdown(&toc_data.entries)),
        Some(OutputFormat::Json) | None => None,
    };
    let mut result = super::to_dual_result(toc_data)?;
    if let Some(text) = text {
        result["content"][0]["text"] = serde_json::Value::String(text);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::md::toc::{OutputFormat, TocOutput};
    use serde_json::json;

    #[tokio::test]
    async fn fetch_rejects_zero_pagination_as_invalid_arguments() {
        let global = crate::Global {
            verbose: false,
            execute_timeout_secs: 30,
            execute_memory_mb: 64,
            execute_output_kb: 256,
        };
        for (field, message) in [
            ("limit", "limit must be greater than 0"),
            ("page", "page must be greater than 0 (1-indexed)"),
        ] {
            let mut args = json!({"url": "https://invalid.invalid", "offset": 100});
            args[field] = json!(0);
            let error = handle_md_fetch(Some(args), &global).await.unwrap_err();
            assert_eq!(error.code, -32602);
            assert_eq!(error.message, message);
        }
    }

    #[test]
    fn toc_formats_text_without_changing_structured_output() {
        let data = json!({
            "url": "https://example.com",
            "title": null,
            "entries": [
                {"level": 1, "text": "First", "char_offset": 0, "char_limit": 20},
                {"level": 2, "text": "Child", "char_offset": 10, "char_limit": 10}
            ],
            "fetch_time_ms": 0
        });
        for format in [
            None,
            Some(OutputFormat::Json),
            Some(OutputFormat::Indented),
            Some(OutputFormat::Markdown),
        ] {
            let output: TocOutput = serde_json::from_value(data.clone()).unwrap();
            let result = toc_result(output, format.as_ref()).unwrap();
            assert_eq!(result["structuredContent"], data);
            let text = result["content"][0]["text"].as_str().unwrap();
            match format {
                None | Some(OutputFormat::Json) => {
                    assert_eq!(
                        serde_json::from_str::<serde_json::Value>(text).unwrap(),
                        data
                    );
                }
                Some(OutputFormat::Indented) => assert_eq!(
                    text,
                    "First  [--offset 0 --limit 20]\n  Child  [--offset 10 --limit 10]"
                ),
                Some(OutputFormat::Markdown) => assert_eq!(
                    text,
                    "* First  [--offset 0 --limit 20]\n  * Child  [--offset 10 --limit 10]"
                ),
            }
        }
    }
}
