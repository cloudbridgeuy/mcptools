# Bitbucket Integration

## CLI Commands

### List Pull Requests

```bash
# List all open PRs (default)
mcptools atlassian bitbucket pr list --repo "myworkspace/myrepo"

# Filter by state (can be repeated)
mcptools atlassian bitbucket pr list --repo "myworkspace/myrepo" --state OPEN
mcptools atlassian bitbucket pr list --repo "myworkspace/myrepo" --state MERGED --state DECLINED

# Limit results
mcptools atlassian bitbucket pr list --repo "myworkspace/myrepo" --limit 20

# Pagination
mcptools atlassian bitbucket pr list --repo "myworkspace/myrepo" --next-page "https://api.bitbucket.org/2.0/..."

# Output as JSON
mcptools atlassian bitbucket pr list --repo "myworkspace/myrepo" --json
```

**Available States:** `OPEN`, `MERGED`, `DECLINED`, `SUPERSEDED`

### Read Pull Request Details

```bash
# Read PR details including diff, diffstat, and comments
mcptools atlassian bitbucket pr read --repo "myworkspace/myrepo" 123

# Skip fetching diff content
mcptools atlassian bitbucket pr read --repo "myworkspace/myrepo" 123 --no-diff

# Only print the diff content
mcptools atlassian bitbucket pr read --repo "myworkspace/myrepo" 123 --diff-only

# Truncate diff output to 200 lines
mcptools atlassian bitbucket pr read --repo "myworkspace/myrepo" 123 --line-limit 200

# No line limit (show full diff)
mcptools atlassian bitbucket pr read --repo "myworkspace/myrepo" 123 --line-limit -1
```

**Read Options:**

- `--repo`: Repository in workspace/repo_slug format (required)
- `--no-diff`: Skip fetching diff content
- `--diff-only`: Only print diff content (skip PR details, diffstat, comments)
- `--line-limit`: Truncate diff output to N lines (-1 = no limit, default: -1 for CLI)
- `--limit`: Maximum comments per page (default: 100)
- `--diff-limit`: Maximum diffstat entries per page (default: 500)

## MCP Tools

### bitbucket_pr_list

```json
{
  "method": "tools/call",
  "params": {
    "name": "bitbucket_pr_list",
    "arguments": {
      "repo": "myworkspace/myrepo",
      "state": ["OPEN"],
      "limit": 10
    }
  }
}
```

**Arguments:**
- `repo` (required): Repository in workspace/repo_slug format
- `state` (optional): Array of states to filter by
- `limit` (optional): Max results per page (default: 10)
- `nextPage` (optional): Pagination URL for next page

### bitbucket_pr_read

```json
{
  "method": "tools/call",
  "params": {
    "name": "bitbucket_pr_read",
    "arguments": {
      "repo": "myworkspace/myrepo",
      "prNumber": 123,
      "lineLimit": 500
    }
  }
}
```

**Arguments:**
- `repo` (required): Repository in workspace/repo_slug format
- `prNumber` (required): Pull request number
- `limit` (optional): Max comments per page (default: 100)
- `diffLimit` (optional): Max diffstat entries per page (default: 500)
- `lineLimit` (optional): Truncate diff to N lines (default: 500, use -1 for unlimited)
- `noDiff` (optional): Skip fetching diff content (default: false)

**Note:** MCP defaults to 500 lines for diff to prevent overwhelming responses.

## Environment Variables

| Variable | Description |
|----------|-------------|
| `BITBUCKET_USERNAME` | Your Bitbucket username |
| `BITBUCKET_APP_PASSWORD` | Your Bitbucket app password |
| `BITBUCKET_BASE_URL` | API base URL (default: `https://api.bitbucket.org/2.0`) |

**Important:** Bitbucket uses App Passwords (not API tokens). Generate at: https://bitbucket.org/account/settings/app-passwords/

Required permissions:
- Repositories: Read
- Pull requests: Read
