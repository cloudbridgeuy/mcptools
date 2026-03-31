# Atlas — Codebase Navigator for AI Agents

Atlas scans a repository, extracts symbols using tree-sitter, and stores them in a local SQLite index. Agents use the index to quickly understand codebase structure without reading every file.

## V1 Scope

- **Symbol index**: Extract functions, classes, structs, enums, traits, interfaces, types, constants, methods, and modules from source files
- **Tree view**: Annotated directory listing from the index
- **Peek**: File summary with symbol signatures and line ranges
- **Languages**: Rust, TypeScript/TSX, JavaScript/JSX, Python, Go

## CLI Commands

```bash
mcptools atlas index                  # Full repo scan, stores in .mcptools/atlas/index.db
mcptools atlas tree [path]            # Directory tree (optional: filter by path)
mcptools atlas tree --depth 2         # Limit depth
mcptools atlas tree --json            # JSON output
mcptools atlas peek <path>            # File symbols with signatures
mcptools atlas peek <path> --json     # JSON output
```

## Supported Symbol Types by Language

| Language | Symbols Extracted |
|----------|-------------------|
| Rust | `function_item`, `struct_item`, `enum_item`, `trait_item`, `type_item`, `const_item`, `static_item`, `mod_item`, `impl` methods |
| TypeScript/JavaScript | `function_declaration`, named arrow functions, `class_declaration`, `method_definition`, `interface_declaration` (TS), `type_alias_declaration` (TS), top-level `const`, exports |
| Python | `function_definition`, `class_definition`, class methods, `UPPER_CASE` assignments (const), decorated definitions |
| Go | `function_declaration`, methods (with receiver), `type_declaration` (struct/interface/other), `const_declaration`, package-level `var` |

**Visibility detection**: Rust (`pub`/`pub(crate)`/private), TS/JS (export/private), Python (underscore prefix), Go (capitalization).

## Index Tiers

| Tier | Extensions | Description |
|------|-----------|-------------|
| **Full** | `rs`, `ts`, `tsx`, `js`, `jsx`, `py`, `go` | Tree-sitter symbol extraction |
| **Light** | `md`, `txt`, `json`, `yaml`, `yml`, `toml`, `cfg`, `ini`, `xml`, `html`, `css`, `scss`, `sql`, `sh`, `bash`, `zsh` | Indexed as files only (V2: LLM descriptions) |
| **Skip** | Everything else | Not indexed |

## Module Layout

### Core (`crates/core/src/atlas/`) — Pure functions, no I/O

| File | Purpose |
|------|---------|
| `types.rs` | `SymbolKind`, `Visibility`, `ContentHash`, `Language`, `IndexTier`, `Symbol`, `FileEntry`, `TreeEntry`, `PeekView` |
| `hash.rs` | `content_hash(bytes) -> ContentHash` (SHA-256) |
| `symbols.rs` | `extract_symbols(tree, source, language, path) -> Vec<Symbol>` |
| `tree_view.rs` | `format_tree(entries, json)`, `format_peek(peek, json)` |

### Shell (`crates/mcptools/src/atlas/`) — I/O operations

| File | Purpose |
|------|---------|
| `db.rs` | SQLite storage: `Database::open`, insert/query symbols and files |
| `fs.rs` | `walk_repo(root)` — gitignore-aware file walker, skips `.git/`, `node_modules/`, `target/`, binary files |
| `parser.rs` | `parse_and_extract(path, source)` — tree-sitter grammar registry and extraction bridge |
| `cli/index.rs` | `atlas index` handler: full repo scan, progress reporting |
| `cli/tree.rs` | `atlas tree` handler: path filtering, depth, JSON output |
| `cli/peek.rs` | `atlas peek` handler: file lookup, symbol display |

## SQLite Schema

Four tables: `symbols`, `files`, `directories`, `metadata`. Indexes on `symbols(file_path)` and `files(path)`. Database at `.mcptools/atlas/index.db` relative to git root.

## Planned Features

- **V2**: LLM-generated file descriptions, incremental updates (content hash diffing)
- **V3**: MCP tool integration for agent access
- **V4**: Cross-file relationship tracking
- **V5**: Semantic search over symbols and descriptions
