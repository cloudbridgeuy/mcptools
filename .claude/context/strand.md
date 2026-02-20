# Strand: Local Rust Code Generation

Strand wraps a local Ollama model (Strand-Rust-Coder-14B) as a stateless, read-only code generation tool. A higher-level agent orchestrates the workflow — reading projects, calling strand for code generation, writing files, running tests.

## CLI Usage

```bash
# Basic
mcptools strand generate "Write a function that adds two numbers"

# With file context
mcptools strand generate "Add error handling" --files src/lib.rs src/types.rs

# Custom model/URL
mcptools strand generate "Write a hello world" \
  --model codellama \
  --ollama-url http://localhost:11434
```

## MCP Tool

Tool name: `generate_code`

| Argument | Type | Required | Default |
|----------|------|----------|---------|
| `instruction` | string | yes | — |
| `context` | string | no | — |
| `files` | string[] | no | [] |
| `ollama_url` | string | no | `http://localhost:11434` |
| `model` | string | no | `strand-rust-coder` |

## Architecture

Follows the Functional Core - Imperative Shell pattern:

- **Core** (`crates/core/src/strand/`): Pure functions — `build_prompt()`, `extract_code()`, types
- **Shell** (`crates/mcptools/src/strand/`): Ollama client via `rig-core`, file I/O, CLI
- **MCP** (`crates/mcptools/src/mcp/tools/strand.rs`): Tool handler bridging MCP to strand module

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `OLLAMA_URL` | `http://localhost:11434` | Ollama API base URL |
| `STRAND_MODEL` | `strand-rust-coder` | Default model name |

## Dependencies

- `rig-core` 0.31.0 — Ollama provider via `CompletionClient` trait
- Requires a running Ollama instance with the target model

## System Preamble

The model is instructed to output only raw Rust code — no markdown fences, no explanations, no commentary. The `extract_code()` function in core provides a safety net to strip any accidental fences or leading text from the response.
