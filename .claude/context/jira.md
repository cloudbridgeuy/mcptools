# Jira Integration

## CLI Commands

### Search

```bash
# List open tickets assigned to you
mcptools atlassian jira search "assignee = currentUser() AND status NOT IN (Done, Closed)"

# With limit
mcptools atlassian jira search "project = PROJ AND status = Open" --limit 20

# Paginate using 8-character hash
mcptools atlassian jira search 'project = "PROD"' --limit 30 --next-page a1b2c3d4

# Output as JSON
mcptools atlassian jira search "project = PROJ" --json
```

### Saved Queries

```bash
# Save a query
mcptools atlassian jira search 'project = "PM" AND status = Open' --save --query devops

# Execute a saved query
mcptools atlassian jira search --query devops

# List all saved queries
mcptools atlassian jira search --list

# View query contents
mcptools atlassian jira search --load --query devops

# Update existing query
mcptools atlassian jira search 'project = "PM"' --save --query devops --update

# Delete a query
mcptools atlassian jira search --delete --query devops
```

### Get Ticket Details

```bash
mcptools atlassian jira get PROJ-123
mcptools atlassian jira get PROJ-123 --json
```

### Create Tickets

```bash
# Simple creation
mcptools atlassian jira create "Fix login bug"

# With all options
mcptools atlassian jira create "Implement feature" \
  --description "Details about the feature" \
  --project PROJ \
  --issue-type Story \
  --priority High \
  --assignee me \
  --assigned-guild DevOps \
  --assigned-pod Platform
```

### Update Tickets

```bash
# Update status
mcptools atlassian jira update PROJ-123 --status "In Progress"

# Assign to yourself
mcptools atlassian jira update PROJ-123 --assignee me

# Update multiple fields
mcptools atlassian jira update PROJ-123 --status Done --priority Low --issue-type Bug
```

### List Field Values

```bash
# List available guild/pod values
mcptools atlassian jira fields

# For a specific project
mcptools atlassian jira fields --project MYPROJECT --field assigned-guild
```

## MCP Tools

### jira_search

```json
{
  "method": "tools/call",
  "params": {
    "name": "jira_search",
    "arguments": {
      "query": "assignee = currentUser() AND status NOT IN (Done, Closed)",
      "limit": 30,
      "nextPageToken": "a1b2c3d4"
    }
  }
}
```

### jira_get

```json
{
  "method": "tools/call",
  "params": {
    "name": "jira_get",
    "arguments": { "issueKey": "PROJ-123" }
  }
}
```

### jira_create

```json
{
  "method": "tools/call",
  "params": {
    "name": "jira_create",
    "arguments": {
      "summary": "Fix login bug",
      "description": "Details...",
      "issueType": "Bug",
      "priority": "High"
    }
  }
}
```

### jira_update

```json
{
  "method": "tools/call",
  "params": {
    "name": "jira_update",
    "arguments": {
      "ticketKey": "PROJ-123",
      "status": "In Progress",
      "assignee": "me"
    }
  }
}
```

### jira_fields

```json
{
  "method": "tools/call",
  "params": {
    "name": "jira_fields",
    "arguments": { "project": "PROD" }
  }
}
```

### Saved Query Tools

- `jira_query_list` - List all saved queries
- `jira_query_save` - Save a query (`name`, `query`, `update`)
- `jira_query_delete` - Delete a query (`name`)
- `jira_query_load` - Load a query (`name`)

## Environment Variables

| Variable | Description | Fallback |
|----------|-------------|----------|
| `JIRA_BASE_URL` | Jira instance URL | `ATLASSIAN_BASE_URL` |
| `JIRA_EMAIL` | Email for Jira auth | `ATLASSIAN_EMAIL` |
| `JIRA_API_TOKEN` | API token for Jira | `ATLASSIAN_API_TOKEN` |

## JQL Query Tips

- `currentUser()` - Your assigned tickets
- `status NOT IN (Done, Closed)` - Filter completed
- `created >= -1w` - Last week
- `priority = High` - Priority filter
- `labels IN (critical, bug)` - Label filter

## Pagination

Pagination tokens are stored in `~/.config/mcptools/pagination/`. The CLI displays 8-character MD5 hashes for convenience. Full tokens also work for backward compatibility.
