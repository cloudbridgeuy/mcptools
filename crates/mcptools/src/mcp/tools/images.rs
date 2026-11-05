use super::JsonRpcError;
use mcptools_core::images::{CHAT_MAINLINE_DEFAULT, DEFAULT_BASE_URL, DEFAULT_MODEL};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::images::{Api, Backend, BackendOpts};

#[derive(Deserialize, JsonSchema)]
pub struct ImagesGenerateArgs {
    pub prompt: String,
    #[schemars(
        description = "Image model (default: gpt-image-2.6-flare). gpt-image-2.6-flare and gpt-image-2.6-sunburst are supported via api=openai."
    )]
    pub model: Option<String>,
    pub size: Option<String>,
    #[schemars(
        description = "Quality: auto|low|medium|high|xhigh|max. For SVG, selects reference raster quality; auto omits the field."
    )]
    pub quality: Option<String>,
    #[serde(default, rename = "outputFormat")]
    #[schemars(
        description = "png|jpeg|webp|gif|svg. GIF and SVG require api=openai; SVG requires n=1."
    )]
    pub output_format: Option<String>,
    #[serde(default, rename = "outputCompression")]
    pub output_compression: Option<u8>,
    #[serde(default, rename = "frameCount")]
    #[schemars(description = "GIF only: 2-64 frames, default 9.")]
    pub frame_count: Option<u8>,
    #[serde(default, rename = "frameDurationMs")]
    #[schemars(description = "GIF only: 20-2000 ms, rounded to 10 ms; default 200.")]
    pub frame_duration_ms: Option<u16>,
    #[schemars(
        description = "Generation only, api=openai: low|medium|high; default high, independent of quality."
    )]
    pub complexity: Option<String>,
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
    #[schemars(
        description = "Local transparent PNG markup path matching the first image dimensions; requires api=openai and cannot be combined with mask."
    )]
    pub markup: Option<String>,
    #[schemars(
        description = "Image model (default: gpt-image-2.6-flare). gpt-image-2.6-flare and gpt-image-2.6-sunburst are supported via api=openai."
    )]
    pub model: Option<String>,
    pub size: Option<String>,
    pub quality: Option<String>,
    #[serde(default, rename = "outputFormat")]
    pub output_format: Option<String>,
    #[serde(default, rename = "outputCompression")]
    pub output_compression: Option<u8>,
    #[serde(default, rename = "frameCount")]
    pub frame_count: Option<u8>,
    #[serde(default, rename = "frameDurationMs")]
    pub frame_duration_ms: Option<u16>,
    pub complexity: Option<String>,
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
    #[schemars(
        description = "Image model (default: gpt-image-2.6-flare). gpt-image-2.6-flare and gpt-image-2.6-sunburst are supported via api=openai."
    )]
    pub model: Option<String>,
    pub size: Option<String>,
    pub quality: Option<String>,
    #[serde(default, rename = "outputFormat")]
    pub output_format: Option<String>,
    #[serde(default, rename = "outputCompression")]
    pub output_compression: Option<u8>,
    #[serde(default, rename = "frameCount")]
    pub frame_count: Option<u8>,
    #[serde(default, rename = "frameDurationMs")]
    pub frame_duration_ms: Option<u16>,
    pub complexity: Option<String>,
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
        frame_count: args.frame_count,
        frame_duration_ms: args.frame_duration_ms,
        complexity: args.complexity,
        background: args.background.unwrap_or_else(|| "auto".to_string()),
        moderation: args.moderation,
        n: args.n.unwrap_or(1),
    };
    mcptools_core::images::validate_generate(&params).map_err(invalid)?;
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
    mcptools_core::images::validate_edit_options(
        args.frame_count,
        args.frame_duration_ms,
        args.complexity.as_deref(),
    )
    .map_err(invalid)?;
    mcptools_core::images::validate_edit_format(args.output_format.as_deref().unwrap_or("png"))
        .map_err(invalid)?;
    for image in &args.images {
        mcptools_core::images::validate_edit_filename(image).map_err(invalid)?;
    }
    if let Some(mask) = &args.mask {
        mcptools_core::images::validate_edit_filename(mask).map_err(invalid)?;
    }
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
        args.markup.map(std::path::PathBuf::from),
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
    mcptools_core::images::validate_edit_options(
        args.frame_count,
        args.frame_duration_ms,
        args.complexity.as_deref(),
    )
    .map_err(invalid)?;
    mcptools_core::images::validate_edit_format(args.output_format.as_deref().unwrap_or("png"))
        .map_err(invalid)?;
    for image in &args.images {
        mcptools_core::images::validate_edit_filename(image).map_err(invalid)?;
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn markup_schema_and_handler_are_wired() {
        let schema = super::super::schema::input_schema_for::<ImagesEditArgs>();
        assert!(schema["properties"]["markup"]["description"]
            .as_str()
            .unwrap()
            .contains("PNG"));
        let global = crate::Global::try_parse_from(["mcptools"]).unwrap();
        let args = serde_json::json!({"prompt": "edit", "images": ["missing.png"], "markup": "missing-markup.png"});
        let error = handle_images_edit(Some(args), &global).await.unwrap_err();
        assert!(error.message.contains("markup requires"));
    }

    #[tokio::test]
    async fn mcp_svg_sends_options_and_saves_xml() {
        let server = MockServer::start().await;
        Mock::given(wiremock::matchers::method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "created": 1, "data": [{"b64_json": "PHN2Zy8+"}]
            })))
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().unwrap();
        let global = crate::Global::try_parse_from(["mcptools"]).unwrap();
        handle_images_generate(
            Some(serde_json::json!({
                "prompt": "fox", "api": "openai", "apiKey": "test", "baseUrl": server.uri(),
                "outputDir": dir.path(), "outputFormat": "svg", "complexity": "medium"
            })),
            &global,
        )
        .await
        .unwrap();
        assert_eq!(
            std::fs::read(dir.path().join("image-1-0.svg")).unwrap(),
            b"<svg/>"
        );
        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].url.path(), "/images/generations");
        let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
        assert_eq!(body["output_format"], "svg");
        assert_eq!(body["n"], 1);
        assert_eq!(body["complexity"], "medium");
        for field in [
            "frame_count",
            "frame_duration_ms",
            "stream",
            "partial_images",
            "output_compression",
            "prompt_rewriter",
        ] {
            assert!(body.get(field).is_none());
        }
    }

    #[tokio::test]
    async fn rejects_invalid_formats_before_requests() {
        let server = MockServer::start().await;
        let global = crate::Global::try_parse_from(["mcptools"]).unwrap();
        for (args, message) in [
            (
                serde_json::json!({"prompt": "fox", "api": "openai", "apiKey": "test", "baseUrl": server.uri(), "outputFormat": "svg", "n": 2}),
                "SVG requires n=1",
            ),
            (
                serde_json::json!({"prompt": "fox", "api": "openai", "apiKey": "test", "baseUrl": server.uri(), "frameCount": 9}),
                "frame options require GIF output",
            ),
            (
                serde_json::json!({"prompt": "fox", "api": "openai", "apiKey": "test", "baseUrl": server.uri(), "frameDurationMs": 200}),
                "frame options require GIF output",
            ),
            (
                serde_json::json!({"prompt": "fox", "api": "openai", "apiKey": "test", "baseUrl": server.uri(), "outputFormat": "svg", "frameCount": 9}),
                "frame options require GIF output",
            ),
            (
                serde_json::json!({"prompt": "fox", "api": "chatgpt", "outputFormat": "gif"}),
                "require api=openai",
            ),
            (
                serde_json::json!({"prompt": "fox", "api": "chatgpt", "outputFormat": "svg"}),
                "require api=openai",
            ),
            (
                serde_json::json!({"prompt": "fox", "api": "chatgpt", "complexity": "high"}),
                "require api=openai",
            ),
        ] {
            let error = handle_images_generate(Some(args), &global)
                .await
                .unwrap_err();
            assert!(error.message.contains(message), "{}", error.message);
        }
        for (args, message) in [
            (
                serde_json::json!({"prompt": "fox", "images": ["in.png"], "outputFormat": "svg"}),
                "GIF and SVG output is generation only",
            ),
            (
                serde_json::json!({"prompt": "fox", "images": ["in.png"], "outputFormat": "gif"}),
                "GIF and SVG output is generation only",
            ),
            (
                serde_json::json!({"prompt": "fox", "images": ["in.svg"]}),
                "SVG input is not supported for edits",
            ),
            (
                serde_json::json!({"prompt": "fox", "images": ["in.png"], "complexity": "high"}),
                "frame options and complexity are generation only",
            ),
        ] {
            let error = handle_images_edit(Some(args.clone()), &global)
                .await
                .unwrap_err();
            assert!(error.message.contains(message), "{}", error.message);
            let error = handle_images_vary(Some(args), &global).await.unwrap_err();
            assert!(error.message.contains(message), "{}", error.message);
        }
        assert!(server.received_requests().await.unwrap().is_empty());
    }

    #[test]
    fn model_schema_describes_image_26_availability() {
        let schemas = [
            super::super::schema::input_schema_for::<ImagesGenerateArgs>(),
            super::super::schema::input_schema_for::<ImagesEditArgs>(),
            super::super::schema::input_schema_for::<ImagesVaryArgs>(),
        ];
        for schema in schemas {
            let description = schema["properties"]["model"]["description"]
                .as_str()
                .unwrap();
            assert!(description.contains("default: gpt-image-2.6-flare"));
            assert!(description.contains("gpt-image-2.6-sunburst"));
            assert!(description.to_lowercase().contains("api"));
        }
    }

    #[tokio::test]
    async fn handlers_pass_models_to_openai_requests() {
        let server = MockServer::start().await;
        Mock::given(wiremock::matchers::method("POST"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"created": 1, "data": []})),
            )
            .mount(&server)
            .await;
        let global = crate::Global::try_parse_from(["mcptools"]).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let image = dir.path().join("input.png");
        std::fs::write(&image, b"image").unwrap();
        for model in ["gpt-image-2.6-flare", "gpt-image-2.6-sunburst"] {
            for command in ["generate", "edit", "vary"] {
                let args = serde_json::json!({
                    "prompt": "otter", "images": [image], "model": model,
                    "api": "openai", "apiKey": "test", "baseUrl": server.uri()
                });
                match command {
                    "generate" => handle_images_generate(Some(args), &global).await.unwrap(),
                    "edit" => handle_images_edit(Some(args), &global).await.unwrap(),
                    _ => handle_images_vary(Some(args), &global).await.unwrap(),
                };
                let requests = server.received_requests().await.unwrap();
                let request = requests.last().unwrap();
                if command == "generate" {
                    let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
                    assert_eq!(body["model"], model);
                } else {
                    let body = String::from_utf8_lossy(&request.body);
                    assert!(body.contains(&format!("name=\"model\"\r\n\r\n{model}\r\n")));
                }
            }
        }
    }
}
