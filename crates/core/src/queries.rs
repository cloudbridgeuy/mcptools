//! Query storage and retrieval functions
//!
//! Pure functions for managing saved JQL queries in the filesystem.
//! This module provides the functional core for query persistence.

use std::fs;
use std::path::Path;

/// Error type for query operations
#[derive(Debug)]
pub enum QueryError {
    IoError(String),
    QueryNotFound(String),
    QueryAlreadyExists(String),
    InvalidQueryName(String),
}

impl std::fmt::Display for QueryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QueryError::IoError(msg) => write!(f, "IO error: {}", msg),
            QueryError::QueryNotFound(name) => write!(f, "Query not found: {}", name),
            QueryError::QueryAlreadyExists(name) => {
                write!(
                    f,
                    "Query already exists: {}. Use --update to overwrite.",
                    name
                )
            }
            QueryError::InvalidQueryName(name) => write!(f, "Invalid query name: {}", name),
        }
    }
}

impl std::error::Error for QueryError {}

impl From<std::io::Error> for QueryError {
    fn from(err: std::io::Error) -> Self {
        QueryError::IoError(err.to_string())
    }
}

/// List all saved queries in the given directory
///
/// Returns a sorted vector of query names (without .jql extension)
pub fn list_queries(queries_dir: &Path) -> Result<Vec<String>, QueryError> {
    // If directory doesn't exist, return empty list
    if !queries_dir.exists() {
        return Ok(Vec::new());
    }

    let mut queries = Vec::new();

    for entry in fs::read_dir(queries_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("jql") {
            if let Some(name) = path.file_stem().and_then(|n| n.to_str()) {
                queries.push(name.to_string());
            }
        }
    }

    queries.sort();
    Ok(queries)
}

/// Load a query from the filesystem
///
/// # Arguments
/// * `queries_dir` - Directory containing .jql files
/// * `name` - Query name (without .jql extension)
///
/// # Returns
/// The query content as a String
pub fn load_query(queries_dir: &Path, name: &str) -> Result<String, QueryError> {
    validate_query_name(name)?;

    let query_path = queries_dir.join(format!("{}.jql", name));

    if !query_path.exists() {
        return Err(QueryError::QueryNotFound(name.to_string()));
    }

    fs::read_to_string(&query_path).map_err(QueryError::from)
}

/// Save a query to the filesystem
///
/// # Arguments
/// * `queries_dir` - Directory to store .jql files
/// * `name` - Query name (without .jql extension)
/// * `query` - JQL query content
/// * `overwrite` - If true, overwrites existing query; if false, errors on existing
///
/// # Returns
/// Ok(()) on success, QueryError on failure
pub fn save_query(
    queries_dir: &Path,
    name: &str,
    query: &str,
    overwrite: bool,
) -> Result<(), QueryError> {
    validate_query_name(name)?;

    // Create directory if it doesn't exist
    fs::create_dir_all(queries_dir)?;

    let query_path = queries_dir.join(format!("{}.jql", name));

    // Check if query already exists
    if query_path.exists() && !overwrite {
        return Err(QueryError::QueryAlreadyExists(name.to_string()));
    }

    fs::write(&query_path, query)?;
    Ok(())
}

/// Delete a query from the filesystem
///
/// # Arguments
/// * `queries_dir` - Directory containing .jql files
/// * `name` - Query name (without .jql extension)
pub fn delete_query(queries_dir: &Path, name: &str) -> Result<(), QueryError> {
    validate_query_name(name)?;

    let query_path = queries_dir.join(format!("{}.jql", name));

    if !query_path.exists() {
        return Err(QueryError::QueryNotFound(name.to_string()));
    }

    fs::remove_file(&query_path)?;
    Ok(())
}

/// Validate query name for security and usability
///
/// Query names must:
/// - Not be empty
/// - Only contain alphanumeric characters, hyphens, and underscores
fn validate_query_name(name: &str) -> Result<(), QueryError> {
    if name.is_empty() {
        return Err(QueryError::InvalidQueryName(
            "Query name cannot be empty".to_string(),
        ));
    }

    if !name
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return Err(QueryError::InvalidQueryName(
            "Query name can only contain alphanumeric characters, hyphens, and underscores"
                .to_string(),
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_save_and_load_query() {
        let temp_dir = TempDir::new().unwrap();
        let queries_dir = temp_dir.path();

        let query = r#"project = "Product Management" AND "Assigned Guild[Dropdown]" = DevOps"#;
        save_query(queries_dir, "devops", query, false).unwrap();

        let loaded = load_query(queries_dir, "devops").unwrap();
        assert_eq!(loaded, query);
    }

    #[test]
    fn test_list_queries() {
        let temp_dir = TempDir::new().unwrap();
        let queries_dir = temp_dir.path();

        save_query(queries_dir, "query1", "SELECT 1", false).unwrap();
        save_query(queries_dir, "query2", "SELECT 2", false).unwrap();
        save_query(queries_dir, "query3", "SELECT 3", false).unwrap();

        let queries = list_queries(queries_dir).unwrap();
        assert_eq!(queries, vec!["query1", "query2", "query3"]);
    }

    #[test]
    fn test_save_existing_without_overwrite() {
        let temp_dir = TempDir::new().unwrap();
        let queries_dir = temp_dir.path();

        save_query(queries_dir, "test", "query1", false).unwrap();

        let result = save_query(queries_dir, "test", "query2", false);
        assert!(result.is_err());
    }

    #[test]
    fn test_save_existing_with_overwrite() {
        let temp_dir = TempDir::new().unwrap();
        let queries_dir = temp_dir.path();

        save_query(queries_dir, "test", "query1", false).unwrap();
        save_query(queries_dir, "test", "query2", true).unwrap();

        let loaded = load_query(queries_dir, "test").unwrap();
        assert_eq!(loaded, "query2");
    }

    #[test]
    fn test_delete_query() {
        let temp_dir = TempDir::new().unwrap();
        let queries_dir = temp_dir.path();

        save_query(queries_dir, "test", "query", false).unwrap();
        delete_query(queries_dir, "test").unwrap();

        let result = load_query(queries_dir, "test");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_query_name() {
        assert!(validate_query_name("valid-query_name").is_ok());
        assert!(validate_query_name("ValidQuery123").is_ok());
        assert!(validate_query_name("").is_err());
        assert!(validate_query_name("invalid name").is_err());
        assert!(validate_query_name("invalid@query").is_err());
    }

    #[test]
    fn test_list_empty_directory() {
        let temp_dir = TempDir::new().unwrap();
        let queries_dir = temp_dir.path();

        let queries = list_queries(queries_dir).unwrap();
        assert_eq!(queries, Vec::<String>::new());
    }

    #[test]
    fn test_list_nonexistent_directory() {
        let temp_dir = TempDir::new().unwrap();
        let queries_dir = temp_dir.path().join("nonexistent");

        let queries = list_queries(&queries_dir).unwrap();
        assert_eq!(queries, Vec::<String>::new());
    }
}
