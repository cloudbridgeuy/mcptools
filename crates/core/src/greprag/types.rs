use std::path::PathBuf;

/// A code snippet returned by ripgrep.
#[derive(Debug, Clone)]
pub struct Snippet {
    /// Path to the file containing this snippet.
    pub file_path: PathBuf,
    /// First line number (1-based).
    pub start_line: usize,
    /// Last line number (1-based, inclusive).
    pub end_line: usize,
    /// The raw text of the snippet.
    pub content: String,
}

/// A snippet with a BM25 relevance score.
#[derive(Debug, Clone)]
pub struct RankedSnippet {
    /// The underlying code snippet.
    pub snippet: Snippet,
    /// BM25 relevance score.
    pub score: f64,
}

/// A deduplicated contiguous block formed by merging overlapping snippets.
#[derive(Debug, Clone)]
pub struct MergedSnippet {
    /// Path to the file containing this block.
    pub file_path: PathBuf,
    /// First line number (1-based).
    pub start_line: usize,
    /// Last line number (1-based, inclusive).
    pub end_line: usize,
    /// The merged text content.
    pub content: String,
    /// Aggregate relevance score.
    pub score: f64,
}
