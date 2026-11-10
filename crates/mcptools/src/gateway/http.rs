use crate::prelude::*;
use mcptools_core::gateway::{self, HttpResponseType, Upstream};
use mcptools_core::oauth::{self, RefreshAction};
use serde_json::{json, Value};
use std::time::Duration;

pub struct Client {
    http: reqwest::Client,
    url: String,
    prefix: String,
    headers: reqwest::header::HeaderMap,
    session: Option<String>,
    protocol: Option<String>,
    next_id: u64,
    secrets: Vec<String>,
    upstream: Upstream,
    tokens: Option<mcptools_core::oauth::Tokens>,
}

impl Client {
    pub async fn start(upstream: &Upstream) -> Result<Self> {
        let mut environment = std::collections::BTreeMap::new();
        for value in upstream.headers.values() {
            let template = gateway::EnvTemplate::parse(value).map_err(|reason| eyre!(reason))?;
            for name in template.variables() {
                if let Ok(value) = std::env::var(name) {
                    environment.insert(name.to_owned(), value);
                }
            }
        }
        let resolved = gateway::resolve_http_headers(upstream, &environment)?;
        let mut secrets = gateway::http_secrets(&resolved);
        secrets.extend(environment.into_values().filter(|value| !value.is_empty()));
        let mut headers = reqwest::header::HeaderMap::new();
        for (name, value) in &resolved {
            let name = reqwest::header::HeaderName::from_bytes(name.as_bytes())
                .map_err(|_| eyre!("Invalid upstream headers"))?;
            let mut value = reqwest::header::HeaderValue::from_str(value)
                .map_err(|_| eyre!("Invalid upstream headers"))?;
            value.set_sensitive(true);
            headers.insert(name, value);
        }
        headers.insert(
            reqwest::header::ACCEPT,
            reqwest::header::HeaderValue::from_static("application/json, text/event-stream"),
        );
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            reqwest::header::HeaderValue::from_static("application/json"),
        );
        headers.remove("Mcp-Session-Id");
        headers.remove("MCP-Protocol-Version");
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|_| eyre!("Cannot create upstream client"))?;
        let mut client = Self {
            http,
            url: upstream
                .url
                .clone()
                .ok_or_else(|| eyre!("Missing upstream URL"))?,
            prefix: upstream.prefix.clone(),
            headers,
            session: None,
            protocol: None,
            next_id: 0,
            secrets,
            upstream: upstream.clone(),
            tokens: if upstream.auth.as_deref() == Some("oauth") {
                Some(super::oauth::load(upstream)?)
            } else {
                None
            },
        };
        if let Some(tokens) = &client.tokens {
            client.secrets.extend(tokens.secrets());
            if let Some(secret) = super::oauth::resolve_client_secret(upstream)? {
                client.secrets.push(secret.clone());
                let authorization = oauth::basic_authorization(&tokens.client_id, &secret);
                client
                    .secrets
                    .extend(gateway::http_secrets(&std::collections::BTreeMap::from([
                        ("Authorization".into(), authorization),
                    ])));
            }
            client.secrets.sort();
            client.secrets.dedup();
        }
        let started = async {
            let result = client.request("initialize", json!({"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"mcptools","version":env!("MCPTOOLS_VERSION")}})).await?;
            let version = result["protocolVersion"].as_str().filter(|version| matches!(*version, "2024-11-05" | "2025-03-26" | "2025-06-18")).ok_or_else(|| eyre!("Upstream {} incompatible protocol", client.prefix))?;
            if !result["capabilities"]["tools"].is_object() {
                return Err(eyre!("Upstream {} incompatible tools", client.prefix));
            }
            client.protocol = Some(version.to_owned());
            client.post(json!({"jsonrpc":"2.0","method":"notifications/initialized"}), None).await?;
            Ok(())
        }.await;
        if let Err(error) = started {
            client.stop().await;
            return Err(error);
        }
        Ok(client)
    }

    pub async fn request(&mut self, method: &str, params: Value) -> Result<Value> {
        self.next_id += 1;
        let id = self.next_id;
        self.post(
            json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}),
            Some(id),
        )
        .await?
        .ok_or_else(|| eyre!("Upstream {} missing response", self.prefix))
    }

    async fn post(&mut self, message: Value, id: Option<u64>) -> Result<Option<Value>> {
        let mut refreshed = false;
        match oauth::refresh_action(self.tokens.as_ref(), super::oauth::now(), None, false) {
            RefreshAction::Refresh => {
                self.refresh().await?;
                refreshed = true;
            }
            RefreshAction::Login => return Err(super::oauth::login_error(&self.upstream)),
            RefreshAction::Send => {}
        }
        let mut response = self.send(&message).await?;
        match oauth::refresh_action(
            self.tokens.as_ref(),
            super::oauth::now(),
            Some(response.status().as_u16()),
            refreshed,
        ) {
            RefreshAction::Refresh => {
                self.refresh().await?;
                response = self.send(&message).await?;
                if oauth::refresh_action(
                    self.tokens.as_ref(),
                    super::oauth::now(),
                    Some(response.status().as_u16()),
                    true,
                ) == RefreshAction::Login
                {
                    return Err(super::oauth::login_error(&self.upstream));
                }
            }
            RefreshAction::Login => return Err(super::oauth::login_error(&self.upstream)),
            RefreshAction::Send => {}
        }
        let status = response.status();
        if !status.is_success() {
            return Err(eyre!("Upstream {} HTTP {}", self.prefix, status.as_u16()));
        }
        if let Some(session) = gateway::http_session_header(
            message["method"].as_str().unwrap_or_default(),
            response
                .headers()
                .get("Mcp-Session-Id")
                .and_then(|value| value.to_str().ok()),
        ) {
            self.session = Some(session.to_owned());
        }
        let Some(id) = id else { return Ok(None) };
        let kind = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .and_then(gateway::http_response_type)
            .ok_or_else(|| eyre!("Upstream {} invalid response type", self.prefix))?;
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|error| {
            if error.is_timeout() {
                eyre!("Upstream {} timed out", self.prefix)
            } else {
                eyre!("Upstream {} response failed", self.prefix)
            }
        })? {
            bytes.extend_from_slice(&chunk);
            if bytes.len() > 4 * 1024 * 1024 {
                return Err(eyre!("Upstream {} response too large", self.prefix));
            }
            if kind == HttpResponseType::EventStream {
                while let Some((end, data)) = gateway::http_event(&bytes) {
                    bytes.drain(..end);
                    if let Ok(value) = serde_json::from_str::<Value>(&data) {
                        if value.get("id") == Some(&json!(id)) {
                            return self.result(value, id).map(Some);
                        }
                    }
                }
            }
        }
        if kind == HttpResponseType::Json {
            let value = serde_json::from_slice(&bytes)
                .map_err(|_| eyre!("Upstream {} invalid JSON", self.prefix))?;
            return self.result(value, id).map(Some);
        }
        Err(eyre!("Upstream {} missing response", self.prefix))
    }

    async fn refresh(&mut self) -> Result<()> {
        let tokens = self
            .tokens
            .as_mut()
            .ok_or_else(|| super::oauth::login_error(&self.upstream))?;
        self.secrets.extend(tokens.secrets());
        let result = super::oauth::refresh(&self.upstream, tokens).await;
        self.secrets.extend(tokens.secrets());
        self.secrets.sort();
        self.secrets.dedup();
        result
    }

    async fn send(&self, message: &Value) -> Result<reqwest::Response> {
        let mut request = self
            .http
            .post(&self.url)
            .headers(self.headers.clone())
            .json(message);
        if let Some(tokens) = &self.tokens {
            request = request.bearer_auth(&tokens.access_token);
        }
        if let Some(session) = &self.session {
            request = request.header("Mcp-Session-Id", session);
        }
        if let Some(protocol) = &self.protocol {
            request = request.header("MCP-Protocol-Version", protocol);
        }
        request.send().await.map_err(|error| {
            if error.is_timeout() {
                eyre!("Upstream {} timed out", self.prefix)
            } else {
                eyre!("Upstream {} request failed", self.prefix)
            }
        })
    }

    fn result(&self, value: Value, id: u64) -> Result<Value> {
        gateway::http_result(value, id, &self.secrets)
            .map_err(|error| eyre!("Upstream {} {error}", self.prefix))
    }

    pub async fn stop(self) {
        if let Some(session) = self.session {
            let mut request = self
                .http
                .delete(&self.url)
                .headers(self.headers)
                .header("Mcp-Session-Id", session)
                .timeout(Duration::from_secs(3));
            if let Some(tokens) = self.tokens {
                request = request.bearer_auth(tokens.access_token);
            }
            if let Some(protocol) = self.protocol {
                request = request.header("MCP-Protocol-Version", protocol);
            }
            let _ = request.send().await;
        }
    }
}
