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

### Create Pull Request

```bash
# Create a PR (source branch auto-detected from current git branch)
mcptools atlassian bitbucket pr create --repo "myworkspace/myrepo" "Fix login bug"

# Specify source branch explicitly
mcptools atlassian bitbucket pr create --repo "myworkspace/myrepo" "Fix login bug" --source feature-branch

# Specify destination branch (defaults to repo's main branch)
mcptools atlassian bitbucket pr create --repo "myworkspace/myrepo" "Fix login bug" --source feature-branch --destination develop

# With description
mcptools atlassian bitbucket pr create --repo "myworkspace/myrepo" "Fix login bug" -d "Fixes the login timeout issue"

# Close source branch after merge
mcptools atlassian bitbucket pr create --repo "myworkspace/myrepo" "Fix login bug" --close-source-branch

# Output as JSON
mcptools atlassian bitbucket pr create --repo "myworkspace/myrepo" "Fix login bug" --json
```

**Create Options:**

- `--repo` / `-r`: Repository in workspace/repo_slug format (required)
- `--source` / `-s`: Source branch name (defaults to current git branch)
- `--destination`: Destination branch (defaults to repo's main branch)
- `--description` / `-d`: PR description
- `--close-source-branch`: Close source branch after merge
- `--json`: Output as JSON

### List Workspaces

```bash
# List all accessible workspaces
mcptools atlassian bitbucket workspace list

# Fetch all workspaces (auto-paginate)
mcptools atlassian bitbucket workspace list --all

# Limit results per page
mcptools atlassian bitbucket workspace list --limit 20

# Pagination
mcptools atlassian bitbucket workspace list --next-page "https://api.bitbucket.org/2.0/..."

# Output as JSON
mcptools atlassian bitbucket workspace list --format json

# Output as CSV
mcptools atlassian bitbucket workspace list --format csv
```

### List Repositories

```bash
# List repos in a workspace
mcptools atlassian bitbucket repo list --workspace "myworkspace"

# Short flag
mcptools atlassian bitbucket repo list -w "myworkspace"

# Fetch all repos (auto-paginate)
mcptools atlassian bitbucket repo list -w "myworkspace" --all

# Limit results per page
mcptools atlassian bitbucket repo list -w "myworkspace" --limit 20

# Pagination
mcptools atlassian bitbucket repo list -w "myworkspace" --next-page "https://api.bitbucket.org/2.0/..."

# Output as JSON
mcptools atlassian bitbucket repo list -w "myworkspace" --format json

# Output as CSV
mcptools atlassian bitbucket repo list -w "myworkspace" --format csv
```

### List Branches

```bash
# List branches in a repo (positional workspace/repo format)
mcptools atlassian bitbucket repo branches myworkspace/myrepo

# Same with flags
mcptools atlassian bitbucket repo branches -w "myworkspace" -r "myrepo"

# Fetch all branches (auto-paginate)
mcptools atlassian bitbucket repo branches myworkspace/myrepo --all

# Filter by name
mcptools atlassian bitbucket repo branches myworkspace/myrepo -q 'name ~ "feature"'

# Sort by newest commit
mcptools atlassian bitbucket repo branches myworkspace/myrepo --sort "-target.date"

# Limit results per page
mcptools atlassian bitbucket repo branches myworkspace/myrepo --limit 20

# Output as JSON
mcptools atlassian bitbucket repo branches myworkspace/myrepo --format json

# Output as CSV
mcptools atlassian bitbucket repo branches myworkspace/myrepo --format csv
```

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

### bitbucket_pr_create

```json
{
  "method": "tools/call",
  "params": {
    "name": "bitbucket_pr_create",
    "arguments": {
      "repo": "myworkspace/myrepo",
      "title": "Fix login bug",
      "sourceBranch": "feature/fix-login"
    }
  }
}
```

**Arguments:**
- `repo` (required): Repository in workspace/repo_slug format
- `title` (required): Title of the pull request
- `sourceBranch` (required): Source branch name
- `destinationBranch` (optional): Destination branch (defaults to repo's main branch)
- `description` (optional): Description of the pull request
- `closeSourceBranch` (optional): Close source branch after merge (default: false)

**Note:** Unlike the CLI, MCP requires `sourceBranch` explicitly (no git detection).

### bitbucket_workspace_list

```json
{
  "method": "tools/call",
  "params": {
    "name": "bitbucket_workspace_list",
    "arguments": {
      "limit": 10
    }
  }
}
```

**Arguments:**
- `limit` (optional): Max results per page (default: 10)
- `nextPage` (optional): Pagination URL for next page

### bitbucket_repo_list

```json
{
  "method": "tools/call",
  "params": {
    "name": "bitbucket_repo_list",
    "arguments": {
      "workspace": "myworkspace",
      "limit": 10
    }
  }
}
```

**Arguments:**
- `workspace` (required): Workspace slug
- `limit` (optional): Max results per page (default: 10)
- `nextPage` (optional): Pagination URL for next page

### bitbucket_repo_branches

```json
{
  "method": "tools/call",
  "params": {
    "name": "bitbucket_repo_branches",
    "arguments": {
      "workspace": "myworkspace",
      "repo": "myrepo",
      "limit": 10
    }
  }
}
```

**Arguments:**
- `workspace` (required): Workspace slug
- `repo` (required): Repository slug
- `limit` (optional): Max results per page (default: 10)
- `nextPage` (optional): Pagination URL for next page
- `query` (optional): Bitbucket query filter (e.g., `name ~ "feature"`)
- `sort` (optional): Sort field (e.g., `-target.date` for newest first)

## Environment Variables

| Variable | Description |
|----------|-------------|
| `BITBUCKET_USERNAME` | Your Bitbucket username |
| `BITBUCKET_APP_PASSWORD` | Your Bitbucket app password |
| `BITBUCKET_BASE_URL` | API base URL (default: `https://api.bitbucket.org/2.0`) |

**Important:** Bitbucket uses App Passwords (not API tokens). Generate at: https://bitbucket.org/account/settings/app-passwords/

Required permissions:
- Repositories: Read
- Pull requests: Read, Write
- Workspaces: Read (for workspace listing)
