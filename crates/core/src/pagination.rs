//! Pagination token storage and retrieval functions
//!
//! Pure functions for managing pagination tokens in the filesystem.
//! This module provides the functional core for pagination token persistence.
//! Tokens are stored using MD5 hashes for space efficiency, with an 8-character
//! hash prefix shown to users.

use std::fs;
use std::path::Path;

/// Error type for pagination operations
#[derive(Debug)]
pub enum PaginationError {
    IoError(String),
    TokenNotFound(String),
    InvalidTokenHash(String),
    HashError(String),
}

impl std::fmt::Display for PaginationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PaginationError::IoError(msg) => write!(f, "IO error: {}", msg),
            PaginationError::TokenNotFound(hash) => {
                write!(f, "Pagination token not found: {}", hash)
            }
            PaginationError::InvalidTokenHash(hash) => {
                write!(f, "Invalid token hash format: {}", hash)
            }
            PaginationError::HashError(msg) => write!(f, "Hash error: {}", msg),
        }
    }
}

impl std::error::Error for PaginationError {}

impl From<std::io::Error> for PaginationError {
    fn from(err: std::io::Error) -> Self {
        PaginationError::IoError(err.to_string())
    }
}

/// Generate MD5 hash of a pagination token
fn hash_token(token: &str) -> String {
    format!("{:x}", md5::compute(token.as_bytes()))
}

/// Save a pagination token to the filesystem
///
/// # Arguments
/// * `pagination_dir` - Directory to store token files
/// * `token` - The pagination token to store
///
/// # Returns
/// The first 8 characters of the MD5 hash on success, PaginationError on failure
pub fn save_token(pagination_dir: &Path, token: &str) -> Result<String, PaginationError> {
    // Create directory if it doesn't exist
    fs::create_dir_all(pagination_dir)?;

    let full_hash = hash_token(token);
    let token_path = pagination_dir.join(&full_hash);

    fs::write(&token_path, token)?;

    // Return first 8 characters of hash
    Ok(full_hash[..8].to_string())
}

/// Load a pagination token from the filesystem
///
/// # Arguments
/// * `pagination_dir` - Directory containing token files
/// * `hash_prefix` - First 8 characters of the MD5 hash (user-provided)
///
/// # Returns
/// The full pagination token as a String
pub fn load_token(pagination_dir: &Path, hash_prefix: &str) -> Result<String, PaginationError> {
    if hash_prefix.len() != 8 {
        return Err(PaginationError::InvalidTokenHash(
            "Token hash must be exactly 8 characters".to_string(),
        ));
    }

    if !hash_prefix.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(PaginationError::InvalidTokenHash(
            "Token hash must contain only hexadecimal characters".to_string(),
        ));
    }

    // Try to find a file matching the prefix
    if !pagination_dir.exists() {
        return Err(PaginationError::TokenNotFound(hash_prefix.to_string()));
    }

    for entry in fs::read_dir(pagination_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() {
            if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                if filename.starts_with(hash_prefix) && filename.len() == 32 {
                    // MD5 hash is always 32 hex characters
                    let token = fs::read_to_string(&path)?;
                    return Ok(token);
                }
            }
        }
    }

    Err(PaginationError::TokenNotFound(hash_prefix.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_save_and_load_token() {
        let temp_dir = TempDir::new().unwrap();
        let pagination_dir = temp_dir.path();

        let token = "Ch0jU3RyaW5nJlVGSlBSQT09JUludCZNell4TURNPRAeGILa5q2lMyJZcHJvamVjdCA9ICJQUk9EIiBBTkQgIkFzc2lnbmVkIEd1aWxkW0Ryb3Bkb3duXSIgPSBEZXZPcHMgQU5EIHN0YXR1cyBOT1QgSU4gKERvbmUsIENsb3NlZCkqAltd";
        let hash = save_token(pagination_dir, token).unwrap();

        assert_eq!(hash.len(), 8);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));

        let loaded = load_token(pagination_dir, &hash).unwrap();
        assert_eq!(loaded, token);
    }

    #[test]
    fn test_hash_consistency() {
        let token = "test_token";
        let hash1 = hash_token(token);
        let hash2 = hash_token(token);
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_load_nonexistent_token() {
        let temp_dir = TempDir::new().unwrap();
        let pagination_dir = temp_dir.path();

        let result = load_token(pagination_dir, "12345678");
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_hash_length() {
        let temp_dir = TempDir::new().unwrap();
        let pagination_dir = temp_dir.path();

        let result = load_token(pagination_dir, "1234567");
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_hash_format() {
        let temp_dir = TempDir::new().unwrap();
        let pagination_dir = temp_dir.path();

        let result = load_token(pagination_dir, "invalidhash");
        assert!(result.is_err());
    }

    #[test]
    fn test_multiple_tokens() {
        let temp_dir = TempDir::new().unwrap();
        let pagination_dir = temp_dir.path();

        let token1 = "token_one";
        let token2 = "token_two";

        let hash1 = save_token(pagination_dir, token1).unwrap();
        let hash2 = save_token(pagination_dir, token2).unwrap();

        assert_ne!(hash1, hash2);

        let loaded1 = load_token(pagination_dir, &hash1).unwrap();
        let loaded2 = load_token(pagination_dir, &hash2).unwrap();

        assert_eq!(loaded1, token1);
        assert_eq!(loaded2, token2);
    }
}
