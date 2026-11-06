pub mod auth;
mod preview;

use crate::open::maybe_open;
use crate::prelude::{println, *};
use mcptools_core::config::{ImageBackend, Images, ResolvedImages};
use mcptools_core::images::{
    self, EditParams, GenerateParams, ImagesResponse, CODEX_RESPONSES_URL, MAX_INPUT_IMAGES,
    VARY_PROMPT,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
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
    #[clap(long, value_enum, env = "MCPTOOLS_IMAGES_API")]
    pub api: Option<Api>,
    #[clap(long, env = "LLM_STREAM_CONFIG_DIR")]
    pub config_dir: Option<String>,
    #[clap(long, env = "OPENAI_API_KEY")]
    pub api_key: Option<String>,
    #[clap(long, env = "OPENAI_BASE_URL")]
    pub base_url: Option<String>,
    #[clap(long, env = "MCPTOOLS_IMAGES_MAINLINE")]
    pub mainline: Option<String>,
}

#[derive(Debug, clap::Args)]
pub struct OutOpts {
    #[clap(
        long,
        env = "OPENAI_IMAGES_MODEL",
        help = "Image model; gpt-image-2.6-flare and gpt-image-2.6-sunburst supported via --api openai"
    )]
    pub model: Option<String>,
    #[clap(long)]
    pub size: Option<String>,
    #[clap(long)]
    pub quality: Option<String>,
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
    #[clap(long)]
    pub preview: bool,
}

#[derive(Debug, clap::Parser)]
#[command(name = "images")]
#[command(
    about = "Image generation, editing and variations; gpt-image-2.6-flare and gpt-image-2.6-sunburst supported via --api openai"
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
    #[clap(long)]
    pub frames: Option<u8>,
    #[clap(long)]
    pub frame_ms: Option<u16>,
    #[clap(long)]
    pub complexity: Option<String>,
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
    pub markup: Option<std::path::PathBuf>,
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
        legacy_dir: Option<std::path::PathBuf>,
        mainline: String,
    },
    OpenAi {
        api_key: String,
        base_url: String,
    },
}

pub(crate) fn settings(supplied: Images) -> Result<ResolvedImages> {
    let file = if supplied.needs_file() {
        crate::config::optional()?.unwrap_or_default().images
    } else {
        Images::default()
    };
    Ok(supplied.resolve(file)?)
}

pub(crate) fn backend_of(config_dir: Option<&str>, settings: ImageBackend) -> Result<Backend> {
    match settings {
        ImageBackend::ChatGpt { mainline } => {
            let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
            let config = mcptools_core::config::optional_config_path(
                std::env::var_os("MCPTOOLS_CONFIG").as_deref(),
                std::env::var_os("XDG_CONFIG_HOME").as_deref(),
                home.as_deref(),
                &std::env::current_dir()?,
            );
            let environment = std::env::var("LLM_STREAM_CONFIG_DIR").ok();
            let (config_dir, legacy_dir) = images::resolve_config_dir(
                config_dir,
                environment.as_deref(),
                config.as_deref(),
                home.as_deref(),
            );
            Ok(Backend::ChatGpt {
                config_dir,
                legacy_dir,
                mainline,
            })
        }
        ImageBackend::OpenAi { api_key, base_url } => Ok(Backend::OpenAi { api_key, base_url }),
    }
}

fn prepare(out: &OutOpts, backend: &BackendOpts) -> Result<ResolvedImages> {
    settings(Images {
        api: backend.api.as_ref().map(|api| {
            match api {
                Api::Chatgpt => "chatgpt",
                Api::Openai => "openai",
            }
            .into()
        }),
        api_key: backend.api_key.clone(),
        base_url: backend.base_url.clone(),
        mainline: backend.mainline.clone(),
        model: out.model.clone(),
        size: out.size.clone(),
        quality: out.quality.clone(),
    })
}

pub async fn run(app: App, _global: crate::Global) -> Result<()> {
    match app.command {
        Commands::Generate(o) => {
            let open = o.out.open;
            let json = o.out.json;
            let preview = o.out.preview;
            let saved = generate_cli(o).await?;
            print_saved(&saved, json)?;
            preview::show(&saved, preview, json);
            maybe_open(&saved.files, open);
            Ok(())
        }
        Commands::Edit(o) => {
            let open = o.out.open;
            let json = o.out.json;
            let preview = o.out.preview;
            let saved = edit_cli(o).await?;
            print_saved(&saved, json)?;
            preview::show(&saved, preview, json);
            maybe_open(&saved.files, open);
            Ok(())
        }
        Commands::Vary(o) => {
            let open = o.out.open;
            let json = o.out.json;
            let preview = o.out.preview;
            let saved = vary_cli(o).await?;
            print_saved(&saved, json)?;
            preview::show(&saved, preview, json);
            maybe_open(&saved.files, open);
            Ok(())
        }
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

fn validate_out(o: &OutOpts, settings: &ResolvedImages, number: u8) -> Result<()> {
    images::validate_size(&settings.size).map_err(|e| eyre!(e))?;
    images::validate_quality(&settings.quality).map_err(|e| eyre!(e))?;
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

fn gen_params(
    prompt: String,
    number: u8,
    o: &OutOpts,
    settings: &ResolvedImages,
) -> GenerateParams {
    GenerateParams {
        prompt,
        model: settings.model.clone(),
        size: settings.size.clone(),
        quality: settings.quality.clone(),
        output_format: o.format.clone(),
        output_compression: o.compression,
        frame_count: None,
        frame_duration_ms: None,
        complexity: None,
        background: o.background.clone(),
        moderation: o.moderation.clone(),
        n: number,
    }
}

fn edit_params(
    prompt: String,
    number: u8,
    fidelity: Option<String>,
    o: &OutOpts,
    settings: &ResolvedImages,
) -> EditParams {
    EditParams {
        prompt,
        model: settings.model.clone(),
        size: settings.size.clone(),
        quality: settings.quality.clone(),
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
    let settings = prepare(&o.out, &o.backend)?;
    validate_out(&o.out, &settings, o.number)?;
    let mut params = gen_params(o.prompt, o.number, &o.out, &settings);
    params.frame_count = o.frames;
    params.frame_duration_ms = o.frame_ms;
    params.complexity = o.complexity;
    images::validate_generate(&params).map_err(|e| eyre!(e))?;
    let backend = backend_of(o.backend.config_dir.as_deref(), settings.backend)?;
    if matches!(backend, Backend::ChatGpt { .. }) && o.number != 1 {
        return Err(eyre!("--api chatgpt generates one image per call; drop -n"));
    }
    let spinner = start_spinner("Generating image...");
    let result = generate_data(params, backend, o.out.output, o.out.output_dir).await;
    spinner.finish_and_clear();
    result
}

async fn edit_cli(o: EditOptions) -> Result<SavedOutput> {
    let settings = prepare(&o.out, &o.backend)?;
    validate_out(&o.out, &settings, o.number)?;
    validate_fidelity(&o.input_fidelity)?;
    images::validate_edit_format(&o.out.format).map_err(|e| eyre!(e))?;
    images::validate_image_count(o.images.len()).map_err(|e| eyre!(e))?;
    let params = edit_params(o.prompt, o.number, o.input_fidelity, &o.out, &settings);
    let backend = backend_of(o.backend.config_dir.as_deref(), settings.backend)?;
    if matches!(backend, Backend::ChatGpt { .. }) && o.number != 1 {
        return Err(eyre!("--api chatgpt generates one image per call; drop -n"));
    }
    let spinner = start_spinner("Editing image...");
    let result = edit_data(
        params,
        o.images,
        o.mask,
        o.markup,
        backend,
        o.out.output,
        o.out.output_dir,
    )
    .await;
    spinner.finish_and_clear();
    result
}

async fn vary_cli(o: VaryOptions) -> Result<SavedOutput> {
    let settings = prepare(&o.out, &o.backend)?;
    validate_out(&o.out, &settings, o.number)?;
    validate_fidelity(&o.input_fidelity)?;
    images::validate_edit_format(&o.out.format).map_err(|e| eyre!(e))?;
    images::validate_image_count(o.images.len()).map_err(|e| eyre!(e))?;
    let backend = backend_of(o.backend.config_dir.as_deref(), settings.backend)?;
    if matches!(backend, Backend::ChatGpt { .. }) && o.number != 1 {
        return Err(eyre!("--api chatgpt generates one image per call; drop -n"));
    }
    let spinner = start_spinner("Creating variation...");
    let result = vary_data(
        o.images,
        o.prompt,
        settings.model,
        settings.size,
        settings.quality,
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
    image_part_bytes(path, bytes)
}

fn image_part_bytes(path: &std::path::Path, bytes: Vec<u8>) -> Result<reqwest::multipart::Part> {
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
    markup: Option<reqwest::multipart::Part>,
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
    if let Some(m) = markup {
        form = form.part("markup", m);
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
    images::validate_generate(&params).map_err(|e| eyre!(e))?;
    images::validate_generate_backend(&params, matches!(backend, Backend::OpenAi { .. }))
        .map_err(|e| eyre!(e))?;
    match backend {
        Backend::OpenAi { api_key, base_url } => {
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
            legacy_dir,
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
            let tokens = auth::ensure_valid(&config_dir, legacy_dir.as_deref()).await?;
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
    markup_path: Option<std::path::PathBuf>,
    backend: Backend,
    output: Option<std::path::PathBuf>,
    output_dir: std::path::PathBuf,
) -> Result<SavedOutput> {
    images::validate_image_count(image_paths.len()).map_err(|e| eyre!(e))?;
    images::validate_edit_format(&params.output_format).map_err(|e| eyre!(e))?;
    for path in image_paths.iter().chain(mask_path.iter()) {
        images::validate_edit_filename(&path.to_string_lossy()).map_err(|e| eyre!(e))?;
    }
    let validated = if let Some(path) = &markup_path {
        if matches!(backend, Backend::ChatGpt { .. }) {
            return Err(eyre!("markup requires --api openai"));
        }
        use tokio::io::AsyncReadExt;
        let file = tokio::fs::File::open(path)
            .await
            .map_err(|e| eyre!("Failed to read '{}': {e}", path.display()))?;
        let mut markup = Vec::new();
        file.take((images::MAX_MARKUP_BYTES + 1) as u64)
            .read_to_end(&mut markup)
            .await
            .map_err(|e| eyre!("Failed to read '{}': {e}", path.display()))?;
        let image = tokio::fs::read(&image_paths[0])
            .await
            .map_err(|e| eyre!("Failed to read '{}': {e}", image_paths[0].display()))?;
        images::validate_markup(&image, &markup, mask_path.is_some()).map_err(|e| eyre!(e))?;
        Some((image, markup))
    } else {
        None
    };
    match backend {
        Backend::OpenAi { api_key, base_url } => {
            if output.is_some() && params.n != 1 {
                return Err(eyre!("--output requires -n 1"));
            }
            let mut parts = Vec::with_capacity(image_paths.len());
            let (mut first, markup_bytes) = match validated {
                Some((image, markup)) => (Some(image), Some(markup)),
                None => (None, None),
            };
            for (index, p) in image_paths.iter().enumerate() {
                if index == 0 {
                    if let Some(image) = first.take() {
                        parts.push(image_part_bytes(p, image)?);
                        continue;
                    }
                }
                parts.push(image_part(p).await?);
            }
            let mask = match mask_path {
                Some(m) => Some(image_part(&m).await?),
                None => None,
            };
            let markup = match (markup_path, markup_bytes) {
                (Some(path), Some(bytes)) => {
                    Some(image_part_bytes(&path, bytes)?.mime_str("image/png")?)
                }
                _ => None,
            };
            let form = edit_form(&params, parts, mask, markup);
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
            legacy_dir,
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
            let tokens = auth::ensure_valid(&config_dir, legacy_dir.as_deref()).await?;
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
    edit_data(params, image_paths, None, None, backend, output, output_dir).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{CommandFactory, Parser};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn cli_gif_sends_options_and_saves_bytes() {
        let server = MockServer::start().await;
        Mock::given(wiremock::matchers::method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "created": 1, "data": [{"b64_json": "R0lGODlh"}]
            })))
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().unwrap();
        let app = App::try_parse_from([
            "images",
            "generate",
            "fox",
            "--api",
            "openai",
            "--api-key",
            "test",
            "--base-url",
            &server.uri(),
            "--output-dir",
            dir.path().to_str().unwrap(),
            "--model",
            images::DEFAULT_MODEL,
            "--size",
            "auto",
            "--quality",
            "auto",
            "--format",
            "gif",
            "--frames",
            "4",
            "--frame-ms",
            "205",
            "--complexity",
            "low",
        ])
        .unwrap();
        let Commands::Generate(options) = app.command else {
            panic!()
        };
        let saved = generate_cli(options).await.unwrap();
        assert!(saved.files[0].ends_with(".gif"));
        assert_eq!(std::fs::read(&saved.files[0]).unwrap(), b"GIF89a");
        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].url.path(), "/images/generations");
        let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
        assert_eq!(body["output_format"], "gif");
        for field in [
            "stream",
            "partial_images",
            "output_compression",
            "prompt_rewriter",
        ] {
            assert!(body.get(field).is_none());
        }
        assert_eq!(body["frame_count"], 4);
        assert_eq!(body["frame_duration_ms"], 210);
        assert_eq!(body["complexity"], "low");
    }

    #[test]
    fn edit_accepts_markup() {
        assert!(App::try_parse_from([
            "images",
            "edit",
            "otter",
            "--image",
            "input.png",
            "--markup",
            "markup.png"
        ])
        .is_ok());
    }

    #[tokio::test]
    async fn markup_is_uploaded_and_chatgpt_rejects_it() {
        let server = MockServer::start().await;
        Mock::given(wiremock::matchers::method("POST"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"created": 1, "data": []})),
            )
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().unwrap();
        let image = dir.path().join("first.png");
        let other = dir.path().join("other.png");
        let markup = dir.path().join("marks.jpg");
        use base64::Engine;
        let png = base64::engine::general_purpose::STANDARD.decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4z8DwHwAFAAH/iZk9HQAAAABJRU5ErkJggg==").unwrap();
        let marks = base64::engine::general_purpose::STANDARD.decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGNg+M/wHwAEAQH/cetH5QAAAABJRU5ErkJggg==").unwrap();
        std::fs::write(&image, &png).unwrap();
        std::fs::write(&other, &png).unwrap();
        std::fs::write(&markup, &marks).unwrap();
        let image = image.to_str().unwrap();
        let other = other.to_str().unwrap();
        let markup = markup.to_str().unwrap();
        let base_url = server.uri();
        let args = [
            "images",
            "edit",
            "otter",
            "--image",
            image,
            "--image",
            other,
            "--markup",
            markup,
            "--api",
            "openai",
            "--api-key",
            "test",
            "--base-url",
            &base_url,
            "--model",
            images::DEFAULT_MODEL,
            "--size",
            "auto",
            "--quality",
            "auto",
        ];
        let Commands::Edit(options) = App::try_parse_from(args).unwrap().command else {
            panic!()
        };
        edit_cli(options).await.unwrap();
        let requests = server.received_requests().await.unwrap();
        let body = &requests[0].body;
        assert!(body.windows(marks.len()).any(|part| part == marks));
        let text = String::from_utf8_lossy(body);
        let markup_headers = text
            .split("\r\n\r\n")
            .find(|headers| headers.contains("name=\"markup\"; filename=\"marks.jpg\""))
            .unwrap();
        assert!(markup_headers
            .lines()
            .any(|header| header.eq_ignore_ascii_case("content-type: image/png")));
        assert_eq!(text.matches("name=\"image[]\"").count(), 2);
        std::fs::write(markup, b"not a PNG").unwrap();
        let Commands::Edit(options) = App::try_parse_from(args).unwrap().command else {
            panic!()
        };
        assert!(edit_cli(options)
            .await
            .unwrap_err()
            .to_string()
            .contains("markup must be a PNG"));
        assert_eq!(server.received_requests().await.unwrap().len(), 1);
        let args = [
            "images",
            "edit",
            "otter",
            "--image",
            image,
            "--markup",
            markup,
            "--api",
            "chatgpt",
            "--mainline",
            images::CHAT_MAINLINE_DEFAULT,
            "--model",
            images::DEFAULT_MODEL,
            "--size",
            "auto",
            "--quality",
            "auto",
        ];
        let Commands::Edit(options) = App::try_parse_from(args).unwrap().command else {
            panic!()
        };
        assert!(edit_cli(options)
            .await
            .unwrap_err()
            .to_string()
            .contains("markup requires"));
        assert_eq!(server.received_requests().await.unwrap().len(), 1);
    }

    #[test]
    fn help_lists_image_26_models() {
        let help = App::command().render_long_help().to_string();
        assert!(help.contains("gpt-image-2.6-flare"));
        assert!(help.contains("gpt-image-2.6-sunburst"));
    }

    #[test]
    fn cli_commands_default_to_image_26_flare() {
        for name in ["generate", "edit", "vary"] {
            let mut args = vec!["images", name];
            if name != "vary" {
                args.push("otter");
            }
            if name != "generate" {
                args.extend(["--image", "input.png"]);
            }
            let app = App::try_parse_from(args).unwrap();
            let out = match app.command {
                Commands::Generate(o) => o.out,
                Commands::Edit(o) => o.out,
                Commands::Vary(o) => o.out,
            };
            let model = Images {
                model: out.model,
                ..Images::default()
            }
            .resolve(Images::default())
            .unwrap()
            .model;
            let expected = std::env::var("OPENAI_IMAGES_MODEL")
                .unwrap_or_else(|_| "gpt-image-2.6-flare".to_string());
            assert_eq!(model, expected);
        }
    }

    #[tokio::test]
    async fn cli_passes_models_to_openai_requests() {
        let server = MockServer::start().await;
        Mock::given(wiremock::matchers::method("POST"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"created": 1, "data": []})),
            )
            .mount(&server)
            .await;
        let base_url = server.uri();
        let dir = tempfile::tempdir().unwrap();
        let image = dir.path().join("input.png");
        std::fs::write(&image, b"image").unwrap();
        let image = image.to_str().unwrap();
        for model in ["gpt-image-2.6-flare", "gpt-image-2.6-sunburst"] {
            for command in ["generate", "edit", "vary"] {
                let mut args = vec!["images", command];
                if command != "vary" {
                    args.push("otter");
                }
                if command != "generate" {
                    args.extend(["--image", image]);
                }
                args.extend([
                    "--model",
                    model,
                    "--api",
                    "openai",
                    "--api-key",
                    "test",
                    "--base-url",
                    &base_url,
                    "--size",
                    "auto",
                    "--quality",
                    "auto",
                ]);
                let app = App::try_parse_from(args).unwrap();
                match app.command {
                    Commands::Generate(o) => {
                        generate_cli(o).await.unwrap();
                    }
                    Commands::Edit(o) => {
                        edit_cli(o).await.unwrap();
                    }
                    Commands::Vary(o) => {
                        vary_cli(o).await.unwrap();
                    }
                }
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
