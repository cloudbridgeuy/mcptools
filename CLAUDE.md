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

**Jev**

| Variable | Default | Description |
|----------|---------|-------------|
| `JEV_PROVIDER` | unset | `opencode`, `openrouter`, `vercel` or `typesafe` to enable Jev classifier |
| `JEV_ENDPOINT` | provider preset | Override endpoint URL |
| `JEV_MODEL` | provider preset | Override model name |
| `JEV_API_KEY` | (none) | API key; falls back to the provider's conventional key var |
| `OPENCODE_API_KEY` | - | Conventional key var when `JEV_PROVIDER=opencode` |
| `OPENROUTER_API_KEY` | - | Conventional key var when `JEV_PROVIDER=openrouter` |
| `AI_GATEWAY_API_KEY` | - | Conventional key var when `JEV_PROVIDER=vercel` |
| `TYPESAFE_API_KEY` | - | Conventional key var when `JEV_PROVIDER=typesafe` |

## Detailed Documentation

For detailed usage of each feature, see the context files:

### Integrations
- **[Jira](.claude/context/jira.md)** - Search, create, update, inspect transitions, and manage issue links; saved queries; MCP tools
- **[Linear](.claude/context/linear.md)** - Issues (including delete to trash) and discovery; MCP project creation, partial property updates, published progress report creation/listing, workspace project statuses
- **[Confluence](.claude/context/confluence.md)** - Search and read pages; CQL queries
- **[Bitbucket](.claude/context/bitbucket.md)** - Pull requests; list workspaces, repos, branches, and deploy keys
- **[HackerNews](.claude/context/hackernews.md)** - Read posts/comments; list stories
- **[Web Scraping](.claude/context/web-scraping.md)** - Fetch pages as Markdown; extract TOC
- **[Images](.claude/context/images.md)** - ChatGPT Images 2.5 generation, editing, variations; subscription or API key
- **[PDF Navigation](.claude/context/pdf.md)** - PDF document tree, section reading, image extraction
- **[UI Annotations](.claude/context/annotations.md)** - Dev overlay annotation management for calendsync
- **[Agent Setup](.claude/context/agent-setup.md)** - User-level agent integration; setup, status, uninstall for codex, claude, opencode

### Infrastructure
- **[Lanes](.claude/context/lanes.md)** - Native Git read-only listing/cleanup planning and guarded Worktrunk creation; operator-only `MCPTOOLS_LANE_REPOS` authorization
- **[MCP Server](.claude/context/mcp-server.md)** - Server configuration; available tools
- **[Upgrade](.claude/context/upgrade.md)** - Self-update mechanism; platform support
- **[Testing & Env Vars](.claude/context/testing.md)** - All environment variables; scripting
- **[Lint](.claude/context/lint.md)** - Unified lint pipeline; skip flags; git hook management

## Common Commands

### Jira

```bash
mcptools atlassian jira search "assignee = currentUser() AND status NOT IN (Done, Closed)"
mcptools atlassian jira get PROJ-123                # alias: `jira read`
mcptools atlassian jira create "Fix bug" --issue-type Bug
mcptools atlassian jira update PROJ-123 --status "In Progress"
mcptools atlassian jira transitions list PROJ-123
mcptools atlassian jira link types
mcptools atlassian jira link list PROJ-123
mcptools atlassian jira link add PROJ-123 --type Blocks PROJ-456
mcptools atlassian jira link delete 10001
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
mcptools atlassian confluence get 123456
mcptools atlassian confluence read 'https://example.atlassian.net/wiki/spaces/ENG/pages/123456/Runbook' --json
```

### Linear

```bash
mcptools linear auth status
mcptools linear issue get GUZ-79
mcptools linear issue update GUZ-22 --state Done
mcptools linear issue delete GUZ-22                 # moves to Linear trash; add --json
mcptools linear comment update COMMENT_ID "Updated note" --json
mcptools linear comment delete COMMENT_ID --json
mcptools linear issue list --team GUZ --state "In Progress"
mcptools linear issue list --assignee me --json
mcptools linear teams list
mcptools linear projects list --team GUZ
mcptools linear users list --query "Ada"
mcptools linear chart GUZ-185 GUZ-186 GUZ-188
```

### Bitbucket

```bash
mcptools atlassian bitbucket pr list --repo "workspace/repo"
mcptools atlassian bitbucket pr read --repo "workspace/repo" 123
mcptools atlassian bitbucket pr create --repo "workspace/repo" "Fix login bug" --source feature-branch
mcptools atlassian bitbucket pr update --repo "workspace/repo" 123 --title "New title" --approve
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

### MCP Server

```bash
mcptools mcp stdio   # For local agents (Claude Desktop)
mcptools mcp stdio --discovery  # tools/list returns find_tools,call_tool (env: MCPTOOLS_DISCOVERY=true)
mcptools mcp stdio --code-mode  # tools/list returns find_tools,execute,call_tool (env: MCPTOOLS_CODE_MODE=true)
mcptools mcp sse     # For web clients
mcptools mcp catalog # Print the compact tool catalog (domain, name, summary)
mcptools mcp declarations [NAME...] # Print TypeScript declarations for named tools (all tools without names)
```

Call find_tools (task, --domain, or --list-domains). A domain list is how you prove a tool is absent. Read the declaration, call by name; same-domain follow-ups need no second find_tools. See [.claude/context/mcp-server.md#agent-loop](.claude/context/mcp-server.md#agent-loop) for the full loop.

## Setup Guides

- [Atlassian Setup](docs/ATLASSIAN_SETUP.md) - Detailed setup instructions
- [Atlassian Quick Start](docs/ATLASSIAN_QUICK_START.md) - Quick reference
