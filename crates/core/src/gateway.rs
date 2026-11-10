use crate::catalog::{build_catalog, CatalogEntry, SUMMARY_MAX_CHARS};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub upstreams: Vec<Upstream>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Upstream {
    pub prefix: String,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub auth: Option<String>,
    #[serde(default)]
    pub client_id: Option<String>,
    #[serde(default)]
    pub client_secret: Option<String>,
    #[serde(default)]
    pub redirect_port: Option<u16>,
    #[serde(default)]
    pub redirect_uri: Option<String>,
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default, rename = "readOnly")]
    pub read_only: BTreeSet<String>,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct UpstreamTool {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: Value,
}

#[derive(Clone)]
pub struct GatewayTool {
    pub name: String,
    pub upstream: usize,
    pub original_name: String,
    pub description: String,
    pub input_schema: Value,
    pub requires_writes: bool,
}

#[derive(Deserialize, Serialize)]
pub struct Cache {
    pub version: u32,
    pub fingerprint: String,
    pub tools: Vec<Vec<UpstreamTool>>,
}

#[derive(Debug, thiserror::Error)]
#[error("Invalid upstream configuration or catalog: {0}")]
pub struct Error(pub String);

#[derive(Clone)]
pub enum StartupFailure {
    Connect,
    Discover,
    Headers(String),
}

pub struct StartupCatalog {
    pub tools: Vec<GatewayTool>,
    pub cacheable: Option<Vec<Vec<UpstreamTool>>>,
    pub warnings: Vec<String>,
}

pub fn startup_catalog(
    config: &Config,
    outcomes: Vec<Result<Vec<UpstreamTool>, StartupFailure>>,
    existing_names: impl IntoIterator<Item = String>,
) -> Result<StartupCatalog, Error> {
    validate_config(config)?;
    if outcomes.len() != config.upstreams.len() {
        return Err(Error("missing upstream startup outcome".into()));
    }
    let mut discovered = Vec::new();
    let mut warnings = Vec::new();
    for (upstream, outcome) in config.upstreams.iter().zip(outcomes) {
        match outcome {
            Ok(tools) => discovered.push(tools),
            Err(failure) => {
                let reason = match failure {
                    StartupFailure::Connect => "connection or initialization failed".into(),
                    StartupFailure::Discover => "catalog fetch failed".into(),
                    StartupFailure::Headers(reason) => reason,
                };
                let hint = if upstream.auth.as_deref() == Some("oauth") {
                    format!("; run mcptools gateway login {}", upstream.prefix)
                } else {
                    String::new()
                };
                warnings.push(format!(
                    "warning: skipping upstream {}: {reason}{hint}",
                    upstream.prefix
                ));
                discovered.push(Vec::new());
            }
        }
    }
    let tools = merge_tools(config, &discovered, existing_names)?;
    Ok(StartupCatalog {
        tools,
        cacheable: warnings.is_empty().then_some(discovered),
        warnings,
    })
}

#[derive(Debug, PartialEq, Eq)]
pub enum HttpResponseType {
    Json,
    EventStream,
}

pub fn http_response_type(value: &str) -> Option<HttpResponseType> {
    match value
        .split(';')
        .next()?
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "application/json" => Some(HttpResponseType::Json),
        "text/event-stream" => Some(HttpResponseType::EventStream),
        _ => None,
    }
}

pub fn http_session_header<'a>(method: &str, value: Option<&'a str>) -> Option<&'a str> {
    value.filter(|value| {
        method == "initialize"
            && !value.is_empty()
            && value.bytes().all(|byte| (33..=126).contains(&byte))
    })
}

pub fn http_event(bytes: &[u8]) -> Option<(usize, String)> {
    let mut start = 0;
    let mut data = Vec::new();
    loop {
        let end = start
            + bytes[start..]
                .iter()
                .position(|byte| matches!(byte, b'\r' | b'\n'))?;
        let line = &bytes[start..end];
        start = end
            + if bytes[end] == b'\r' && bytes.get(end + 1) == Some(&b'\n') {
                2
            } else {
                1
            };
        if line.is_empty() {
            return Some((start, data.join("\n")));
        }
        if let Some(value) = line.strip_prefix(b"data:") {
            data.push(
                String::from_utf8_lossy(value.strip_prefix(b" ").unwrap_or(value)).into_owned(),
            );
        }
    }
}

const HTTP_SECRET_MIN_CHARS: usize = 8;

fn is_credential_header(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    matches!(
        name.as_str(),
        "authorization" | "proxy-authorization" | "cookie"
    ) || ["token", "key", "secret", "password", "auth"]
        .iter()
        .any(|part| name.contains(part))
}

/// Header values that must be redacted from HTTP upstream results: full
/// credential header values plus the bare credential after an auth scheme
/// (`Bearer <credential>`), skipping candidates shorter than 8 characters.
pub fn http_secrets(headers: &BTreeMap<String, String>) -> Vec<String> {
    let mut secrets = BTreeSet::new();
    for (name, value) in headers {
        if !is_credential_header(name) {
            continue;
        }
        let value = value.trim();
        secrets.insert(value.to_owned());
        if let Some((scheme, credential)) = value.split_once(' ') {
            if !scheme.is_empty()
                && scheme
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-')
            {
                secrets.insert(credential.trim().to_owned());
            }
        }
    }
    secrets
        .into_iter()
        .filter(|secret| secret.chars().count() >= HTTP_SECRET_MIN_CHARS)
        .collect()
}

pub fn http_result(value: Value, id: u64, secrets: &[String]) -> Result<Value, &'static str> {
    if value["jsonrpc"] != "2.0" || value["id"] != id || value.get("error").is_some() {
        return Err("invalid JSON-RPC response");
    }
    let mut result = value.get("result").cloned().ok_or("missing result")?;
    redact_http_secrets(&mut result, secrets)?;
    Ok(result)
}

pub struct EnvTemplate<'a> {
    parts: Vec<EnvPart<'a>>,
}

enum EnvPart<'a> {
    Literal(&'a str),
    Variable(&'a str),
}

impl<'a> EnvTemplate<'a> {
    pub fn parse(mut text: &'a str) -> Result<Self, &'static str> {
        let mut parts = Vec::new();
        while let Some(index) = text.find('$') {
            parts.push(EnvPart::Literal(&text[..index]));
            text = &text[index + 1..];
            if let Some(rest) = text.strip_prefix('$') {
                parts.push(EnvPart::Literal("$"));
                text = rest;
            } else if let Some(rest) = text.strip_prefix('{') {
                let end = rest.find('}').ok_or("unterminated environment reference")?;
                let name = &rest[..end];
                if name.is_empty()
                    || !name.as_bytes()[0].is_ascii_alphabetic() && !name.starts_with('_')
                    || !name
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
                {
                    return Err("invalid environment reference name");
                }
                parts.push(EnvPart::Variable(name));
                text = &rest[end + 1..];
            } else {
                parts.push(EnvPart::Literal("$"));
            }
        }
        parts.push(EnvPart::Literal(text));
        Ok(Self { parts })
    }

    pub fn variables(&self) -> BTreeSet<&'a str> {
        self.parts
            .iter()
            .filter_map(|part| match part {
                EnvPart::Variable(name) => Some(*name),
                EnvPart::Literal(_) => None,
            })
            .collect()
    }

    pub fn resolve(&self, environment: &BTreeMap<String, String>) -> Result<String, String> {
        let mut result = String::new();
        for part in &self.parts {
            result.push_str(match part {
                EnvPart::Literal(text) => text,
                EnvPart::Variable(name) => environment
                    .get(*name)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| (*name).to_owned())?,
            });
        }
        Ok(result)
    }
}

pub fn resolve_http_headers(
    upstream: &Upstream,
    environment: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, String>, Error> {
    upstream.headers.iter().map(|(name, value)| {
        let template = EnvTemplate::parse(value).map_err(|reason| Error(format!("upstream {} header {name}: {reason}", upstream.prefix)))?;
        let resolved = template.resolve(environment).map_err(|variable| Error(format!("upstream {} header {name}: environment variable {variable} is unset, empty or not Unicode", upstream.prefix)))?;
        if reqwest::header::HeaderValue::from_str(&resolved).is_err() {
            return Err(Error(format!("upstream {} header {name}: invalid resolved HTTP header", upstream.prefix)));
        }
        Ok((name.clone(), resolved))
    }).collect()
}

fn redact_http_secrets(value: &mut Value, secrets: &[String]) -> Result<(), &'static str> {
    match value {
        Value::String(text) => {
            for secret in secrets {
                *text = text.replace(secret, "[redacted]");
            }
        }
        Value::Array(values) => {
            for value in values {
                redact_http_secrets(value, secrets)?;
            }
        }
        Value::Object(values) => {
            for (mut key, mut value) in std::mem::take(values) {
                for secret in secrets {
                    key = key.replace(secret, "[redacted]");
                }
                redact_http_secrets(&mut value, secrets)?;
                if values.insert(key, value).is_some() {
                    return Err("response keys collide after credential filtering");
                }
            }
        }
        _ => {}
    }
    Ok(())
}

pub fn validate_config(config: &Config) -> Result<(), Error> {
    let mut prefixes = BTreeSet::new();
    for (index, upstream) in config.upstreams.iter().enumerate() {
        if upstream.prefix.is_empty()
            || !upstream.prefix.as_bytes()[0].is_ascii_alphabetic()
            || !upstream
                .prefix
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric())
            || !prefixes.insert(&upstream.prefix)
        {
            return Err(Error(format!("gateway.upstreams[{index}].prefix must be unique ASCII letters/digits starting with a letter")));
        }
        if let Some(auth) = &upstream.auth {
            if auth != "oauth"
                || upstream.url.is_none()
                || upstream
                    .headers
                    .keys()
                    .any(|name| name.eq_ignore_ascii_case("authorization"))
            {
                return Err(Error(format!("gateway.upstreams[{index}].auth oauth requires HTTP and no Authorization header")));
            }
        } else if upstream.client_id.is_some()
            || upstream.client_secret.is_some()
            || upstream.redirect_port.is_some()
            || upstream.redirect_uri.is_some()
            || !upstream.scopes.is_empty()
        {
            return Err(Error(format!(
                "gateway.upstreams[{index}] oauth settings require auth = oauth"
            )));
        }
        crate::oauth::validate_settings(upstream)
            .map_err(|error| Error(format!("gateway.upstreams[{index}] {error}")))?;
        if let Some(secret) = &upstream.client_secret {
            EnvTemplate::parse(secret).map_err(|reason| {
                Error(format!(
                    "upstream {} client_secret: {reason}",
                    upstream.prefix
                ))
            })?;
        }
        if upstream
            .client_id
            .as_ref()
            .is_some_and(|id| id.trim().is_empty())
            || upstream
                .scopes
                .iter()
                .any(|scope| scope.is_empty() || scope.chars().any(char::is_whitespace))
        {
            return Err(Error(format!(
                "gateway.upstreams[{index}] invalid oauth settings"
            )));
        }
        match (&upstream.url, upstream.command.as_deref()) {
            (None, Some(command)) if !command.trim().is_empty() && !command.contains('\0') && upstream.headers.is_empty() => {},
            (Some(url), None) if upstream.args.is_empty() && upstream.env.is_empty() => {
                let parsed = reqwest::Url::parse(url).map_err(|_| Error(format!("gateway.upstreams[{index}].url is invalid")))?;
                let loopback = parsed.host_str().is_some_and(|host| host == "localhost" || host.trim_matches(['[', ']']).parse::<std::net::IpAddr>().is_ok_and(|ip| ip.is_loopback()));
                if !(parsed.scheme() == "https" || parsed.scheme() == "http" && loopback) || !parsed.username().is_empty() || parsed.password().is_some() || parsed.fragment().is_some() {
                    return Err(Error(format!("gateway.upstreams[{index}].url must use HTTPS or loopback HTTP")));
                }
                for (name, value) in &upstream.headers {
                    if reqwest::header::HeaderName::from_bytes(name.as_bytes()).is_err() || reqwest::header::HeaderValue::from_str(value).is_err() {
                        return Err(Error(format!("gateway.upstreams[{index}].headers contains an invalid header")));
                    }
                    EnvTemplate::parse(value).map_err(|reason| Error(format!("upstream {} header {name}: {reason}", upstream.prefix)))?;
                }
            }
            _ => return Err(Error(format!("gateway.upstreams[{index}] must specify either command or url with matching options"))),
        }
        if upstream.args.iter().any(|arg| arg.contains('\0')) {
            return Err(Error(format!(
                "gateway.upstreams[{index}].args must contain no NUL"
            )));
        }
        if upstream
            .env
            .iter()
            .any(|(key, value)| key.is_empty() || key.contains(['=', '\0']) || value.contains('\0'))
        {
            return Err(Error(format!("gateway.upstreams[{index}].env needs nonempty keys without '=' or NUL, and values without NUL")));
        }
    }
    Ok(())
}

pub fn fingerprint(config: &Config) -> String {
    let mut sanitized = config.clone();
    for upstream in &mut sanitized.upstreams {
        upstream.client_secret = None;
        for value in upstream.headers.values_mut() {
            value.clear();
        }
    }
    format!(
        "{:x}",
        md5::compute(serde_json::to_vec(&sanitized).expect("serializable config"))
    )
}

pub fn reusable_cache(config: &Config, cache: &Cache) -> bool {
    cache.version == 1
        && cache.fingerprint == fingerprint(config)
        && cache.tools.len() == config.upstreams.len()
}

pub fn merge_tools(
    config: &Config,
    discovered: &[Vec<UpstreamTool>],
    existing_names: impl IntoIterator<Item = String>,
) -> Result<Vec<GatewayTool>, Error> {
    if discovered.len() != config.upstreams.len() {
        return Err(Error("missing upstream catalog".into()));
    }
    let mut names: BTreeSet<String> = existing_names.into_iter().collect();
    let mut tools = Vec::new();
    for (index, (upstream, discovered)) in config.upstreams.iter().zip(discovered).enumerate() {
        for tool in discovered {
            if tool.name.is_empty()
                || !tool.input_schema.is_object()
                || tool.input_schema.get("type").and_then(Value::as_str) != Some("object")
            {
                return Err(Error(
                    "tool names must be nonempty and inputSchema must be an object schema".into(),
                ));
            }
            let normalized: String = tool
                .name
                .chars()
                .map(|character| {
                    if character.is_ascii_alphanumeric() || character == '_' {
                        character
                    } else {
                        '_'
                    }
                })
                .collect();
            let name = format!("{}_{}", upstream.prefix, normalized);
            if !names.insert(name.clone()) {
                return Err(Error(format!("duplicate tool name: {name}")));
            }
            tools.push(GatewayTool {
                name,
                upstream: index,
                original_name: tool.name.clone(),
                description: tool.description.clone(),
                input_schema: tool.input_schema.clone(),
                requires_writes: !upstream.read_only.contains(&tool.name),
            });
        }
    }
    Ok(tools)
}

pub fn merge_catalog(mut existing: Vec<CatalogEntry>, tools: &[GatewayTool]) -> Vec<CatalogEntry> {
    let summaries: Vec<String> = tools
        .iter()
        .map(|tool| tool.description.chars().take(SUMMARY_MAX_CHARS).collect())
        .collect();
    existing.extend(build_catalog(
        tools
            .iter()
            .zip(&summaries)
            .map(|(tool, summary)| (tool.name.as_str(), summary.as_str())),
    ));
    existing
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn startup_skips_runtime_failures_without_caching_or_changing_indices() {
        let config = config(json!({"upstreams":[
            {"prefix":"broken","url":"https://example.test","auth":"oauth","client_secret":"secret-value","client_id":"id"},
            {"prefix":"healthy","command":"x"}
        ]})).unwrap();
        for failure in [StartupFailure::Connect, StartupFailure::Discover] {
            let tools =
                serde_json::from_value(json!([{"name":"echo","inputSchema":{"type":"object"}}]))
                    .unwrap();
            let catalog = startup_catalog(&config, vec![Err(failure), Ok(tools)], []).unwrap();
            assert_eq!(catalog.tools[0].name, "healthy_echo");
            assert_eq!(catalog.tools[0].upstream, 1);
            assert!(catalog.cacheable.is_none());
            assert_eq!(catalog.warnings.len(), 1);
            assert!(catalog.warnings[0].contains("run mcptools gateway login broken"));
            assert!(!catalog.warnings[0].contains("secret-value"));
        }
    }

    #[test]
    fn startup_keeps_invalid_configuration_and_tool_clashes_fatal() {
        let mut config = config(json!({"upstreams":[{"prefix":"a","command":"x"}]})).unwrap();
        let tools =
            serde_json::from_value(json!([{"name":"echo","inputSchema":{"type":"object"}}]))
                .unwrap();
        assert!(startup_catalog(&config, vec![Ok(tools)], ["a_echo".into()]).is_err());
        config.upstreams[0].prefix = "invalid_prefix".into();
        assert!(startup_catalog(&config, vec![Err(StartupFailure::Connect)], []).is_err());
    }

    #[test]
    fn http_session_headers_and_response_types() {
        assert_eq!(
            http_session_header("initialize", Some("session-1")),
            Some("session-1")
        );
        assert_eq!(http_session_header("initialize", None), None);
        assert!(http_session_header("initialize", Some("bad\nvalue")).is_none());
        assert!(http_session_header("initialize", Some("session with spaces")).is_none());
        assert!(http_session_header("tools/list", Some("later-session")).is_none());
        assert!(http_session_header("notifications/initialized", Some("later-session")).is_none());
        assert_eq!(
            http_response_type("application/json; charset=utf-8"),
            Some(HttpResponseType::Json)
        );
        assert_eq!(
            http_response_type("text/event-stream; charset=utf-8"),
            Some(HttpResponseType::EventStream)
        );
        assert_eq!(http_response_type("text/html"), None);
    }

    #[test]
    fn http_events_support_split_frames_and_line_endings() {
        for ending in ["\n", "\r\n", "\r"] {
            let event = format!(
                ": heartbeat{ending}data: {{\"jsonrpc\":{ending}data: \"2.0\"}}{ending}{ending}"
            );
            assert_eq!(
                http_event(event.as_bytes()),
                Some((event.len(), "{\"jsonrpc\":\n\"2.0\"}".into()))
            );
            assert_eq!(
                http_event(event.trim_end_matches(['\r', '\n']).as_bytes()),
                None
            );
        }
    }

    #[test]
    fn http_results_filter_nested_keys_and_values_and_reject_collisions() {
        let secrets = vec!["secret-token".into()];
        let result = http_result(json!({"jsonrpc":"2.0","id":1,"result":{"secret-token":[{"text":"echo secret-token"}]}}), 1, &secrets).unwrap();
        assert!(!serde_json::to_string(&result)
            .unwrap()
            .contains("secret-token"));
        assert!(http_result(
            json!({"jsonrpc":"2.0","id":1,"result":{"secret-token":1,"[redacted]":2}}),
            1,
            &secrets
        )
        .is_err());
        assert!(http_result(json!({"jsonrpc":"2.0","id":2,"result":{}}), 1, &secrets).is_err());
    }

    #[test]
    fn http_secrets_only_cover_credential_headers() {
        let headers = |pairs: &[(&str, &str)]| -> BTreeMap<String, String> {
            pairs
                .iter()
                .map(|(name, value)| ((*name).into(), (*value).into()))
                .collect()
        };
        let secrets = http_secrets(&headers(&[
            ("X-Team", "engineering-team"),
            ("Content-Type", "application/json"),
        ]));
        assert!(secrets.is_empty(), "{secrets:?}");
        let result = http_result(
            json!({"jsonrpc":"2.0","id":1,"result":{"text":"engineering"}}),
            1,
            &http_secrets(&headers(&[("X-Team", "eng")])),
        )
        .unwrap();
        assert_eq!(result, json!({"text":"engineering"}));

        let secrets = http_secrets(&headers(&[("authorization", "Bearer abc123456789")]));
        assert!(secrets.contains(&"abc123456789".to_string()), "{secrets:?}");
        assert!(secrets.contains(&"Bearer abc123456789".to_string()));
        let result = http_result(
            json!({"jsonrpc":"2.0","id":1,"result":{"text":"token abc123456789"}}),
            1,
            &secrets,
        )
        .unwrap();
        assert_eq!(result, json!({"text":"token [redacted]"}));

        assert!(http_secrets(&headers(&[("Authorization", "short")])).is_empty());
        assert!(
            http_secrets(&headers(&[("Authorization", "Bearer short")]))
                == vec!["Bearer short".to_string()]
        );

        let secrets = http_secrets(&headers(&[
            ("Cookie", "session=abcdef123456"),
            ("X-Api-Key", "key-abcdef123456"),
            ("Proxy-Authorization", "Basic dXNlcjpwYXNz"),
            ("X-Client-Secret", "s3cr3t-value"),
        ]));
        for expected in [
            "session=abcdef123456",
            "key-abcdef123456",
            "Basic dXNlcjpwYXNz",
            "dXNlcjpwYXNz",
            "s3cr3t-value",
        ] {
            assert!(
                secrets.contains(&expected.to_string()),
                "{expected} {secrets:?}"
            );
        }
    }

    #[test]
    fn http_header_templates_resolve_from_a_map_without_recursive_expansion() {
        let environment = BTreeMap::from([
            ("TOKEN".into(), "${OTHER}".into()),
            ("_suffix2".into(), "value".into()),
        ]);
        let template =
            EnvTemplate::parse("Bearer ${TOKEN}/${_suffix2}/${TOKEN} $$ $cash $${OTHER} café")
                .unwrap();
        assert_eq!(template.variables(), BTreeSet::from(["TOKEN", "_suffix2"]));
        assert_eq!(
            template.resolve(&environment).unwrap(),
            "Bearer ${OTHER}/value/${OTHER} $ $cash ${OTHER} café"
        );
        for literal in ["plain", "$cash", "$", "café", ""] {
            let template = EnvTemplate::parse(literal).unwrap();
            assert!(template.variables().is_empty());
            assert_eq!(template.resolve(&BTreeMap::new()).unwrap(), literal);
        }
    }

    #[test]
    fn http_header_templates_missing_empty_and_invalid_values_are_safe() {
        let config = config(json!({"upstreams": [{"prefix": "remote", "url": "https://example.com", "headers": {"Authorization": "Bearer ${TOKEN}"}}]})).unwrap();
        for environment in [
            BTreeMap::new(),
            BTreeMap::from([("TOKEN".into(), String::new())]),
            BTreeMap::from([("TOKEN".into(), "secret\nvalue".into())]),
        ] {
            let error = resolve_http_headers(&config.upstreams[0], &environment)
                .unwrap_err()
                .to_string();
            assert!(error.contains("remote"));
            assert!(error.contains("Authorization"));
            assert!(!error.contains("Bearer"));
            assert!(!error.contains("secret"));
            if environment.get("TOKEN").is_none_or(String::is_empty) {
                assert!(error.contains("TOKEN"));
            }
        }
        let mut resolved = config.clone();
        resolved.upstreams[0].headers = resolve_http_headers(
            &config.upstreams[0],
            &BTreeMap::from([("TOKEN".into(), "resolved-secret".into())]),
        )
        .unwrap();
        assert_eq!(fingerprint(&config), fingerprint(&resolved));
    }

    #[test]
    fn http_header_templates_reject_malformed_references() {
        for template in ["${", "${1X}", "${}", "${TOKEN:-default}", "${${TOKEN}}"] {
            let result = config(
                json!({"upstreams": [{"prefix": "remote", "url": "https://example.com", "headers": {"Authorization": template}}]}),
            );
            assert!(result.is_err(), "malformed header template accepted");
            let error = result.err().unwrap().to_string();
            assert!(error.contains("remote"));
            assert!(error.contains("Authorization"));
            assert!(!error.contains(template));
        }
    }

    #[test]
    fn oauth_client_secret_templates_resolve_and_reject_malformed_references() {
        let environment = BTreeMap::from([("SECRET".into(), "${NO_RECURSION}".into())]);
        for (text, expected) in [
            ("plain-secret", "plain-secret"),
            ("${SECRET}$$$cash", "${NO_RECURSION}$$cash"),
            ("$${SECRET}", "${SECRET}"),
        ] {
            let config = config(json!({"upstreams":[{"prefix":"remote","url":"https://example.com","auth":"oauth","client_id":"id","client_secret":text}]})).unwrap();
            assert_eq!(
                crate::oauth::resolve_client_secret(&config.upstreams[0], &environment)
                    .unwrap()
                    .as_deref(),
                Some(expected)
            );
        }
        for text in ["${", "${1X}", "${}", "${SECRET:-default}", "${${SECRET}}"] {
            let result = config(
                json!({"upstreams":[{"prefix":"remote","url":"https://example.com","auth":"oauth","client_id":"id","client_secret":text}]}),
            );
            assert!(result.is_err(), "malformed client_secret template accepted");
            let error = result.err().unwrap().to_string();
            assert!(error.contains("remote") && error.contains("client_secret"));
            assert!(!error.contains(text));
        }
        let config = config(json!({"upstreams":[{"prefix":"remote","url":"https://example.com","auth":"oauth","client_id":"id","client_secret":"private-prefix-${SECRET}"}]})).unwrap();
        for environment in [
            BTreeMap::new(),
            BTreeMap::from([("SECRET".into(), String::new())]),
        ] {
            let error = crate::oauth::resolve_client_secret(&config.upstreams[0], &environment)
                .unwrap_err()
                .to_string();
            assert!(error.contains("remote") && error.contains("SECRET"));
            assert!(!error.contains("private-prefix"));
        }
        let mut upstream = config.upstreams[0].clone();
        upstream.client_secret = None;
        assert_eq!(
            crate::oauth::resolve_client_secret(&upstream, &BTreeMap::new()).unwrap(),
            None
        );
    }

    #[test]
    fn http_config_accepts_secure_and_loopback_urls() {
        for url in [
            "https://example.com/mcp",
            "http://127.0.0.1:8080/mcp",
            "http://[::1]/mcp",
            "http://localhost/mcp",
        ] {
            assert!(config(json!({"upstreams": [{"prefix": "remote", "url": url, "headers": {"Authorization": "secret"}}]})).is_ok(), "{url}");
        }
    }

    #[test]
    fn http_config_rejects_insecure_urls_and_mixed_transports() {
        for upstream in [
            json!({"prefix": "a", "url": "http://example.com"}),
            json!({"prefix": "a", "url": "ftp://localhost"}),
            json!({"prefix": "a", "url": "https://example.com", "command": "x"}),
            json!({"prefix": "a", "url": "https://example.com", "command": ""}),
            json!({"prefix": "a", "url": "https://example.com", "args": ["x"]}),
            json!({"prefix": "a"}),
        ] {
            assert!(config(json!({"upstreams": [upstream]})).is_err());
        }
    }

    #[test]
    fn http_cache_fingerprint_excludes_header_values() {
        let first = config(json!({"upstreams": [{"prefix": "a", "url": "https://example.com", "headers": {"Authorization": "secret"}}]})).unwrap();
        let second = config(json!({"upstreams": [{"prefix": "a", "url": "https://example.com", "headers": {"Authorization": "other"}}]})).unwrap();
        assert_eq!(fingerprint(&first), fingerprint(&second));
    }

    #[test]
    fn oauth_config_and_cache_exclude_configured_secrets() {
        let base = serde_json::json!({"upstreams":[{"prefix":"a","url":"https://example.com","auth":"oauth","client_id":"id","client_secret":"first","redirect_port":12345,"redirect_uri":"https://example.com/return"}]});
        let first = config(base.clone()).unwrap();
        let mut changed = base.clone();
        changed["upstreams"][0]["client_secret"] = serde_json::json!("second");
        assert_eq!(fingerprint(&first), fingerprint(&config(changed).unwrap()));
        assert_eq!(
            crate::oauth::redirect_uri(&first.upstreams[0], 5555),
            "https://example.com/return"
        );
        for (field, value) in [
            ("redirect_port", serde_json::json!(65536)),
            ("redirect_port", serde_json::json!(-1)),
            ("redirect_port", serde_json::json!(0)),
            (
                "redirect_uri",
                serde_json::json!("https://example.com/path?query=yes"),
            ),
            ("client_secret", serde_json::json!("")),
            ("auth", serde_json::Value::Null),
        ] {
            let mut invalid = base.clone();
            invalid["upstreams"][0][field] = value;
            assert!(config(invalid).is_err(), "{field}");
        }
        let mut local = first.upstreams[0].clone();
        local.redirect_uri = None;
        assert_eq!(
            crate::oauth::redirect_uri(&local, 5555),
            "http://127.0.0.1:5555/callback"
        );
        local.redirect_port = Some(65535);
        assert!(crate::oauth::validate_settings(&local).is_ok());
    }

    fn config(value: serde_json::Value) -> Result<Config, Error> {
        let config = serde_json::from_value(value).map_err(|error| Error(error.to_string()))?;
        validate_config(&config)?;
        Ok(config)
    }

    #[test]
    fn config_rejects_ambiguous_prefixes_unknown_fields_and_invalid_process_arguments() {
        for value in [
            json!({"upstreams": [{"prefix": "has_under", "command": "x"}]}),
            json!({"upstreams": [{"prefix": "a", "command": ""}]}),
            json!({"upstreams": [{"prefix": "a", "command": "x", "url": "https://example.com"}]}),
            json!({"upstreams": [{"prefix": "a", "command": "x"}, {"prefix": "a", "command": "y"}]}),
            json!({"upstreams": [{"prefix": "a", "command": "x", "env": {"BAD=KEY": "secret"}}]}),
        ] {
            assert!(config(value).is_err());
        }
    }

    #[test]
    fn cache_key_covers_all_gateway_configuration_and_ignores_toml_formatting() {
        let value = json!({"upstreams": [{"prefix": "a", "command": "x"}]});
        let config = self::config(value.clone()).unwrap();
        assert_eq!(
            fingerprint(&config),
            fingerprint(
                &crate::config::parse("[[gateway.upstreams]]\nprefix = 'a'\ncommand = 'x'\n")
                    .unwrap()
                    .gateway
            )
        );
        let cache = Cache {
            version: 1,
            fingerprint: fingerprint(&config),
            tools: vec![vec![]],
        };
        assert!(reusable_cache(&config, &cache));
        for field in ["prefix", "command", "args", "env", "readOnly"] {
            let mut changed = value.clone();
            changed["upstreams"][0][field] = match field {
                "args" | "readOnly" => json!(["echo"]),
                "env" => json!({"TOKEN": "changed"}),
                _ => json!("changed"),
            };
            assert!(!reusable_cache(&self::config(changed).unwrap(), &cache));
        }
    }

    #[test]
    fn merge_rejects_collisions_and_ignores_upstream_safety_annotations() {
        let config =
            config(json!({"upstreams":[{"prefix":"fake","command":"x","readOnly":["read"]}]}))
                .unwrap();
        let tools: Vec<UpstreamTool> = serde_json::from_value(json!([
            {"name":"echo","inputSchema":{"type":"object"},"annotations":{"readOnlyHint":true}},
            {"name":"read","inputSchema":{"type":"object"}}
        ]))
        .unwrap();
        let merged = merge_tools(&config, std::slice::from_ref(&tools), []).unwrap();
        assert!(merged[0].requires_writes);
        assert!(!merged[1].requires_writes);
        assert_eq!(merge_catalog(vec![], &merged)[0].domain, "fake");
        assert!(merge_tools(&config, &[tools], ["fake_echo".into()]).is_err());
    }

    #[test]
    fn merge_normalizes_each_non_identifier_character_and_preserves_original_policy() {
        let config = config(json!({"upstreams":[{"prefix":"fake","command":"x","readOnly":["resolve-library-id", "query_docs"]}]})).unwrap();
        let tools: Vec<UpstreamTool> = serde_json::from_value(json!([
            {"name":"resolve-library-id","inputSchema":{"type":"object"}},
            {"name":"query.docs","inputSchema":{"type":"object"}},
            {"name":"9_Az /é😀-$","inputSchema":{"type":"object"}}
        ]))
        .unwrap();
        let merged = merge_tools(&config, &[tools], []).unwrap();
        assert_eq!(merged[0].name, "fake_resolve_library_id");
        assert_eq!(merged[0].original_name, "resolve-library-id");
        assert!(!merged[0].requires_writes);
        assert_eq!(merged[1].name, "fake_query_docs");
        assert!(merged[1].requires_writes);
        assert_eq!(merged[2].name, "fake_9_Az______");
        assert_eq!(merged[2].original_name, "9_Az /é😀-$");
    }

    #[test]
    fn merge_rejects_normalized_collisions_with_upstream_or_reserved_names() {
        let config = config(json!({"upstreams":[{"prefix":"fake","command":"x"}]})).unwrap();
        let tools: Vec<UpstreamTool> = serde_json::from_value(json!([
            {"name":"a-b","inputSchema":{"type":"object"}},
            {"name":"a_b","inputSchema":{"type":"object"}}
        ]))
        .unwrap();
        assert!(merge_tools(&config, std::slice::from_ref(&tools), [])
            .err()
            .unwrap()
            .to_string()
            .contains("duplicate tool name: fake_a_b"));
        assert!(merge_tools(&config, &[tools[..1].to_vec()], ["fake_a_b".into()]).is_err());
    }
}
