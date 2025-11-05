# CLAUDE.md

## Functional Core - Imperative Shell

We advocate the use of this pattern when writing code for this repo.

The pattern is based on separating code into two distinct layers:

1. Functional Core: Pure, testable business logic free of side effects (no I/O, no external state mutations). It operates only on the data it's given.
2. Imperative Shell: Responsible for side effects like database calls, network requests, and sending emails. It uses the functional core to perform business logic.

Example Transformation

```
# Before (mixed logic and side effects):
function sendUserExpiryEmail(): void {
  for (const user of db.getUsers()) {
    if (user.subscriptionEndDate > Date.now()) continue;
    if (user.isFreeTrial) continue;
    email.send(user.email, "Your account has expired " + user.name + ".");
  }
}
```

After (separated):

- Functional Core:
  - getExpiredUsers(users, cutoff) - pure filtering logic
  - generateExpiryEmails(users) - pure email generation
- Imperative Shell:
  - email.bulkSend(generateExpiryEmails(getExpiredUsers(db.getUsers(), Date.now())))

Benefits

- More testable (core logic can be tested in isolation)
- More maintainable
- More reusable (e.g., easily adding reminder emails by reusing getExpiredUsers)
- More adaptable (imperative shell can be swapped out)

The pattern is based on Gary Bernhardt's original talk on the concept.

## Jira Ticket Management with mcptools

### Searching Jira Tickets

#### Basic Search

To list your current open Jira tickets, use the following command:

```bash
# List open tickets assigned to you
mcptools atlassian jira search "assignee = currentUser() AND status NOT IN (Done, Closed)"

# Optional: Limit the number of results
mcptools atlassian jira search "assignee = currentUser() AND status NOT IN (Done, Closed)" --limit 10
```

#### Pagination with Short Token Hashes

When search results exceed the limit, the CLI will display a compact pagination command:

```bash
# Example output:
# Found 30 issue(s):
# [results...]
# To fetch the next page, run:
#   mcptools atlassian jira search '...' --limit 30 --next-page a1b2c3d4

# Fetch the next page using the 8-character hash
mcptools atlassian jira search 'project = "PROD"' --limit 30 --next-page a1b2c3d4
```

**How Pagination Works:**

- When results are paginated, the full pagination token is stored locally in `~/.config/mcptools/pagination/`
- Instead of showing the full 100+ character token, only an 8-character MD5 hash is displayed
- You can copy and paste the hash directly into the `--next-page` parameter
- The system automatically resolves the hash to the full token when executing the search
- For backward compatibility, you can still pass the full token if needed (useful for scripts or saved commands)

#### Saved Queries

You can save and reuse frequently used search queries:

```bash
# Save a query
mcptools atlassian jira search 'project = "PM" AND "Assigned Guild[Dropdown]" = DevOps' --save --query devops

# Execute a saved query
mcptools atlassian jira search --query devops

# Execute with custom limit
mcptools atlassian jira search --query devops --limit 20

# Paginate through saved query results using the hash
mcptools atlassian jira search --query devops --limit 30 --next-page a1b2c3d4

# Update existing query
mcptools atlassian jira search 'project = "PM" AND status = Open' --save --query devops --update

# List all saved queries
mcptools atlassian jira search --list

# View query contents
mcptools atlassian jira search --load --query devops

# Delete a query
mcptools atlassian jira search --delete --query devops
```

### Retrieving Details of a Specific Ticket

To get detailed information about a specific Jira ticket, use the ticket key:

```bash
# Retrieve details for a specific ticket
mcptools atlassian jira get PROJ-123
```

#### Advanced JQL Query Tips

You can customize your Jira ticket search with advanced JQL queries:

```bash
# Search tickets in a specific project
mcptools atlassian jira search "project = MYPROJECT AND status = 'In Progress'"

# Search tickets with specific labels
mcptools atlassian jira search "labels IN (critical, bug)"

# Combine multiple conditions
mcptools atlassian jira search "assignee = currentUser() AND priority = High AND created >= -1w"
```

**JQL Query Tips:**

- Use `currentUser()` to find tickets assigned to you
- Use `status NOT IN (Done, Closed)` to filter out completed tickets
- Supports time-based queries like `created >= -1w` (tickets created in the last week)
- Can filter by project, status, priority, labels, and more

### Updating Jira Tickets

To update a Jira ticket's fields, use the update command:

```bash
# Update a single field
mcptools atlassian jira update PROJ-123 --status "In Progress"

# Update multiple fields at once
mcptools atlassian jira update PROJ-123 --status "In Progress" --priority "High" --assignee "guzmán@example.com"

# Assign to yourself
mcptools atlassian jira update PROJ-123 --assignee "me"

# Update custom guild and pod fields
mcptools atlassian jira update PROJ-123 --assigned-guild "DevOps" --assigned-pod "Platform"

# Update issue type
mcptools atlassian jira update PROJ-123 --issue-type "Story"

# Output as JSON
mcptools atlassian jira update PROJ-123 --status "Done" --json
```

**Update Options:**

- `--status`: Transition to a new status (e.g., "In Progress", "Done"). Validates against available workflow transitions.
- `--priority`: Set priority (e.g., "High", "Medium", "Low")
- `--issue-type`: Change issue type (e.g., "Story", "Bug", "Epic")
- `--assignee`: Assign to a user. Accepts:
  - Email address: `user@example.com`
  - Display name: `John Doe`
  - Account ID: `5f7a1c2b3d4e5f6a`
  - Special value: `me` (current authenticated user)
- `--assigned-guild`: Set custom guild field
- `--assigned-pod`: Set custom pod field
- `--json`: Output results as JSON format

**Partial Updates:**

The update command supports partial updates - if one field fails to update, others may still succeed. Each field's update result is reported separately.

### Listing Jira Custom Field Values

To discover available values for custom Jira fields, use the fields command:

```bash
# List all custom field values for default project (PROD)
mcptools atlassian jira fields

# List values for a specific project
mcptools atlassian jira fields --project "MYPROJECT"

# List values for a specific field only
mcptools atlassian jira fields --field "assigned-guild"

# List values for a specific field in a specific project
mcptools atlassian jira fields --project "MYPROJECT" --field "assigned-pod"

# Output as JSON
mcptools atlassian jira fields --json
```

**Field Options:**

- `--project`: Project key to query (defaults to "PROD")
- `--field`: Specific field to display. Options:
  - `assigned-guild`: Custom guild assignments
  - `assigned-pod`: Custom pod assignments
  - Omit to show all available fields
- `--json`: Output results as JSON format

**Typical Workflow:**

1. Use `jira fields` to discover available values for guild and pod assignments
2. Use `jira update` to set those fields on your tickets
3. Use `jira search` to find tickets by their current field values
4. Use `jira get` to view detailed ticket information

### Using Jira Commands via MCP Server

Both `jira_update` and `jira_fields` commands are available through the MCP server for integration with Claude and other clients:

**Example MCP Calls:**

Get available field values:
```json
{
  "method": "tools/call",
  "params": {
    "name": "jira_fields",
    "arguments": {"project": "PROD"}
  }
}
```

Update a ticket:
```json
{
  "method": "tools/call",
  "params": {
    "name": "jira_update",
    "arguments": {
      "ticketKey": "PROJ-123",
      "status": "In Progress",
      "assignee": "me",
      "assignedGuild": "DevOps"
    }
  }
}
```

**Environment Variables:**
Ensure you have set the following environment variables before using Jira commands:

- `ATLASSIAN_BASE_URL`
- `ATLASSIAN_EMAIL`
- `ATLASSIAN_API_TOKEN`

See the [Atlassian Configuration](#atlassian-configuration) section for details on setting up these credentials.
