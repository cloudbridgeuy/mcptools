# GrepRAG: Code Context Retrieval via Ripgrep

GrepRAG uses a fine-tuned local model (greprag-0.6b, Qwen3 architecture) to generate regex patterns from code context, execute them as ripgrep commands, rank results by BM25 relevance, and return the most relevant code snippets from a repository. Designed for retrieving cross-file references relevant to a given code snippet.

## CLI Usage

```bash
# Basic — retrieve code snippets matching a code context
mcptools grep-rag retrieve "self.deck.draw()" --repo-path ./my-project

# With custom token budget
mcptools grep-rag retrieve "fn process(input: &str)" \
  --repo-path ./src \
  --token-budget 2048

# With custom model/URL
mcptools grep-rag retrieve "fn process(input: &str)" \
  --repo-path ./src \
  --model greprag \
  --ollama-url http://localhost:11434
```

Output: code snippets with file/line headers (`// file: path/to/file.rs (lines N-M)`), deduplicated, selected within token budget.

## MCP Tool

Tool name: `greprag_retrieve`

| Argument | Type | Required | Default |
|----------|------|----------|---------|
| `local_context` | string | yes | — |
| `repo_path` | string | no | `.` |
| `token_budget` | integer | no | `4096` |
| `ollama_url` | string | no | `http://localhost:11434` |
| `model` | string | no | `greprag` |

## Architecture

Follows the Functional Core - Imperative Shell pattern:

- **Core** (`crates/core/src/greprag/`): Pure functions — parsing (`parse_rg_commands`, `parse_rg_output`), IDF (`build_doc_frequencies`, `extract_query_identifiers`), ranking (`bm25_rank`), dedup (`dedup_overlapping`), selection (`select_top_k`), formatting (`format_context`), types (`Snippet`, `RankedSnippet`, `MergedSnippet`)
- **Shell** (`crates/mcptools/src/greprag/`): Ollama client via `rig-core`, command execution via `tokio::process::Command`, repo scanning via `tree-sitter` + `ignore` crate, CLI
- **MCP** (`crates/mcptools/src/mcp/tools/greprag.rs`): Tool handler bridging MCP to greprag module

### Core Modules

| Module | Purpose |
|--------|---------|
| `types.rs` | `Snippet`, `RankedSnippet`, `MergedSnippet` types |
| `parse.rs` | `parse_rg_output`, `parse_rg_commands` |
| `idf.rs` | `DocFreqMap`, `build_doc_frequencies`, `extract_query_identifiers` |
| `rank.rs` | `bm25_rank` |
| `dedup.rs` | `dedup_overlapping` — merge overlapping/adjacent snippets |
| `select.rs` | `select_top_k` — token-budget selection |
| `format.rs` | `format_context` — LLM-ready context formatting |

## Model Details

The model outputs **regex patterns** (one per line), not full rg commands. The `parse_rg_commands()` function wraps each pattern into `rg -n 'PATTERN' <repo_path>`.

| Property | Value |
|----------|-------|
| Base model | Qwen3-0.6B |
| Chat format | ChatML (`<\|im_start\|>` / `<\|im_end\|>`) |
| Quantization | Q4_K_M (~397 MB) |
| License | Apache 2.0 |

See [GrepRAG Setup](../../docs/GREPRAG_SETUP.md) for model installation.

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `OLLAMA_URL` | `http://localhost:11434` | Ollama API base URL |
| `GREPRAG_MODEL` | `greprag` | Default model name |

## Dependencies

- `rig-core` — Ollama provider via `CompletionClient` trait
- `shlex` — Shell-like command string splitting for rg command execution
- `tree-sitter` + `tree-sitter-rust` — AST parsing for identifier extraction (repo scan)
- `ignore` — .gitignore-aware file walking (same crate ripgrep uses)
- Requires a running Ollama instance with the greprag model imported
- Modelfile at `models/greprag/Modelfile`

## Pipeline (V1 + V2 + V3 + V4)

1. **V1 — Query generation**: Calls Ollama model to produce regex patterns from local context, wraps into `rg` commands via `parse_rg_commands()`
2. **V2 — Command execution**: Runs rg commands as subprocesses via `execute_rg_commands()`, parses output into `Vec<Snippet>` via `parse_rg_output()`
3. **V3 — BM25 ranking**: Scans repo with tree-sitter to build identifier document frequencies (`scan_repo_identifiers` → `build_doc_frequencies`), extracts query terms from local context (`extract_query_identifiers`), scores snippets with BM25 (`bm25_rank`)
4. **V4 — Dedup, select, format**: Merges overlapping/adjacent snippets (`dedup_overlapping`), selects top-K within token budget (`select_top_k`), formats for LLM context (`format_context`)

Note: Ollama call and repo scan run concurrently via `tokio::join!`.

### BM25 Details

- Constants: K1=1.2, B=0.75
- IDF formula: `ln((N - df + 0.5) / (df + 0.5) + 1)` (non-negative variant)
- Average document length computed across snippet set (not whole repo)
- Repo scan extracts `identifier` and `type_identifier` tree-sitter nodes from `.rs` files
- Token budget approximation: (content bytes + ~60 header overhead) / 4; zero-score snippets filtered

Future versions:
- **V5**: Stopword filtering for common identifiers
