use crate::prelude::*;
use mcptools_core::{
    gateway::Upstream,
    oauth::{self, Tokens},
};
use rand::RngCore;
use reqwest::Client;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[derive(Debug, clap::Parser)]
pub struct App {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, clap::Subcommand)]
enum Command {
    Login { name: String },
    Logout { name: String },
}

pub async fn run(app: App) -> Result<()> {
    let (name, login) = match app.command {
        Command::Login { name } => (name, true),
        Command::Logout { name } => (name, false),
    };
    let config =
        crate::config::optional()?.ok_or_else(|| eyre!("Missing gateway configuration"))?;
    let upstream = config
        .gateway
        .upstreams
        .iter()
        .find(|upstream| upstream.prefix == name && upstream.auth.as_deref() == Some("oauth"))
        .ok_or_else(|| eyre!("Unknown OAuth upstream"))?;
    if login {
        login_upstream(upstream).await
    } else {
        match std::fs::remove_file(token_path(upstream)?) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(eyre!("Cannot remove OAuth credentials")),
        }
        Ok(())
    }
}

fn client() -> Result<Client> {
    Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|_| eyre!("Cannot create OAuth client"))
}

fn token_path(upstream: &Upstream) -> Result<PathBuf> {
    let root = std::env::var_os("XDG_DATA_HOME")
        .filter(|path| PathBuf::from(path).is_absolute())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
        .ok_or_else(|| eyre!("Cannot locate OAuth credentials"))?;
    Ok(root
        .join("mcptools/oauth")
        .join(format!("{}.json", upstream.prefix)))
}

pub fn load(upstream: &Upstream) -> Result<Tokens> {
    let bytes = std::fs::read(token_path(upstream)?).map_err(|_| login_error(upstream))?;
    let tokens: Tokens = serde_json::from_slice(&bytes).map_err(|_| login_error(upstream))?;
    if upstream
        .url
        .as_deref()
        .is_none_or(|url| !oauth::resource_matches(&tokens.resource, url))
        || oauth::safe_url(&tokens.token_endpoint).is_none()
        || tokens.access_token.is_empty()
    {
        return Err(login_error(upstream));
    }
    Ok(tokens)
}

fn save(upstream: &Upstream, tokens: &Tokens) -> Result<()> {
    let path = token_path(upstream)?;
    let parent = path
        .parent()
        .ok_or_else(|| eyre!("Cannot store OAuth credentials"))?;
    std::fs::create_dir_all(parent).map_err(|_| eyre!("Cannot store OAuth credentials"))?;
    let mut staged = tempfile::NamedTempFile::new_in(parent)
        .map_err(|_| eyre!("Cannot store OAuth credentials"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        staged
            .as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o600))
            .map_err(|_| eyre!("Cannot protect OAuth credentials"))?;
    }
    serde_json::to_writer(&mut staged, tokens)
        .map_err(|_| eyre!("Cannot store OAuth credentials"))?;
    staged
        .as_file()
        .sync_all()
        .map_err(|_| eyre!("Cannot store OAuth credentials"))?;
    staged
        .persist(path)
        .map_err(|_| eyre!("Cannot store OAuth credentials"))?;
    Ok(())
}

pub fn login_error(upstream: &Upstream) -> color_eyre::eyre::Report {
    eyre!(
        "OAuth authentication failed for {}; run mcptools gateway login {}",
        upstream.prefix,
        upstream.prefix
    )
}

pub(super) fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

async fn json_response(mut response: reqwest::Response) -> Result<Value> {
    if !response.status().is_success() {
        return Err(eyre!("OAuth endpoint rejected request"));
    }
    if response
        .content_length()
        .is_some_and(|size| size > 1024 * 1024)
    {
        return Err(eyre!("OAuth response too large"));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| eyre!("OAuth response failed"))?
    {
        if bytes.len().saturating_add(chunk.len()) > 1024 * 1024 {
            return Err(eyre!("OAuth response too large"));
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| eyre!("Invalid OAuth response"))
}

async fn get_json(client: &Client, url: &str) -> Result<Value> {
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|_| eyre!("OAuth discovery failed"))?;
    json_response(response).await
}

fn random() -> String {
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    oauth::verifier(&bytes)
}

async fn login_upstream(upstream: &Upstream) -> Result<()> {
    let client = client()?;
    let resource = upstream
        .url
        .as_deref()
        .ok_or_else(|| eyre!("Missing OAuth resource"))?;
    let response = client
        .post(resource)
        .header("Accept", "application/json, text/event-stream")
        .json(&json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}))
        .send()
        .await
        .map_err(|_| eyre!("OAuth discovery failed"))?;
    if response.status() != reqwest::StatusCode::UNAUTHORIZED {
        return Err(eyre!("OAuth discovery requires an HTTP 401 challenge"));
    }
    let metadata_url = response
        .headers()
        .get_all(reqwest::header::WWW_AUTHENTICATE)
        .iter()
        .filter_map(|header| header.to_str().ok())
        .find_map(oauth::metadata_challenge)
        .ok_or_else(|| eyre!("Missing OAuth resource metadata"))?;
    let metadata_url =
        oauth::metadata_url(resource, &metadata_url).map_err(|error| eyre!(error))?;
    let metadata = get_json(&client, metadata_url.as_str()).await?;
    let issuer_text = oauth::issuer(&metadata, resource).map_err(|error| eyre!(error))?;
    let resource = oauth::resource(&metadata, resource).map_err(|error| eyre!(error))?;
    let [discovery, openid] = oauth::discovery_urls(issuer_text).map_err(|error| eyre!(error))?;
    let server = match get_json(&client, discovery.as_str()).await {
        Ok(server) => server,
        Err(_) => get_json(&client, openid.as_str()).await?,
    };
    let server =
        oauth::ServerMetadata::parse(&server, issuer_text).map_err(|error| eyre!(error))?;
    let port = upstream.redirect_port.unwrap_or(0);
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port))
        .await
        .map_err(|_| eyre!("Cannot bind OAuth callback port {port} on 127.0.0.1"))?;
    let redirect = oauth::redirect_uri(upstream, listener.local_addr()?.port());
    let callback_url =
        reqwest::Url::parse(&redirect).map_err(|_| eyre!("Invalid OAuth redirect URI"))?;
    let (client_id, client_secret) = if let Some(id) = &upstream.client_id {
        (id.clone(), None)
    } else {
        let registration = server
            .registration_endpoint()
            .map_err(|error| eyre!(error))?;
        let response = client.post(registration).json(&json!({"client_name":"mcptools","redirect_uris":[redirect],"grant_types":["authorization_code","refresh_token"],"response_types":["code"],"token_endpoint_auth_method":"none"})).send().await.map_err(|_| eyre!("OAuth registration failed"))?;
        let registered = json_response(response).await?;
        oauth::registered_client(&registered).map_err(|error| eyre!(error))?
    };
    let client_auth = server
        .client_auth(upstream.client_secret.is_some() || client_secret.is_some())
        .map_err(|error| eyre!(error))?;
    let verifier = random();
    let state = random();
    let url = server
        .authorization_url(
            &client_id,
            &redirect,
            &state,
            &verifier,
            resource,
            &upstream.scopes,
        )
        .map_err(|error| eyre!(error))?;
    std::println!("Open this URL to authorize: {url}");
    #[cfg(target_os = "macos")]
    let browser = "open";
    #[cfg(not(target_os = "macos"))]
    let browser = "xdg-open";
    let _ = std::process::Command::new(browser)
        .arg(url.as_str())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    let code = tokio::time::timeout(Duration::from_secs(180), async {
        loop {
            let (mut socket, _) = listener
                .accept()
                .await
                .map_err(|_| eyre!("OAuth callback failed"))?;
            let mut bytes = [0u8; 8192];
            let size =
                match tokio::time::timeout(Duration::from_secs(3), socket.read(&mut bytes)).await {
                    Ok(Ok(size)) => size,
                    _ => continue,
                };
            let request = String::from_utf8_lossy(&bytes[..size]);
            let target = request
                .lines()
                .next()
                .and_then(|line| line.strip_prefix("GET "))
                .and_then(|line| line.split_once(' '))
                .map(|(target, _)| target);
            let (code, response) = oauth::callback_response(
                &upstream.prefix,
                target.unwrap_or_default(),
                &state,
                callback_url.path(),
            );
            let _ = socket.write_all(response.as_bytes()).await;
            if let Some(code) = code {
                break Ok::<_, color_eyre::eyre::Report>(code);
            }
        }
    })
    .await
    .map_err(|_| eyre!("OAuth callback timed out"))??;
    let tokens = Tokens {
        access_token: String::new(),
        refresh_token: None,
        client_id,
        client_secret,
        client_auth,
        token_endpoint: server.token_endpoint,
        resource: resource.to_owned(),
        expires_at: None,
    };
    let form = vec![
        ("grant_type", "authorization_code"),
        ("code", code.as_str()),
        ("redirect_uri", redirect.as_str()),
        ("code_verifier", verifier.as_str()),
        ("resource", resource),
    ];
    let response = token_request(&client, upstream, &tokens, form)
        .send()
        .await
        .map_err(|_| eyre!("OAuth token request failed"))?;
    let value = json_response(response).await?;
    let tokens = tokens
        .updated(&value, now())
        .map_err(|error| eyre!(error))?;
    save(upstream, &tokens)?;
    Ok(())
}

fn token_request<'a>(
    client: &Client,
    upstream: &'a Upstream,
    tokens: &'a Tokens,
    mut form: Vec<(&'a str, &'a str)>,
) -> reqwest::RequestBuilder {
    let secret = upstream
        .client_secret
        .as_deref()
        .or(tokens.client_secret.as_deref());
    let mut request = client.post(&tokens.token_endpoint);
    match (tokens.client_auth, secret) {
        (oauth::ClientAuth::Basic, Some(secret)) => {
            request = request.header(
                reqwest::header::AUTHORIZATION,
                oauth::basic_authorization(&tokens.client_id, secret),
            );
        }
        (_, secret) => {
            form.push(("client_id", &tokens.client_id));
            if let Some(secret) = secret {
                form.push(("client_secret", secret));
            }
        }
    }
    request.form(&form)
}

pub async fn refresh(upstream: &Upstream, tokens: &mut Tokens) -> Result<()> {
    let refresh_token = tokens
        .refresh_token
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| login_error(upstream))?;
    let mut form = vec![
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
        ("resource", &tokens.resource),
    ];
    if let Some(redirect) = &upstream.redirect_uri {
        form.push(("redirect_uri", redirect));
    }
    let response = token_request(&client()?, upstream, tokens, form)
        .send()
        .await
        .map_err(|_| login_error(upstream))?;
    let value = json_response(response)
        .await
        .map_err(|_| login_error(upstream))?;
    *tokens = tokens
        .updated(&value, now())
        .map_err(|_| login_error(upstream))?;
    save(upstream, tokens).map_err(|_| login_error(upstream))?;
    Ok(())
}
