# CLAUDE.md

## Functional Core - Imperative Shell

This codebase follows the Functional Core / Imperative Shell pattern:

1. **Functional Core**: Pure, testable business logic free of side effects (no I/O, no external state mutations). Located in `crates/core/`.
2. **Imperative Shell**: Handles side effects like HTTP requests and file I/O. Uses the functional core for business logic. Located in `crates/mcptools/`.

Based on Gary Bernhardt's original talk on the concept.

## Code Quality

Agents must use `cargo xtask lint` for all code quality checks. Never call `cargo fmt`, `cargo check`, `cargo clippy`, `cargo test`, `cargo machete`, or `typos` directly.

On failure, actionable errors are printed to stdout. Full verbose output is stored in `target/xtask-lint.log`. See **[Lint](.claude/context/lint.md)** for flags and hook management.

## Quick Reference

### Installation

```bash
cargo xtask install              # Install to ~/.local/bin
cargo xtask install --path /usr/local/bin  # Custom path
```

### Upgrade

```bash
mcptools upgrade                 # Upgrade to latest version
mcptools upgrade --force         # Force upgrade
```

### Environment Variables

**Atlassian (Shared)**
```bash
export ATLASSIAN_BASE_URL="https://your-domain.atlassian.net"
export ATLASSIAN_EMAIL="your-email@company.com"
export ATLASSIAN_API_TOKEN="your-api-token"
```

**Service-Specific Overrides**

| Service | Variables | Fallback |
|---------|-----------|----------|
| Jira | `JIRA_BASE_URL`, `JIRA_EMAIL`, `JIRA_API_TOKEN`, `JIRA_BOARD_ID` | `ATLASSIAN_*` |
| Confluence | `CONFLUENCE_BASE_URL`, `CONFLUENCE_EMAIL`, `CONFLUENCE_API_TOKEN` | `ATLASSIAN_*` |
| Bitbucket | `BITBUCKET_USERNAME`, `BITBUCKET_APP_PASSWORD` | None (required) |

**Atlas**

| Variable | Default | Description |
|----------|---------|-------------|
| `ATLAS_DB_PATH` | `.mcptools/atlas/index.db` | Database location |
| `ATLAS_PRIMER_PATH` | `.mcptools/atlas/primer.md` | Primer file location |
| `ATLAS_MAX_FILE_TOKENS` | `10000` | Max tokens per file for LLM |
| `OLLAMA_URL` | `http://localhost:11434` | Ollama API base URL |
| `ATLAS_FILE_MODEL` | `atlas` | Model for file descriptions |
| `ATLAS_DIR_MODEL` | `atlas` | Model for directory descriptions |

## Detailed Documentation

For detailed usage of each feature, see the context files:

### Integrations
- **[Jira](.claude/context/jira.md)** - Search, create, update tickets; saved queries; MCP tools
- **[Confluence](.claude/context/confluence.md)** - Search pages; CQL queries
- **[Bitbucket](.claude/context/bitbucket.md)** - Pull requests; list workspaces, repos, branches, and deploy keys
- **[HackerNews](.claude/context/hackernews.md)** - Read posts/comments; list stories
- **[Web Scraping](.claude/context/web-scraping.md)** - Fetch pages as Markdown; extract TOC
- **[Strand](.claude/context/strand.md)** - Local Rust code generation via Ollama
- **[Atlas](.claude/context/atlas.md)** - Codebase navigation for AI agents; symbol index, tree view, peek
- **[GrepRAG](.claude/context/greprag.md)** - Code context retrieval via local model + ripgrep
- **[PDF Navigation](.claude/context/pdf.md)** - PDF document tree, section reading, image extraction
- **[UI Annotations](.claude/context/annotations.md)** - Dev overlay annotation management for calendsync

### Infrastructure
- **[MCP Server](.claude/context/mcp-server.md)** - Server configuration; available tools
- **[Upgrade](.claude/context/upgrade.md)** - Self-update mechanism; platform support
- **[Testing & Env Vars](.claude/context/testing.md)** - All environment variables; scripting
- **[Lint](.claude/context/lint.md)** - Unified lint pipeline; skip flags; git hook management

## Common Commands

### Atlas

```bash
mcptools atlas init                   # Create primer (mental model) and run initial index
mcptools atlas index                  # Build full index (symbols + descriptions, bottom-up)
mcptools atlas index --parallel 4     # Parallel LLM workers for file descriptions
mcptools atlas index --incremental    # Skip files/dirs that already have descriptions
mcptools atlas tree [path]            # Show annotated directory tree (--json)
mcptools atlas peek <path>            # Show file or directory summary + symbols (--json)
```

### Jira

```bash
mcptools atlassian jira search "assignee = currentUser() AND status NOT IN (Done, Closed)"
mcptools atlassian jira get PROJ-123                # alias: `jira read`
mcptools atlassian jira create "Fix bug" --issue-type Bug
mcptools atlassian jira update PROJ-123 --status "In Progress"
mcptools atlassian jira update PROJ-123 -d "## Summary\nFixed the **login** issue"
mcptools atlassian jira attachment list PROJ-123
mcptools atlassian jira attachment download PROJ-123 12345
mcptools atlassian jira attachment upload PROJ-123 report.pdf screenshot.png
mcptools atlassian jira sprint list --board 1
mcptools atlassian jira sprint list --board 1 --state active,future,closed
mcptools atlassian jira update PROJ-123 --sprint "Sprint 30" --board 1
mcptools atlassian jira create "New task" --sprint "Sprint 30" --board 1
mcptools atlassian jira comment add PROJ-123 "This is my comment"
mcptools atlassian jira comment list PROJ-123
mcptools atlassian jira comment update PROJ-123 12345 "Updated comment"
mcptools atlassian jira comment delete PROJ-123 12345
```

### Confluence

```bash
mcptools atlassian confluence search "text ~ 'deployment'"
```

### Bitbucket

```bash
mcptools atlassian bitbucket pr list --repo "workspace/repo"
mcptools atlassian bitbucket pr read --repo "workspace/repo" 123
mcptools atlassian bitbucket pr create --repo "workspace/repo" "Fix login bug" --source feature-branch
mcptools atlassian bitbucket workspace list
mcptools atlassian bitbucket repo list -w "my-workspace" --all
mcptools atlassian bitbucket repo branches "my-workspace/my-repo" --all
mcptools atlassian bitbucket repo deploy-key list -w "my-workspace" -r "my-repo"
mcptools atlassian bitbucket repo deploy-key add -w "my-workspace" -r "my-repo" -l "ci-key" --key-file ~/.ssh/id_ed25519.pub
mcptools atlassian bitbucket repo deploy-key remove -w "my-workspace" -r "my-repo" --key-id 123
```

### HackerNews

```bash
mcptools hn read 8863
mcptools hn list --story-type top
```

### PDF

```bash
mcptools pdf toc document.pdf
mcptools pdf read document.pdf s-1-0
mcptools pdf read document.pdf                              # whole document
mcptools pdf peek document.pdf s-1-0
mcptools pdf peek document.pdf --position middle --limit 300
mcptools pdf images document.pdf
mcptools pdf images document.pdf s-1-0
mcptools pdf image document.pdf Im1 --output photo.jpg
mcptools pdf image document.pdf --random
mcptools pdf image document.pdf --random --section s-1-0
mcptools pdf info document.pdf
```

### Web Scraping

```bash
mcptools md toc https://docs.example.com
mcptools md fetch https://docs.example.com --selector "main"
```

### Strand

```bash
mcptools strand generate "Write a function that adds two numbers"
mcptools strand generate "Add error handling" --files src/lib.rs src/types.rs
mcptools strand generate "Refactor this" --system-prompt "Focus on readability"
```

| Variable | Default | Description |
|----------|---------|-------------|
| `OLLAMA_URL` | `http://localhost:11434` | Ollama API base URL |
| `STRAND_MODEL` | `maternion/strand-rust-coder` | Default model name |
| `STRAND_SYSTEM_PROMPT` | — | Optional system prompt override |

### GrepRAG

```bash
mcptools grep-rag retrieve "self.deck.draw()" --repo-path ./my-project
```

| Variable | Default | Description |
|----------|---------|-------------|
| `OLLAMA_URL` | `http://localhost:11434` | Ollama API base URL |
| `GREPRAG_MODEL` | `greprag` | Default model name |

### MCP Server

```bash
mcptools mcp stdio   # For local agents (Claude Desktop)
mcptools mcp sse     # For web clients
```

## Setup Guides

- [Atlassian Setup](docs/ATLASSIAN_SETUP.md) - Detailed setup instructions
- [Atlassian Quick Start](docs/ATLASSIAN_QUICK_START.md) - Quick reference
- [GrepRAG Setup](docs/GREPRAG_SETUP.md) - Model download and Ollama import
