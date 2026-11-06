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
        } else if upstream.client_id.is_some() || !upstream.scopes.is_empty() {
            return Err(Error(format!(
                "gateway.upstreams[{index}] oauth settings require auth = oauth"
            )));
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
