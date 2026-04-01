# Atlas — Codebase Navigator for AI Agents

Atlas scans a repository, extracts symbols using tree-sitter, and stores them in a local SQLite index. Agents use the index to quickly understand codebase structure without reading every file. V2 adds LLM-generated file descriptions and a project primer workflow.

## V1 Scope

- **Symbol index**: Extract functions, classes, structs, enums, traits, interfaces, types, constants, methods, and modules from source files
- **Tree view**: Annotated directory listing from the index
- **Peek**: File summary with symbol signatures and line ranges
- **Languages**: Rust, TypeScript/TSX, JavaScript/JSX, Python, Go

## V2 Scope (Implemented)

- **Project primer**: `atlas init` creates a mental model document via editor + LLM refinement
- **LLM file descriptions**: `atlas index` generates one-line descriptions for indexed files using a local Ollama model
- **Config file**: `.mcptools/config.toml` for per-project settings (db path, primer path, models, token limits)
- **Environment variables**: Override config via `ATLAS_*` and `OLLAMA_URL` env vars

## CLI Commands

```bash
mcptools atlas init                   # Create primer (mental model) via $EDITOR + LLM
mcptools atlas index                  # Full repo scan, stores in .mcptools/atlas/index.db
mcptools atlas index --parallel 4     # Parallel LLM workers for faster indexing
mcptools atlas tree [path]            # Directory tree (optional: filter by path)
mcptools atlas tree --depth 2         # Limit depth
mcptools atlas tree --json            # JSON output
mcptools atlas peek <path>            # File symbols with signatures
mcptools atlas peek <path> --json     # JSON output
```

### `atlas init` Workflow

1. Opens `$VISUAL` / `$EDITOR` with a primer template (questions about the project)
2. Sends user answers to a local LLM for refinement
3. Opens editor again with the refined draft for final edits
4. Saves to `.mcptools/atlas/primer.md` (configurable)
5. Adds `index.db` to `.gitignore` if not already present

## Configuration

### Config File (`.mcptools/config.toml`)

```toml
[atlas]
db_path = ".mcptools/atlas/index.db"
primer_path = ".mcptools/atlas/primer.md"
max_file_tokens = 10000
ollama_url = "http://localhost:11434"
file_model = "atlas"
dir_model = "haiku"
```

All fields are optional and fall back to defaults. Environment variables override config file values.

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `ATLAS_DB_PATH` | `.mcptools/atlas/index.db` | Database location |
| `ATLAS_PRIMER_PATH` | `.mcptools/atlas/primer.md` | Primer file location |
| `ATLAS_MAX_FILE_TOKENS` | `10000` | Max tokens per file for LLM |
| `OLLAMA_URL` | `http://localhost:11434` | Ollama API base URL |
| `ATLAS_FILE_MODEL` | `atlas` | Model for file descriptions |
| `ATLAS_DIR_MODEL` | `haiku` | Model for directory descriptions |

### LLM Setup

See [Atlas Setup](../../docs/ATLAS_SETUP.md) for Ollama model download and Modelfile creation.

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
| `config.rs` | `AtlasConfig`, `PrimerPath`, `DbPath`, `ModelName`, `BaseUrl`, `LlmProviderKind` — config parsing from TOML + env var overlay, defaults |
| `hash.rs` | `content_hash(bytes) -> ContentHash` (SHA-256) |
| `symbols.rs` | `extract_symbols(tree, source, language, path) -> Vec<Symbol>` |
| `tree_view.rs` | `format_tree(entries, json)`, `format_peek(peek, json)` |
| `parse.rs` | `parse_description(response) -> FileDescription` — parse SHORT/LONG format from LLM output |
| `prompts.rs` | `file_system_prompt`, `build_file_prompt`, `build_primer_refinement_prompt` — LLM prompt assembly |

### Shell (`crates/mcptools/src/atlas/`) — I/O operations

| File | Purpose |
|------|---------|
| `config.rs` | `load_config(repo_root)` — reads `.mcptools/config.toml`, collects env vars, delegates to core parser |
| `db.rs` | SQLite storage: `Database::open`, insert/query symbols and files |
| `fs.rs` | `walk_repo(root)` — gitignore-aware file walker, skips `.git/`, `node_modules/`, `target/`, binary files |
| `llm.rs` | `create_file_provider(config)` — Ollama-backed LLM provider via rig |
| `parser.rs` | `parse_and_extract(path, source)` — tree-sitter grammar registry and extraction bridge |
| `cli/init.rs` | `atlas init` handler: primer creation via editor + LLM refinement |
| `cli/index.rs` | `atlas index` handler: full repo scan, progress reporting, LLM descriptions |
| `cli/tree.rs` | `atlas tree` handler: path filtering, depth, JSON output |
| `cli/peek.rs` | `atlas peek` handler: file lookup, symbol display |

## SQLite Schema

Four tables: `symbols`, `files`, `directories`, `metadata`. Indexes on `symbols(file_path)` and `files(path)`. Database at `.mcptools/atlas/index.db` relative to git root.

## Planned Features

- **V2**: ~~LLM-generated file descriptions~~ Implemented, incremental updates (content hash diffing) deferred to V3
- **V3**: MCP tool integration for agent access
- **V4**: Cross-file relationship tracking
- **V5**: Semantic search over symbols and descriptions
