use std::collections::BTreeSet;
use std::fmt::Write;
use std::path::Path;

use crate::atlas::types::{DirectoryPeekView, PeekView, Symbol, TreeEntry, Visibility};

/// Remove a single trailing newline, if present.
fn trim_trailing_newline(mut s: String) -> String {
    if s.ends_with('\n') {
        s.pop();
    }
    s
}

/// Sort tree entries so directories appear immediately before their children.
///
/// Directories sort before sibling files at the same path level by appending
/// a `/` separator to directory paths during comparison.
pub fn sort_tree_entries(entries: &mut [TreeEntry]) {
    entries.sort_by(|a, b| {
        let a_path = a.path.to_string_lossy();
        let b_path = b.path.to_string_lossy();
        // Primary: sort by path. Tiebreak: directories before files.
        a_path.cmp(&b_path).then_with(|| b.is_dir.cmp(&a.is_dir))
    });
}

/// Extract all unique parent directory paths from a set of file paths.
///
/// Walks each file path upward, collecting every non-empty ancestor. Returns
/// a sorted, deduplicated list.
pub fn extract_parent_paths(file_paths: &[impl AsRef<Path>]) -> Vec<String> {
    let mut parents = BTreeSet::new();
    for file_path in file_paths {
        let mut current: &Path = file_path.as_ref();
        while let Some(parent) = current.parent() {
            if parent.as_os_str().is_empty() {
                break;
            }
            parents.insert(parent.to_string_lossy().to_string());
            current = parent;
        }
    }
    parents.into_iter().collect()
}

/// Format a tree view from a list of entries.
/// Returns human-readable text or JSON depending on the format flag.
pub fn format_tree(entries: &[TreeEntry], json: bool) -> String {
    if json {
        serde_json::to_string_pretty(entries).unwrap_or_else(|_| "[]".to_string())
    } else {
        format_tree_human(entries)
    }
}

/// Format a peek view for a file.
/// Returns human-readable text or JSON depending on the format flag.
pub fn format_peek(peek: &PeekView, json: bool) -> String {
    if json {
        serde_json::to_string_pretty(peek).unwrap_or_else(|_| "{}".to_string())
    } else {
        format_peek_human(peek)
    }
}

/// Format a peek view for a directory.
/// Returns human-readable text or JSON depending on the format flag.
pub fn format_directory_peek(peek: &DirectoryPeekView, json: bool) -> String {
    if json {
        serde_json::to_string_pretty(peek).unwrap_or_else(|_| "{}".to_string())
    } else {
        format_directory_peek_human(peek)
    }
}

fn format_directory_peek_human(peek: &DirectoryPeekView) -> String {
    if peek.path.as_os_str().is_empty() && peek.children.is_empty() && peek.symbols.is_empty() {
        return String::from("(empty)");
    }

    let mut out = String::new();

    let _ = writeln!(out, "{}/", peek.path.display());

    if let Some(desc) = &peek.short_description {
        let _ = writeln!(out, "{desc}");
    }

    if let Some(long) = &peek.long_description {
        let _ = writeln!(out);
        let _ = writeln!(out, "{long}");
    }

    if !peek.children.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(out, "Contents:");
        for child in &peek.children {
            let slash = if child.is_dir { "/" } else { "" };
            match &child.short_description {
                Some(desc) => {
                    let _ = writeln!(out, "  {}{slash} \u{2014} {desc}", child.name);
                }
                None => {
                    let _ = writeln!(out, "  {}{slash}", child.name);
                }
            }
        }
    }

    if !peek.symbols.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(out, "Symbols:");
        for sym in &peek.symbols {
            let line = format_symbol_line(sym);
            let _ = writeln!(out, "  {line}");
        }
    }

    trim_trailing_newline(out)
}

fn format_tree_human(entries: &[TreeEntry]) -> String {
    if entries.is_empty() {
        return String::from("(empty)");
    }

    let mut out = String::new();
    for entry in entries {
        let depth = entry.path.components().count().saturating_sub(1);
        let indent = "  ".repeat(depth);
        let slash = if entry.is_dir { "/" } else { "" };

        match &entry.short_description {
            Some(desc) => {
                let _ = writeln!(out, "{indent}{}{slash} \u{2014} {desc}", entry.name);
            }
            None => {
                let _ = writeln!(out, "{indent}{}{slash}", entry.name);
            }
        }
    }

    trim_trailing_newline(out)
}

fn format_peek_human(peek: &PeekView) -> String {
    if peek.path.as_os_str().is_empty() && peek.symbols.is_empty() {
        return String::from("(empty)");
    }

    let mut out = String::new();

    let _ = writeln!(out, "{}", peek.path.display());

    if let Some(desc) = &peek.short_description {
        let _ = writeln!(out, "{desc}");
    }

    if let Some(long) = &peek.long_description {
        let _ = writeln!(out);
        let _ = writeln!(out, "{long}");
    }

    if !peek.symbols.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(out, "Symbols:");
        for sym in &peek.symbols {
            let line = format_symbol_line(sym);
            let _ = writeln!(out, "  {line}");
        }
    }

    trim_trailing_newline(out)
}

fn format_symbol_line(sym: &Symbol) -> String {
    let range = format!("[{}-{}]", sym.start_line, sym.end_line);

    if let Some(sig) = &sym.signature {
        // Signatures already include visibility keywords (e.g., "pub fn foo()"),
        // so don't prepend a redundant visibility prefix.
        format!("{sig}    {range}")
    } else {
        let vis_prefix = match sym.visibility {
            Visibility::Public | Visibility::Export => "pub ",
            Visibility::PublicCrate => "pub(crate) ",
            Visibility::Private => "",
        };
        format!("{vis_prefix}{} {}    {range}", sym.kind, sym.name)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::atlas::types::{DirectoryPeekView, SymbolKind, Visibility};

    fn make_tree_entry(name: &str, path: &str, is_dir: bool, desc: Option<&str>) -> TreeEntry {
        TreeEntry {
            name: name.to_string(),
            path: PathBuf::from(path),
            is_dir,
            short_description: desc.map(String::from),
        }
    }

    fn make_symbol(
        name: &str,
        kind: SymbolKind,
        signature: Option<&str>,
        visibility: Visibility,
        start: u32,
        end: u32,
    ) -> Symbol {
        Symbol {
            file_path: PathBuf::from("src/lib.rs"),
            name: name.to_string(),
            kind,
            signature: signature.map(String::from),
            visibility,
            start_line: start,
            end_line: end,
        }
    }

    // ---- format_tree tests ----

    #[test]
    fn format_tree_mixed_files_and_dirs() {
        let entries = vec![
            make_tree_entry("src", "src", true, None),
            make_tree_entry("atlas", "src/atlas", true, None),
            make_tree_entry("mod.rs", "src/atlas/mod.rs", false, Some("re-exports")),
            make_tree_entry(
                "types.rs",
                "src/atlas/types.rs",
                false,
                Some("domain types"),
            ),
            make_tree_entry(
                "symbols.rs",
                "src/atlas/symbols.rs",
                false,
                Some("symbol extraction"),
            ),
        ];

        let result = format_tree(&entries, false);
        let expected = "\
src/
  atlas/
    mod.rs \u{2014} re-exports
    types.rs \u{2014} domain types
    symbols.rs \u{2014} symbol extraction";
        assert_eq!(result, expected);
    }

    #[test]
    fn format_tree_depth_limited_entries() {
        // Simulate entries that only go one level deep (as if depth-limited by query).
        let entries = vec![
            make_tree_entry("src", "src", true, None),
            make_tree_entry("Cargo.toml", "Cargo.toml", false, Some("manifest")),
        ];

        let result = format_tree(&entries, false);
        let expected = "src/\nCargo.toml \u{2014} manifest";
        assert_eq!(result, expected);
    }

    #[test]
    fn format_tree_json_produces_valid_json_array() {
        let entries = vec![
            make_tree_entry("src", "src", true, None),
            make_tree_entry("main.rs", "src/main.rs", false, Some("entry point")),
        ];

        let result = format_tree(&entries, true);
        let parsed: serde_json::Value = serde_json::from_str(&result).expect("valid JSON");
        assert!(parsed.is_array());
        assert_eq!(parsed.as_array().unwrap().len(), 2);
    }

    #[test]
    fn format_tree_empty_input() {
        let result = format_tree(&[], false);
        assert_eq!(result, "(empty)");
    }

    #[test]
    fn format_tree_empty_input_json() {
        let result = format_tree(&[], true);
        let parsed: serde_json::Value = serde_json::from_str(&result).expect("valid JSON");
        assert!(parsed.is_array());
        assert!(parsed.as_array().unwrap().is_empty());
    }

    // ---- format_peek tests ----

    #[test]
    fn format_peek_with_symbols() {
        let peek = PeekView {
            path: PathBuf::from("src/atlas/symbols.rs"),
            short_description: Some("Symbol extraction utilities".to_string()),
            long_description: None,
            symbols: vec![
                make_symbol(
                    "extract_symbols",
                    SymbolKind::Function,
                    Some("fn extract_symbols(tree: &Tree, source: &[u8], language: Language, file_path: &Path) -> Vec<Symbol>"),
                    Visibility::Public,
                    1,
                    45,
                ),
                make_symbol(
                    "extract_rust_symbols",
                    SymbolKind::Function,
                    Some("fn extract_rust_symbols(cursor: &mut TreeCursor, source: &[u8]) -> Vec<Symbol>"),
                    Visibility::Private,
                    47,
                    120,
                ),
            ],
        };

        let result = format_peek(&peek, false);
        assert!(result.contains("src/atlas/symbols.rs"));
        assert!(result.contains("Symbol extraction utilities"));
        assert!(result.contains("Symbols:"));
        assert!(result.contains("fn extract_symbols"));
        assert!(result.contains("[1-45]"));
        assert!(result.contains("fn extract_rust_symbols"));
        assert!(result.contains("[47-120]"));
    }

    #[test]
    fn format_peek_no_symbols() {
        let peek = PeekView {
            path: PathBuf::from("config.yaml"),
            short_description: Some("Application configuration".to_string()),
            long_description: Some("Contains database and API settings.".to_string()),
            symbols: vec![],
        };

        let result = format_peek(&peek, false);
        assert!(result.contains("config.yaml"));
        assert!(result.contains("Application configuration"));
        assert!(result.contains("Contains database and API settings."));
        assert!(!result.contains("Symbols:"));
    }

    #[test]
    fn format_peek_json_produces_valid_json() {
        let peek = PeekView {
            path: PathBuf::from("src/lib.rs"),
            short_description: Some("Library root".to_string()),
            long_description: None,
            symbols: vec![make_symbol(
                "add",
                SymbolKind::Function,
                Some("fn add(a: i32, b: i32) -> i32"),
                Visibility::Public,
                1,
                3,
            )],
        };

        let result = format_peek(&peek, true);
        let parsed: serde_json::Value = serde_json::from_str(&result).expect("valid JSON");
        assert!(parsed.is_object());
        assert_eq!(parsed["path"], "src/lib.rs");
        assert!(parsed["symbols"].is_array());
        assert_eq!(parsed["symbols"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn format_peek_empty_input() {
        let peek = PeekView {
            path: PathBuf::new(),
            short_description: None,
            long_description: None,
            symbols: vec![],
        };

        let result = format_peek(&peek, false);
        assert_eq!(result, "(empty)");
    }

    #[test]
    fn format_peek_symbol_without_signature_shows_kind_and_name() {
        let peek = PeekView {
            path: PathBuf::from("src/lib.rs"),
            short_description: None,
            long_description: None,
            symbols: vec![make_symbol(
                "Config",
                SymbolKind::Struct,
                None,
                Visibility::Public,
                10,
                25,
            )],
        };

        let result = format_peek(&peek, false);
        assert!(result.contains("pub struct Config"));
        assert!(result.contains("[10-25]"));
    }

    #[test]
    fn format_peek_pub_crate_visibility() {
        let peek = PeekView {
            path: PathBuf::from("src/lib.rs"),
            short_description: None,
            long_description: None,
            symbols: vec![make_symbol(
                "helper",
                SymbolKind::Function,
                None,
                Visibility::PublicCrate,
                5,
                10,
            )],
        };

        let result = format_peek(&peek, false);
        assert!(result.contains("pub(crate) function helper"));
    }

    #[test]
    fn format_peek_export_visibility() {
        let peek = PeekView {
            path: PathBuf::from("src/index.ts"),
            short_description: None,
            long_description: None,
            symbols: vec![make_symbol(
                "greet",
                SymbolKind::Function,
                Some("function greet(name: string): string"),
                Visibility::Export,
                1,
                5,
            )],
        };

        let result = format_peek(&peek, false);
        assert!(result.contains("function greet(name: string): string"));
    }

    // ---- format_directory_peek tests ----

    fn make_directory_peek(
        path: &str,
        short: Option<&str>,
        long: Option<&str>,
        children: Vec<TreeEntry>,
        symbols: Vec<Symbol>,
    ) -> DirectoryPeekView {
        DirectoryPeekView {
            path: PathBuf::from(path),
            short_description: short.map(String::from),
            long_description: long.map(String::from),
            children,
            symbols,
        }
    }

    #[test]
    fn format_directory_peek_with_children_and_descriptions() {
        let peek = make_directory_peek(
            "src/atlas",
            Some("Atlas codebase navigation module"),
            Some("Detailed long description here."),
            vec![
                make_tree_entry("mod.rs", "src/atlas/mod.rs", false, Some("re-exports")),
                make_tree_entry(
                    "types.rs",
                    "src/atlas/types.rs",
                    false,
                    Some("domain types"),
                ),
                make_tree_entry(
                    "symbols",
                    "src/atlas/symbols",
                    true,
                    Some("symbol extraction"),
                ),
            ],
            vec![make_symbol(
                "extract_symbols",
                SymbolKind::Function,
                Some("fn extract_symbols(...)"),
                Visibility::Public,
                1,
                45,
            )],
        );

        let result = format_directory_peek(&peek, false);
        assert!(result.contains("src/atlas/"));
        assert!(result.contains("Atlas codebase navigation module"));
        assert!(result.contains("Detailed long description here."));
        assert!(result.contains("Contents:"));
        assert!(result.contains("  mod.rs \u{2014} re-exports"));
        assert!(result.contains("  types.rs \u{2014} domain types"));
        assert!(result.contains("  symbols/ \u{2014} symbol extraction"));
        assert!(result.contains("Symbols:"));
        assert!(result.contains("fn extract_symbols(...)"));
        assert!(result.contains("[1-45]"));
    }

    #[test]
    fn format_directory_peek_json_produces_valid_json() {
        let peek = make_directory_peek(
            "src/atlas",
            Some("Atlas module"),
            None,
            vec![make_tree_entry(
                "mod.rs",
                "src/atlas/mod.rs",
                false,
                Some("re-exports"),
            )],
            vec![make_symbol(
                "foo",
                SymbolKind::Function,
                Some("fn foo()"),
                Visibility::Public,
                1,
                10,
            )],
        );

        let result = format_directory_peek(&peek, true);
        let parsed: serde_json::Value = serde_json::from_str(&result).expect("valid JSON");
        assert!(parsed.is_object());
        assert_eq!(parsed["path"], "src/atlas");
        assert_eq!(parsed["short_description"], "Atlas module");
        assert!(parsed["children"].is_array());
        assert_eq!(parsed["children"].as_array().unwrap().len(), 1);
        assert!(parsed["symbols"].is_array());
        assert_eq!(parsed["symbols"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn format_directory_peek_empty() {
        let peek = make_directory_peek("", None, None, vec![], vec![]);
        let result = format_directory_peek(&peek, false);
        assert_eq!(result, "(empty)");
    }

    #[test]
    fn format_directory_peek_no_symbols() {
        let peek = make_directory_peek(
            "docs",
            Some("Documentation"),
            None,
            vec![make_tree_entry(
                "README.md",
                "docs/README.md",
                false,
                Some("readme"),
            )],
            vec![],
        );

        let result = format_directory_peek(&peek, false);
        assert!(result.contains("docs/"));
        assert!(result.contains("Documentation"));
        assert!(result.contains("Contents:"));
        assert!(result.contains("  README.md \u{2014} readme"));
        assert!(!result.contains("Symbols:"));
    }

    // ---- sort_tree_entries tests ----

    #[test]
    fn sort_tree_entries_interleaves_dirs_and_files() {
        let mut entries = vec![
            make_tree_entry("mod.rs", "src/atlas/mod.rs", false, Some("re-exports")),
            make_tree_entry("atlas", "src/atlas", true, None),
            make_tree_entry("Cargo.toml", "Cargo.toml", false, Some("manifest")),
            make_tree_entry("src", "src", true, None),
            make_tree_entry("types.rs", "src/atlas/types.rs", false, Some("types")),
        ];

        sort_tree_entries(&mut entries);

        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["Cargo.toml", "src", "atlas", "mod.rs", "types.rs"]
        );
    }

    #[test]
    fn sort_tree_entries_dir_before_same_name_file() {
        let mut entries = vec![
            make_tree_entry("utils", "utils", false, None),
            make_tree_entry("utils", "utils", true, None),
        ];

        sort_tree_entries(&mut entries);

        assert!(entries[0].is_dir);
        assert!(!entries[1].is_dir);
    }

    #[test]
    fn sort_tree_entries_empty() {
        let mut entries: Vec<TreeEntry> = vec![];
        sort_tree_entries(&mut entries);
        assert!(entries.is_empty());
    }

    // ---- extract_parent_paths tests ----

    #[test]
    fn extract_parent_paths_from_nested_files() {
        let files = vec!["src/atlas/mod.rs", "src/atlas/types.rs", "src/lib.rs"];
        let parents = extract_parent_paths(&files);
        assert_eq!(parents, vec!["src", "src/atlas"]);
    }

    #[test]
    fn extract_parent_paths_deduplicates() {
        let files = vec!["a/b/c.rs", "a/b/d.rs", "a/e.rs"];
        let parents = extract_parent_paths(&files);
        assert_eq!(parents, vec!["a", "a/b"]);
    }

    #[test]
    fn extract_parent_paths_root_level_files() {
        let files = vec!["Cargo.toml", "README.md"];
        let parents = extract_parent_paths(&files);
        assert!(parents.is_empty());
    }

    #[test]
    fn extract_parent_paths_empty_input() {
        let files: Vec<&str> = vec![];
        let parents = extract_parent_paths(&files);
        assert!(parents.is_empty());
    }
}
