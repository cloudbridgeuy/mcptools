# Testing & Environment Variables

All CLI arguments can be provided via environment variables, useful for scripting, testing, and CI/CD pipelines.

## Global Variables

| Variable | Description |
|----------|-------------|
| `MCPTOOLS_VERBOSE` | Enable verbose output (default: false) |

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
| `MD_TIMEOUT` | Timeout in seconds (default: 30) |
| `MD_SELECTOR` | CSS selector |
| `MD_STRATEGY` | Selection strategy (first, last, all, n) |
| `MD_INDEX` | Index for 'n' strategy |
| `MD_PAGINATED` | Enable pagination |
| `MD_OFFSET` | Character offset |
| `MD_LIMIT` | Characters per page |
| `MD_PAGE` | Page number |

## Strand Variables

| Variable | Description |
|----------|-------------|
| `OLLAMA_URL` | Ollama API base URL (default: `http://localhost:11434`) |
| `STRAND_MODEL` | Model name for code generation (default: `strand-rust-coder`) |

## UI Annotations Variables

| Variable | Description |
|----------|-------------|
| `CALENDSYNC_DEV_URL` | Dev server base URL (default: `http://localhost:3000`) |

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

1. CLI arguments (highest priority)
2. Environment variables
3. Default values (lowest priority)

Service-specific variables (e.g., `JIRA_*`) take precedence over shared variables (`ATLASSIAN_*`).
