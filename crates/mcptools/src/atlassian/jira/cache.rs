use crate::prelude::*;
use std::fs;
use std::path::PathBuf;

/// Get the tmp directory for token caching
fn get_token_cache_dir() -> Result<PathBuf> {
    let cache_dir = dirs_next::cache_dir()
        .ok_or_else(|| eyre!("Unable to determine cache directory"))?
        .join("mcptools");

    fs::create_dir_all(&cache_dir).map_err(|e| eyre!("Failed to create cache directory: {}", e))?;

    Ok(cache_dir)
}

/// Generate a 6-character hash for a token and cache it
pub fn cache_token(token: &str) -> Result<String> {
    let hash = md5::compute(token.as_bytes());
    let hash_str = format!("{:x}", hash);
    let short_hash = hash_str[..6].to_string();

    let cache_dir = get_token_cache_dir()?;
    let cache_file = cache_dir.join(&short_hash);

    fs::write(&cache_file, token).map_err(|e| eyre!("Failed to write token to cache: {}", e))?;

    Ok(short_hash)
}

/// Resolve a token from cache by its 6-character hash
pub fn resolve_token(hash_or_token: &str) -> Result<String> {
    // If it looks like a token (long string with special chars), return as-is
    if hash_or_token.len() > 10 {
        return Ok(hash_or_token.to_string());
    }

    // Try to load from cache
    let cache_dir = get_token_cache_dir()?;
    let cache_file = cache_dir.join(hash_or_token);

    if cache_file.exists() {
        fs::read_to_string(&cache_file).map_err(|e| eyre!("Failed to read token from cache: {}", e))
    } else {
        Err(eyre!(
            "Token cache file not found for hash: {}. Token may have expired.",
            hash_or_token
        ))
    }
}
