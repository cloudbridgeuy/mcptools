# GrepRAG: Code Context Retrieval via Ripgrep

GrepRAG uses a fine-tuned local model (greprag-0.6b, Qwen3 architecture) to generate regex patterns from code context, execute them as ripgrep commands, and return relevant code snippets from a repository. Designed for retrieving cross-file references relevant to a given code snippet.

## CLI Usage

```bash
# Basic — retrieve code snippets matching a code context
mcptools grep-rag retrieve "self.deck.draw()" --repo-path ./my-project

# With custom model/URL
mcptools grep-rag retrieve "fn process(input: &str)" \
  --repo-path ./src \
  --model greprag \
  --ollama-url http://localhost:11434
```

Output: code snippets with file paths and line numbers (`// path:start-end` headers).

## MCP Tool

Tool name: `greprag_retrieve`

| Argument | Type | Required | Default |
|----------|------|----------|---------|
| `local_context` | string | yes | — |
| `repo_path` | string | no | `.` |
| `token_budget` | integer | no | reserved for future use |
| `ollama_url` | string | no | `http://localhost:11434` |
| `model` | string | no | `greprag` |

## Architecture

Follows the Functional Core - Imperative Shell pattern:

- **Core** (`crates/core/src/greprag/`): Pure functions — `parse_rg_commands()`, `parse_rg_output()`, types (`Snippet`, `RankedSnippet`, `MergedSnippet`)
- **Shell** (`crates/mcptools/src/greprag/`): Ollama client via `rig-core`, command execution via `tokio::process::Command`, CLI
- **MCP** (`crates/mcptools/src/mcp/tools/greprag.rs`): Tool handler bridging MCP to greprag module

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
- Requires a running Ollama instance with the greprag model imported
- Modelfile at `models/greprag/Modelfile`

## Pipeline (V1 + V2)

1. **V1 — Query generation**: Calls Ollama model to produce regex patterns from local context, wraps into `rg` commands via `parse_rg_commands()`
2. **V2 — Command execution**: Runs rg commands as subprocesses via `execute_rg_commands()`, parses output into `Vec<Snippet>` via `parse_rg_output()`, formats with `format_snippets_raw()`

Future versions will:
- **V3**: Rank results with BM25 scoring
- **V4**: Deduplicate and merge overlapping snippets
- **V5**: Apply token budget truncation
