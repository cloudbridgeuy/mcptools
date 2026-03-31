# Atlas — Codebase Navigator for AI Agents

Atlas scans a repository, extracts symbols using tree-sitter, and stores them in a local SQLite index. Agents use the index to quickly understand codebase structure without reading every file.

## V1 Scope

- **Symbol index**: Extract functions, classes, structs, enums, traits, interfaces, types, constants, methods, and modules from source files
- **Tree view**: Annotated directory listing from the index
- **Peek**: File summary with symbol signatures and line ranges
- **Languages**: Rust, TypeScript, JavaScript, Python, Go

## CLI Commands

```bash
mcptools atlas index                  # Full repo scan, stores in .mcptools/atlas/index.db
mcptools atlas tree [path]            # Directory tree (optional: filter by path)
mcptools atlas tree --depth 2         # Limit depth
mcptools atlas tree --json            # JSON output
mcptools atlas peek <path>            # File symbols with signatures
mcptools atlas peek <path> --json     # JSON output
```

## Module Layout

### Core (`crates/core/src/atlas/`)

Pure functions, no I/O:

| File | Purpose |
|------|---------|
| `types.rs` | `SymbolKind`, `Visibility`, `ContentHash`, `Language`, `Symbol`, `FileEntry`, `TreeEntry`, `PeekView` |
| `hash.rs` | `content_hash(bytes) -> ContentHash` (SHA-256) |
| `symbols.rs` | `extract_symbols(tree, source, language, path) -> Vec<Symbol>` |
| `tree_view.rs` | `format_tree(entries, json)`, `format_peek(peek, json)` |

### Shell (`crates/mcptools/src/atlas/`)

I/O operations:

| File | Purpose |
|------|---------|
| `db.rs` | SQLite storage (open, insert, query) |
| `fs.rs` | `walk_repo(root)` — gitignore-aware file walker |
| `parser.rs` | `parse_and_extract(path, source)` — tree-sitter bridge |
| `cli/index.rs` | `atlas index` command handler |
| `cli/tree.rs` | `atlas tree` command handler |
| `cli/peek.rs` | `atlas peek` command handler |

## Index Location

The SQLite database is stored at `.mcptools/atlas/index.db` relative to the git root. This directory should be added to `.gitignore`.

## Planned Features

- **V2**: LLM-generated file descriptions, incremental updates
- **V3**: MCP tool integration for agent access
- **V4**: Cross-file relationship tracking
- **V5**: Semantic search over symbols and descriptions
