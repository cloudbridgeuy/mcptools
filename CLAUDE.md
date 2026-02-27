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

## Detailed Documentation

For detailed usage of each feature, see the context files:

### Integrations
- **[Jira](.claude/context/jira.md)** - Search, create, update tickets; saved queries; MCP tools
- **[Confluence](.claude/context/confluence.md)** - Search pages; CQL queries
- **[Bitbucket](.claude/context/bitbucket.md)** - List and read pull requests
- **[HackerNews](.claude/context/hackernews.md)** - Read posts/comments; list stories
- **[Web Scraping](.claude/context/web-scraping.md)** - Fetch pages as Markdown; extract TOC
- **[Strand](.claude/context/strand.md)** - Local Rust code generation via Ollama
- **[PDF Navigation](.claude/context/pdf.md)** - PDF document tree, section reading, image extraction
- **[UI Annotations](.claude/context/annotations.md)** - Dev overlay annotation management for calendsync

### Infrastructure
- **[MCP Server](.claude/context/mcp-server.md)** - Server configuration; available tools
- **[Upgrade](.claude/context/upgrade.md)** - Self-update mechanism; platform support
- **[Testing & Env Vars](.claude/context/testing.md)** - All environment variables; scripting
- **[Lint](.claude/context/lint.md)** - Unified lint pipeline; skip flags; git hook management

## Common Commands

### Jira

```bash
mcptools atlassian jira search "assignee = currentUser() AND status NOT IN (Done, Closed)"
mcptools atlassian jira get PROJ-123
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
```

### Confluence

```bash
mcptools atlassian confluence search "text ~ 'deployment'"
```

### Bitbucket

```bash
mcptools atlassian bitbucket pr list --repo "workspace/repo"
mcptools atlassian bitbucket pr read --repo "workspace/repo" 123
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
```

### MCP Server

```bash
mcptools mcp stdio   # For local agents (Claude Desktop)
mcptools mcp sse     # For web clients
```

## Setup Guides

- [Atlassian Setup](docs/ATLASSIAN_SETUP.md) - Detailed setup instructions
- [Atlassian Quick Start](docs/ATLASSIAN_QUICK_START.md) - Quick reference
