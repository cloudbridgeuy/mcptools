# CLAUDE.md

## Functional Core - Imperative Shell

This codebase follows the Functional Core / Imperative Shell pattern:

1. **Functional Core**: Pure, testable business logic free of side effects (no I/O, no external state mutations). Located in `crates/core/`.
2. **Imperative Shell**: Handles side effects like HTTP requests and file I/O. Uses the functional core for business logic. Located in `crates/mcptools/`.

Based on Gary Bernhardt's original talk on the concept.

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
| Jira | `JIRA_BASE_URL`, `JIRA_EMAIL`, `JIRA_API_TOKEN` | `ATLASSIAN_*` |
| Confluence | `CONFLUENCE_BASE_URL`, `CONFLUENCE_EMAIL`, `CONFLUENCE_API_TOKEN` | `ATLASSIAN_*` |
| Bitbucket | `BITBUCKET_USERNAME`, `BITBUCKET_APP_PASSWORD` | None (required) |

## Detailed Documentation

For detailed usage of each feature, see the context files:

### Integrations
- **[Jira](docs/context/jira.md)** - Search, create, update tickets; saved queries; MCP tools
- **[Confluence](docs/context/confluence.md)** - Search pages; CQL queries
- **[Bitbucket](docs/context/bitbucket.md)** - List and read pull requests
- **[HackerNews](docs/context/hackernews.md)** - Read posts/comments; list stories
- **[Web Scraping](docs/context/web-scraping.md)** - Fetch pages as Markdown; extract TOC

### Infrastructure
- **[MCP Server](docs/context/mcp-server.md)** - Server configuration; available tools
- **[Upgrade](docs/context/upgrade.md)** - Self-update mechanism; platform support
- **[Testing & Env Vars](docs/context/testing.md)** - All environment variables; scripting

## Common Commands

### Jira

```bash
mcptools atlassian jira search "assignee = currentUser() AND status NOT IN (Done, Closed)"
mcptools atlassian jira get PROJ-123
mcptools atlassian jira create "Fix bug" --issue-type Bug
mcptools atlassian jira update PROJ-123 --status "In Progress"
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

### Web Scraping

```bash
mcptools md toc https://docs.example.com
mcptools md fetch https://docs.example.com --selector "main"
```

### MCP Server

```bash
mcptools mcp stdio   # For local agents (Claude Desktop)
mcptools mcp sse     # For web clients
```

## Setup Guides

- [Atlassian Setup](docs/ATLASSIAN_SETUP.md) - Detailed setup instructions
- [Atlassian Quick Start](docs/ATLASSIAN_QUICK_START.md) - Quick reference
