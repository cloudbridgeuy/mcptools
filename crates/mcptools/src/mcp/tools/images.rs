use super::JsonRpcError;
use mcptools_core::images::{CHAT_MAINLINE_DEFAULT, DEFAULT_BASE_URL, DEFAULT_MODEL};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::images::{Api, Backend, BackendOpts};

#[derive(Deserialize, JsonSchema)]
pub struct ImagesGenerateArgs {
    pub prompt: String,
    pub model: Option<String>,
    pub size: Option<String>,
    pub quality: Option<String>,
    #[serde(default, rename = "outputFormat")]
    pub output_format: Option<String>,
    #[serde(default, rename = "outputCompression")]
    pub output_compression: Option<u8>,
    pub background: Option<String>,
    pub moderation: Option<String>,
    pub n: Option<u8>,
    #[serde(default, rename = "outputDir")]
    pub output_dir: Option<String>,
    pub api: Option<String>,
    #[serde(default, rename = "configDir")]
    pub config_dir: Option<String>,
    #[serde(default, rename = "apiKey")]
    pub api_key: Option<String>,
    #[serde(default, rename = "baseUrl")]
    pub base_url: Option<String>,
    #[serde(default, rename = "mainline")]
    pub mainline: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct ImagesEditArgs {
    pub prompt: String,
    pub images: Vec<String>,
    pub mask: Option<String>,
    pub model: Option<String>,
    pub size: Option<String>,
    pub quality: Option<String>,
    #[serde(default, rename = "outputFormat")]
    pub output_format: Option<String>,
    #[serde(default, rename = "outputCompression")]
    pub output_compression: Option<u8>,
    pub background: Option<String>,
    pub moderation: Option<String>,
    #[serde(default, rename = "inputFidelity")]
    pub input_fidelity: Option<String>,
    pub n: Option<u8>,
    #[serde(default, rename = "outputDir")]
    pub output_dir: Option<String>,
    pub api: Option<String>,
    #[serde(default, rename = "configDir")]
    pub config_dir: Option<String>,
    #[serde(default, rename = "apiKey")]
    pub api_key: Option<String>,
    #[serde(default, rename = "baseUrl")]
    pub base_url: Option<String>,
    #[serde(default, rename = "mainline")]
    pub mainline: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct ImagesVaryArgs {
    pub images: Vec<String>,
    pub prompt: Option<String>,
    pub model: Option<String>,
    pub size: Option<String>,
    pub quality: Option<String>,
    #[serde(default, rename = "outputFormat")]
    pub output_format: Option<String>,
    #[serde(default, rename = "outputCompression")]
    pub output_compression: Option<u8>,
    pub background: Option<String>,
    pub moderation: Option<String>,
    #[serde(default, rename = "inputFidelity")]
    pub input_fidelity: Option<String>,
    pub n: Option<u8>,
    #[serde(default, rename = "outputDir")]
    pub output_dir: Option<String>,
    pub api: Option<String>,
    #[serde(default, rename = "configDir")]
    pub config_dir: Option<String>,
    #[serde(default, rename = "apiKey")]
    pub api_key: Option<String>,
    #[serde(default, rename = "baseUrl")]
    pub base_url: Option<String>,
    #[serde(default, rename = "mainline")]
    pub mainline: Option<String>,
}

#[derive(Deserialize, Default)]
struct BackendArgs {
    api: Option<String>,
    #[serde(default, rename = "configDir")]
    config_dir: Option<String>,
    #[serde(default, rename = "apiKey")]
    api_key: Option<String>,
    #[serde(default, rename = "baseUrl")]
    base_url: Option<String>,
    #[serde(default, rename = "mainline")]
    mainline: Option<String>,
}

fn invalid(e: impl std::fmt::Display) -> JsonRpcError {
    JsonRpcError {
        code: -32602,
        message: format!("Invalid arguments: {e}"),
        data: None,
    }
}

fn exec_err(e: impl std::fmt::Display) -> JsonRpcError {
    JsonRpcError {
        code: -32603,
        message: format!("Tool execution error: {e}"),
        data: None,
    }
}

fn backend_of(a: &BackendArgs) -> Result<Backend, JsonRpcError> {
    let opts = BackendOpts {
        api: match a.api.as_deref().unwrap_or("chatgpt") {
            "openai" => Api::Openai,
            "chatgpt" => Api::Chatgpt,
            other => {
                return Err(JsonRpcError {
                    code: -32602,
                    message: format!("Invalid api '{other}': want chatgpt|openai"),
                    data: None,
                })
            }
        },
        config_dir: a.config_dir.clone(),
        api_key: a
            .api_key
            .clone()
            .or_else(|| std::env::var("OPENAI_API_KEY").ok()),
        base_url: a
            .base_url
            .clone()
            .unwrap_or_else(|| DEFAULT_BASE_URL.to_string()),
        mainline: a
            .mainline
            .clone()
            .unwrap_or_else(|| CHAT_MAINLINE_DEFAULT.to_string()),
    };
    match opts.api {
        Api::Chatgpt => Ok(Backend::ChatGpt {
            config_dir: mcptools_core::images::resolve_config_dir(opts.config_dir.as_deref()),
            mainline: opts.mainline,
        }),
        Api::Openai => {
            let key = opts.api_key.filter(|k| !k.is_empty()).ok_or(JsonRpcError {
                code: -32603,
                message: "Missing OpenAI API key. Set OPENAI_API_KEY.".to_string(),
                data: None,
            })?;
            Ok(Backend::OpenAi {
                api_key: key,
                base_url: opts.base_url,
            })
        }
    }
}

fn out_dir(dir: Option<String>) -> std::path::PathBuf {
    dir.map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("."))
}

pub async fn handle_images_generate(
    arguments: Option<serde_json::Value>,
    _global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    let args: ImagesGenerateArgs =
        serde_json::from_value(arguments.unwrap_or(serde_json::Value::Null)).map_err(invalid)?;
    let params = mcptools_core::images::GenerateParams {
        prompt: args.prompt,
        model: args.model.unwrap_or_else(|| DEFAULT_MODEL.to_string()),
        size: args.size.unwrap_or_else(|| "auto".to_string()),
        quality: args.quality.unwrap_or_else(|| "auto".to_string()),
        output_format: args.output_format.unwrap_or_else(|| "png".to_string()),
        output_compression: args.output_compression,
        background: args.background.unwrap_or_else(|| "auto".to_string()),
        moderation: args.moderation,
        n: args.n.unwrap_or(1),
    };
    let saved = crate::images::generate_data(
        params,
        backend_of(&BackendArgs {
            api: args.api,
            config_dir: args.config_dir,
            api_key: args.api_key,
            base_url: args.base_url,
            mainline: args.mainline,
        })?,
        None,
        out_dir(args.output_dir),
    )
    .await
    .map_err(exec_err)?;
    super::to_dual_result(saved)
}

pub async fn handle_images_edit(
    arguments: Option<serde_json::Value>,
    _global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    let args: ImagesEditArgs =
        serde_json::from_value(arguments.unwrap_or(serde_json::Value::Null)).map_err(invalid)?;
    let params = mcptools_core::images::EditParams {
        prompt: args.prompt,
        model: args.model.unwrap_or_else(|| DEFAULT_MODEL.to_string()),
        size: args.size.unwrap_or_else(|| "auto".to_string()),
        quality: args.quality.unwrap_or_else(|| "auto".to_string()),
        output_format: args.output_format.unwrap_or_else(|| "png".to_string()),
        output_compression: args.output_compression,
        background: args.background.unwrap_or_else(|| "auto".to_string()),
        moderation: args.moderation,
        input_fidelity: args.input_fidelity,
        n: args.n.unwrap_or(1),
    };
    let saved = crate::images::edit_data(
        params,
        args.images
            .into_iter()
            .map(std::path::PathBuf::from)
            .collect(),
        args.mask.map(std::path::PathBuf::from),
        backend_of(&BackendArgs {
            api: args.api,
            config_dir: args.config_dir,
            api_key: args.api_key,
            base_url: args.base_url,
            mainline: args.mainline,
        })?,
        None,
        out_dir(args.output_dir),
    )
    .await
    .map_err(exec_err)?;
    super::to_dual_result(saved)
}

pub async fn handle_images_vary(
    arguments: Option<serde_json::Value>,
    _global: &crate::Global,
) -> Result<serde_json::Value, JsonRpcError> {
    let args: ImagesVaryArgs =
        serde_json::from_value(arguments.unwrap_or(serde_json::Value::Null)).map_err(invalid)?;
    let saved = crate::images::vary_data(
        args.images
            .into_iter()
            .map(std::path::PathBuf::from)
            .collect(),
        args.prompt,
        args.model.unwrap_or_else(|| DEFAULT_MODEL.to_string()),
        args.size.unwrap_or_else(|| "auto".to_string()),
        args.quality.unwrap_or_else(|| "auto".to_string()),
        args.output_format.unwrap_or_else(|| "png".to_string()),
        args.output_compression,
        args.background.unwrap_or_else(|| "auto".to_string()),
        args.moderation,
        args.input_fidelity,
        args.n.unwrap_or(1),
        backend_of(&BackendArgs {
            api: args.api,
            config_dir: args.config_dir,
            api_key: args.api_key,
            base_url: args.base_url,
            mainline: args.mainline,
        })?,
        None,
        out_dir(args.output_dir),
    )
    .await
    .map_err(exec_err)?;
    super::to_dual_result(saved)
}
