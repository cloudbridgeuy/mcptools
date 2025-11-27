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

The `jira_search`, `jira_update`, and `jira_fields` commands are available through the MCP server for integration with Claude and other clients:

#### Search Tickets via MCP

Search for Jira tickets with optional pagination:

```json
{
  "method": "tools/call",
  "params": {
    "name": "jira_search",
    "arguments": {
      "query": "assignee = currentUser() AND status NOT IN (Done, Closed)",
      "limit": 30
    }
  }
}
```

Fetch the next page using a pagination token hash (8 characters):

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

Or with a full pagination token (for backward compatibility):

```json
{
  "method": "tools/call",
  "params": {
    "name": "jira_search",
    "arguments": {
      "query": "assignee = currentUser() AND status NOT IN (Done, Closed)",
      "limit": 30,
      "nextPageToken": "Ch0jU3RyaW5nJlVGSlBSQT09JUludCZNell4TURNPRAeGILa5q2lMyJZ..."
    }
  }
}
```

**Search Arguments:**

- `query` (required): JQL query string
- `limit` (optional): Number of results per page (default: 10, max: 100)
- `nextPageToken` (optional): Pagination token (either 8-char hash or full token)

**Response Format:**

The response includes:
- `issues`: Array of issue objects with `key`, `summary`, `status`, `assignee`, `description`
- `nextPageToken`: 8-character hash for fetching the next page (if more results exist)
- `totalIssuesCount`: Total number of issues matching the query

#### Get Available Field Values via MCP

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

#### Update a Ticket via MCP

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

Jira supports service-specific environment variables that take precedence over the shared Atlassian credentials:

| Variable | Description | Fallback |
|----------|-------------|----------|
| `JIRA_BASE_URL` | Jira instance URL | `ATLASSIAN_BASE_URL` |
| `JIRA_EMAIL` | Email for Jira auth | `ATLASSIAN_EMAIL` |
| `JIRA_API_TOKEN` | API token for Jira | `ATLASSIAN_API_TOKEN` |

**Note:** Atlassian may require different API tokens for different services (Jira vs Confluence). Use service-specific variables when needed, or set only `ATLASSIAN_*` variables if using the same credentials for all services.

See the [Atlassian Configuration](docs/ATLASSIAN_SETUP.md) guide for detailed setup instructions.

## Bitbucket Pull Request Management with mcptools

### Listing Pull Requests

#### Basic Usage

To list pull requests in a Bitbucket repository:

```bash
# List all open PRs (default)
mcptools atlassian bitbucket pr list --repo "myworkspace/myrepo"

# Filter by state (can be repeated)
mcptools atlassian bitbucket pr list --repo "myworkspace/myrepo" --state OPEN
mcptools atlassian bitbucket pr list --repo "myworkspace/myrepo" --state MERGED --state DECLINED

# Limit results
mcptools atlassian bitbucket pr list --repo "myworkspace/myrepo" --limit 20

# Output as JSON
mcptools atlassian bitbucket pr list --repo "myworkspace/myrepo" --json
```

**Available States:**
- `OPEN`: Open pull requests
- `MERGED`: Merged pull requests
- `DECLINED`: Declined pull requests
- `SUPERSEDED`: Superseded pull requests

#### Pagination

When results exceed the limit, use the pagination URL:

```bash
# Fetch the next page using the URL from previous response
mcptools atlassian bitbucket pr list --repo "myworkspace/myrepo" --next-page "https://api.bitbucket.org/2.0/repositories/..."
```

### Reading Pull Request Details

To read detailed information about a specific pull request:

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

### Using Bitbucket Commands via MCP Server

The `bitbucket_pr_list` and `bitbucket_pr_read` commands are available through the MCP server:

#### List PRs via MCP

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

**Response includes:**
- `pull_requests`: Array of PR objects with `id`, `title`, `author`, `state`, `source_branch`, `destination_branch`
- `next_page`: URL for fetching additional results (if available)
- `total_count`: Total number of PRs (if available)

#### Read PR Details via MCP

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

**Line Limit Behavior:**

The MCP tool defaults to 500 lines for the diff to prevent overwhelming responses. To get more lines:
- Set `lineLimit` to a higher value (e.g., 1000)
- Set `lineLimit` to -1 for the complete diff

When truncated, the response includes a message indicating how to fetch more.

**Response includes:**
- `id`, `title`, `description`, `state`, `author`
- `source_branch`, `destination_branch`, `source_repo`, `destination_repo`
- `source_commit`, `destination_commit`
- `created_on`, `updated_on`
- `reviewers`, `approvals`
- `html_link`: URL to view PR in browser
- `diffstat`: File changes summary
- `diff_content`: The actual diff (unless `noDiff` is true)
- `comments`: Array of PR comments

**Environment Variables:**
Ensure you have set the following environment variables:

- `BITBUCKET_USERNAME`: Your Bitbucket username
- `BITBUCKET_APP_PASSWORD`: Your Bitbucket app password
