/// Content of a file to be included as context for code generation.
#[derive(Debug, Clone)]
pub struct FileContent {
    /// File path (relative or absolute).
    pub path: String,
    /// Full text content of the file.
    pub content: String,
}

/// A request for Rust code generation.
#[derive(Debug, Clone)]
pub struct CodeRequest {
    /// The instruction describing what code to generate or modify.
    pub instruction: String,
    /// Optional additional context for the generation.
    pub context: Option<String>,
    /// Files to include as context.
    pub files: Vec<FileContent>,
}
