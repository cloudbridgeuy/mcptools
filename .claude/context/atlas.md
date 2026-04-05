# Atlas — Codebase Navigator for AI Agents

Atlas scans a repository, extracts symbols using tree-sitter, and stores them in a local SQLite index. Agents use the index to quickly understand codebase structure without reading every file. V2 adds LLM-generated file descriptions and a project primer workflow.

## V1 Scope

- **Symbol index**: Extract functions, classes, structs, enums, traits, interfaces, types, constants, methods, and modules from source files
- **Tree view**: Annotated directory listing from the index
- **Peek**: File summary with symbol signatures and line ranges
- **Languages**: Rust, TypeScript/TSX, JavaScript/JSX, Python, Go

## V2 Scope (Implemented)

- **Project primer**: `atlas init` creates a mental model document via editor + LLM refinement, then runs the initial index
- **LLM file descriptions**: `atlas index` generates one-line descriptions for indexed files using a local Ollama model
- **LLM directory descriptions**: Directory summaries generated from child descriptions (uses Ollama provider)
- **Single bottom-up pass**: Directories are processed deepest-first; for each directory, files are described first (Ollama), then the directory itself (Ollama), so parents always see their children's descriptions
- **Config file**: `.mcptools/config.toml` for per-project settings (db path, primer path, models, token limits)
- **Environment variables**: Override config via `ATLAS_*` and `OLLAMA_URL` env vars

## CLI Commands

```bash
mcptools atlas init                   # Create primer (mental model) via $EDITOR + LLM, then run index
mcptools atlas index                  # Build index with bottom-up LLM descriptions
mcptools atlas index --parallel 4     # Parallel LLM workers for file descriptions
mcptools atlas index --incremental    # Skip files/dirs that already have descriptions
mcptools atlas tree [path]            # Directory tree with descriptions (optional: filter by path)
mcptools atlas tree --depth 2         # Limit depth
mcptools atlas tree --json            # JSON output
mcptools atlas peek <path>            # File or directory summary with symbols/children
mcptools atlas peek <path> --json     # JSON output
mcptools atlas update                 # Incremental update (changed files only)
mcptools atlas sync                   # Force full re-index
mcptools atlas status                 # Index health summary
mcptools atlas status --json          # JSON output
```

### `atlas init` Workflow

1. Opens `$VISUAL` / `$EDITOR` with a primer template (questions about the project)
2. Sends user answers to a local LLM for refinement
3. Opens editor again with the refined draft for final edits
4. Saves to `.mcptools/atlas/primer.md` (configurable)
5. Adds `index.db` to `.gitignore` if not already present
6. Runs the full index automatically

### Bottom-Up Enrichment (`atlas index`)

After the tree-sitter scan, `atlas index` performs a single bottom-up pass over all directories (deepest first). For each directory:

1. **File descriptions**: Describes the files in that directory using the Ollama model (`ATLAS_FILE_MODEL`). Runs with `--parallel N` workers for concurrency.
2. **Directory description**: Describes the directory itself using the Ollama provider (`ATLAS_DIR_MODEL`, default `atlas`), which has access to all children's descriptions (both files and subdirectories).

Because directories are processed deepest-first, parent directories always see their children's descriptions. The Ollama provider requires a running Ollama server (`ollama serve`) and the model to be available.

### `atlas peek` for Files and Directories

`atlas peek <path>` accepts either a file or directory path. For files it shows the file summary and symbol signatures. For directories it shows the directory description, child entries with their descriptions, and aggregated symbols.

### `atlas update` — Incremental Update

`atlas update` performs a hash-based change detection pass. It compares content hashes of files on disk against the stored hashes in the index. Only files whose content has changed are re-indexed (symbols re-extracted, LLM descriptions regenerated). Parent directories affected by changed files are also re-described to keep the bottom-up summaries consistent.

### `atlas sync` — Force Full Re-Index

`atlas sync` clears the existing index and performs a complete rebuild from scratch. This is equivalent to clearing the database and then running `atlas index`. Use this when the index is corrupt, after major refactors, or when you want a guaranteed-fresh index.

## Configuration

### Config File (`.mcptools/config.toml`)

```toml
[atlas]
db_path = ".mcptools/atlas/index.db"
primer_path = ".mcptools/atlas/primer.md"
max_file_tokens = 10000
ollama_url = "http://localhost:11434"
file_model = "atlas"
dir_model = "atlas"
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
| `ATLAS_DIR_MODEL` | `atlas` | Model for directory descriptions |

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
| `tree_view.rs` | `format_tree(entries, json)`, `format_peek(peek, json)`, `format_status(status, json)`, `IndexStatus` |
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
| `cli/peek.rs` | `atlas peek` handler: file or directory lookup, symbol display |
| `cli/update.rs` | `atlas update` handler: incremental update via hash-based change detection |
| `cli/sync.rs` | `atlas sync` handler: clear index and force full re-index |
| `cli/status.rs` | `atlas status` handler: index health display |
| `data.rs` | Shared data functions: `atlas_tree_data`, `atlas_peek_data`, `atlas_status_data` (used by CLI + MCP) |

## SQLite Schema

Four tables: `symbols`, `files`, `directories`, `metadata`. Indexes on `symbols(file_path)` and `files(path)`. Database at `.mcptools/atlas/index.db` relative to git root.

## MCP Tools (V5)

Atlas exposes three MCP tools and one MCP resource for AI agent access.

### Tools

| Tool | Description | Input |
|------|-------------|-------|
| `atlas_tree_view` | Browse annotated directory tree | `path` (optional), `depth` (optional, default 1) |
| `atlas_peek` | Get file/directory summary + symbols | `path` (required) |
| `atlas_status` | Check index health | (none) |

All tools return JSON. They call the same shared data functions as the CLI commands.

### Primer Resource

The project primer is exposed as an MCP resource at `atlas://primer`. Clients can read it via `resources/list` and `resources/read` for automatic injection at session start.

### Agent Navigation Workflow

1. Read primer (MCP resource) — understand project purpose and architecture
2. `atlas_tree_view` at root — pick a direction
3. `atlas_tree_view` with path — drill into directories of interest
4. `atlas_peek` with file path — get summary + symbols before reading
5. Use grep/read to pull the actual code

## Planned Features

- **V2**: ~~LLM-generated file descriptions~~ Implemented
- **V3**: ~~MCP tool integration~~ Implemented (V5)
- **V4**: ~~Incremental updates~~ Implemented
- **V5**: ~~MCP tools + status + primer resource~~ Implemented
- **V6**: Cross-file relationship tracking
- **V7**: Semantic search over symbols and descriptions
