use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const DEFAULT_MODEL: &str = "gpt-image-2.5-flare";
pub const FLARE_MODEL: &str = "gpt-image-2.5-flare";
pub const FLARE_SNAPSHOT: &str = "gpt-image-2.5-flare-2026-09-08";
pub const SUNBURST_MODEL: &str = "gpt-image-2.5-sunburst";
pub const SUNBURST_SNAPSHOT: &str = "gpt-image-2.5-sunburst-2026-09-08";
pub const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";
pub const CODEX_RESPONSES_URL: &str = "https://chatgpt.com/backend-api/codex/responses";
pub const CHAT_MAINLINE_DEFAULT: &str = "gpt-5.6-sol";
pub const OAUTH_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
pub const OAUTH_TOKEN_URL: &str = "https://auth.openai.com/oauth/token";
pub const OAUTH_SCOPE: &str =
    "openid profile email offline_access api.connectors.read api.connectors.invoke";
pub const EXPIRY_SKEW_SECS: u64 = 60;
pub const NOT_SIGNED_IN: &str = "not signed in — run: llm-stream --login";
pub const VARY_PROMPT: &str =
    "Create a variation of this image, preserving subject, style and composition.";
pub const MAX_INPUT_IMAGES: usize = 16;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateParams {
    pub prompt: String,
    pub model: String,
    pub size: String,
    pub quality: String,
    pub output_format: String,
    pub output_compression: Option<u8>,
    pub background: String,
    pub moderation: Option<String>,
    pub n: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditParams {
    pub prompt: String,
    pub model: String,
    pub size: String,
    pub quality: String,
    pub output_format: String,
    pub output_compression: Option<u8>,
    pub background: String,
    pub moderation: Option<String>,
    pub input_fidelity: Option<String>,
    pub n: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageData {
    #[serde(default)]
    pub b64_json: Option<String>,
    #[serde(default)]
    pub revised_prompt: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TokenDetails {
    #[serde(default)]
    pub image_tokens: u32,
    #[serde(default)]
    pub text_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Usage {
    #[serde(default)]
    pub input_tokens: u32,
    #[serde(default)]
    pub output_tokens: u32,
    #[serde(default)]
    pub total_tokens: u32,
    #[serde(default)]
    pub input_tokens_details: Option<TokenDetails>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImagesResponse {
    #[serde(default)]
    pub created: u64,
    #[serde(default)]
    pub background: Option<String>,
    #[serde(default)]
    pub data: Vec<ImageData>,
    #[serde(default)]
    pub output_format: Option<String>,
    #[serde(default)]
    pub quality: Option<String>,
    #[serde(default)]
    pub size: Option<String>,
    #[serde(default)]
    pub usage: Option<Usage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenSet {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: String,
    #[serde(default)]
    pub id_token: String,
    #[serde(default)]
    pub account_id: Option<String>,
}

#[derive(Debug)]
pub enum TokenState {
    Missing,
    Expired(TokenSet),
    Valid(TokenSet),
}

pub fn jwt_expiry(access_token: &str) -> Result<u64, String> {
    let payload = access_token
        .split('.')
        .nth(1)
        .filter(|p| !p.is_empty())
        .ok_or_else(|| "malformed JWT".to_string())?;
    let bytes = base64_decode_url_no_pad(payload)?;
    let v: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|_| "malformed JWT".to_string())?;
    v.get("exp")
        .and_then(|e| e.as_u64())
        .ok_or_else(|| "token is missing the `exp` claim".to_string())
}

fn base64_decode_url_no_pad(s: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(s.len());
    let mut bits: u32 = 0;
    let mut width = 0;
    for c in s.chars() {
        let v = match c {
            'A'..='Z' => c as u32 - 'A' as u32,
            'a'..='z' => c as u32 - 'a' as u32 + 26,
            '0'..='9' => c as u32 - '0' as u32 + 52,
            '-' => 62,
            '_' => 63,
            _ => return Err("malformed JWT".to_string()),
        };
        bits = (bits << 6) | v;
        width += 6;
        while width >= 8 {
            width -= 8;
            out.push((bits >> width) as u8);
        }
    }
    Ok(out)
}

pub fn classify_tokens(tokens: Option<TokenSet>, now_secs: u64) -> TokenState {
    match tokens {
        None => TokenState::Missing,
        Some(set) => match jwt_expiry(&set.access_token) {
            Ok(exp) if exp > now_secs + EXPIRY_SKEW_SECS => TokenState::Valid(set),
            _ => TokenState::Expired(set),
        },
    }
}

pub fn resolve_config_dir(explicit: Option<&str>) -> std::path::PathBuf {
    if let Some(dir) = explicit.filter(|d| !d.is_empty()) {
        return expand_tilde(dir);
    }
    if let Ok(dir) = std::env::var("LLM_STREAM_CONFIG_DIR") {
        if !dir.is_empty() {
            return expand_tilde(&dir);
        }
    }
    expand_tilde("~/.config/llm-stream")
}

fn expand_tilde(path: &str) -> std::path::PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return std::path::PathBuf::from(home).join(rest);
        }
    }
    std::path::PathBuf::from(path)
}

pub fn data_url(mime: &str, b64: &str) -> String {
    format!("data:{mime};base64,{b64}")
}

pub fn mime_for_filename(name: &str) -> &str {
    let ext = name.rsplit('.').next().unwrap_or("").to_lowercase();
    match ext.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        _ => "image/png",
    }
}

pub fn build_image_tool(
    image_model: &str,
    action: &str,
    size: &str,
    quality: &str,
    output_format: &str,
    output_compression: Option<u8>,
    background: &str,
    moderation: Option<&str>,
    input_fidelity: Option<&str>,
    mask_data_url: Option<&str>,
) -> serde_json::Value {
    let mut tool = serde_json::json!({
        "type": "image_generation",
        "model": image_model,
        "action": action,
        "size": size,
        "quality": quality,
        "output_format": output_format,
        "background": background,
    });
    let map = tool.as_object_mut().expect("object");
    if let Some(c) = output_compression {
        map.insert("output_compression".to_string(), serde_json::json!(c));
    }
    if let Some(m) = moderation {
        map.insert("moderation".to_string(), serde_json::json!(m));
    }
    if let Some(f) = input_fidelity {
        map.insert("input_fidelity".to_string(), serde_json::json!(f));
    }
    if let Some(mask) = mask_data_url {
        map.insert(
            "input_image_mask".to_string(),
            serde_json::json!({"image_url": mask}),
        );
    }
    tool
}

pub fn build_responses_body(
    mainline: &str,
    text: &str,
    image_data_urls: &[String],
    tool: serde_json::Value,
) -> serde_json::Value {
    let mut content = vec![serde_json::json!({"type": "input_text", "text": text})];
    for url in image_data_urls {
        content.push(serde_json::json!({"type": "input_image", "image_url": url}));
    }
    serde_json::json!({
        "model": mainline,
        "input": [{"type": "message", "role": "user", "content": content}],
        "tools": [tool],
        "stream": false,
        "store": false,
    })
}

pub fn extract_image_b64(response: &serde_json::Value) -> Result<String, String> {
    if let Some(items) = response.get("output").and_then(|o| o.as_array()) {
        for item in items {
            if item.get("type").and_then(|t| t.as_str()) == Some("image_generation_call") {
                if let Some(b64) = item.get("result").and_then(|r| r.as_str()) {
                    return Ok(b64.to_string());
                }
            }
        }
    }
    Err(server_failure_message(response))
}

pub fn parse_sse_payloads(text: &str) -> Vec<serde_json::Value> {
    let mut out = Vec::new();
    for event in text.split("\n\n") {
        let mut data = String::new();
        for line in event.lines() {
            if let Some(rest) = line.strip_prefix("data:") {
                let rest = rest.strip_prefix(' ').unwrap_or(rest);
                if !data.is_empty() {
                    data.push('\n');
                }
                data.push_str(rest);
            }
        }
        let data = data.trim();
        if data.is_empty() || data == "[DONE]" {
            continue;
        }
        if let Ok(v) = serde_json::from_str(data) {
            out.push(v);
        }
    }
    out
}

fn stream_failure(event: &serde_json::Value) -> Option<String> {
    match event.get("type").and_then(|t| t.as_str()) {
        Some("error") => event
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
            .map(|m| m.to_string()),
        Some("response.failed") => Some(
            event
                .get("response")
                .and_then(|r| r.get("error"))
                .and_then(|e| e.get("message"))
                .and_then(|m| m.as_str())
                .unwrap_or("the model stopped without producing an image")
                .to_string(),
        ),
        _ => None,
    }
}

pub fn extract_stream_image(events: &[serde_json::Value]) -> Result<String, String> {
    let mut partial: Option<String> = None;
    let mut done: Option<String> = None;
    for e in events {
        if let Some(msg) = stream_failure(e) {
            return Err(msg);
        }
        if let Some(b) = image_result_in(e) {
            done = Some(b);
        }
        if let Some(item) = e.get("item") {
            if let Some(b) = image_result_in(item) {
                done = Some(b);
            }
        }
        match e.get("type").and_then(|t| t.as_str()) {
            Some("response.image_generation_call.partial_image") => {
                if let Some(b) = e.get("partial_image_b64").and_then(|b| b.as_str()) {
                    partial = Some(b.to_string());
                }
            }
            Some("response.completed") => {
                if let Some(resp) = e.get("response") {
                    if let Ok(b) = extract_image_b64(resp) {
                        done = Some(b);
                    }
                }
            }
            _ => {
                if e.get("output").is_some() {
                    if let Ok(b) = extract_image_b64(e) {
                        done = Some(b);
                    }
                }
            }
        }
    }
    done.or(partial)
        .ok_or_else(|| "stream carried no image".to_string())
}

fn image_result_in(v: &serde_json::Value) -> Option<String> {
    if v.get("type").and_then(|t| t.as_str()) == Some("image_generation_call") {
        v.get("result")
            .and_then(|r| r.as_str())
            .map(|r| r.to_string())
    } else {
        None
    }
}

fn server_failure_message(response: &serde_json::Value) -> String {
    if let Some(msg) = response
        .get("error")
        .and_then(|e| e.get("message"))
        .and_then(|m| m.as_str())
    {
        return msg.to_string();
    }
    if let Some(items) = response.get("output").and_then(|o| o.as_array()) {
        for item in items {
            let t = item.get("type").and_then(|t| t.as_str()).unwrap_or("");
            if t == "response.failed" {
                if let Some(msg) = item
                    .get("response")
                    .and_then(|r| r.get("error"))
                    .and_then(|e| e.get("message"))
                    .and_then(|m| m.as_str())
                {
                    return msg.to_string();
                }
                return "the model stopped without producing an image".to_string();
            }
        }
    }
    format!("response carried no image: {response}")
}

pub fn default_generate_params(prompt: String) -> GenerateParams {
    GenerateParams {
        prompt,
        model: DEFAULT_MODEL.to_string(),
        size: "auto".to_string(),
        quality: "auto".to_string(),
        output_format: "png".to_string(),
        output_compression: None,
        background: "auto".to_string(),
        moderation: None,
        n: 1,
    }
}

pub fn default_edit_params(prompt: String) -> EditParams {
    EditParams {
        prompt,
        model: DEFAULT_MODEL.to_string(),
        size: "auto".to_string(),
        quality: "auto".to_string(),
        output_format: "png".to_string(),
        output_compression: None,
        background: "auto".to_string(),
        moderation: None,
        input_fidelity: None,
        n: 1,
    }
}

pub fn validate_n(n: u8) -> Result<u8, String> {
    if (1..=10).contains(&n) {
        Ok(n)
    } else {
        Err(format!("n must be between 1 and 10, got {n}"))
    }
}

pub fn validate_compression(v: Option<u8>) -> Result<Option<u8>, String> {
    match v {
        None => Ok(None),
        Some(c) if c <= 100 => Ok(Some(c)),
        Some(c) => Err(format!("compression must be 0-100, got {c}")),
    }
}

pub fn validate_quality(q: &str) -> Result<(), String> {
    match q {
        "auto" | "low" | "medium" | "high" | "xhigh" | "max" => Ok(()),
        other => Err(format!(
            "quality must be auto|low|medium|high|xhigh|max, got {other}"
        )),
    }
}

pub fn validate_background(b: &str) -> Result<(), String> {
    match b {
        "auto" | "transparent" | "opaque" => Ok(()),
        other => Err(format!(
            "background must be auto|transparent|opaque, got {other}"
        )),
    }
}

pub fn validate_format(f: &str) -> Result<(), String> {
    match f {
        "png" | "jpeg" | "webp" => Ok(()),
        other => Err(format!("format must be png|jpeg|webp, got {other}")),
    }
}

pub fn validate_size(s: &str) -> Result<(), String> {
    if s == "auto" || s == "1024x1024" || s == "1536x1024" || s == "1024x1536" {
        return Ok(());
    }
    let mut parts = s.split('x');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(w), Some(h), None) => {
            let w: u32 = w
                .parse()
                .map_err(|_| format!("invalid size '{s}', want WIDTHxHEIGHT"))?;
            let h: u32 = h
                .parse()
                .map_err(|_| format!("invalid size '{s}', want WIDTHxHEIGHT"))?;
            if !w.is_multiple_of(16) || !h.is_multiple_of(16) {
                return Err(format!(
                    "size width and height must be multiples of 16, got {s}"
                ));
            }
            if w == 0 || h == 0 || w > 3840 || h > 3840 {
                return Err(format!("size edges must be 1-3840, got {s}"));
            }
            Ok(())
        }
        _ => Err(format!("invalid size '{s}', want auto or WIDTHxHEIGHT")),
    }
}

pub fn validate_image_count(n: usize) -> Result<(), String> {
    if n == 0 {
        return Err("at least one --image is required".to_string());
    }
    if n > MAX_INPUT_IMAGES {
        return Err(format!(
            "at most {MAX_INPUT_IMAGES} images allowed, got {n}"
        ));
    }
    Ok(())
}

pub fn extension_for_format(format: &str) -> &str {
    match format {
        "jpeg" => "jpg",
        "webp" => "webp",
        _ => "png",
    }
}

pub fn suggested_filename(prefix: &str, index: usize, format: &str, created: u64) -> String {
    format!(
        "{prefix}-{created}-{index}.{}",
        extension_for_format(format)
    )
}

pub fn build_generate_body(p: &GenerateParams) -> serde_json::Value {
    let mut body = serde_json::json!({
        "model": p.model,
        "prompt": p.prompt,
        "size": p.size,
        "quality": p.quality,
        "output_format": p.output_format,
        "background": p.background,
        "n": p.n,
    });
    let map = body.as_object_mut().expect("object");
    if let Some(c) = p.output_compression {
        map.insert("output_compression".to_string(), serde_json::json!(c));
    }
    if let Some(m) = &p.moderation {
        map.insert("moderation".to_string(), serde_json::json!(m));
    }
    body
}

pub fn error_hint(status: u16, body: &str) -> String {
    if body.contains("moderation_blocked") {
        return "Request blocked by moderation. Revise the prompt or input images.".to_string();
    }
    if body.contains("organization_verification") || status == 403 {
        return format!("OpenAI rejected the request (HTTP {status}). Organization verification may be required for GPT Image models. Body: {body}");
    }
    if status == 401 {
        return "Unauthorized (HTTP 401). Check OPENAI_API_KEY.".to_string();
    }
    format!("OpenAI request failed (HTTP {status}): {body}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_body_omits_unset_optionals() {
        let p = default_generate_params("otter".to_string());
        let b = build_generate_body(&p);
        assert_eq!(b["model"], "gpt-image-2.5-flare");
        assert!(b.get("output_compression").is_none());
        assert!(b.get("moderation").is_none());
    }

    #[test]
    fn generate_body_includes_optionals() {
        let mut p = default_generate_params("otter".to_string());
        p.output_compression = Some(50);
        p.moderation = Some("low".to_string());
        let b = build_generate_body(&p);
        assert_eq!(b["output_compression"], 50);
        assert_eq!(b["moderation"], "low");
    }

    #[test]
    fn rejects_bad_counts() {
        assert!(validate_n(0).is_err());
        assert!(validate_n(11).is_err());
        assert!(validate_n(4).is_ok());
        assert!(validate_compression(Some(101)).is_err());
        assert!(validate_compression(Some(100)).is_ok());
    }

    #[test]
    fn validates_enums_and_sizes() {
        assert!(validate_quality("xhigh").is_ok());
        assert!(validate_quality("ultra").is_err());
        assert!(validate_background("opaque").is_ok());
        assert!(validate_background("red").is_err());
        assert!(validate_format("webp").is_ok());
        assert!(validate_format("gif").is_err());
        assert!(validate_size("auto").is_ok());
        assert!(validate_size("1024x1024").is_ok());
        assert!(validate_size("1536x864").is_ok());
        assert!(validate_size("1535x864").is_err());
        assert!(validate_size("big").is_err());
    }

    #[test]
    fn moderation_hint_detected() {
        let h = error_hint(400, r#"{"code":"moderation_blocked"}"#);
        assert!(h.contains("moderation"));
        let h = error_hint(401, "bad key");
        assert!(h.contains("OPENAI_API_KEY"));
    }

    #[test]
    fn filenames_use_format_extension() {
        assert_eq!(suggested_filename("img", 0, "jpeg", 1), "img-1-0.jpg");
        assert_eq!(suggested_filename("img", 2, "png", 7), "img-7-2.png");
    }

    #[test]
    fn expiry_reads_exp_claim() {
        assert_eq!(
            jwt_expiry("h.eyJleHAiOjE3ODU5NjYzMjF9.s"),
            Ok(1_785_966_321)
        );
        assert!(jwt_expiry("garbage").is_err());
        assert!(jwt_expiry("h.eyJleHAiOjB9.s").is_ok());
    }

    #[test]
    fn tokens_classify_with_skew() {
        let set = TokenSet {
            access_token: "h.eyJleHAiOjIwMDAwMDAwMDB9.s".into(),
            refresh_token: "r".into(),
            id_token: "i".into(),
            account_id: None,
        };
        assert!(matches!(
            classify_tokens(Some(set.clone()), 1_000_000_000),
            TokenState::Valid(_)
        ));
        assert!(matches!(
            classify_tokens(Some(set.clone()), 1_999_999_950),
            TokenState::Expired(_)
        ));
        assert!(matches!(classify_tokens(None, 0), TokenState::Missing));
    }

    #[test]
    fn responses_tool_carries_image_model_and_action() {
        let t = build_image_tool(
            "gpt-image-2.5-sunburst",
            "edit",
            "1024x1024",
            "high",
            "png",
            None,
            "auto",
            None,
            None,
            None,
        );
        assert_eq!(t["type"], "image_generation");
        assert_eq!(t["model"], "gpt-image-2.5-sunburst");
        assert_eq!(t["action"], "edit");
        assert!(t.get("input_image_mask").is_none());
    }

    #[test]
    fn responses_body_embeds_text_and_reference_images() {
        let tool = build_image_tool(
            "gpt-image-2.5-flare",
            "generate",
            "auto",
            "auto",
            "png",
            None,
            "auto",
            None,
            None,
            None,
        );
        let b = build_responses_body(
            "gpt-5.6-sol",
            "otter",
            &["data:image/png;base64,xx".to_string()],
            tool,
        );
        assert_eq!(b["stream"], false);
        assert_eq!(b["store"], false);
        assert_eq!(b["input"][0]["content"][0]["type"], "input_text");
        assert_eq!(b["input"][0]["content"][1]["type"], "input_image");
    }

    #[test]
    fn extract_finds_image_call_result() {
        let r = serde_json::json!({
            "output": [
                {"type": "reasoning", "id": "rs_1"},
                {"type": "image_generation_call", "id": "ig_1", "result": "QUJD"}
            ]
        });
        assert_eq!(extract_image_b64(&r), Ok("QUJD".to_string()));
    }

    #[test]
    fn extract_surfaces_server_failure_sentence() {
        let r = serde_json::json!({
            "output": [],
            "error": {"message": "quota exceeded"}
        });
        assert_eq!(extract_image_b64(&r), Err("quota exceeded".to_string()));
        let r = serde_json::json!({"output": []});
        assert!(extract_image_b64(&r).is_err());
    }

    #[test]
    fn stream_reads_output_item_done_shape() {
        let events = vec![serde_json::json!({
            "type": "response.output_item.done",
            "item": {"type": "image_generation_call", "id": "ig_1", "status": "completed", "result": "QUJD"}
        })];
        assert_eq!(extract_stream_image(&events), Ok("QUJD".to_string()));
    }

    #[test]
    fn stream_prefers_final_result_over_partials() {
        let events = vec![
            serde_json::json!({"type": "response.image_generation_call.partial_image", "partial_image_b64": "AAA", "partial_image_index": 0}),
            serde_json::json!({"type": "response.completed", "response": {"output": [{"type": "image_generation_call", "result": "QUJD"}]}}),
        ];
        assert_eq!(extract_stream_image(&events), Ok("QUJD".to_string()));
    }

    #[test]
    fn stream_falls_back_to_partial_when_no_final() {
        let events = vec![
            serde_json::json!({"type": "response.image_generation_call.partial_image", "partial_image_b64": "AAA", "partial_image_index": 0}),
        ];
        assert_eq!(extract_stream_image(&events), Ok("AAA".to_string()));
    }

    #[test]
    fn stream_reports_mid_stream_failure() {
        let events = vec![serde_json::json!({"type": "error", "error": {"message": "overloaded"}})];
        assert_eq!(extract_stream_image(&events), Err("overloaded".to_string()));
        assert!(extract_stream_image(&[]).is_err());
    }

    #[test]
    fn sse_split_skips_done_and_blanks() {
        let raw = "event: a\ndata: {\"type\":\"x\"}\n\ndata: [DONE]\n\n";
        let v = parse_sse_payloads(raw);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0]["type"], "x");
    }
}
