# Atlassian Module - Quick Start Guide

## 5-Minute Setup

### 1. Generate Credentials

#### For Jira/Confluence

- Go to https://id.atlassian.com/manage-profile/security/api-tokens
- Click "Create API token"
- Copy the generated token

#### For Bitbucket

- Go to https://bitbucket.org/account/settings/app-passwords/
- Click "Create app password"
- Select permissions: Repositories (Read), Pull requests (Read)
- Copy the generated password

### 2. Set Environment Variables

```bash
# Shared Atlassian credentials (used by Jira and Confluence)
export ATLASSIAN_BASE_URL="https://your-domain.atlassian.net"
export ATLASSIAN_EMAIL="your-email@company.com"
export ATLASSIAN_API_TOKEN="your-api-token-here"

# Service-specific overrides (optional - takes precedence)
# export JIRA_API_TOKEN="your-jira-specific-token"
# export CONFLUENCE_API_TOKEN="your-confluence-specific-token"

# Bitbucket
export BITBUCKET_USERNAME="your-bitbucket-username"
export BITBUCKET_APP_PASSWORD="your-app-password-here"
```

**Note:** Service-specific variables (`JIRA_*`, `CONFLUENCE_*`) override `ATLASSIAN_*` when set.

### 3. Test Configuration

```bash
# Test Jira
mcptools atlassian jira search "project IS NOT EMPTY" --limit 5

# Test Bitbucket
mcptools atlassian bitbucket pr list --repo "your-workspace/your-repo" --limit 5
```

---

## Common Jira Queries

### View Your Assigned Issues

```bash
mcptools atlassian jira search "assignee = currentUser() AND status != Done"
```

### Find Open Issues in a Project

```bash
mcptools atlassian jira search "project = PROJ AND status = Open"
```

### Search Issues by Text

```bash
mcptools atlassian jira search "text ~ 'database' AND type = Bug"
```

### Recently Updated Issues

```bash
mcptools atlassian jira search "updated >= -7d ORDER BY updated DESC"
```

### High Priority Issues

```bash
mcptools atlassian jira search "priority = High AND status NOT IN (Done, Closed)"
```

### Get JSON Output (for scripting)

```bash
mcptools atlassian jira search "project = PROJ" --json | jq '.issues[] | {key, summary, status}'
```

### Save and Reuse Queries

```bash
# Save a query
mcptools atlassian jira search 'project = "PROJ" AND status = Open' --save --query my-open-issues

# Execute saved query
mcptools atlassian jira search --query my-open-issues

# List saved queries
mcptools atlassian jira search --list

# Delete a saved query
mcptools atlassian jira search --delete --query my-open-issues
```

### Create Tickets

```bash
# Create a simple ticket
mcptools atlassian jira create "Fix login bug"

# Create with all options
mcptools atlassian jira create "Implement new feature" \
  --description "Details about the feature" \
  --project PROJ \
  --issue-type Story \
  --priority High \
  --assignee me
```

### Update Tickets

```bash
# Update status
mcptools atlassian jira update PROJ-123 --status "In Progress"

# Assign to yourself
mcptools atlassian jira update PROJ-123 --assignee me

# Update multiple fields
mcptools atlassian jira update PROJ-123 --status Done --priority Low
```

### List Field Values

```bash
# List available guild/pod values
mcptools atlassian jira fields

# For a specific project
mcptools atlassian jira fields --project MYPROJECT
```

---

## Common Confluence Queries

### Find Pages About a Topic

```bash
mcptools atlassian confluence search "text ~ 'deployment'"
```

### Search in a Specific Space

```bash
mcptools atlassian confluence search "space = WIKI AND text ~ 'api'"
```

### Recent Pages

```bash
mcptools atlassian confluence search "lastModified >= -30d ORDER BY lastModified DESC"
```

### Get JSON Output

```bash
mcptools atlassian confluence search "text ~ 'guide'" --json | jq '.pages[] | {title, page_type, url}'
```

### Limit Results

```bash
mcptools atlassian confluence search "type = page" --limit 20
```

---

## Common Bitbucket Queries

### List Open PRs

```bash
mcptools atlassian bitbucket pr list --repo "myworkspace/myrepo"
```

### Filter by State

```bash
# Only open PRs
mcptools atlassian bitbucket pr list --repo "myworkspace/myrepo" --state OPEN

# Merged and declined
mcptools atlassian bitbucket pr list --repo "myworkspace/myrepo" --state MERGED --state DECLINED
```

### Read PR Details

```bash
# Full PR with diff
mcptools atlassian bitbucket pr read --repo "myworkspace/myrepo" 123

# Without diff (faster)
mcptools atlassian bitbucket pr read --repo "myworkspace/myrepo" 123 --no-diff

# Truncate diff to 100 lines
mcptools atlassian bitbucket pr read --repo "myworkspace/myrepo" 123 --line-limit 100
```

### Only Show Diff

```bash
mcptools atlassian bitbucket pr read --repo "myworkspace/myrepo" 123 --diff-only
```

### Get JSON Output

```bash
mcptools atlassian bitbucket pr list --repo "myworkspace/myrepo" --json
```

---

## Tips & Tricks

### Save Results to File

```bash
mcptools atlassian jira search "project = PROJ" --json > issues.json
```

### Filter JSON Results with jq

```bash
# Get only open issues
mcptools atlassian jira search "project = PROJ" --json | jq '.issues[] | select(.status == "Open")'

# Count issues by status
mcptools atlassian jira search "project = PROJ" --json | jq '.issues | group_by(.status) | map({status: .[0].status, count: length})'
```

### Use Verbose Mode for Debugging

```bash
mcptools --verbose atlassian jira search "project = PROJ"
```

### Combine with Other Tools

```bash
# Search and pipe to grep
mcptools atlassian jira search "project = PROJ" | grep -i "database"

# Count results
mcptools atlassian jira search "project = PROJ" --json | jq '.issues | length'
```

---

## CLI Argument Options

### Jira Search Command

```
mcptools atlassian jira search [OPTIONS] <QUERY>

Arguments:
  <QUERY>                    JQL query string

Options:
  -l, --limit <LIMIT>        Max results [default: 10]
  --json                     Output as JSON
  --help                     Show help
```

### Confluence Search Command

```
mcptools atlassian confluence search [OPTIONS] <QUERY>

Arguments:
  <QUERY>                    CQL query string

Options:
  -l, --limit <LIMIT>        Max results [default: 10]
  --json                     Output as JSON
  --help                     Show help
```

### Bitbucket PR List Command

```
mcptools atlassian bitbucket pr list [OPTIONS] --repo <REPO>

Options:
  -r, --repo <REPO>          Repository (workspace/repo_slug)
  --state <STATE>            Filter by state (OPEN, MERGED, DECLINED, SUPERSEDED)
  -l, --limit <LIMIT>        Max results [default: 10]
  --next-page <URL>          Pagination URL
  --json                     Output as JSON
  --help                     Show help
```

### Bitbucket PR Read Command

```
mcptools atlassian bitbucket pr read [OPTIONS] --repo <REPO> <PR_NUMBER>

Arguments:
  <PR_NUMBER>                Pull request number

Options:
  -r, --repo <REPO>          Repository (workspace/repo_slug)
  --no-diff                  Skip fetching diff
  --diff-only                Only show diff content
  --line-limit <N>           Truncate diff to N lines (-1 = no limit)
  -l, --limit <LIMIT>        Max comments [default: 100]
  --diff-limit <LIMIT>       Max diffstat entries [default: 500]
  --help                     Show help
```

### Global Options

```
--atlassian-url <URL>        Override ATLASSIAN_BASE_URL env var
--atlassian-email <EMAIL>    Override ATLASSIAN_EMAIL env var
--atlassian-token <TOKEN>    Override ATLASSIAN_API_TOKEN env var
--verbose                    Enable verbose logging
```

---

## Query Language Reference

### JQL (Jira Query Language)

**Common Fields:**

- `project` - Project key (e.g., `project = PROJ`)
- `status` - Issue status (e.g., `status = Open`)
- `assignee` - Who it's assigned to (e.g., `assignee = currentUser()`)
- `text` - Full text search (e.g., `text ~ 'keyword'`)
- `type` - Issue type (e.g., `type = Bug`)
- `priority` - Priority level (e.g., `priority = High`)
- `updated` - Last update date (e.g., `updated >= -7d`)
- `created` - Creation date (e.g., `created >= -30d`)

**Operators:**

- `=` - Equals
- `!=` - Not equals
- `~` - Contains (text search)
- `>`, `<`, `>=`, `<=` - Comparison
- `IN` - One of multiple values
- `NOT IN` - None of values
- `AND` - Combine conditions
- `OR` - Either condition

### CQL (Confluence Query Language)

**Common Fields:**

- `space` - Space key (e.g., `space = WIKI`)
- `text` - Full text search (e.g., `text ~ 'keyword'`)
- `type` - Page type (e.g., `type = page`)
- `lastModified` - Last modification date (e.g., `lastModified >= -30d`)
- `creator` - Page creator
- `title` - Page title

**Operators:** Similar to JQL

---

## Common Errors & Solutions

| Error                                                  | Cause                      | Solution                                            |
| ------------------------------------------------------ | -------------------------- | --------------------------------------------------- |
| "Neither JIRA_BASE_URL nor ATLASSIAN_BASE_URL..."      | Missing env var            | Set `JIRA_BASE_URL` or `ATLASSIAN_BASE_URL`         |
| "Neither CONFLUENCE_API_TOKEN nor ATLASSIAN_API_TOKEN..."| Missing env var          | Set `CONFLUENCE_API_TOKEN` or `ATLASSIAN_API_TOKEN` |
| "BITBUCKET_USERNAME environment variable not set"      | Missing env var            | `export BITBUCKET_USERNAME="..."`                   |
| "Jira API error [401]"                                 | Invalid credentials        | Check email and API token                           |
| "Jira API error [403]"                                 | Insufficient permissions   | Check user account permissions                      |
| "Jira API error [404]"                                 | Wrong base URL or project  | Verify base URL and project key                     |
| "Bitbucket API error [401]"                            | Invalid credentials        | Check username and app password                     |
| "Repository not found"                                 | Wrong repo format or access| Use `workspace/repo_slug` format                    |
| Connection timeout                                     | Network issue              | Check internet and URL accessibility                |

---

## More Information

For detailed setup instructions, see: [ATLASSIAN_SETUP.md](ATLASSIAN_SETUP.md)

For MCP usage, see: [CLAUDE.md](CLAUDE.md#atlassian-configuration)

For API documentation:

- [Jira Cloud REST API v3](https://developer.atlassian.com/cloud/jira/rest/v3)
- [Confluence Cloud REST API v2](https://developer.atlassian.com/cloud/confluence/rest/v2)
- [JQL Reference](https://support.atlassian.com/jira-software-cloud/docs/advanced-searching-using-jql/)
- [CQL Reference](https://support.atlassian.com/confluence-cloud/docs/advanced-searching-using-cql/)
