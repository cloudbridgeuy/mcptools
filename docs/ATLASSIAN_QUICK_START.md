# Atlassian Module - Quick Start Guide

## 5-Minute Setup

### 1. Generate API Token
- Go to https://id.atlassian.com/manage-profile/security/api-tokens
- Click "Create API token"
- Copy the generated token

### 2. Set Environment Variables
```bash
export ATLASSIAN_BASE_URL="https://your-domain.atlassian.net"
export ATLASSIAN_EMAIL="your-email@company.com"
export ATLASSIAN_API_TOKEN="your-api-token-here"
```

### 3. Test Configuration
```bash
mcptools atlassian jira search "project IS NOT EMPTY" --limit 5
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

| Error | Cause | Solution |
|-------|-------|----------|
| "ATLASSIAN_BASE_URL environment variable not set" | Missing env var | `export ATLASSIAN_BASE_URL="..."`  |
| "Jira API error [401]" | Invalid credentials | Check email and API token |
| "Jira API error [403]" | Insufficient permissions | Check user account permissions |
| "Jira API error [404]" | Wrong base URL or project | Verify base URL and project key |
| Connection timeout | Network issue | Check internet and URL accessibility |

---

## More Information

For detailed setup instructions, see: [ATLASSIAN_SETUP.md](ATLASSIAN_SETUP.md)

For MCP usage, see: [CLAUDE.md](CLAUDE.md#atlassian-configuration)

For API documentation:
- [Jira Cloud REST API v3](https://developer.atlassian.com/cloud/jira/rest/v3)
- [Confluence Cloud REST API v2](https://developer.atlassian.com/cloud/confluence/rest/v2)
- [JQL Reference](https://support.atlassian.com/jira-software-cloud/docs/advanced-searching-using-jql/)
- [CQL Reference](https://support.atlassian.com/confluence-cloud/docs/advanced-searching-using-cql/)
