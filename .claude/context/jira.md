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
mcptools atlassian jira get PROJ-123          # alias: `jira read`
mcptools atlassian jira get PROJ-123 --json
```

Ticket output includes labels, component names, due date, and the parent issue key
when present. The same output is returned after creation.

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
  --label backend --label api \
  --component Backend \
  --due-date 2026-10-31 \
  --parent PROJ-100
```

### Update Tickets

```bash
# Update status
mcptools atlassian jira update PROJ-123 --status "In Progress"

# Assign to yourself
mcptools atlassian jira update PROJ-123 --assignee me

# Update description (markdown supported)
mcptools atlassian jira update PROJ-123 -d "## Summary\nFixed the **login** issue"

# Update multiple fields
mcptools atlassian jira update PROJ-123 --status Done --priority Low --issue-type Bug

mcptools atlassian jira update PROJ-123 --label backend --label api --component Backend --due-date 2026-10-31 --parent PROJ-100
mcptools atlassian jira update PROJ-123 --clear-labels --clear-components --clear-due-date --clear-parent
```

`--label` and `--component` are repeatable. On update, they replace the complete
sets, not append. Clear flags remove the corresponding values. Setting and
clearing the same field is rejected before HTTP requests. Due dates must be valid
calendar dates in `YYYY-MM-DD` format. Blank labels, component names, and parent
keys are rejected. Component values are names; parent values are issue keys.

Jira create screens, edit metadata, and project configuration determine which
fields and parent relationships are permitted. Jira field errors are returned;
no special epic-parent behavior is assumed. Parent clearing uses the documented
`{"update":{"parent":[{"set":{"none":true}}]}}` operation for standard issue types; setting a
parent uses `fields.parent: {"key": "PROJ-100"}`.

MCP update results report per-field failures and `partial_failure`. CLI JSON
updates return an error if any field fails, instead of silently returning the
unchanged ticket.

### Comments

```bash
# Add a comment
mcptools atlassian jira comment add PROJ-123 "This is my comment"

# Add a comment with markdown formatting
mcptools atlassian jira comment add PROJ-123 "## Status Update\nFixed the **login** issue"

# List all comments on a ticket
mcptools atlassian jira comment list PROJ-123
mcptools atlassian jira comment list PROJ-123 --json

# Update a comment by ID
mcptools atlassian jira comment update PROJ-123 12345 "Updated comment body"

# Delete a comment by ID
mcptools atlassian jira comment delete PROJ-123 12345
```

### Transitions

```bash
mcptools atlassian jira transitions list PROJ-123
mcptools atlassian jira transitions list PROJ-123 --json
```

Use the returned target statuses with `jira update --status`.

### Issue Links

```bash
mcptools atlassian jira link types
mcptools atlassian jira link list PROJ-123
mcptools atlassian jira link add PROJ-123 --type Blocks PROJ-456
mcptools atlassian jira link delete 10001
```

`link add FROM --type NAME TO` means `FROM <outward relation> TO`. For the
`Blocks` type, `PROJ-123 --type Blocks PROJ-456` means PROJ-123 blocks
PROJ-456. Jira's request field names are counterintuitive: FROM maps to
`inwardIssue` and TO maps to `outwardIssue`. Every subcommand accepts `--json`.

### Attachments

```bash
# List attachments on a ticket
mcptools atlassian jira attachment list PROJ-123
mcptools atlassian jira attachment list PROJ-123 --json

# Download an attachment by ID
mcptools atlassian jira attachment download PROJ-123 12345
mcptools atlassian jira attachment download PROJ-123 12345 --output ./file.pdf

# Upload files as attachments
mcptools atlassian jira attachment upload PROJ-123 report.pdf screenshot.png
```

### Boards

```bash
mcptools atlassian jira board list
mcptools atlassian jira board list --project PROJ --name Delivery --type scrum
mcptools atlassian jira board list --json
```

Board listing follows all Jira pages. `--project` accepts a project key or ID, `--name` is a partial name filter, and `--type` accepts `scrum` or `kanban`. Use the returned board ID with sprint commands.

### Sprints

```bash
# List sprints on a board (active + future by default)
mcptools atlassian jira sprint list --board 1

# List all sprint states
mcptools atlassian jira sprint list --board 1 --state active,future,closed

# Output as JSON
mcptools atlassian jira sprint list --board 1 --json

# Assign a ticket to a sprint (update)
mcptools atlassian jira update PROJ-123 --sprint "Sprint 30" --board 1

# Create a ticket and assign to a sprint
mcptools atlassian jira create "New task" --project PROJ --sprint "Sprint 30" --board 1
```

The `--board` flag can be omitted if `JIRA_BOARD_ID` is set.

### Markdown Description Support

The `--description` flag on `create` and `update` accepts markdown and converts it to Atlassian Document Format (ADF). Supported elements:

- Paragraphs (blank-line separated)
- Headings (`# H1` through `###### H6`)
- **Bold** (`**text**`), *italic* (`*text*`), `inline code`
- Links (`[text](url)`)
- Bullet lists (`- item`, `* item`)
- Ordered lists (`1. item`)
- Code blocks (triple backticks with optional language)

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
      "allowWrites": true,
      "summary": "Fix login bug",
      "description": "Details...",
      "issueType": "Bug",
      "priority": "High",
      "labels": ["backend", "api"],
      "components": ["Backend"],
      "dueDate": "2026-10-31",
      "parent": "PROJ-100"
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
      "allowWrites": true,
      "ticketKey": "PROJ-123",
      "status": "In Progress",
      "assignee": "me",
      "labels": ["backend", "api"],
      "components": ["Backend"],
      "dueDate": "2026-10-31",
      "parent": "PROJ-100"
    }
  }
}
```

Both mutation tools retain `allowWrites: true` gating. Update `labels` and
`components` replace the complete sets; empty arrays clear them. Omitted fields
remain unchanged. Use `clearLabels`, `clearComponents`, `clearDueDate`, or
`clearParent` with `true` to clear values; do not also set that field. For example:

```json
{
  "allowWrites": true,
  "ticketKey": "PROJ-123",
  "clearLabels": true,
  "clearComponents": true,
  "clearDueDate": true,
  "clearParent": true
}
```

### jira_comment_add

```json
{
  "method": "tools/call",
  "params": {
    "name": "jira_comment_add",
    "arguments": {
      "allowWrites": true,
      "issueKey": "PROJ-123",
      "comment": "This is my comment with **markdown** support"
    }
  }
}
```

### jira_comment_list

```json
{
  "method": "tools/call",
  "params": {
    "name": "jira_comment_list",
    "arguments": { "issueKey": "PROJ-123" }
  }
}
```

### jira_comment_update

```json
{
  "method": "tools/call",
  "params": {
    "name": "jira_comment_update",
    "arguments": {
      "allowWrites": true,
      "issueKey": "PROJ-123",
      "commentId": "12345",
      "comment": "Updated comment body"
    }
  }
}
```

### jira_comment_delete

```json
{
  "method": "tools/call",
  "params": {
    "name": "jira_comment_delete",
    "arguments": {
      "allowWrites": true,
      "issueKey": "PROJ-123",
      "commentId": "12345"
    }
  }
}
```

### jira_transition_list

```json
{
  "method": "tools/call",
  "params": {
    "name": "jira_transition_list",
    "arguments": { "issueKey": "PROJ-123" }
  }
}
```

### jira_link_list and jira_link_type_list

```json
{
  "method": "tools/call",
  "params": {
    "name": "jira_link_list",
    "arguments": { "issueKey": "PROJ-123" }
  }
}
```

`jira_link_type_list` accepts an empty arguments object and returns each type's
name plus its inward and outward relation text.

### jira_link_create and jira_link_delete

```json
{
  "method": "tools/call",
  "params": {
    "name": "jira_link_create",
    "arguments": {
      "allowWrites": true,
      "fromIssue": "PROJ-123",
      "linkType": "Blocks",
      "toIssue": "PROJ-456"
    }
  }
}
```

The user-facing statement is `fromIssue <outward relation> toIssue`. Jira's
request maps `fromIssue` to `inwardIssue` and `toIssue` to `outwardIssue`.
`jira_link_delete` takes `linkId` from `jira_link_list` and also requires
`allowWrites: true`.

### jira_board_list

```json
{
  "method": "tools/call",
  "params": {
    "name": "jira_board_list",
    "arguments": {
      "project": "PROJ",
      "name": "Delivery",
      "type": "scrum"
    }
  }
}
```

This read-only tool follows all pages and returns board IDs, names, types, and project location fields. All filters are optional.

### jira_sprint_list

```json
{
  "method": "tools/call",
  "params": {
    "name": "jira_sprint_list",
    "arguments": {
      "boardId": 1,
      "state": "active,future"
    }
  }
}
```

### jira_attachment_list

```json
{
  "method": "tools/call",
  "params": {
    "name": "jira_attachment_list",
    "arguments": { "issueKey": "PROJ-123" }
  }
}
```

### jira_attachment_download

```json
{
  "method": "tools/call",
  "params": {
    "name": "jira_attachment_download",
    "arguments": {
      "issueKey": "PROJ-123",
      "attachmentId": "12345",
      "outputPath": "/tmp/file.pdf"
    }
  }
}
```

### jira_attachment_upload

```json
{
  "method": "tools/call",
  "params": {
    "name": "jira_attachment_upload",
    "arguments": {
      "allowWrites": true,
      "issueKey": "PROJ-123",
      "filePaths": ["/path/to/report.pdf", "/path/to/screenshot.png"]
    }
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
| `JIRA_BOARD_ID` | Default board ID for sprint operations | None |

## JQL Query Tips

- `currentUser()` - Your assigned tickets
- `status NOT IN (Done, Closed)` - Filter completed
- `created >= -1w` - Last week
- `priority = High` - Priority filter
- `labels IN (critical, bug)` - Label filter

## Pagination

Pagination tokens are stored in `~/.config/mcptools/pagination/`. The CLI displays 8-character MD5 hashes for convenience. Full tokens also work for backward compatibility.
