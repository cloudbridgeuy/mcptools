use std::path::Path;

use crate::atlas::types::{Language, Symbol, SymbolKind, Visibility};

/// Extract symbols from a parsed tree-sitter AST.
///
/// Pure function: AST and source bytes in, symbols out. No I/O.
pub fn extract_symbols(
    tree: &tree_sitter::Tree,
    source: &[u8],
    language: Language,
    file_path: &Path,
) -> Vec<Symbol> {
    let mut symbols = Vec::new();
    let root = tree.root_node();

    match language {
        Language::Rust => extract_rust_symbols(root, source, file_path, &mut symbols),
        Language::TypeScript | Language::Tsx => {
            extract_ts_js_symbols(root, source, file_path, &mut symbols, true);
        }
        Language::JavaScript | Language::Jsx => {
            extract_ts_js_symbols(root, source, file_path, &mut symbols, false);
        }
        Language::Python => extract_python_symbols(root, source, file_path, &mut symbols),
        Language::Go => extract_go_symbols(root, source, file_path, &mut symbols),
    }

    symbols
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Get the text of a node as a UTF-8 string slice.
fn node_text<'a>(node: tree_sitter::Node, source: &'a [u8]) -> &'a str {
    std::str::from_utf8(&source[node.byte_range()]).unwrap_or("")
}

/// Extract the signature: text from node start up to (but not including) the body.
/// If there is no body child, the full node text is the signature.
fn extract_signature(
    node: tree_sitter::Node,
    source: &[u8],
    body_kinds: &[&str],
) -> Option<String> {
    let start = node.start_byte();
    let mut cursor = node.walk();

    let body_start = node
        .children(&mut cursor)
        .find(|c| body_kinds.contains(&c.kind()))
        .map(|c| c.start_byte());

    let end = body_start.unwrap_or_else(|| node.end_byte());
    let raw = std::str::from_utf8(&source[start..end]).unwrap_or("");
    let sig = raw.trim_end().to_string();
    if sig.is_empty() {
        None
    } else {
        Some(sig)
    }
}

/// Find a named child by field name or node kind.
fn find_child_by_kind<'a>(
    node: tree_sitter::Node<'a>,
    kind: &str,
) -> Option<tree_sitter::Node<'a>> {
    let mut cursor = node.walk();
    let child = node.children(&mut cursor).find(|c| c.kind() == kind);
    child
}

/// Build a `Symbol` from common fields, computing line numbers from the node.
fn make_symbol(
    file_path: &Path,
    node: tree_sitter::Node,
    name: String,
    kind: SymbolKind,
    signature: Option<String>,
    visibility: Visibility,
) -> Symbol {
    Symbol {
        file_path: file_path.to_path_buf(),
        name,
        kind,
        signature,
        visibility,
        start_line: node.start_position().row as u32 + 1,
        end_line: node.end_position().row as u32 + 1,
    }
}

// ---------------------------------------------------------------------------
// Rust
// ---------------------------------------------------------------------------

fn rust_visibility(node: tree_sitter::Node, source: &[u8]) -> Visibility {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "visibility_modifier" {
            let text = node_text(child, source);
            if text.contains("pub(crate)") {
                return Visibility::PublicCrate;
            }
            return Visibility::Public;
        }
    }
    Visibility::Private
}

fn rust_name(node: tree_sitter::Node, source: &[u8]) -> Option<String> {
    // Try the "name" field first, then look for specific identifier-like children.
    if let Some(name_node) = node.child_by_field_name("name") {
        return Some(node_text(name_node, source).to_string());
    }
    // For const_item / static_item, the name field works.
    // For type_item, try "name" field.
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "identifier" | "type_identifier" => {
                return Some(node_text(child, source).to_string());
            }
            _ => {}
        }
    }
    None
}

fn extract_rust_symbols(
    node: tree_sitter::Node,
    source: &[u8],
    file_path: &Path,
    symbols: &mut Vec<Symbol>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "function_item" => {
                if let Some(name) = rust_name(child, source) {
                    symbols.push(make_symbol(
                        file_path,
                        child,
                        name,
                        SymbolKind::Function,
                        extract_signature(child, source, &["block"]),
                        rust_visibility(child, source),
                    ));
                }
            }
            "struct_item" => {
                if let Some(name) = rust_name(child, source) {
                    symbols.push(make_symbol(
                        file_path,
                        child,
                        name,
                        SymbolKind::Struct,
                        extract_signature(
                            child,
                            source,
                            &["field_declaration_list", "ordered_field_declaration_list"],
                        ),
                        rust_visibility(child, source),
                    ));
                }
            }
            "enum_item" => {
                if let Some(name) = rust_name(child, source) {
                    symbols.push(make_symbol(
                        file_path,
                        child,
                        name,
                        SymbolKind::Enum,
                        extract_signature(child, source, &["enum_variant_list"]),
                        rust_visibility(child, source),
                    ));
                }
            }
            "trait_item" => {
                if let Some(name) = rust_name(child, source) {
                    symbols.push(make_symbol(
                        file_path,
                        child,
                        name,
                        SymbolKind::Trait,
                        extract_signature(child, source, &["declaration_list"]),
                        rust_visibility(child, source),
                    ));
                }
            }
            "impl_item" => {
                // Extract methods from impl blocks
                extract_rust_impl_methods(child, source, file_path, symbols);
            }
            "const_item" | "static_item" => {
                if let Some(name) = rust_name(child, source) {
                    symbols.push(make_symbol(
                        file_path,
                        child,
                        name,
                        SymbolKind::Const,
                        Some(node_text(child, source).trim_end().to_string()),
                        rust_visibility(child, source),
                    ));
                }
            }
            "mod_item" => {
                if let Some(name) = rust_name(child, source) {
                    symbols.push(make_symbol(
                        file_path,
                        child,
                        name,
                        SymbolKind::Module,
                        extract_signature(child, source, &["declaration_list"]),
                        rust_visibility(child, source),
                    ));
                }
            }
            "type_item" => {
                if let Some(name) = rust_name(child, source) {
                    symbols.push(make_symbol(
                        file_path,
                        child,
                        name,
                        SymbolKind::Type,
                        Some(node_text(child, source).trim_end().to_string()),
                        rust_visibility(child, source),
                    ));
                }
            }
            _ => {}
        }
    }
}

fn extract_rust_impl_methods(
    impl_node: tree_sitter::Node,
    source: &[u8],
    file_path: &Path,
    symbols: &mut Vec<Symbol>,
) {
    // Find the declaration_list (body of impl block)
    let body = match find_child_by_kind(impl_node, "declaration_list") {
        Some(b) => b,
        None => return,
    };

    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        if child.kind() == "function_item" {
            if let Some(name) = rust_name(child, source) {
                symbols.push(make_symbol(
                    file_path,
                    child,
                    name,
                    SymbolKind::Method,
                    extract_signature(child, source, &["block"]),
                    rust_visibility(child, source),
                ));
            }
        }
    }
}

// ---------------------------------------------------------------------------
// TypeScript / JavaScript
// ---------------------------------------------------------------------------

/// Check if a node is inside an export_statement (its parent is export_statement).
fn is_exported(node: tree_sitter::Node) -> bool {
    node.parent()
        .map(|p| p.kind() == "export_statement")
        .unwrap_or(false)
}

fn ts_js_visibility(node: tree_sitter::Node) -> Visibility {
    if is_exported(node) {
        Visibility::Export
    } else {
        Visibility::Private
    }
}

fn extract_ts_js_symbols(
    node: tree_sitter::Node,
    source: &[u8],
    file_path: &Path,
    symbols: &mut Vec<Symbol>,
    is_typescript: bool,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "function_declaration" => {
                if let Some(name) = child.child_by_field_name("name") {
                    symbols.push(make_symbol(
                        file_path,
                        child,
                        node_text(name, source).to_string(),
                        SymbolKind::Function,
                        extract_signature(child, source, &["statement_block"]),
                        ts_js_visibility(child),
                    ));
                }
            }
            "class_declaration" => {
                if let Some(name) = child.child_by_field_name("name") {
                    let name_str = node_text(name, source).to_string();
                    symbols.push(make_symbol(
                        file_path,
                        child,
                        name_str,
                        SymbolKind::Class,
                        extract_signature(child, source, &["class_body"]),
                        ts_js_visibility(child),
                    ));
                    // Extract methods from class body
                    extract_ts_js_class_methods(child, source, file_path, symbols);
                }
            }
            "interface_declaration" if is_typescript => {
                if let Some(name) = child.child_by_field_name("name") {
                    symbols.push(make_symbol(
                        file_path,
                        child,
                        node_text(name, source).to_string(),
                        SymbolKind::Interface,
                        extract_signature(child, source, &["object_type", "interface_body"]),
                        ts_js_visibility(child),
                    ));
                }
            }
            "type_alias_declaration" if is_typescript => {
                if let Some(name) = child.child_by_field_name("name") {
                    symbols.push(make_symbol(
                        file_path,
                        child,
                        node_text(name, source).to_string(),
                        SymbolKind::Type,
                        Some(node_text(child, source).trim_end().to_string()),
                        ts_js_visibility(child),
                    ));
                }
            }
            "lexical_declaration" => {
                // Top-level const: `const FOO = ...` or `const foo = (...) => ...`
                extract_ts_js_lexical_declaration_with_visibility(
                    child,
                    source,
                    file_path,
                    symbols,
                    ts_js_visibility(child),
                );
            }
            "export_statement" => {
                // Recurse into export_statement children; ts_js_visibility()
                // detects the export_statement parent and returns Export.
                extract_ts_js_symbols(child, source, file_path, symbols, is_typescript);
            }
            _ => {}
        }
    }
}

fn extract_ts_js_class_methods(
    class_node: tree_sitter::Node,
    source: &[u8],
    file_path: &Path,
    symbols: &mut Vec<Symbol>,
) {
    let body = match find_child_by_kind(class_node, "class_body") {
        Some(b) => b,
        None => return,
    };

    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        if child.kind() == "method_definition" {
            if let Some(name) = child.child_by_field_name("name") {
                symbols.push(make_symbol(
                    file_path,
                    child,
                    node_text(name, source).to_string(),
                    SymbolKind::Method,
                    extract_signature(child, source, &["statement_block"]),
                    Visibility::Public,
                ));
            }
        }
    }
}

fn extract_ts_js_lexical_declaration_with_visibility(
    node: tree_sitter::Node,
    source: &[u8],
    file_path: &Path,
    symbols: &mut Vec<Symbol>,
    visibility: Visibility,
) {
    // Check that this is a `const` declaration (not `let`)
    let mut cursor = node.walk();
    let is_const = node.children(&mut cursor).any(|c| c.kind() == "const");

    if !is_const {
        return;
    }

    // Find variable declarators
    let mut cursor2 = node.walk();
    for child in node.children(&mut cursor2) {
        if child.kind() == "variable_declarator" {
            if let Some(name_node) = child.child_by_field_name("name") {
                let name = node_text(name_node, source).to_string();
                // Check if the value is an arrow function
                let is_arrow = child
                    .child_by_field_name("value")
                    .map(|v| v.kind() == "arrow_function")
                    .unwrap_or(false);

                let kind = if is_arrow {
                    SymbolKind::Function
                } else {
                    SymbolKind::Const
                };

                let signature = if is_arrow {
                    // For arrow functions: extract up to the body
                    let arrow = child.child_by_field_name("value").unwrap();
                    let sig_start = node.start_byte();
                    let body_start = find_child_by_kind(arrow, "statement_block")
                        .or_else(|| arrow.child_by_field_name("body"))
                        .map(|b| b.start_byte())
                        .unwrap_or_else(|| node.end_byte());
                    let raw = std::str::from_utf8(&source[sig_start..body_start]).unwrap_or("");
                    Some(raw.trim_end().to_string())
                } else {
                    Some(node_text(node, source).trim_end().to_string())
                };

                symbols.push(make_symbol(
                    file_path, node, name, kind, signature, visibility,
                ));
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Python
// ---------------------------------------------------------------------------

fn python_visibility_from_name(name: &str) -> Visibility {
    if name.starts_with('_') {
        Visibility::Private
    } else {
        Visibility::Public
    }
}

fn extract_python_symbols(
    node: tree_sitter::Node,
    source: &[u8],
    file_path: &Path,
    symbols: &mut Vec<Symbol>,
) {
    extract_python_symbols_inner(node, source, file_path, symbols, false);
}

fn extract_python_symbols_inner(
    node: tree_sitter::Node,
    source: &[u8],
    file_path: &Path,
    symbols: &mut Vec<Symbol>,
    inside_class: bool,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "function_definition" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    let name = node_text(name_node, source).to_string();
                    let kind = if inside_class {
                        SymbolKind::Method
                    } else {
                        SymbolKind::Function
                    };
                    symbols.push(make_symbol(
                        file_path,
                        child,
                        name.clone(),
                        kind,
                        extract_signature(child, source, &["block"]),
                        python_visibility_from_name(&name),
                    ));
                }
            }
            "decorated_definition" => {
                // Decorated definitions wrap function_definition or class_definition
                extract_python_symbols_inner(child, source, file_path, symbols, inside_class);
            }
            "class_definition" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    let name = node_text(name_node, source).to_string();
                    symbols.push(make_symbol(
                        file_path,
                        child,
                        name.clone(),
                        SymbolKind::Class,
                        extract_signature(child, source, &["block"]),
                        python_visibility_from_name(&name),
                    ));
                    // Recurse into class body for methods
                    if let Some(body) = child.child_by_field_name("body") {
                        extract_python_symbols_inner(body, source, file_path, symbols, true);
                    }
                }
            }
            "expression_statement" if !inside_class => {
                // Check for UPPER_CASE assignment: `MAX_SIZE = 100`
                extract_python_const_assignment(child, source, file_path, symbols);
            }
            _ => {}
        }
    }
}

fn is_upper_snake_case(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

fn extract_python_const_assignment(
    expr_stmt: tree_sitter::Node,
    source: &[u8],
    file_path: &Path,
    symbols: &mut Vec<Symbol>,
) {
    let mut cursor = expr_stmt.walk();
    for child in expr_stmt.children(&mut cursor) {
        if child.kind() == "assignment" {
            if let Some(left) = child.child_by_field_name("left") {
                if left.kind() == "identifier" {
                    let name = node_text(left, source).to_string();
                    if is_upper_snake_case(&name) {
                        symbols.push(make_symbol(
                            file_path,
                            child,
                            name.clone(),
                            SymbolKind::Const,
                            Some(node_text(child, source).trim_end().to_string()),
                            python_visibility_from_name(&name),
                        ));
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Go
// ---------------------------------------------------------------------------

fn go_visibility_from_name(name: &str) -> Visibility {
    if name.starts_with(|c: char| c.is_ascii_uppercase()) {
        Visibility::Public
    } else {
        Visibility::Private
    }
}

fn extract_go_symbols(
    node: tree_sitter::Node,
    source: &[u8],
    file_path: &Path,
    symbols: &mut Vec<Symbol>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "function_declaration" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    let name = node_text(name_node, source).to_string();
                    symbols.push(make_symbol(
                        file_path,
                        child,
                        name.clone(),
                        SymbolKind::Function,
                        extract_signature(child, source, &["block"]),
                        go_visibility_from_name(&name),
                    ));
                }
            }
            "method_declaration" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    let name = node_text(name_node, source).to_string();
                    symbols.push(make_symbol(
                        file_path,
                        child,
                        name.clone(),
                        SymbolKind::Method,
                        extract_signature(child, source, &["block"]),
                        go_visibility_from_name(&name),
                    ));
                }
            }
            "type_declaration" => {
                extract_go_type_declaration(child, source, file_path, symbols);
            }
            "const_declaration" => {
                extract_go_const_or_var(child, source, file_path, symbols);
            }
            "var_declaration" => {
                // Only top-level (package-level) var declarations
                extract_go_const_or_var(child, source, file_path, symbols);
            }
            _ => {}
        }
    }
}

fn extract_go_type_declaration(
    type_decl: tree_sitter::Node,
    source: &[u8],
    file_path: &Path,
    symbols: &mut Vec<Symbol>,
) {
    let mut cursor = type_decl.walk();
    for child in type_decl.children(&mut cursor) {
        if child.kind() == "type_spec" {
            if let Some(name_node) = child.child_by_field_name("name") {
                let name = node_text(name_node, source).to_string();
                let type_node = child.child_by_field_name("type");
                let kind = match type_node.map(|n| n.kind()) {
                    Some("struct_type") => SymbolKind::Struct,
                    Some("interface_type") => SymbolKind::Interface,
                    _ => SymbolKind::Type,
                };

                let body_kind = match kind {
                    SymbolKind::Struct => "field_declaration_list",
                    SymbolKind::Interface => "interface_type",
                    _ => "",
                };

                let signature = if body_kind.is_empty() {
                    Some(node_text(type_decl, source).trim_end().to_string())
                } else if let Some(type_n) = type_node {
                    // For struct/interface, signature is up to the body
                    let start = type_decl.start_byte();
                    let body_start = find_child_by_kind(type_n, body_kind)
                        .map(|b| b.start_byte())
                        .unwrap_or_else(|| type_decl.end_byte());
                    let raw = std::str::from_utf8(&source[start..body_start]).unwrap_or("");
                    Some(raw.trim_end().to_string())
                } else {
                    Some(node_text(type_decl, source).trim_end().to_string())
                };

                symbols.push(make_symbol(
                    file_path,
                    type_decl,
                    name.clone(),
                    kind,
                    signature,
                    go_visibility_from_name(&name),
                ));
            }
        }
    }
}

fn extract_go_const_or_var(
    decl: tree_sitter::Node,
    source: &[u8],
    file_path: &Path,
    symbols: &mut Vec<Symbol>,
) {
    let mut cursor = decl.walk();
    for child in decl.children(&mut cursor) {
        if child.kind() == "const_spec" || child.kind() == "var_spec" {
            if let Some(name_node) = child.child_by_field_name("name") {
                let name = node_text(name_node, source).to_string();
                symbols.push(make_symbol(
                    file_path,
                    child,
                    name.clone(),
                    SymbolKind::Const,
                    Some(node_text(child, source).trim_end().to_string()),
                    go_visibility_from_name(&name),
                ));
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn parse(source: &str, lang: tree_sitter::Language) -> tree_sitter::Tree {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&lang).unwrap();
        parser.parse(source, None).unwrap()
    }

    fn test_path() -> PathBuf {
        PathBuf::from("test.rs")
    }

    // -----------------------------------------------------------------------
    // Rust tests
    // -----------------------------------------------------------------------

    #[test]
    fn rust_function() {
        let src = "fn hello() {}";
        let tree = parse(src, tree_sitter_rust::LANGUAGE.into());
        let syms = extract_symbols(&tree, src.as_bytes(), Language::Rust, &test_path());
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "hello");
        assert_eq!(syms[0].kind, SymbolKind::Function);
        assert_eq!(syms[0].visibility, Visibility::Private);
    }

    #[test]
    fn rust_pub_fn() {
        let src = "pub fn greet(name: &str) -> String { name.to_string() }";
        let tree = parse(src, tree_sitter_rust::LANGUAGE.into());
        let syms = extract_symbols(&tree, src.as_bytes(), Language::Rust, &test_path());
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "greet");
        assert_eq!(syms[0].visibility, Visibility::Public);
        let sig = syms[0].signature.as_ref().unwrap();
        assert!(sig.contains("pub fn greet(name: &str) -> String"));
        assert!(!sig.contains("name.to_string()"));
    }

    #[test]
    fn rust_pub_crate() {
        let src = "pub(crate) fn internal() {}";
        let tree = parse(src, tree_sitter_rust::LANGUAGE.into());
        let syms = extract_symbols(&tree, src.as_bytes(), Language::Rust, &test_path());
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].visibility, Visibility::PublicCrate);
    }

    #[test]
    fn rust_struct() {
        let src = "pub struct Foo { pub x: i32 }";
        let tree = parse(src, tree_sitter_rust::LANGUAGE.into());
        let syms = extract_symbols(&tree, src.as_bytes(), Language::Rust, &test_path());
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "Foo");
        assert_eq!(syms[0].kind, SymbolKind::Struct);
        assert_eq!(syms[0].visibility, Visibility::Public);
        let sig = syms[0].signature.as_ref().unwrap();
        assert!(sig.contains("pub struct Foo"));
    }

    #[test]
    fn rust_enum() {
        let src = "pub enum Color { Red, Green, Blue }";
        let tree = parse(src, tree_sitter_rust::LANGUAGE.into());
        let syms = extract_symbols(&tree, src.as_bytes(), Language::Rust, &test_path());
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "Color");
        assert_eq!(syms[0].kind, SymbolKind::Enum);
    }

    #[test]
    fn rust_trait() {
        let src = "pub trait Display { fn fmt(&self) -> String; }";
        let tree = parse(src, tree_sitter_rust::LANGUAGE.into());
        let syms = extract_symbols(&tree, src.as_bytes(), Language::Rust, &test_path());
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "Display");
        assert_eq!(syms[0].kind, SymbolKind::Trait);
    }

    #[test]
    fn rust_impl_method() {
        let src = r#"
struct Foo;
impl Foo {
    pub fn method(&self) -> i32 { 42 }
    fn private_method(&self) {}
}
"#;
        let tree = parse(src, tree_sitter_rust::LANGUAGE.into());
        let syms = extract_symbols(&tree, src.as_bytes(), Language::Rust, &test_path());
        // struct Foo + 2 methods
        assert_eq!(syms.len(), 3);
        assert_eq!(syms[0].name, "Foo");
        assert_eq!(syms[0].kind, SymbolKind::Struct);
        assert_eq!(syms[1].name, "method");
        assert_eq!(syms[1].kind, SymbolKind::Method);
        assert_eq!(syms[1].visibility, Visibility::Public);
        assert_eq!(syms[2].name, "private_method");
        assert_eq!(syms[2].kind, SymbolKind::Method);
        assert_eq!(syms[2].visibility, Visibility::Private);
    }

    #[test]
    fn rust_const_and_static() {
        let src = r#"
const MAX: i32 = 100;
static GLOBAL: &str = "hi";
"#;
        let tree = parse(src, tree_sitter_rust::LANGUAGE.into());
        let syms = extract_symbols(&tree, src.as_bytes(), Language::Rust, &test_path());
        assert_eq!(syms.len(), 2);
        assert_eq!(syms[0].name, "MAX");
        assert_eq!(syms[0].kind, SymbolKind::Const);
        assert_eq!(syms[1].name, "GLOBAL");
        assert_eq!(syms[1].kind, SymbolKind::Const);
    }

    #[test]
    fn rust_mod() {
        let src = "mod inner {}";
        let tree = parse(src, tree_sitter_rust::LANGUAGE.into());
        let syms = extract_symbols(&tree, src.as_bytes(), Language::Rust, &test_path());
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "inner");
        assert_eq!(syms[0].kind, SymbolKind::Module);
    }

    #[test]
    fn rust_type_alias() {
        let src = "type Alias = Vec<i32>;";
        let tree = parse(src, tree_sitter_rust::LANGUAGE.into());
        let syms = extract_symbols(&tree, src.as_bytes(), Language::Rust, &test_path());
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "Alias");
        assert_eq!(syms[0].kind, SymbolKind::Type);
    }

    // -----------------------------------------------------------------------
    // TypeScript tests
    // -----------------------------------------------------------------------

    #[test]
    fn ts_function() {
        let src = "function hello(x: number): string { return String(x); }";
        let tree = parse(src, tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into());
        let syms = extract_symbols(
            &tree,
            src.as_bytes(),
            Language::TypeScript,
            &PathBuf::from("test.ts"),
        );
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "hello");
        assert_eq!(syms[0].kind, SymbolKind::Function);
        assert_eq!(syms[0].visibility, Visibility::Private);
    }

    #[test]
    fn ts_arrow_function() {
        let src = "const greet = (name: string): string => { return name; };";
        let tree = parse(src, tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into());
        let syms = extract_symbols(
            &tree,
            src.as_bytes(),
            Language::TypeScript,
            &PathBuf::from("test.ts"),
        );
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "greet");
        assert_eq!(syms[0].kind, SymbolKind::Function);
    }

    #[test]
    fn ts_class_and_method() {
        let src = r#"
class Foo {
    bar() { return 1; }
    baz(x: number) { return x; }
}
"#;
        let tree = parse(src, tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into());
        let syms = extract_symbols(
            &tree,
            src.as_bytes(),
            Language::TypeScript,
            &PathBuf::from("test.ts"),
        );
        assert_eq!(syms.len(), 3); // class + 2 methods
        assert_eq!(syms[0].name, "Foo");
        assert_eq!(syms[0].kind, SymbolKind::Class);
        assert_eq!(syms[1].name, "bar");
        assert_eq!(syms[1].kind, SymbolKind::Method);
        assert_eq!(syms[2].name, "baz");
        assert_eq!(syms[2].kind, SymbolKind::Method);
    }

    #[test]
    fn ts_interface() {
        let src = "interface Foo { x: number; }";
        let tree = parse(src, tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into());
        let syms = extract_symbols(
            &tree,
            src.as_bytes(),
            Language::TypeScript,
            &PathBuf::from("test.ts"),
        );
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "Foo");
        assert_eq!(syms[0].kind, SymbolKind::Interface);
    }

    #[test]
    fn ts_type_alias() {
        let src = "type ID = string | number;";
        let tree = parse(src, tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into());
        let syms = extract_symbols(
            &tree,
            src.as_bytes(),
            Language::TypeScript,
            &PathBuf::from("test.ts"),
        );
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "ID");
        assert_eq!(syms[0].kind, SymbolKind::Type);
    }

    #[test]
    fn ts_export() {
        let src = "export function hello() {}";
        let tree = parse(src, tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into());
        let syms = extract_symbols(
            &tree,
            src.as_bytes(),
            Language::TypeScript,
            &PathBuf::from("test.ts"),
        );
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "hello");
        assert_eq!(syms[0].visibility, Visibility::Export);
    }

    #[test]
    fn ts_const() {
        let src = "const MAX = 100;";
        let tree = parse(src, tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into());
        let syms = extract_symbols(
            &tree,
            src.as_bytes(),
            Language::TypeScript,
            &PathBuf::from("test.ts"),
        );
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "MAX");
        assert_eq!(syms[0].kind, SymbolKind::Const);
    }

    // -----------------------------------------------------------------------
    // JavaScript tests
    // -----------------------------------------------------------------------

    #[test]
    fn js_function() {
        let src = "function hello() { return 1; }";
        let tree = parse(src, tree_sitter_javascript::LANGUAGE.into());
        let syms = extract_symbols(
            &tree,
            src.as_bytes(),
            Language::JavaScript,
            &PathBuf::from("test.js"),
        );
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "hello");
        assert_eq!(syms[0].kind, SymbolKind::Function);
    }

    #[test]
    fn js_arrow_function() {
        let src = "const greet = (name) => { return name; };";
        let tree = parse(src, tree_sitter_javascript::LANGUAGE.into());
        let syms = extract_symbols(
            &tree,
            src.as_bytes(),
            Language::JavaScript,
            &PathBuf::from("test.js"),
        );
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "greet");
        assert_eq!(syms[0].kind, SymbolKind::Function);
    }

    #[test]
    fn js_class_and_method() {
        let src = r#"
class Animal {
    speak() { return "..."; }
}
"#;
        let tree = parse(src, tree_sitter_javascript::LANGUAGE.into());
        let syms = extract_symbols(
            &tree,
            src.as_bytes(),
            Language::JavaScript,
            &PathBuf::from("test.js"),
        );
        assert_eq!(syms.len(), 2);
        assert_eq!(syms[0].name, "Animal");
        assert_eq!(syms[0].kind, SymbolKind::Class);
        assert_eq!(syms[1].name, "speak");
        assert_eq!(syms[1].kind, SymbolKind::Method);
    }

    #[test]
    fn js_export() {
        let src = "export function hello() {}";
        let tree = parse(src, tree_sitter_javascript::LANGUAGE.into());
        let syms = extract_symbols(
            &tree,
            src.as_bytes(),
            Language::JavaScript,
            &PathBuf::from("test.js"),
        );
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].visibility, Visibility::Export);
    }

    #[test]
    fn js_const() {
        let src = "const API_URL = 'https://example.com';";
        let tree = parse(src, tree_sitter_javascript::LANGUAGE.into());
        let syms = extract_symbols(
            &tree,
            src.as_bytes(),
            Language::JavaScript,
            &PathBuf::from("test.js"),
        );
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "API_URL");
        assert_eq!(syms[0].kind, SymbolKind::Const);
    }

    // -----------------------------------------------------------------------
    // Python tests
    // -----------------------------------------------------------------------

    #[test]
    fn python_def() {
        let src = "def hello():\n    pass";
        let tree = parse(src, tree_sitter_python::LANGUAGE.into());
        let syms = extract_symbols(
            &tree,
            src.as_bytes(),
            Language::Python,
            &PathBuf::from("test.py"),
        );
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "hello");
        assert_eq!(syms[0].kind, SymbolKind::Function);
        assert_eq!(syms[0].visibility, Visibility::Public);
    }

    #[test]
    fn python_class_and_method() {
        let src = r#"
class Foo:
    def bar(self):
        pass
    def _private(self):
        pass
"#;
        let tree = parse(src, tree_sitter_python::LANGUAGE.into());
        let syms = extract_symbols(
            &tree,
            src.as_bytes(),
            Language::Python,
            &PathBuf::from("test.py"),
        );
        assert_eq!(syms.len(), 3);
        assert_eq!(syms[0].name, "Foo");
        assert_eq!(syms[0].kind, SymbolKind::Class);
        assert_eq!(syms[1].name, "bar");
        assert_eq!(syms[1].kind, SymbolKind::Method);
        assert_eq!(syms[2].name, "_private");
        assert_eq!(syms[2].kind, SymbolKind::Method);
        assert_eq!(syms[2].visibility, Visibility::Private);
    }

    #[test]
    fn python_upper_case_const() {
        let src = "MAX_SIZE = 100\nother = 42";
        let tree = parse(src, tree_sitter_python::LANGUAGE.into());
        let syms = extract_symbols(
            &tree,
            src.as_bytes(),
            Language::Python,
            &PathBuf::from("test.py"),
        );
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "MAX_SIZE");
        assert_eq!(syms[0].kind, SymbolKind::Const);
    }

    #[test]
    fn python_private_function() {
        let src = "def _internal():\n    pass";
        let tree = parse(src, tree_sitter_python::LANGUAGE.into());
        let syms = extract_symbols(
            &tree,
            src.as_bytes(),
            Language::Python,
            &PathBuf::from("test.py"),
        );
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].visibility, Visibility::Private);
    }

    // -----------------------------------------------------------------------
    // Go tests
    // -----------------------------------------------------------------------

    #[test]
    fn go_func() {
        let src = "package main\nfunc Hello() {}";
        let tree = parse(src, tree_sitter_go::LANGUAGE.into());
        let syms = extract_symbols(
            &tree,
            src.as_bytes(),
            Language::Go,
            &PathBuf::from("test.go"),
        );
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "Hello");
        assert_eq!(syms[0].kind, SymbolKind::Function);
        assert_eq!(syms[0].visibility, Visibility::Public);
    }

    #[test]
    fn go_unexported_func() {
        let src = "package main\nfunc hello() {}";
        let tree = parse(src, tree_sitter_go::LANGUAGE.into());
        let syms = extract_symbols(
            &tree,
            src.as_bytes(),
            Language::Go,
            &PathBuf::from("test.go"),
        );
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].visibility, Visibility::Private);
    }

    #[test]
    fn go_method() {
        let src =
            "package main\nfunc (s *Server) Handle(w http.ResponseWriter, r *http.Request) {}";
        let tree = parse(src, tree_sitter_go::LANGUAGE.into());
        let syms = extract_symbols(
            &tree,
            src.as_bytes(),
            Language::Go,
            &PathBuf::from("test.go"),
        );
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "Handle");
        assert_eq!(syms[0].kind, SymbolKind::Method);
    }

    #[test]
    fn go_struct() {
        let src = "package main\ntype Server struct { port int }";
        let tree = parse(src, tree_sitter_go::LANGUAGE.into());
        let syms = extract_symbols(
            &tree,
            src.as_bytes(),
            Language::Go,
            &PathBuf::from("test.go"),
        );
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "Server");
        assert_eq!(syms[0].kind, SymbolKind::Struct);
    }

    #[test]
    fn go_interface() {
        let src = "package main\ntype Handler interface { Handle() }";
        let tree = parse(src, tree_sitter_go::LANGUAGE.into());
        let syms = extract_symbols(
            &tree,
            src.as_bytes(),
            Language::Go,
            &PathBuf::from("test.go"),
        );
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "Handler");
        assert_eq!(syms[0].kind, SymbolKind::Interface);
    }

    #[test]
    fn go_type_alias() {
        let src = "package main\ntype ID int";
        let tree = parse(src, tree_sitter_go::LANGUAGE.into());
        let syms = extract_symbols(
            &tree,
            src.as_bytes(),
            Language::Go,
            &PathBuf::from("test.go"),
        );
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "ID");
        assert_eq!(syms[0].kind, SymbolKind::Type);
    }

    #[test]
    fn go_const() {
        let src = "package main\nconst MaxRetries = 3";
        let tree = parse(src, tree_sitter_go::LANGUAGE.into());
        let syms = extract_symbols(
            &tree,
            src.as_bytes(),
            Language::Go,
            &PathBuf::from("test.go"),
        );
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "MaxRetries");
        assert_eq!(syms[0].kind, SymbolKind::Const);
    }

    #[test]
    fn go_var() {
        let src = "package main\nvar defaultTimeout = 30";
        let tree = parse(src, tree_sitter_go::LANGUAGE.into());
        let syms = extract_symbols(
            &tree,
            src.as_bytes(),
            Language::Go,
            &PathBuf::from("test.go"),
        );
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "defaultTimeout");
        assert_eq!(syms[0].kind, SymbolKind::Const);
        assert_eq!(syms[0].visibility, Visibility::Private);
    }

    // -----------------------------------------------------------------------
    // Cross-cutting tests
    // -----------------------------------------------------------------------

    #[test]
    fn signature_excludes_body() {
        let src = "pub fn add(a: i32, b: i32) -> i32 {\n    a + b\n}";
        let tree = parse(src, tree_sitter_rust::LANGUAGE.into());
        let syms = extract_symbols(&tree, src.as_bytes(), Language::Rust, &test_path());
        let sig = syms[0].signature.as_ref().unwrap();
        assert!(sig.contains("pub fn add(a: i32, b: i32) -> i32"));
        assert!(!sig.contains("a + b"));
    }

    #[test]
    fn empty_file_returns_empty() {
        let src = "";
        let tree = parse(src, tree_sitter_rust::LANGUAGE.into());
        let syms = extract_symbols(&tree, src.as_bytes(), Language::Rust, &test_path());
        assert!(syms.is_empty());
    }

    #[test]
    fn comments_only_returns_empty() {
        let src = "// just a comment\n// another comment\n";
        let tree = parse(src, tree_sitter_rust::LANGUAGE.into());
        let syms = extract_symbols(&tree, src.as_bytes(), Language::Rust, &test_path());
        assert!(syms.is_empty());
    }

    #[test]
    fn line_numbers_are_one_based() {
        let src = "\nfn hello() {}\n";
        let tree = parse(src, tree_sitter_rust::LANGUAGE.into());
        let syms = extract_symbols(&tree, src.as_bytes(), Language::Rust, &test_path());
        assert_eq!(syms[0].start_line, 2);
        assert_eq!(syms[0].end_line, 2);
    }

    // -----------------------------------------------------------------------
    // Edge-case tests
    // -----------------------------------------------------------------------

    #[test]
    fn ts_js_let_declaration_skipped() {
        let src = "let x = 5;";
        let tree = parse(src, tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into());
        let syms = extract_symbols(
            &tree,
            src.as_bytes(),
            Language::TypeScript,
            &PathBuf::from("test.ts"),
        );
        assert!(
            syms.is_empty(),
            "let declarations should produce zero symbols"
        );
    }

    #[test]
    fn python_decorated_definition() {
        let src = "@decorator\ndef foo():\n    pass";
        let tree = parse(src, tree_sitter_python::LANGUAGE.into());
        let syms = extract_symbols(
            &tree,
            src.as_bytes(),
            Language::Python,
            &PathBuf::from("test.py"),
        );
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "foo");
        assert_eq!(syms[0].kind, SymbolKind::Function);
    }

    #[test]
    fn ts_exported_arrow_function() {
        let src = "export const foo = () => {};";
        let tree = parse(src, tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into());
        let syms = extract_symbols(
            &tree,
            src.as_bytes(),
            Language::TypeScript,
            &PathBuf::from("test.ts"),
        );
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "foo");
        assert_eq!(syms[0].kind, SymbolKind::Function);
        assert_eq!(syms[0].visibility, Visibility::Export);
    }

    #[test]
    fn go_grouped_const_declarations() {
        let src = "package main\nconst (\n  A = 1\n  B = 2\n)";
        let tree = parse(src, tree_sitter_go::LANGUAGE.into());
        let syms = extract_symbols(
            &tree,
            src.as_bytes(),
            Language::Go,
            &PathBuf::from("test.go"),
        );
        assert_eq!(syms.len(), 2);
        assert_eq!(syms[0].name, "A");
        assert_eq!(syms[0].kind, SymbolKind::Const);
        assert_eq!(syms[1].name, "B");
        assert_eq!(syms[1].kind, SymbolKind::Const);
    }
}
