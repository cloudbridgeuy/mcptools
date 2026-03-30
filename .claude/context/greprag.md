# GrepRAG: Code Context Retrieval via Ripgrep

GrepRAG uses a fine-tuned local model (greprag-0.6b, Qwen3 architecture) to generate regex patterns from code context, wrapping them into runnable `rg` commands targeting a repository. Designed for retrieving cross-file references relevant to a given code snippet.

## CLI Usage

```bash
# Basic — generate rg commands for a code snippet
mcptools grep-rag retrieve "self.deck.draw()" --repo-path ./my-project

# With custom model/URL
mcptools grep-rag retrieve "fn process(input: &str)" \
  --repo-path ./src \
  --model greprag \
  --ollama-url http://localhost:11434
```

Output: one `rg -n 'PATTERN' <repo-path>` command per line, directly copy-pasteable.

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

- **Core** (`crates/core/src/greprag/`): Pure functions — `parse_rg_commands()`, types (`Snippet`, `RankedSnippet`, `MergedSnippet`)
- **Shell** (`crates/mcptools/src/greprag/`): Ollama client via `rig-core`, CLI
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
- Requires a running Ollama instance with the greprag model imported
- Modelfile at `models/greprag/Modelfile`

## V1 Scope

V1 generates and prints rg commands only. Future versions will:
- **V2**: Execute the rg commands against `repo_path`
- **V3**: Rank results with BM25 scoring
- **V4**: Deduplicate and merge overlapping snippets
- **V5**: Apply token budget truncation
