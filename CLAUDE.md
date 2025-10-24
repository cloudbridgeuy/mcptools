# CLAUDE.md

[... previous content remains the same ...]

## Jira Ticket Management with mcptools

### Listing Your Jira Tickets

To list your current open Jira tickets, use the following command:

```bash
# List open tickets assigned to you
mcptools atlassian jira search "assignee = currentUser() AND status NOT IN (Done, Closed)"

# Optional: Limit the number of results
mcptools atlassian jira search "assignee = currentUser() AND status NOT IN (Done, Closed)" --limit 10
```

### Retrieving Details of a Specific Ticket

To get detailed information about a specific Jira ticket, use the ticket key:

```bash
# Retrieve details for a specific ticket
mcptools atlassian jira get PROJ-123
```

### Customizing Search Queries

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

**Environment Variables:**
Ensure you have set the following environment variables before using Jira commands:
- `ATLASSIAN_BASE_URL`
- `ATLASSIAN_EMAIL`
- `ATLASSIAN_API_TOKEN`

See the [Atlassian Configuration](#atlassian-configuration) section for details on setting up these credentials.