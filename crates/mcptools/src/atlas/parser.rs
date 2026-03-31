use std::path::Path;

use mcptools_core::atlas::{extract_symbols, types::Language, types::Symbol};

/// Parse a file with tree-sitter and extract symbols.
///
/// Returns `None` for files without a supported grammar.
pub fn parse_and_extract(path: &Path, source: &[u8]) -> Option<Vec<Symbol>> {
    let ext = path.extension()?.to_str()?;
    let lang = Language::from_extension(ext)?;
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&grammar_for(lang)).ok()?;
    let tree = parser.parse(source, None)?;
    Some(extract_symbols(&tree, source, lang, path))
}

/// Get the tree-sitter grammar for a language.
fn grammar_for(lang: Language) -> tree_sitter::Language {
    match lang {
        Language::Rust => tree_sitter_rust::LANGUAGE.into(),
        Language::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        Language::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
        Language::JavaScript | Language::Jsx => tree_sitter_javascript::LANGUAGE.into(),
        Language::Python => tree_sitter_python::LANGUAGE.into(),
        Language::Go => tree_sitter_go::LANGUAGE.into(),
    }
}
