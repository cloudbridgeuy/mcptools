pub mod auth;

use crate::prelude::{eprintln, println, *};
use mcptools_core::images::{
    self, EditParams, GenerateParams, ImagesResponse, CHAT_MAINLINE_DEFAULT, CODEX_RESPONSES_URL,
    DEFAULT_BASE_URL, DEFAULT_MODEL, MAX_INPUT_IMAGES, VARY_PROMPT,
};
use schemars::JsonSchema;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct SavedOutput {
    pub files: Vec<String>,
    pub model: String,
    pub size: String,
    pub quality: String,
    pub output_format: String,
    pub usage: Option<images::Usage>,
}

#[derive(Debug, Clone, clap::ValueEnum)]
pub enum Api {
    Chatgpt,
    Openai,
}

#[derive(Debug, clap::Args)]
pub struct BackendOpts {
    #[clap(
        long,
        value_enum,
        env = "MCPTOOLS_IMAGES_API",
        default_value = "chatgpt"
    )]
    pub api: Api,
    #[clap(long, env = "LLM_STREAM_CONFIG_DIR")]
    pub config_dir: Option<String>,
    #[clap(long, env = "OPENAI_API_KEY")]
    pub api_key: Option<String>,
    #[clap(long, env = "OPENAI_BASE_URL", default_value = DEFAULT_BASE_URL)]
    pub base_url: String,
    #[clap(long, env = "MCPTOOLS_IMAGES_MAINLINE", default_value = CHAT_MAINLINE_DEFAULT)]
    pub mainline: String,
}

#[derive(Debug, clap::Args)]
pub struct OutOpts {
    #[clap(long, env = "OPENAI_IMAGES_MODEL", default_value = DEFAULT_MODEL)]
    pub model: String,
    #[clap(long, default_value = "auto")]
    pub size: String,
    #[clap(long, default_value = "auto")]
    pub quality: String,
    #[clap(long = "format", default_value = "png")]
    pub format: String,
    #[clap(long)]
    pub compression: Option<u8>,
    #[clap(long, default_value = "auto")]
    pub background: String,
    #[clap(long)]
    pub moderation: Option<String>,
    #[clap(long, short = 'o')]
    pub output: Option<std::path::PathBuf>,
    #[clap(long, default_value = ".")]
    pub output_dir: std::path::PathBuf,
    #[clap(long)]
    pub json: bool,
    #[clap(long)]
    pub open: bool,
}

#[derive(Debug, clap::Parser)]
#[command(name = "images")]
#[command(
    about = "ChatGPT Images 2.5 generation, editing and variations (gpt-image-2.5-flare/sunburst)"
)]
pub struct App {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, clap::Subcommand)]
pub enum Commands {
    #[clap(name = "generate")]
    Generate(GenerateOptions),
    #[clap(name = "edit")]
    Edit(EditOptions),
    #[clap(name = "vary")]
    Vary(VaryOptions),
}

#[derive(Debug, clap::Parser)]
pub struct GenerateOptions {
    pub prompt: String,
    #[clap(long, short = 'n', default_value_t = 1)]
    pub number: u8,
    #[clap(flatten)]
    pub out: OutOpts,
    #[clap(flatten)]
    pub backend: BackendOpts,
}

#[derive(Debug, clap::Parser)]
pub struct EditOptions {
    pub prompt: String,
    #[clap(long = "image", required = true)]
    pub images: Vec<std::path::PathBuf>,
    #[clap(long)]
    pub mask: Option<std::path::PathBuf>,
    #[clap(long)]
    pub input_fidelity: Option<String>,
    #[clap(long, short = 'n', default_value_t = 1)]
    pub number: u8,
    #[clap(flatten)]
    pub out: OutOpts,
    #[clap(flatten)]
    pub backend: BackendOpts,
}

#[derive(Debug, clap::Parser)]
pub struct VaryOptions {
    #[clap(long = "image", required = true)]
    pub images: Vec<std::path::PathBuf>,
    #[clap(long)]
    pub prompt: Option<String>,
    #[clap(long)]
    pub input_fidelity: Option<String>,
    #[clap(long, short = 'n', default_value_t = 1)]
    pub number: u8,
    #[clap(flatten)]
    pub out: OutOpts,
    #[clap(flatten)]
    pub backend: BackendOpts,
}

pub enum Backend {
    ChatGpt {
        config_dir: std::path::PathBuf,
        mainline: String,
    },
    OpenAi {
        api_key: String,
        base_url: String,
    },
}

fn backend_of(o: &BackendOpts) -> Result<Backend> {
    match o.api {
        Api::Chatgpt => Ok(Backend::ChatGpt {
            config_dir: images::resolve_config_dir(o.config_dir.as_deref()),
            mainline: o.mainline.clone(),
        }),
        Api::Openai => Ok(Backend::OpenAi {
            api_key: o.api_key.clone().filter(|k| !k.is_empty()).ok_or_else(|| {
                eyre!("Missing OpenAI API key. Set OPENAI_API_KEY or pass --api-key.")
            })?,
            base_url: o.base_url.clone(),
        }),
    }
}

pub async fn run(app: App, _global: crate::Global) -> Result<()> {
    match app.command {
        Commands::Generate(o) => {
            let open = o.out.open;
            let json = o.out.json;
            let saved = generate_cli(o).await?;
            print_saved(&saved, json)?;
            maybe_open(&saved.files, open);
            Ok(())
        }
        Commands::Edit(o) => {
            let open = o.out.open;
            let json = o.out.json;
            let saved = edit_cli(o).await?;
            print_saved(&saved, json)?;
            maybe_open(&saved.files, open);
            Ok(())
        }
        Commands::Vary(o) => {
            let open = o.out.open;
            let json = o.out.json;
            let saved = vary_cli(o).await?;
            print_saved(&saved, json)?;
            maybe_open(&saved.files, open);
            Ok(())
        }
    }
}

fn opener() -> &'static str {
    if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    }
}

fn maybe_open(files: &[String], open: bool) {
    if !open {
        return;
    }
    match std::process::Command::new(opener()).args(files).status() {
        Ok(_) => {}
        Err(e) => eprintln!("warning: failed to open image: {e}"),
    }
}

fn print_saved(saved: &SavedOutput, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(saved)?);
    } else {
        for f in &saved.files {
            println!("{f}");
        }
    }
    Ok(())
}

fn validate_out(o: &OutOpts, number: u8) -> Result<()> {
    images::validate_size(&o.size).map_err(|e| eyre!(e))?;
    images::validate_quality(&o.quality).map_err(|e| eyre!(e))?;
    images::validate_format(&o.format).map_err(|e| eyre!(e))?;
    images::validate_background(&o.background).map_err(|e| eyre!(e))?;
    images::validate_compression(o.compression).map_err(|e| eyre!(e))?;
    images::validate_n(number).map_err(|e| eyre!(e))?;
    if let Some(m) = &o.moderation {
        if m != "auto" && m != "low" {
            return Err(eyre!("moderation must be auto|low, got {m}"));
        }
    }
    if o.output.is_some() && number != 1 {
        return Err(eyre!("--output requires -n 1, got {number}"));
    }
    Ok(())
}

fn validate_fidelity(f: &Option<String>) -> Result<()> {
    if let Some(v) = f {
        if v != "high" && v != "low" {
            return Err(eyre!("input-fidelity must be high|low, got {v}"));
        }
    }
    Ok(())
}

fn gen_params(prompt: String, number: u8, o: &OutOpts) -> GenerateParams {
    GenerateParams {
        prompt,
        model: o.model.clone(),
        size: o.size.clone(),
        quality: o.quality.clone(),
        output_format: o.format.clone(),
        output_compression: o.compression,
        background: o.background.clone(),
        moderation: o.moderation.clone(),
        n: number,
    }
}

fn edit_params(prompt: String, number: u8, fidelity: Option<String>, o: &OutOpts) -> EditParams {
    EditParams {
        prompt,
        model: o.model.clone(),
        size: o.size.clone(),
        quality: o.quality.clone(),
        output_format: o.format.clone(),
        output_compression: o.compression,
        background: o.background.clone(),
        moderation: o.moderation.clone(),
        input_fidelity: fidelity,
        n: number,
    }
}

fn start_spinner(msg: &str) -> indicatif::ProgressBar {
    let spinner = indicatif::ProgressBar::new_spinner();
    spinner.set_style(
        indicatif::ProgressStyle::default_spinner()
            .template("{spinner:.cyan} {msg}")
            .unwrap(),
    );
    spinner.set_message(msg.to_string());
    spinner.enable_steady_tick(std::time::Duration::from_millis(100));
    spinner
}

async fn generate_cli(o: GenerateOptions) -> Result<SavedOutput> {
    validate_out(&o.out, o.number)?;
    let backend = backend_of(&o.backend)?;
    if matches!(backend, Backend::ChatGpt { .. }) && o.number != 1 {
        return Err(eyre!("--api chatgpt generates one image per call; drop -n"));
    }
    let spinner = start_spinner("Generating image...");
    let result = generate_data(
        gen_params(o.prompt, o.number, &o.out),
        backend,
        o.out.output,
        o.out.output_dir,
    )
    .await;
    spinner.finish_and_clear();
    result
}

async fn edit_cli(o: EditOptions) -> Result<SavedOutput> {
    validate_out(&o.out, o.number)?;
    validate_fidelity(&o.input_fidelity)?;
    images::validate_image_count(o.images.len()).map_err(|e| eyre!(e))?;
    let backend = backend_of(&o.backend)?;
    if matches!(backend, Backend::ChatGpt { .. }) && o.number != 1 {
        return Err(eyre!("--api chatgpt generates one image per call; drop -n"));
    }
    let spinner = start_spinner("Editing image...");
    let result = edit_data(
        edit_params(o.prompt, o.number, o.input_fidelity, &o.out),
        o.images,
        o.mask,
        backend,
        o.out.output,
        o.out.output_dir,
    )
    .await;
    spinner.finish_and_clear();
    result
}

async fn vary_cli(o: VaryOptions) -> Result<SavedOutput> {
    validate_out(&o.out, o.number)?;
    validate_fidelity(&o.input_fidelity)?;
    images::validate_image_count(o.images.len()).map_err(|e| eyre!(e))?;
    let backend = backend_of(&o.backend)?;
    if matches!(backend, Backend::ChatGpt { .. }) && o.number != 1 {
        return Err(eyre!("--api chatgpt generates one image per call; drop -n"));
    }
    let spinner = start_spinner("Creating variation...");
    let result = vary_data(
        o.images,
        o.prompt,
        o.out.model.clone(),
        o.out.size.clone(),
        o.out.quality.clone(),
        o.out.format.clone(),
        o.out.compression,
        o.out.background.clone(),
        o.out.moderation.clone(),
        o.input_fidelity,
        o.number,
        backend,
        o.out.output,
        o.out.output_dir,
    )
    .await;
    spinner.finish_and_clear();
    result
}

fn resolve_targets(
    output: &Option<std::path::PathBuf>,
    dir: &std::path::PathBuf,
    count: usize,
    format: &str,
    created: u64,
) -> Result<Vec<std::path::PathBuf>> {
    if let Some(single) = output {
        return Ok(vec![single.clone()]);
    }
    std::fs::create_dir_all(dir)?;
    Ok((0..count)
        .map(|i| dir.join(images::suggested_filename("image", i, format, created)))
        .collect())
}

fn decode_b64(b64: &str) -> Result<Vec<u8>> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|e| eyre!("Failed to decode image: {e}"))
}

fn write_targets(targets: &[std::path::PathBuf], bytes: &[Vec<u8>]) -> Result<Vec<String>> {
    let mut out = Vec::with_capacity(bytes.len());
    for (path, data) in targets.iter().zip(bytes.iter()) {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        std::fs::write(path, data)?;
        out.push(path.to_string_lossy().to_string());
    }
    Ok(out)
}

async fn post_openai_generate(
    client: &reqwest::Client,
    base_url: &str,
    key: &str,
    body: &serde_json::Value,
) -> Result<ImagesResponse> {
    let url = format!("{}/images/generations", base_url.trim_end_matches('/'));
    let resp = client
        .post(&url)
        .bearer_auth(key)
        .json(body)
        .send()
        .await
        .map_err(|e| eyre!("Image request failed: {e}"))?;
    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(eyre!(images::error_hint(status.as_u16(), &text)));
    }
    resp.json()
        .await
        .map_err(|e| eyre!("Failed to parse image response: {e}"))
}

async fn image_part(path: &std::path::Path) -> Result<reqwest::multipart::Part> {
    let bytes = tokio::fs::read(path)
        .await
        .map_err(|e| eyre!("Failed to read '{}': {e}", path.display()))?;
    let name = path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "image.png".to_string());
    let mime = images::mime_for_filename(&name).to_string();
    reqwest::multipart::Part::bytes(bytes)
        .file_name(name)
        .mime_str(&mime)
        .map_err(|e| eyre!("Invalid mime: {e}"))
}

fn edit_form(
    p: &EditParams,
    image_parts: Vec<reqwest::multipart::Part>,
    mask: Option<reqwest::multipart::Part>,
) -> reqwest::multipart::Form {
    let mut form = reqwest::multipart::Form::new()
        .text("model", p.model.clone())
        .text("prompt", p.prompt.clone())
        .text("size", p.size.clone())
        .text("quality", p.quality.clone())
        .text("output_format", p.output_format.clone())
        .text("background", p.background.clone())
        .text("n", p.n.to_string());
    if let Some(c) = p.output_compression {
        form = form.text("output_compression", c.to_string());
    }
    if let Some(m) = &p.moderation {
        form = form.text("moderation", m.clone());
    }
    if let Some(f) = &p.input_fidelity {
        form = form.text("input_fidelity", f.clone());
    }
    for part in image_parts {
        form = form.part("image[]", part);
    }
    if let Some(m) = mask {
        form = form.part("mask", m);
    }
    form
}

async fn post_openai_edit(
    client: &reqwest::Client,
    base_url: &str,
    key: &str,
    form: reqwest::multipart::Form,
) -> Result<ImagesResponse> {
    let url = format!("{}/images/edits", base_url.trim_end_matches('/'));
    let resp = client
        .post(&url)
        .bearer_auth(key)
        .multipart(form)
        .send()
        .await
        .map_err(|e| eyre!("Image edit request failed: {e}"))?;
    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(eyre!(images::error_hint(status.as_u16(), &text)));
    }
    resp.json()
        .await
        .map_err(|e| eyre!("Failed to parse image response: {e}"))
}

async fn read_data_url(path: &std::path::Path) -> Result<String> {
    use base64::Engine;
    let bytes = tokio::fs::read(path)
        .await
        .map_err(|e| eyre!("Failed to read '{}': {e}", path.display()))?;
    let name = path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "image.png".to_string());
    Ok(images::data_url(
        images::mime_for_filename(&name),
        &base64::engine::general_purpose::STANDARD.encode(&bytes),
    ))
}

async fn post_responses(
    client: &reqwest::Client,
    tokens: &images::TokenSet,
    body: &serde_json::Value,
) -> Result<String> {
    let mut stream_body = body.clone();
    stream_body["stream"] = serde_json::json!(true);
    let mut req = client
        .post(CODEX_RESPONSES_URL)
        .bearer_auth(&tokens.access_token)
        .header("accept", "text/event-stream")
        .header("openai-beta", "responses=experimental")
        .header("originator", "mcptools")
        .json(&stream_body);
    if let Some(account) = tokens.account_id.as_deref() {
        req = req.header("chatgpt-account-id", account);
    }
    let resp = req
        .send()
        .await
        .map_err(|e| eyre!("ChatGPT request failed: {e}"))?;
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(eyre!("ChatGPT request failed (HTTP {status}): {text}"));
    }
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
        if let Ok(b64) = images::extract_image_b64(&v) {
            return Ok(b64);
        }
    }
    let events = images::parse_sse_payloads(&text);
    images::extract_stream_image(&events).map_err(|e| eyre!(e))
}

async fn save_subscription_image(
    b64: &str,
    model: String,
    size: String,
    quality: String,
    output_format: String,
    output: Option<std::path::PathBuf>,
    output_dir: std::path::PathBuf,
) -> Result<SavedOutput> {
    let bytes = decode_b64(b64)?;
    let created = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let targets = resolve_targets(&output, &output_dir, 1, &output_format, created)?;
    let files = write_targets(&targets, std::slice::from_ref(&bytes))?;
    Ok(SavedOutput {
        files,
        model,
        size,
        quality,
        output_format,
        usage: None,
    })
}

pub async fn generate_data(
    params: GenerateParams,
    backend: Backend,
    output: Option<std::path::PathBuf>,
    output_dir: std::path::PathBuf,
) -> Result<SavedOutput> {
    match backend {
        Backend::OpenAi { api_key, base_url } => {
            images::validate_n(params.n).map_err(|e| eyre!(e))?;
            if output.is_some() && params.n != 1 {
                return Err(eyre!("--output requires -n 1"));
            }
            let body = images::build_generate_body(&params);
            let client = reqwest::Client::new();
            let resp = post_openai_generate(&client, &base_url, &api_key, &body).await?;
            let mut binaries = Vec::with_capacity(resp.data.len());
            for d in &resp.data {
                let b = d
                    .b64_json
                    .as_deref()
                    .ok_or_else(|| eyre!("API returned no image data (b64_json missing)"))?;
                binaries.push(decode_b64(b)?);
            }
            let targets = resolve_targets(
                &output,
                &output_dir,
                binaries.len(),
                &params.output_format,
                resp.created,
            )?;
            let files = write_targets(&targets, &binaries)?;
            Ok(SavedOutput {
                files,
                model: params.model,
                size: resp.size.unwrap_or(params.size),
                quality: resp.quality.unwrap_or(params.quality),
                output_format: resp.output_format.unwrap_or(params.output_format),
                usage: resp.usage,
            })
        }
        Backend::ChatGpt {
            config_dir,
            mainline,
        } => {
            let tool = images::build_image_tool(
                &params.model,
                "generate",
                &params.size,
                &params.quality,
                &params.output_format,
                params.output_compression,
                &params.background,
                params.moderation.as_deref(),
                None,
                None,
            );
            let body = images::build_responses_body(&mainline, &params.prompt, &[], tool);
            let tokens = auth::ensure_valid(&config_dir).await?;
            let client = reqwest::Client::new();
            let b64 = post_responses(&client, &tokens, &body).await?;
            save_subscription_image(
                &b64,
                params.model,
                params.size,
                params.quality,
                params.output_format,
                output,
                output_dir,
            )
            .await
        }
    }
}

pub async fn edit_data(
    params: EditParams,
    image_paths: Vec<std::path::PathBuf>,
    mask_path: Option<std::path::PathBuf>,
    backend: Backend,
    output: Option<std::path::PathBuf>,
    output_dir: std::path::PathBuf,
) -> Result<SavedOutput> {
    images::validate_image_count(image_paths.len()).map_err(|e| eyre!(e))?;
    match backend {
        Backend::OpenAi { api_key, base_url } => {
            if output.is_some() && params.n != 1 {
                return Err(eyre!("--output requires -n 1"));
            }
            let mut parts = Vec::with_capacity(image_paths.len());
            for p in &image_paths {
                parts.push(image_part(p).await?);
            }
            let mask = match mask_path {
                Some(m) => Some(image_part(&m).await?),
                None => None,
            };
            let form = edit_form(&params, parts, mask);
            let client = reqwest::Client::new();
            let resp = post_openai_edit(&client, &base_url, &api_key, form).await?;
            let mut binaries = Vec::with_capacity(resp.data.len());
            for d in &resp.data {
                let b = d
                    .b64_json
                    .as_deref()
                    .ok_or_else(|| eyre!("API returned no image data (b64_json missing)"))?;
                binaries.push(decode_b64(b)?);
            }
            let targets = resolve_targets(
                &output,
                &output_dir,
                binaries.len(),
                &params.output_format,
                resp.created,
            )?;
            let files = write_targets(&targets, &binaries)?;
            Ok(SavedOutput {
                files,
                model: params.model,
                size: resp.size.unwrap_or(params.size),
                quality: resp.quality.unwrap_or(params.quality),
                output_format: resp.output_format.unwrap_or(params.output_format),
                usage: resp.usage,
            })
        }
        Backend::ChatGpt {
            config_dir,
            mainline,
        } => {
            let mut urls = Vec::with_capacity(image_paths.len());
            for p in &image_paths {
                urls.push(read_data_url(p).await?);
            }
            let mask_url = match mask_path {
                Some(m) => Some(read_data_url(&m).await?),
                None => None,
            };
            let tool = images::build_image_tool(
                &params.model,
                "edit",
                &params.size,
                &params.quality,
                &params.output_format,
                params.output_compression,
                &params.background,
                params.moderation.as_deref(),
                params.input_fidelity.as_deref(),
                mask_url.as_deref(),
            );
            let body = images::build_responses_body(&mainline, &params.prompt, &urls, tool);
            let tokens = auth::ensure_valid(&config_dir).await?;
            let client = reqwest::Client::new();
            let b64 = post_responses(&client, &tokens, &body).await?;
            save_subscription_image(
                &b64,
                params.model,
                params.size,
                params.quality,
                params.output_format,
                output,
                output_dir,
            )
            .await
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn vary_data(
    image_paths: Vec<std::path::PathBuf>,
    prompt: Option<String>,
    model: String,
    size: String,
    quality: String,
    output_format: String,
    output_compression: Option<u8>,
    background: String,
    moderation: Option<String>,
    input_fidelity: Option<String>,
    n: u8,
    backend: Backend,
    output: Option<std::path::PathBuf>,
    output_dir: std::path::PathBuf,
) -> Result<SavedOutput> {
    let params = EditParams {
        prompt: prompt.unwrap_or_else(|| VARY_PROMPT.to_string()),
        model,
        size,
        quality,
        output_format,
        output_compression,
        background,
        moderation,
        input_fidelity,
        n,
    };
    edit_data(params, image_paths, None, backend, output, output_dir).await
}
