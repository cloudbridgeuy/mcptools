use crate::prelude::*;
use mcptools_core::images::{TokenSet, TokenState, OAUTH_CLIENT_ID, OAUTH_SCOPE, OAUTH_TOKEN_URL};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

pub fn auth_path(config_dir: &std::path::Path) -> std::path::PathBuf {
    config_dir.join("auth.json")
}

pub fn load(config_dir: &std::path::Path) -> Result<Option<TokenSet>> {
    let file = auth_path(config_dir);
    if !file.exists() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(&file)?;
    Ok(Some(serde_json::from_str(&raw).map_err(|e| {
        eyre!("Failed to parse {}: {e}", file.display())
    })?))
}

pub fn save(config_dir: &std::path::Path, tokens: &TokenSet) -> Result<()> {
    let file = auth_path(config_dir);
    std::fs::create_dir_all(config_dir)?;
    let mut handle = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&file)?;
    use std::io::Write;
    handle.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    handle.write_all(serde_json::to_string_pretty(tokens)?.as_bytes())?;
    handle.write_all(b"\n")?;
    Ok(())
}

pub async fn refresh(config_dir: &std::path::Path, tokens: &TokenSet) -> Result<TokenSet> {
    if tokens.refresh_token.is_empty() {
        return Err(eyre!(mcptools_core::images::NOT_SIGNED_IN));
    }
    let params = [
        ("grant_type", "refresh_token"),
        ("refresh_token", tokens.refresh_token.as_str()),
        ("client_id", OAUTH_CLIENT_ID),
        ("scope", OAUTH_SCOPE),
    ];
    let client = reqwest::Client::new();
    let resp = client
        .post(OAUTH_TOKEN_URL)
        .form(&params)
        .send()
        .await
        .map_err(|e| eyre!("Token refresh failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(eyre!(
            "Token refresh failed (HTTP {}). Run: llm-stream --login",
            resp.status()
        ));
    }
    let raw: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| eyre!("Token refresh failed: {e}"))?;
    let mut fresh = TokenSet {
        access_token: raw
            .get("access_token")
            .and_then(|v| v.as_str())
            .ok_or_else(|| eyre!("Token refresh failed: no access_token"))?
            .to_string(),
        refresh_token: raw
            .get("refresh_token")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        id_token: raw
            .get("id_token")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        account_id: None,
    };
    if fresh.refresh_token.is_empty() {
        fresh.refresh_token.clone_from(&tokens.refresh_token);
    }
    if fresh.id_token.is_empty() {
        fresh.id_token.clone_from(&tokens.id_token);
        fresh.account_id.clone_from(&tokens.account_id);
    }
    save(config_dir, &fresh)?;
    Ok(fresh)
}

pub async fn ensure_valid(config_dir: &std::path::Path) -> Result<TokenSet> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    match mcptools_core::images::classify_tokens(load(config_dir)?, now) {
        TokenState::Valid(set) => Ok(set),
        TokenState::Expired(set) => refresh(config_dir, &set).await,
        TokenState::Missing => Err(eyre!(mcptools_core::images::NOT_SIGNED_IN)),
    }
}
