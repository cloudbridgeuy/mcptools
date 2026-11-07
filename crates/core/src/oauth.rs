use base64::{
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
    Engine,
};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

#[derive(Clone, Deserialize, Serialize)]
pub struct Tokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub client_id: String,
    pub client_secret: Option<String>,
    #[serde(default)]
    pub client_auth: ClientAuth,
    pub token_endpoint: String,
    pub resource: String,
    pub expires_at: Option<u64>,
}

impl Tokens {
    pub fn updated(&self, value: &Value, now: u64) -> Result<Self, &'static str> {
        if value.get("ok").is_some_and(|ok| ok != true)
            || !(field(value, "token_type")?.eq_ignore_ascii_case("bearer")
                || value["ok"] == true && value["token_type"] == "user")
        {
            return Err("Invalid OAuth token");
        }
        Ok(Self {
            access_token: field(value, "access_token")?.to_owned(),
            refresh_token: value["refresh_token"]
                .as_str()
                .map(str::to_owned)
                .or_else(|| self.refresh_token.clone()),
            expires_at: value["expires_in"]
                .as_u64()
                .map(|seconds| now.saturating_add(seconds)),
            ..self.clone()
        })
    }

    pub fn expired(&self, now: u64) -> bool {
        self.expires_at.is_some_and(|expiry| expiry <= now)
    }

    pub fn secrets(&self) -> Vec<String> {
        let headers = std::collections::BTreeMap::from([(
            "Authorization".to_owned(),
            format!("Bearer {}", self.access_token),
        )]);
        let mut secrets = crate::gateway::http_secrets(&headers);
        secrets.extend(
            [
                &Some(self.access_token.clone()),
                &self.refresh_token,
                &self.client_secret,
            ]
            .into_iter()
            .filter_map(|value| value.as_ref())
            .filter(|value| !value.is_empty())
            .cloned(),
        );
        secrets.sort();
        secrets.dedup();
        secrets
    }
}

#[derive(Clone, Copy, Default, Deserialize, Serialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ClientAuth {
    #[default]
    Post,
    Basic,
}

pub fn basic_authorization(client_id: &str, secret: &str) -> String {
    let encode = |value: &str| {
        urlencoding::encode(value)
            .replace("%20", "+")
            .replace('~', "%7E")
            .replace("%2A", "*")
    };
    format!(
        "Basic {}",
        STANDARD.encode(format!("{}:{}", encode(client_id), encode(secret)))
    )
}

pub fn validate_settings(upstream: &crate::gateway::Upstream) -> Result<(), &'static str> {
    if upstream.client_secret.is_some() && upstream.client_id.is_none() {
        return Err("oauth client_secret requires client_id");
    }
    if upstream.client_secret.as_deref().is_some_and(str::is_empty) {
        return Err("oauth client_secret must be nonempty");
    }
    if upstream.redirect_port == Some(0) {
        return Err("oauth redirect_port must be 1-65535");
    }
    if let Some(redirect) = &upstream.redirect_uri {
        if upstream.redirect_port.is_none() {
            return Err("oauth redirect_uri requires redirect_port");
        }
        if safe_url(redirect).is_none_or(|url| url.scheme() != "https" || url.query().is_some()) {
            return Err("oauth redirect_uri must use HTTPS without credentials, query or fragment");
        }
    }
    Ok(())
}

pub fn redirect_uri(upstream: &crate::gateway::Upstream, bound_port: u16) -> String {
    upstream
        .redirect_uri
        .clone()
        .unwrap_or_else(|| format!("http://127.0.0.1:{bound_port}/callback"))
}

#[derive(Debug, PartialEq)]
pub enum RefreshAction {
    Send,
    Refresh,
    Login,
}

pub fn refresh_action(
    tokens: Option<&Tokens>,
    now: u64,
    status: Option<u16>,
    refreshed: bool,
) -> RefreshAction {
    let Some(tokens) = tokens else {
        return RefreshAction::Send;
    };
    if !match status {
        Some(status) => status == 401,
        None => tokens.expired(now),
    } {
        return RefreshAction::Send;
    }
    if refreshed || tokens.refresh_token.as_deref().is_none_or(str::is_empty) {
        return RefreshAction::Login;
    }
    RefreshAction::Refresh
}

fn field<'a>(value: &'a Value, name: &str) -> Result<&'a str, &'static str> {
    value
        .get(name)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or("Invalid OAuth metadata")
}

fn endpoint(value: &Value, name: &str, issuer: &Url) -> Result<String, &'static str> {
    let url = safe_url(field(value, name)?).ok_or("Invalid OAuth endpoint")?;
    if issuer.scheme() == "https" && url.scheme() != "https" {
        return Err("Invalid OAuth endpoint");
    }
    Ok(url.to_string())
}

pub fn metadata_url(resource: &str, metadata: &str) -> Result<Url, &'static str> {
    let resource = safe_url(resource).ok_or("Invalid OAuth resource")?;
    let metadata = safe_url(metadata).ok_or("Invalid OAuth metadata URL")?;
    if resource.scheme() == "https" && metadata.scheme() != "https" {
        return Err("Invalid OAuth metadata URL");
    }
    Ok(metadata)
}

pub fn resource_matches(resource: &str, upstream: &str) -> bool {
    let (Some(resource), Some(upstream)) = (safe_url(resource), safe_url(upstream)) else {
        return false;
    };
    resource.scheme() == upstream.scheme()
        && resource.host() == upstream.host()
        && resource.port_or_known_default() == upstream.port_or_known_default()
        && upstream
            .path()
            .strip_prefix(resource.path())
            .is_some_and(|suffix| {
                suffix.is_empty() || resource.path().ends_with('/') || suffix.starts_with('/')
            })
}

pub fn resource<'a>(metadata: &'a Value, upstream: &str) -> Result<&'a str, &'static str> {
    let resource = field(metadata, "resource")?;
    if !resource_matches(resource, upstream) {
        return Err("OAuth resource mismatch");
    }
    Ok(resource)
}

pub fn issuer<'a>(metadata: &'a Value, upstream: &str) -> Result<&'a str, &'static str> {
    resource(metadata, upstream)?;
    let text = metadata["authorization_servers"]
        .as_array()
        .and_then(|values| values.first())
        .and_then(Value::as_str)
        .ok_or("Missing OAuth issuer")?;
    let issuer = safe_url(text)
        .filter(|url| url.query().is_none())
        .ok_or("Invalid OAuth issuer")?;
    if safe_url(upstream).ok_or("Invalid OAuth resource")?.scheme() == "https"
        && issuer.scheme() != "https"
    {
        return Err("Invalid OAuth issuer");
    }
    Ok(text)
}

pub fn discovery_urls(issuer: &str) -> Result<[Url; 2], &'static str> {
    let issuer = safe_url(issuer)
        .filter(|url| url.query().is_none())
        .ok_or("Invalid OAuth issuer")?;
    let path = issuer.path().trim_end_matches('/');
    let mut oauth = issuer.clone();
    oauth.set_path(&format!("/.well-known/oauth-authorization-server{path}"));
    let mut openid = issuer.clone();
    openid.set_path(&format!("{path}/.well-known/openid-configuration"));
    Ok([oauth, openid])
}

pub struct ServerMetadata {
    authorization_endpoint: String,
    pub token_endpoint: String,
    registration_endpoint: Option<String>,
    auth_methods: Option<Vec<String>>,
}

impl ServerMetadata {
    pub fn parse(value: &Value, issuer_text: &str) -> Result<Self, &'static str> {
        if field(value, "issuer")? != issuer_text {
            return Err("OAuth issuer mismatch");
        }
        let issuer = safe_url(issuer_text).ok_or("Invalid OAuth issuer")?;
        Ok(Self {
            authorization_endpoint: endpoint(value, "authorization_endpoint", &issuer)?,
            token_endpoint: endpoint(value, "token_endpoint", &issuer)?,
            registration_endpoint: value
                .get("registration_endpoint")
                .map(|_| endpoint(value, "registration_endpoint", &issuer))
                .transpose()?,
            auth_methods: value
                .get("token_endpoint_auth_methods_supported")
                .map(|methods| {
                    serde_json::from_value(methods.clone())
                        .map_err(|_| "Invalid OAuth authentication methods")
                })
                .transpose()?,
        })
    }

    pub fn client_auth(&self, has_secret: bool) -> Result<ClientAuth, &'static str> {
        let Some(methods) = self.auth_methods.as_ref().filter(|_| has_secret) else {
            return Ok(ClientAuth::Post);
        };
        if methods.iter().any(|method| method == "client_secret_post") {
            Ok(ClientAuth::Post)
        } else if methods.iter().any(|method| method == "client_secret_basic") {
            Ok(ClientAuth::Basic)
        } else {
            Err("Unsupported OAuth client authentication method")
        }
    }

    pub fn registration_endpoint(&self) -> Result<&str, &'static str> {
        self.registration_endpoint.as_deref().ok_or("Configure client_id for this upstream or use an authorization server with dynamic client registration")
    }

    pub fn authorization_url(
        &self,
        client_id: &str,
        redirect: &str,
        state: &str,
        verifier: &str,
        resource: &str,
        scopes: &[String],
    ) -> Result<Url, &'static str> {
        let mut url = Url::parse(&self.authorization_endpoint)
            .map_err(|_| "Invalid OAuth authorization endpoint")?;
        url.query_pairs_mut()
            .append_pair("response_type", "code")
            .append_pair("client_id", client_id)
            .append_pair("redirect_uri", redirect)
            .append_pair("state", state)
            .append_pair("code_challenge", &challenge(verifier))
            .append_pair("code_challenge_method", "S256")
            .append_pair("resource", resource);
        if !scopes.is_empty() {
            url.query_pairs_mut()
                .append_pair("scope", &scopes.join(" "));
        }
        Ok(url)
    }
}

pub fn registered_client(value: &Value) -> Result<(String, Option<String>), &'static str> {
    let id = field(value, "client_id")?.to_owned();
    let secret = match value["token_endpoint_auth_method"]
        .as_str()
        .unwrap_or("none")
    {
        "none" => None,
        "client_secret_post" => Some(field(value, "client_secret")?.to_owned()),
        _ => return Err("Unsupported OAuth client authentication method"),
    };
    Ok((id, secret))
}

pub fn verifier(bytes: &[u8; 32]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

pub fn safe_url(text: &str) -> Option<Url> {
    let url = Url::parse(text).ok()?;
    let loopback = url.host_str().is_some_and(|host| {
        host == "localhost"
            || host
                .trim_matches(['[', ']'])
                .parse::<std::net::IpAddr>()
                .is_ok_and(|ip| ip.is_loopback())
    });
    (url.host().is_some()
        && (url.scheme() == "https" || url.scheme() == "http" && loopback)
        && url.username().is_empty()
        && url.password().is_none()
        && url.fragment().is_none())
    .then_some(url)
}

pub fn challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

pub fn callback(target: &str, state: &str, path: &str) -> Option<String> {
    if target.split('?').next()? != path {
        return None;
    }
    let url = Url::parse(&format!("http://127.0.0.1{target}")).ok()?;
    if url.path() != path || url.fragment().is_some() {
        return None;
    }
    let values: Vec<_> = url.query_pairs().collect();
    let states: Vec<_> = values.iter().filter(|(key, _)| key == "state").collect();
    let codes: Vec<_> = values.iter().filter(|(key, _)| key == "code").collect();
    (states.len() == 1 && states[0].1 == state && codes.len() == 1 && !codes[0].1.is_empty())
        .then(|| codes[0].1.to_string())
}

pub fn metadata_challenge(header: &str) -> Option<String> {
    let regex = regex::Regex::new(r#"(?i)(?:^|[,\s])resource_metadata\s*=\s*\"([^\"]+)\""#).ok()?;
    let value = regex.captures(header)?.get(1)?.as_str();
    safe_url(value).map(|url| url.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens() -> Tokens {
        Tokens {
            access_token: "access-secret-value".into(),
            refresh_token: Some("refresh-secret-value".into()),
            client_id: "client".into(),
            client_secret: Some("tiny".into()),
            client_auth: ClientAuth::Post,
            token_endpoint: "https://example.com/token".into(),
            resource: "https://example.com/mcp".into(),
            expires_at: Some(42),
        }
    }

    #[test]
    fn refresh_decisions_bound_retries_and_require_refresh_credentials() {
        let mut tokens = tokens();
        for (now, status, refreshed, expected) in [
            (41, None, false, RefreshAction::Send),
            (42, None, false, RefreshAction::Refresh),
            (41, Some(401), false, RefreshAction::Refresh),
            (41, Some(401), true, RefreshAction::Login),
            (42, Some(401), true, RefreshAction::Login),
            (42, Some(200), true, RefreshAction::Send),
            (42, Some(500), false, RefreshAction::Send),
        ] {
            assert_eq!(
                refresh_action(Some(&tokens), now, status, refreshed),
                expected
            );
        }
        tokens.refresh_token = None;
        assert_eq!(
            refresh_action(Some(&tokens), 42, None, false),
            RefreshAction::Login
        );
        assert_eq!(
            refresh_action(None, 42, Some(401), false),
            RefreshAction::Send
        );
    }

    #[test]
    fn resource_metadata_accepts_same_origin_path_segment_prefixes() {
        for resource in [
            "https://mcp.slack.com/mcp",
            "https://mcp.slack.com",
            "https://mcp.slack.com/",
        ] {
            let metadata = serde_json::json!({"resource":resource,"authorization_servers":["https://mcp.slack.com"]});
            assert_eq!(
                issuer(&metadata, "https://mcp.slack.com/mcp"),
                Ok("https://mcp.slack.com"),
                "resource: {resource}"
            );
        }
        for resource in ["https://example.com/api", "https://example.com/api/"] {
            let metadata = serde_json::json!({"resource":resource,"authorization_servers":["https://example.com"]});
            assert_eq!(
                issuer(&metadata, "https://example.com/api/mcp"),
                Ok("https://example.com"),
                "resource: {resource}"
            );
        }
    }

    #[test]
    fn resource_metadata_rejects_partial_segments_and_other_origins() {
        for resource in [
            "https://mcp.slack.com/m",
            "https://mcp.slack.com/mcp/child",
            "https://other.example/mcp",
            "https://mcp.slack.com:8443/mcp",
            "http://mcp.slack.com/mcp",
        ] {
            let metadata = serde_json::json!({"resource":resource,"authorization_servers":["https://mcp.slack.com"]});
            assert_eq!(
                issuer(&metadata, "https://mcp.slack.com/mcp"),
                Err("OAuth resource mismatch"),
                "resource: {resource}"
            );
        }
    }

    #[test]
    fn metadata_authorization_and_token_decisions_are_pure() {
        let metadata = serde_json::json!({"resource":"https://example.com/mcp", "authorization_servers":["https://example.com/tenant"]});
        let issuer = issuer(&metadata, "https://example.com/mcp").unwrap();
        let [oauth, oidc] = discovery_urls(issuer).unwrap();
        assert_eq!(
            oauth.as_str(),
            "https://example.com/.well-known/oauth-authorization-server/tenant"
        );
        assert_eq!(
            oidc.as_str(),
            "https://example.com/tenant/.well-known/openid-configuration"
        );
        assert!(metadata_url("https://example.com/mcp", "http://127.0.0.1/meta").is_err());
        let server = serde_json::json!({"issuer":issuer,"authorization_endpoint":"https://example.com/authorize","token_endpoint":"https://example.com/token"});
        let parsed = ServerMetadata::parse(&server, issuer).unwrap();
        assert!(parsed
            .registration_endpoint()
            .unwrap_err()
            .contains("client_id"));
        assert!(ServerMetadata::parse(&server, "https://other.example").is_err());
        let verifier = verifier(&[0; 32]);
        assert_eq!(verifier.len(), 43);
        let url = parsed
            .authorization_url(
                "client",
                "http://127.0.0.1/callback",
                "state",
                &verifier,
                "https://example.com/mcp",
                &["read".into(), "write".into()],
            )
            .unwrap();
        let params: std::collections::BTreeMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(params["scope"], "read write");
        assert_eq!(params["code_challenge"], challenge(&verifier));
        let tokens = tokens();
        let updated = tokens
            .updated(
                &serde_json::json!({"token_type":"bEaReR","access_token":"new","expires_in":10}),
                50,
            )
            .unwrap();
        assert_eq!(updated.expires_at, Some(60));
        assert_eq!(updated.refresh_token, tokens.refresh_token);
        assert!(tokens
            .updated(
                &serde_json::json!({"token_type":"Basic","access_token":"new"}),
                50
            )
            .is_err());
        assert!(
            registered_client(&serde_json::json!({"client_id":"public"}))
                .unwrap()
                .1
                .is_none()
        );
        assert_eq!(registered_client(&serde_json::json!({"client_id":"private","client_secret":"secret","token_endpoint_auth_method":"client_secret_post"})).unwrap().1.as_deref(), Some("secret"));
    }

    #[test]
    fn pkce_and_callback_are_strict() {
        assert_eq!(
            challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
        assert_eq!(
            callback("/callback?state=ok&code=abc", "ok", "/callback").as_deref(),
            Some("abc")
        );
        assert!(callback("/callback-wrong?state=ok&code=abc", "ok", "/callback").is_none());
        assert!(callback("/x/../callback?state=ok&code=abc", "ok", "/callback").is_none());
        assert!(callback("/callback?state=no&code=abc", "ok", "/callback").is_none());
        assert!(callback("/callback?state=ok&state=ok&code=abc", "ok", "/callback").is_none());
    }

    #[test]
    fn tokens_feed_http_redaction() {
        let tokens = tokens();
        assert!(tokens.expired(42));
        assert!(!tokens.expired(41));
        let result = crate::gateway::http_result(
            serde_json::json!({"jsonrpc":"2.0","id":1,"result":{"value":"Bearer access-secret-value refresh-secret-value tiny"}}),
            1,
            &tokens.secrets(),
        ).unwrap().to_string();
        assert!(!result.contains("access-secret-value"));
        assert!(!result.contains("refresh-secret-value"));
        assert!(!result.contains("tiny"));
    }

    #[test]
    fn url_and_challenge_reject_unsafe_values() {
        assert!(safe_url("http://example.com/path").is_none());
        assert!(safe_url("https://user@example.com/path").is_none());
        assert!(safe_url("http://127.0.0.1:123/a").is_some());
        assert_eq!(
            metadata_challenge("Bearer resource_metadata=\"https://example.com/meta\""),
            Some("https://example.com/meta".into())
        );
    }

    #[test]
    fn slack_top_level_user_token_and_ok_false() {
        let tokens = tokens();
        let updated = tokens.updated(&serde_json::json!({"ok":true,"access_token":"user-token","token_type":"user","authed_user":{"id":"U12345","scope":"read,write"}}), 50);
        assert!(updated.is_ok(), "documented user token must be accepted");
        assert_eq!(updated.unwrap().access_token, "user-token");
    }

    #[test]
    fn slack_ok_false_rejects_token_shaped_response() {
        assert!(tokens().updated(&serde_json::json!({"ok":false,"error":"invalid_code","access_token":"bad","token_type":"Bearer"}), 50).is_err(), "ok:false must reject even a token-shaped response");
    }

    #[test]
    fn confidential_auth_prefers_post_and_supports_basic_only_metadata() {
        for (methods, expected) in [
            (
                serde_json::json!(["client_secret_post", "client_secret_basic"]),
                Some(ClientAuth::Post),
            ),
            (
                serde_json::json!(["client_secret_basic"]),
                Some(ClientAuth::Basic),
            ),
            (serde_json::json!(["none"]), None),
            (serde_json::json!([]), None),
        ] {
            let metadata = serde_json::json!({"issuer":"https://example.com", "authorization_endpoint":"https://example.com/auth", "token_endpoint":"https://example.com/token", "token_endpoint_auth_methods_supported":methods});
            let server = ServerMetadata::parse(&metadata, "https://example.com").unwrap();
            assert_eq!(server.client_auth(true).ok(), expected);
            assert_eq!(server.client_auth(false).unwrap(), ClientAuth::Post);
        }
        assert_eq!(
            basic_authorization("id: +", "secret/~*"),
            format!("Basic {}", STANDARD.encode("id%3A+%2B:secret%2F%7E*"))
        );
    }

    #[test]
    fn proxy_redirect_path_preserves_state_and_path_checks() {
        assert_eq!(
            callback("/slack/return?state=ok&code=abc", "ok", "/slack/return").as_deref(),
            Some("abc")
        );
        for target in [
            "/callback?state=ok&code=abc",
            "/slack/return?state=wrong&code=abc",
            "/slack/return?state=ok&code=abc&code=other",
            "/slack/x/../return?state=ok&code=abc",
        ] {
            assert!(callback(target, "ok", "/slack/return").is_none());
        }
    }
}
