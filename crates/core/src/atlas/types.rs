use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// Parsed from tree-sitter node types — exhaustive, no string matching downstream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SymbolKind {
    Function,
    Method,
    Struct,
    Enum,
    Trait,
    Class,
    Interface,
    Type,
    Const,
    Module,
}

impl fmt::Display for SymbolKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Function => "function",
            Self::Method => "method",
            Self::Struct => "struct",
            Self::Enum => "enum",
            Self::Trait => "trait",
            Self::Class => "class",
            Self::Interface => "interface",
            Self::Type => "type",
            Self::Const => "const",
            Self::Module => "module",
        };
        f.write_str(s)
    }
}

impl FromStr for SymbolKind {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "function" => Ok(Self::Function),
            "method" => Ok(Self::Method),
            "struct" => Ok(Self::Struct),
            "enum" => Ok(Self::Enum),
            "trait" => Ok(Self::Trait),
            "class" => Ok(Self::Class),
            "interface" => Ok(Self::Interface),
            "type" => Ok(Self::Type),
            "const" => Ok(Self::Const),
            "module" => Ok(Self::Module),
            other => Err(format!("unknown SymbolKind: {other}")),
        }
    }
}

/// Parsed from tree-sitter visibility modifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Visibility {
    Public,
    PublicCrate,
    Export,
    Private,
}

impl fmt::Display for Visibility {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Public => "public",
            Self::PublicCrate => "public_crate",
            Self::Export => "export",
            Self::Private => "private",
        };
        f.write_str(s)
    }
}

impl FromStr for Visibility {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "public" => Ok(Self::Public),
            "public_crate" => Ok(Self::PublicCrate),
            "export" => Ok(Self::Export),
            "private" => Ok(Self::Private),
            other => Err(format!("unknown Visibility: {other}")),
        }
    }
}

/// SHA-256 content hash. Private inner field — construct only via `ContentHash::new()`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentHash([u8; 32]);

impl ContentHash {
    /// Create a new `ContentHash` from raw bytes.
    pub fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Return the lowercase hex representation of the hash.
    pub fn hex(&self) -> String {
        use std::fmt::Write;
        let mut s = String::with_capacity(64);
        for b in &self.0 {
            write!(s, "{b:02x}").expect("writing to String never fails");
        }
        s
    }

    /// Parse a hex string back into a `ContentHash`.
    pub fn from_hex(hex: &str) -> Result<Self, String> {
        if hex.len() != 64 {
            return Err(format!("expected 64 hex characters, got {}", hex.len()));
        }
        let mut bytes = [0u8; 32];
        for (i, byte) in bytes.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16)
                .map_err(|e| format!("invalid hex at position {}: {e}", i * 2))?;
        }
        Ok(Self(bytes))
    }
}

/// Which tree-sitter tier a file belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexTier {
    /// Full: tree-sitter + LLM (V2).
    Full,
    /// Light: LLM description only (V2).
    Light,
    /// Not indexed.
    Skip,
}

impl IndexTier {
    /// Determine the index tier from a file extension.
    pub fn from_extension(ext: &str) -> Self {
        if Language::from_extension(ext).is_some() {
            return Self::Full;
        }
        match ext {
            "md" | "txt" | "json" | "yaml" | "yml" | "toml" | "cfg" | "ini" | "xml" | "html"
            | "css" | "scss" | "sql" | "sh" | "bash" | "zsh" => Self::Light,
            _ => Self::Skip,
        }
    }
}

/// A symbol extracted from source code.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Symbol {
    pub file_path: PathBuf,
    pub name: String,
    pub kind: SymbolKind,
    pub signature: Option<String>,
    pub visibility: Visibility,
    pub start_line: u32,
    pub end_line: u32,
}

/// A file entry in the index (write model — stored in SQLite).
#[derive(Debug, Clone)]
pub struct FileEntry {
    pub path: PathBuf,
    pub content_hash: ContentHash,
    pub tree_sitter_hash: Option<ContentHash>,
    pub short_description: Option<String>,
    pub long_description: Option<String>,
    pub indexed_at: String,
}

/// A row in the tree view output (read model — display-optimized).
#[derive(Debug, Clone, Serialize)]
pub struct TreeEntry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub short_description: Option<String>,
}

/// The peek view for a file (read model — display-optimized).
#[derive(Debug, Clone, Serialize)]
pub struct PeekView {
    pub path: PathBuf,
    pub short_description: Option<String>,
    pub long_description: Option<String>,
    pub symbols: Vec<Symbol>,
}

/// A directory entry in the index (write model).
#[derive(Debug, Clone)]
pub struct DirectoryEntry {
    pub path: PathBuf,
    pub short_description: Option<String>,
    pub long_description: Option<String>,
    pub indexed_at: String,
}

/// Peek view for a directory (read model).
#[derive(Debug, Clone, Serialize)]
pub struct DirectoryPeekView {
    pub path: PathBuf,
    pub short_description: Option<String>,
    pub long_description: Option<String>,
    pub children: Vec<TreeEntry>,
    pub symbols: Vec<Symbol>,
}

/// Which language a file belongs to (for tree-sitter grammar selection).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    Rust,
    TypeScript,
    Tsx,
    JavaScript,
    Jsx,
    Python,
    Go,
}

impl Language {
    /// Determine the language from a file extension, or `None` if unsupported.
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext {
            "rs" => Some(Self::Rust),
            "ts" => Some(Self::TypeScript),
            "tsx" => Some(Self::Tsx),
            "js" => Some(Self::JavaScript),
            "jsx" => Some(Self::Jsx),
            "py" => Some(Self::Python),
            "go" => Some(Self::Go),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symbol_kind_roundtrips_through_display_and_from_str() {
        let kinds = [
            SymbolKind::Function,
            SymbolKind::Method,
            SymbolKind::Struct,
            SymbolKind::Enum,
            SymbolKind::Trait,
            SymbolKind::Class,
            SymbolKind::Interface,
            SymbolKind::Type,
            SymbolKind::Const,
            SymbolKind::Module,
        ];
        for kind in kinds {
            let s = kind.to_string();
            let parsed: SymbolKind = s.parse().unwrap();
            assert_eq!(parsed, kind, "round-trip failed for {kind:?}");
        }
    }

    #[test]
    fn symbol_kind_from_str_rejects_unknown() {
        assert!("unknown".parse::<SymbolKind>().is_err());
    }

    #[test]
    fn visibility_roundtrips_through_display_and_from_str() {
        let variants = [
            Visibility::Public,
            Visibility::PublicCrate,
            Visibility::Export,
            Visibility::Private,
        ];
        for vis in variants {
            let s = vis.to_string();
            let parsed: Visibility = s.parse().unwrap();
            assert_eq!(parsed, vis, "round-trip failed for {vis:?}");
        }
    }

    #[test]
    fn visibility_from_str_rejects_unknown() {
        assert!("unknown".parse::<Visibility>().is_err());
    }

    #[test]
    fn content_hash_hex_produces_correct_string() {
        let mut bytes = [0u8; 32];
        bytes[0] = 0xab;
        bytes[1] = 0xcd;
        bytes[31] = 0xff;
        let hash = ContentHash::new(bytes);
        let hex = hash.hex();
        assert_eq!(hex.len(), 64);
        assert!(hex.starts_with("abcd"));
        assert!(hex.ends_with("ff"));
    }

    #[test]
    fn content_hash_roundtrips_through_hex() {
        let mut bytes = [0u8; 32];
        bytes[0] = 0xab;
        bytes[15] = 0xff;
        bytes[31] = 0x01;
        let hash = ContentHash::new(bytes);
        let hex = hash.hex();
        let parsed = ContentHash::from_hex(&hex).unwrap();
        assert_eq!(parsed, hash);
    }

    #[test]
    fn content_hash_from_hex_rejects_invalid() {
        assert!(ContentHash::from_hex("abc").is_err()); // too short
        assert!(ContentHash::from_hex(&"g".repeat(64)).is_err()); // invalid hex char
    }

    #[test]
    fn content_hash_hex_all_zeros() {
        let hash = ContentHash::new([0u8; 32]);
        assert_eq!(hash.hex(), "0".repeat(64));
    }

    #[test]
    fn index_tier_from_extension_full() {
        for ext in ["rs", "ts", "tsx", "js", "jsx", "py", "go"] {
            assert_eq!(
                IndexTier::from_extension(ext),
                IndexTier::Full,
                "expected Full for .{ext}"
            );
        }
    }

    #[test]
    fn index_tier_from_extension_light() {
        for ext in [
            "md", "txt", "json", "yaml", "yml", "toml", "html", "css", "sh",
        ] {
            assert_eq!(
                IndexTier::from_extension(ext),
                IndexTier::Light,
                "expected Light for .{ext}"
            );
        }
    }

    #[test]
    fn index_tier_from_extension_skip() {
        for ext in ["png", "jpg", "exe", "bin", "wasm", ""] {
            assert_eq!(
                IndexTier::from_extension(ext),
                IndexTier::Skip,
                "expected Skip for .{ext}"
            );
        }
    }

    #[test]
    fn language_from_extension_known() {
        let cases = [
            ("rs", Language::Rust),
            ("ts", Language::TypeScript),
            ("tsx", Language::Tsx),
            ("js", Language::JavaScript),
            ("jsx", Language::Jsx),
            ("py", Language::Python),
            ("go", Language::Go),
        ];
        for (ext, expected) in cases {
            assert_eq!(
                Language::from_extension(ext),
                Some(expected),
                "wrong language for .{ext}"
            );
        }
    }

    #[test]
    fn language_from_extension_unknown() {
        assert_eq!(Language::from_extension("rb"), None);
        assert_eq!(Language::from_extension(""), None);
        assert_eq!(Language::from_extension("cpp"), None);
    }
}
