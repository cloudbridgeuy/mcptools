# Testing & Environment Variables

Environment variables support scripting, testing, and CI/CD pipelines. Persistent settings can also use the file keys below.

`MCPTOOLS_LANE_REPOS` stays operator-controlled environment only so agent-writable config cannot grant lane access. `RALPH_INBOX` stays operator-controlled because it grants an external notification write target. `CHROME` selects the runtime executable; `LLM_STREAM_BIN` is test-only. These have no config keys. `PORT` sets the Linear chart `--serve` port per invocation (default 0, ephemeral) and is outside this migration; `CALENDSYNC_DEV_URL` is development-only.

## Global Variables

| Variable | Description |
|----------|-------------|
| `MCPTOOLS_VERBOSE` | Enable verbose output (default: false); `[general].verbose` |
| `MCPTOOLS_DISCOVERY` | List only `find_tools` via `tools/list`. Accepts `y`, `yes`, `t`, `true`, `on`, `1` (on) and `n`, `no`, `f`, `false`, `off`, `0` (off), any case; other values exit 2 (default: false); `[mcp].discovery` |
| `MCPTOOLS_CODE_MODE` | List `find_tools,execute` via `tools/list`; wins over `MCPTOOLS_DISCOVERY`. Same accepted values, other values exit 2 (default: false); `[mcp].code_mode` |
| `MCPTOOLS_EXECUTE_TIMEOUT_SECS` | Execute timeout; `[mcp].execute_timeout_secs` (default: 30) |
| `MCPTOOLS_EXECUTE_MEMORY_MB` | Execute memory; `[mcp].execute_memory_mb` (default: 64) |
| `MCPTOOLS_EXECUTE_OUTPUT_KB` | Execute output; `[mcp].execute_output_kb` (default: 256) |

## Diagram Variables

| Variable | File key |
|----------|----------|
| `MCPTOOLS_DIAGRAM_HOST` | `[diagram].host` |
| `MCPTOOLS_DIAGRAM_PORT` | `[diagram].port` |
| `MCPTOOLS_DIAGRAM_SHARE_SECONDS` | `[diagram].share_seconds` |
| `MCPTOOLS_DIAGRAM_TTL_SECONDS` | `[diagram].ttl_seconds` |
| `MCPTOOLS_DIAGRAM_IDLE_CHECK_SECONDS` | `[diagram].idle_check_seconds` |
| `MCPTOOLS_DIAGRAM_IDLE_GRACE_SECONDS` | `[diagram].idle_grace_seconds` |

See [diagram configuration](diagram-serve.md#configuration) for defaults and behavior.

## Atlassian Variables

### Shared (Fallback)

| Variable | Description |
|----------|-------------|
| `ATLASSIAN_BASE_URL` | Base URL (e.g., `https://company.atlassian.net`) |
| `ATLASSIAN_EMAIL` | Email for authentication |
| `ATLASSIAN_API_TOKEN` | API token for authentication |

### Jira-Specific (Override)

| Variable | Description |
|----------|-------------|
| `JIRA_BASE_URL` | Jira instance URL (fallback: `ATLASSIAN_BASE_URL`) |
| `JIRA_EMAIL` | Jira email (fallback: `ATLASSIAN_EMAIL`) |
| `JIRA_API_TOKEN` | Jira API token (fallback: `ATLASSIAN_API_TOKEN`) |
| `JIRA_QUERY` | JQL query for search command |
| `JIRA_ISSUE_KEY` | Issue key for get command |

### Confluence-Specific (Override)

| Variable | Description |
|----------|-------------|
| `CONFLUENCE_BASE_URL` | Confluence URL (fallback: `ATLASSIAN_BASE_URL`) |
| `CONFLUENCE_EMAIL` | Confluence email (fallback: `ATLASSIAN_EMAIL`) |
| `CONFLUENCE_API_TOKEN` | Confluence API token (fallback: `ATLASSIAN_API_TOKEN`) |
| `CONFLUENCE_QUERY` | CQL query for search command |

### Bitbucket (No Fallback)

| Variable | Description |
|----------|-------------|
| `BITBUCKET_USERNAME` | Bitbucket username (required) |
| `BITBUCKET_APP_PASSWORD` | Bitbucket app password (required) |
| `BITBUCKET_BASE_URL` | API URL (default: `https://api.bitbucket.org/2.0`) |

## HackerNews Variables

| Variable | Description |
|----------|-------------|
| `HN_ITEM` | Item ID or URL for read command |
| `HN_LIMIT` | Number of items to return |

## Web Scraping Variables

| Variable | Description |
|----------|-------------|
| `MD_URL` | URL to fetch |
| `MD_TIMEOUT` | Timeout in seconds (default: 30); `[md].timeout` |
| `MD_SELECTOR` | CSS selector |
| `MD_STRATEGY` | Selection strategy (first, last, all, n) |
| `MD_INDEX` | Index for 'n' strategy |
| `MD_PAGINATED` | Enable pagination |
| `MD_OFFSET` | Character offset |
| `MD_LIMIT` | Characters per page |
| `MD_PAGE` | Page number |
| `MD_OUTPUT` | TOC output format |

## UI Annotations Variables

| Variable | Description |
|----------|-------------|
| `CALENDSYNC_DEV_URL` | Dev server base URL (default: `http://localhost:3000`) |

## Images Variables

| Variable | Description |
|----------|-------------|
| `MCPTOOLS_IMAGES_API` | Backend: `chatgpt` (subscription, default) or `openai` (API key) |
| `LLM_STREAM_CONFIG_DIR` | Config dir holding subscription `auth.json` (default: `~/.config/llm-stream`) |
| `MCPTOOLS_IMAGES_MAINLINE` | Chat model fronting the image tool (default: `gpt-5.6-sol`) |
| `OPENAI_IMAGES_MODEL` | Image model (default: `gpt-image-2.6-flare`) |
| `OPENAI_API_KEY` | Key for `--api openai` |
| `OPENAI_BASE_URL` | Base URL for `--api openai` (default: `https://api.openai.com/v1`) |

## Jev Variables

Jev also reads `[jev]` fields `provider`, `endpoint`, `model`, and `api_key` from `~/.config/mcptools/config.toml` (or `MCPTOOLS_CONFIG`). Environment values override file values; `JEV_API_KEY` takes precedence over the selected provider's key variable, then `api_key`. Run `chmod 600` on files containing keys.

| Variable | Description |
|----------|-------------|
| `JEV_PROVIDER` | `opencode`, `openrouter`, `vercel` or `typesafe` to enable Jev (unset in env and file selects local) |
| `JEV_ENDPOINT` | Override gateway endpoint (defaults from provider) |
| `JEV_MODEL` | Override model (defaults from provider) |
| `JEV_API_KEY` | API key (falls back to the provider's conventional variable) |
| `OPENCODE_API_KEY` | Key used by `opencode` preset |
| `OPENROUTER_API_KEY` | Key used by `openrouter` preset |
| `AI_GATEWAY_API_KEY` | Key used by `vercel` preset |
| `TYPESAFE_API_KEY` | Key used by `typesafe` preset |

On `jev-1.13-free` the full 61-tool catalog plus `none` returns in under 1.2 s per call. `none` separates in-scope from out-of-scope tasks except for one near-miss. The 0.05-floor selection rule matches the live probabilities for every query the spike ran. This covers the `opencode` preset only; other presets are unverified beyond gateway documentation.

Live run against the `opencode` preset, one row per query (`JEV_PROVIDER=opencode`, `-k 5`, timed per call):

| task | backend | fallback | none | tools | latency |
|------|---------|----------|------|-------|---------|
| `mark the ticket done` | `jev` | — | 0.01 | `jira_update`, `linear_issue_update` | 1.2 s |
| `table of contents of this pdf` | `jev` | — | 0.0 | `pdf_toc` | 1.4 s |
| `table of contents of this web page` | `jev` | — | 0.0 | `md_toc` | 1.2 s |
| `update the jira comment I just listed` | `jev` | — | 0.0 | `jira_comment_update` | 1.2 s |
| `jira_comment_update` | `jev` | — | 0.0 | `jira_comment_update` | 1.9 s |
| `pdf_toc` | `jev` | — | 0.0 | `pdf_toc` | 1.1 s |
| `book a flight to Paris` | `jev` | — | 1.0 | — | 1.1 s |

## Usage Examples

### Scripting

```bash
# Set variables once, run multiple commands
export JIRA_BASE_URL="https://company.atlassian.net"
export JIRA_EMAIL="user@company.com"
export JIRA_API_TOKEN="your-token"

# Now commands use these automatically
mcptools atlassian jira search "assignee = currentUser()"
mcptools atlassian jira get PROJ-123
```

### Testing with Environment Variables

```bash
# Test a specific issue
JIRA_ISSUE_KEY=PROJ-123 mcptools atlassian jira get

# Test HackerNews with specific item
HN_ITEM=8863 HN_LIMIT=5 mcptools hn read

# Test web scraping with preset config
MD_URL=https://example.com MD_SELECTOR=main mcptools md fetch
```

### CI/CD Integration

```yaml
# GitHub Actions example
env:
  ATLASSIAN_BASE_URL: ${{ secrets.ATLASSIAN_BASE_URL }}
  ATLASSIAN_EMAIL: ${{ secrets.ATLASSIAN_EMAIL }}
  ATLASSIAN_API_TOKEN: ${{ secrets.ATLASSIAN_API_TOKEN }}

steps:
  - run: mcptools atlassian jira search "project = CI"
```

### .env File (Manual Sourcing)

mcptools does **not** auto-load `.env` files. Source them manually:

```bash
# .env file
ATLASSIAN_BASE_URL=https://company.atlassian.net
ATLASSIAN_EMAIL=user@company.com
ATLASSIAN_API_TOKEN=secret-token

# Source before running
source .env
mcptools atlassian jira search "assignee = currentUser()"
```

## Precedence

1. CLI flags or per-call arguments (highest priority)
2. Environment variables
3. Config file values
4. Built-in defaults (lowest priority)

Service-specific variables (e.g., `JIRA_*`) take precedence over shared variables (`ATLASSIAN_*`).


Atlassian settings can also be stored in `~/.config/mcptools/config.toml`.
Set `MCPTOOLS_CONFIG` to use another file. Protect credentials with `chmod 600`.
Environment values override file values; service settings override shared settings within each source.

```toml
[atlassian]
base_url = "https://example.atlassian.net"
email = "user@example.com"
api_token = "token"

[atlassian.jira]
base_url = "https://example.atlassian.net"
email = "user@example.com"
api_token = "token"
board_id = 42
query = "project = EXAMPLE"
issue_key = "EXAMPLE-1"

[atlassian.confluence]
base_url = "https://example.atlassian.net"
email = "user@example.com"
api_token = "token"
query = "space = EXAMPLE"

[atlassian.bitbucket]
base_url = "https://api.bitbucket.org/2.0"
username = "user"
app_password = "password"
```

Images reads `[images]` settings from the mcptools config file. CLI or MCP arguments override environment, then file values, then built-in defaults. Subscription auth prefers `auth.json` beside the mcptools config and falls back to llm-stream credentials. Explicit config directories take precedence.
