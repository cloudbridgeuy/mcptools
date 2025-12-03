# mcptools

Useful MCP Tools to use with LLM Coding Agents

## Overview

`mcptools` is a Model Context Protocol (MCP) server that exposes various tools for LLM agents to interact with external services. Currently provides tools for:

- **Atlassian Jira**: Search, create, update tickets, and manage custom fields (`jira_search`, `jira_create`, `jira_get`, `jira_update`, `jira_fields`)
- **Atlassian Confluence**: Search pages using CQL (`confluence_search`)
- **Atlassian Bitbucket**: List and read pull requests with diff support (`bitbucket_pr_list`, `bitbucket_pr_read`)
- **HackerNews**: Access HN posts, comments, and stories (`hn_read_item`, `hn_list_items`)
- **Web Scraping**: Fetch web pages and convert to Markdown with CSS selector filtering, section extraction, and pagination (`md_fetch`, `md_toc`)

## Installation

```bash
# Build and install to ~/.local/bin
cargo xtask install

# Or specify a custom installation path
cargo xtask install --path /usr/local/bin
```

## Usage

### MCP Server Modes

The MCP server can run in two transport modes:

#### 1. stdio Transport (for local agents)

```bash
mcptools mcp stdio
```

This mode communicates via standard input/output, making it suitable for local LLM agents like Claude Desktop or other MCP clients that spawn server processes.

#### 2. SSE Transport (for web-based agents)

```bash
mcptools mcp sse --port 3000 --host 127.0.0.1
```

This mode runs an HTTP server with Server-Sent Events (SSE) support, suitable for web-based clients.

Endpoints:

- `GET /sse` - SSE endpoint for real-time updates
- `POST /message` - JSON-RPC endpoint for tool calls

### Configuring MCP Clients

#### Claude Desktop Configuration

Add to your Claude Desktop config file (`~/Library/Application Support/Claude/claude_desktop_config.json` on macOS):

```json
{
  "mcpServers": {
    "mcptools": {
      "command": "mcptools",
      "args": ["mcp", "stdio"]
    }
  }
}
```

#### Claude Code Configuration

**Option 1: Using the `claude mcp add` command (recommended)**

```bash
claude mcp add mcptools -- mcptools mcp stdio
```

This automatically adds the server to your Claude Code configuration.

**Option 2: Manual configuration**

Add to your Claude Code config file (`~/Library/Application Support/Claude/claude_code_config.json` on macOS, `%APPDATA%\Claude\claude_code_config.json` on Windows):

```json
{
  "mcpServers": {
    "mcptools": {
      "command": "mcptools",
      "args": ["mcp", "stdio"]
    }
  }
}
```

After adding the configuration, restart Claude Code for the changes to take effect. The mcptools will be available for use in your coding sessions.

#### Generic MCP Client (stdio)

Any MCP client can connect by spawning the process and communicating via JSON-RPC 2.0 over stdio:

```bash
mcptools mcp stdio
```

Send JSON-RPC requests via stdin, receive responses via stdout.

## Available Tools

### Atlassian Tools

**Environment Variables:** Each service supports its own credentials that override the shared `ATLASSIAN_*` variables:
- Jira: `JIRA_BASE_URL`, `JIRA_EMAIL`, `JIRA_API_TOKEN` (fallback: `ATLASSIAN_*`)
- Confluence: `CONFLUENCE_BASE_URL`, `CONFLUENCE_EMAIL`, `CONFLUENCE_API_TOKEN` (fallback: `ATLASSIAN_*`)
- Bitbucket: `BITBUCKET_USERNAME`, `BITBUCKET_APP_PASSWORD`

For detailed setup instructions, see [docs/ATLASSIAN_SETUP.md](docs/ATLASSIAN_SETUP.md). For a quick start guide, see [docs/ATLASSIAN_QUICK_START.md](docs/ATLASSIAN_QUICK_START.md).

#### jira_search

Search Jira issues using JQL (Jira Query Language).

**Parameters:**

- `query` (string) - JQL query to search issues (e.g., `project = PROJ AND status = Open`)
- `queryName` (string) - Name of a saved query to execute instead of providing raw JQL
- `limit` (number, optional) - Maximum results to return (default: 10, max: 100)
- `nextPageToken` (string, optional) - Pagination token for fetching the next page

**Example:**

```json
{
  "method": "tools/call",
  "params": {
    "name": "jira_search",
    "arguments": {
      "query": "assignee = currentUser() AND status NOT IN (Done, Closed)",
      "limit": 20
    }
  }
}
```

#### jira_get

Get detailed information about a specific Jira ticket.

**Parameters:**

- `issueKey` (string, required) - Jira issue key (e.g., `PROJ-123`)

**Example:**

```json
{
  "method": "tools/call",
  "params": {
    "name": "jira_get",
    "arguments": {
      "issueKey": "PROJ-123"
    }
  }
}
```

#### jira_create

Create a new Jira ticket.

**Parameters:**

- `summary` (string, required) - Title/summary of the ticket
- `description` (string, optional) - Description of the ticket
- `project` (string, optional) - Project key (default: PROD)
- `issueType` (string, optional) - Issue type (e.g., Bug, Story, Task)
- `priority` (string, optional) - Priority (e.g., High, Medium, Low)
- `assignee` (string, optional) - Assignee (email, display name, or "me")
- `assignedGuild` (string, optional) - Custom guild field
- `assignedPod` (string, optional) - Custom pod field

**Example:**

```json
{
  "method": "tools/call",
  "params": {
    "name": "jira_create",
    "arguments": {
      "summary": "Fix login bug",
      "description": "Users cannot log in with SSO",
      "issueType": "Bug",
      "priority": "High"
    }
  }
}
```

#### jira_update

Update fields on an existing Jira ticket.

**Parameters:**

- `ticketKey` (string, required) - Ticket key (e.g., PROJ-123)
- `status` (string, optional) - New status (e.g., "In Progress", "Done")
- `priority` (string, optional) - New priority
- `issueType` (string, optional) - New issue type
- `assignee` (string, optional) - New assignee (email, display name, or "me")
- `assignedGuild` (string, optional) - New assigned guild
- `assignedPod` (string, optional) - New assigned pod

**Example:**

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

#### jira_fields

List available values for Jira custom fields.

**Parameters:**

- `project` (string, optional) - Project key (default: PROD)
- `field` (string, optional) - Specific field to display (assigned-guild or assigned-pod)

#### jira_query_list

List all saved Jira queries.

**Parameters:** None

**Example:**

```json
{
  "method": "tools/call",
  "params": {
    "name": "jira_query_list",
    "arguments": {}
  }
}
```

#### jira_query_save

Save a Jira JQL query with a name for later reuse.

**Parameters:**

- `name` (string, required) - Name for the saved query (alphanumeric, hyphens, underscores only)
- `query` (string, required) - JQL query to save
- `update` (boolean, optional) - If true, overwrites an existing query with the same name (default: false)

**Example:**

```json
{
  "method": "tools/call",
  "params": {
    "name": "jira_query_save",
    "arguments": {
      "name": "my-open-bugs",
      "query": "project = PROJ AND type = Bug AND status != Done"
    }
  }
}
```

#### jira_query_delete

Delete a saved Jira query by name.

**Parameters:**

- `name` (string, required) - Name of the saved query to delete

#### jira_query_load

Load and display the contents of a saved Jira query.

**Parameters:**

- `name` (string, required) - Name of the saved query to load

#### confluence_search

Search Confluence pages using CQL (Confluence Query Language).

**Parameters:**

- `query` (string, required) - CQL query to search pages
- `limit` (number, optional) - Maximum results to return (default: 10)

**Example:**

```json
{
  "method": "tools/call",
  "params": {
    "name": "confluence_search",
    "arguments": {
      "query": "space = WIKI AND text ~ 'deployment'",
      "limit": 10
    }
  }
}
```

#### bitbucket_pr_list

List pull requests for a Bitbucket repository.

**Parameters:**

- `repo` (string, required) - Repository in `workspace/repo_slug` format
- `state` (array, optional) - Filter by PR state(s): OPEN, MERGED, DECLINED, SUPERSEDED
- `limit` (number, optional) - Max results per page (default: 10)
- `nextPage` (string, optional) - Pagination URL for fetching the next page

**Example:**

```json
{
  "method": "tools/call",
  "params": {
    "name": "bitbucket_pr_list",
    "arguments": {
      "repo": "myworkspace/myrepo",
      "state": ["OPEN"],
      "limit": 20
    }
  }
}
```

#### bitbucket_pr_read

Read details of a specific Bitbucket pull request including diff, diffstat, and comments.

**Parameters:**

- `repo` (string, required) - Repository in `workspace/repo_slug` format
- `prNumber` (number, required) - Pull request number
- `limit` (number, optional) - Max comments per page (default: 100)
- `diffLimit` (number, optional) - Max diffstat entries per page (default: 500)
- `lineLimit` (number, optional) - Truncate diff to N lines (default: 500, use -1 for unlimited)
- `noDiff` (boolean, optional) - Skip fetching diff content (default: false)

**Example:**

```json
{
  "method": "tools/call",
  "params": {
    "name": "bitbucket_pr_read",
    "arguments": {
      "repo": "myworkspace/myrepo",
      "prNumber": 123,
      "lineLimit": 200
    }
  }
}
```

**Note:** The `lineLimit` parameter defaults to 500 lines to prevent overwhelming responses. Use `lineLimit: -1` for the complete diff.

### HackerNews Tools

#### hn_read_item

Read HackerNews posts and comments with pagination support.

**Parameters:**

- `item` (string, required) - HackerNews item ID (e.g., "8863") or full URL (e.g., "https://news.ycombinator.com/item?id=8863")
- `limit` (number, optional) - Number of comments per page (default: 10)
- `page` (number, optional) - Page number, 1-indexed (default: 1)
- `thread` (string, optional) - Comment thread ID to read a specific comment thread

**Example Usage:**

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "tools/call",
  "params": {
    "name": "hn_read_item",
    "arguments": {
      "item": "8863",
      "limit": 5,
      "page": 1
    }
  }
}
```

**Response:**

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "content": [
      {
        "type": "text",
        "text": "{\"id\":8863,\"title\":\"My YC app: Dropbox...\",\"comments\":[...],\"pagination\":{...}}"
      }
    ]
  }
}
```

#### hn_list_items

List HackerNews stories with pagination support.

**Parameters:**

- `story_type` (string, optional) - Type of stories: "top", "new", "best", "ask", "show", "job" (default: "top")
- `limit` (number, optional) - Number of stories per page (default: 30)
- `page` (number, optional) - Page number, 1-indexed (default: 1)

**Example Usage:**

```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "tools/call",
  "params": {
    "name": "hn_list_items",
    "arguments": {
      "story_type": "top",
      "limit": 10,
      "page": 1
    }
  }
}
```

### Web Scraping Tools

#### md_fetch

Fetch web pages using headless Chrome and convert to Markdown. Supports CSS selector filtering to extract specific page elements, character-based pagination, and section extraction using offsets from `md_toc`.

**Parameters:**

- `url` (string, required) - URL of the web page to fetch
- `timeout` (number, optional) - Timeout in seconds (default: 30)
- `raw_html` (boolean, optional) - Return raw HTML instead of Markdown (default: false)
- `selector` (string, optional) - CSS selector to filter content (e.g., "article", "main", "div.content")
- `strategy` (string, optional) - Selection strategy when multiple elements match: "first", "last", "all", "n" (default: "first")
- `index` (number, optional) - Index for "n" strategy (0-indexed)
- `offset` (number, optional) - Character offset to start from (default: 0). When provided, takes precedence over `page`. Use with values from `md_toc` to extract specific sections
- `limit` (number, optional) - Characters per page for pagination (default: 1000)
- `page` (number, optional) - Page number, 1-indexed (default: 1). Ignored if `offset` is provided

**Example Usage:**

```json
// Basic fetch with default pagination
{
  "jsonrpc": "2.0",
  "id": 3,
  "method": "tools/call",
  "params": {
    "name": "md_fetch",
    "arguments": {
      "url": "https://docs.example.com/guide"
    }
  }
}

// Extract main content only
{
  "jsonrpc": "2.0",
  "id": 4,
  "method": "tools/call",
  "params": {
    "name": "md_fetch",
    "arguments": {
      "url": "https://example.com",
      "selector": "main"
    }
  }
}

// Get all article elements with custom pagination
{
  "jsonrpc": "2.0",
  "id": 5,
  "method": "tools/call",
  "params": {
    "name": "md_fetch",
    "arguments": {
      "url": "https://blog.example.com",
      "selector": "article",
      "strategy": "all",
      "limit": 500,
      "page": 2
    }
  }
}
```

**Response includes pagination metadata:**

```json
{
  "url": "https://example.com",
  "title": "Page Title",
  "content": "...(paginated markdown content)...",
  "pagination": {
    "current_page": 1,
    "total_pages": 23,
    "total_characters": 22161,
    "limit": 1000,
    "has_more": true
  }
}
```

#### md_toc

Extract table of contents from web pages by parsing markdown headings (H1-H6). Returns character offsets and limits for each section, enabling precise section extraction with `md_fetch`. Sections are defined as: heading + all content until the next same-or-higher-level heading.

**Parameters:**

- `url` (string, required) - URL of the web page to fetch
- `timeout` (number, optional) - Timeout in seconds (default: 30)
- `selector` (string, optional) - CSS selector to filter content (e.g., "article", "main")
- `strategy` (string, optional) - Selection strategy when multiple elements match: "first", "last", "all", "n" (default: "first")
- `index` (number, optional) - Index for "n" strategy (0-indexed)
- `output` (string, optional) - Output format: "indented", "markdown", "json" (default: "indented")

**Example Usage:**

```json
// Get table of contents with section offsets
{
  "jsonrpc": "2.0",
  "id": 6,
  "method": "tools/call",
  "params": {
    "name": "md_toc",
    "arguments": {
      "url": "https://docs.example.com/guide"
    }
  }
}
```

**Response:**

```json
{
  "url": "https://docs.example.com/guide",
  "title": "User Guide",
  "entries": [
    {
      "level": 2,
      "text": "Getting Started",
      "char_offset": 0,
      "char_limit": 1234
    },
    {
      "level": 3,
      "text": "Installation",
      "char_offset": 156,
      "char_limit": 580
    },
    {
      "level": 2,
      "text": "Advanced Usage",
      "char_offset": 1234,
      "char_limit": 2000
    }
  ],
  "fetch_time_ms": 1523
}
```

**Workflow: Extract Specific Sections**

```json
// Step 1: Get TOC to find sections
{
  "method": "tools/call",
  "params": {
    "name": "md_toc",
    "arguments": {"url": "https://docs.example.com"}
  }
}

// Step 2: Use char_offset and char_limit to fetch specific section
{
  "method": "tools/call",
  "params": {
    "name": "md_fetch",
    "arguments": {
      "url": "https://docs.example.com",
      "offset": 156,
      "limit": 580
    }
  }
}
// Returns only the "Installation" section
```

## MCP Protocol Implementation

This server implements the Model Context Protocol specification with the following methods:

### initialize

Establishes connection and returns server capabilities.

**Request:**

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "initialize",
  "params": {}
}
```

**Response:**

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "protocolVersion": "2024-11-05",
    "capabilities": {
      "tools": {}
    },
    "serverInfo": {
      "name": "mcptools",
      "version": "0.0.0"
    }
  }
}
```

### tools/list

Returns list of available tools.

**Request:**

```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "tools/list",
  "params": {}
}
```

**Response:**

```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "result": {
    "tools": [
      {
        "name": "hn_read_item",
        "description": "Read a HackerNews post and its comments...",
        "inputSchema": {
          "type": "object",
          "properties": {
            "item": {
              "type": "string",
              "description": "HackerNews item ID or URL"
            },
            "limit": {
              "type": "number",
              "description": "Number of comments per page (default: 10)"
            },
            "page": {
              "type": "number",
              "description": "Page number, 1-indexed (default: 1)"
            },
            "thread": {
              "type": "string",
              "description": "Comment thread ID to read (optional)"
            }
          },
          "required": ["item"]
        }
      }
    ]
  }
}
```

### tools/call

Executes a specific tool.

**Request:**

```json
{
  "jsonrpc": "2.0",
  "id": 3,
  "method": "tools/call",
  "params": {
    "name": "hn_read_item",
    "arguments": {
      "item": "8863",
      "limit": 2
    }
  }
}
```

**Response:**

```json
{
  "jsonrpc": "2.0",
  "id": 3,
  "result": {
    "content": [
      {
        "type": "text",
        "text": "{\"id\":8863,\"title\":\"My YC app: Dropbox - Throw away your USB drive\",\"url\":\"http://www.getdropbox.com/u/2/screencast.html\",\"author\":\"dhouston\",\"score\":104,\"time\":\"2007-04-04 19:16:40 UTC\",\"text\":null,\"total_comments\":71,\"comments\":[{\"id\":9224,\"author\":\"BrandonM\",\"time\":\"2007-04-05 15:16:54 UTC\",\"text\":\"I have a few qualms with this app...\",\"replies_count\":1},{\"id\":8917,\"author\":\"brett\",\"time\":\"2007-04-04 21:48:13 UTC\",\"text\":\"This is genius...\",\"replies_count\":0}],\"pagination\":{\"current_page\":1,\"total_pages\":17,\"total_comments\":33,\"limit\":2,\"next_page_command\":\"mcptools hn read 8863 --page 2\",\"prev_page_command\":null}}"
      }
    ]
  }
}
```

## Testing with curl (SSE mode)

Start the server:

```bash
mcptools mcp sse --port 3000
```

Test the endpoints:

```bash
# Initialize
curl -X POST http://127.0.0.1:3000/message \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}'

# List tools
curl -X POST http://127.0.0.1:3000/message \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}'

# Call hn_read_item tool
curl -X POST http://127.0.0.1:3000/message \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"hn_read_item","arguments":{"item":"8863","limit":2}}}'
```

## CLI Usage (Non-MCP)

You can also use the tools directly via CLI without running an MCP server.

### Atlassian

#### Jira

```bash
# Search for issues assigned to you
mcptools atlassian jira search "assignee = currentUser() AND status NOT IN (Done, Closed)"

# Search with limit
mcptools atlassian jira search "project = PROJ AND status = Open" --limit 20

# Get ticket details
mcptools atlassian jira get PROJ-123

# Create a new ticket
mcptools atlassian jira create --summary "Fix bug" --description "Details..." --issue-type Bug

# Update a ticket
mcptools atlassian jira update PROJ-123 --status "In Progress" --assignee me

# List custom field values
mcptools atlassian jira fields --project PROJ

# Output as JSON
mcptools atlassian jira search "project = PROJ" --json
```

#### Confluence

```bash
# Search for pages
mcptools atlassian confluence search "text ~ 'deployment'"

# Search in a specific space
mcptools atlassian confluence search "space = WIKI AND text ~ 'api'" --limit 10

# Output as JSON
mcptools atlassian confluence search "text ~ 'guide'" --json
```

#### Bitbucket Pull Requests

```bash
# List open PRs in a repository
mcptools atlassian bitbucket pr list --repo "myworkspace/myrepo"

# Filter by state
mcptools atlassian bitbucket pr list --repo "myworkspace/myrepo" --state OPEN --state MERGED

# Read PR details with diff
mcptools atlassian bitbucket pr read --repo "myworkspace/myrepo" 123

# Skip diff content
mcptools atlassian bitbucket pr read --repo "myworkspace/myrepo" 123 --no-diff

# Limit diff output to 200 lines
mcptools atlassian bitbucket pr read --repo "myworkspace/myrepo" 123 --line-limit 200

# Only show diff (skip details and comments)
mcptools atlassian bitbucket pr read --repo "myworkspace/myrepo" 123 --diff-only
```

### HackerNews (hn)

```bash
# Read a post with default settings (10 comments)
mcptools hn read 8863

# Read with custom limit
mcptools hn read 8863 --limit 5

# Navigate to specific page
mcptools hn read 8863 --page 2

# Read specific comment thread
mcptools hn read 8863 --thread 9224

# Output as JSON
mcptools hn read 8863 --json

# Use full URL instead of ID
mcptools hn read "https://news.ycombinator.com/item?id=8863"

# List stories
mcptools hn list --story-type top --limit 20
```

### Web Scraping (md)

#### md fetch - Fetch and convert web pages

```bash
# Basic fetch (converts to Markdown)
mcptools md fetch https://example.com

# With CSS selector to extract specific content
mcptools md fetch https://docs.example.com --selector "main"
mcptools md fetch https://example.com --selector "article.post"

# With pagination (1000 characters per page by default)
mcptools md fetch https://example.com --page 2
mcptools md fetch https://example.com --limit 500 --page 1

# Extract specific section using offset (from md toc)
mcptools md fetch https://docs.example.com --offset 1234 --limit 580

# Selection strategies for multiple matches
mcptools md fetch https://example.com --selector "article" --strategy all
mcptools md fetch https://example.com --selector "article" --strategy last
mcptools md fetch https://example.com --selector "p" --strategy n --index 2

# Get raw HTML instead of Markdown
mcptools md fetch https://example.com --raw-html

# Output as JSON
mcptools md fetch https://example.com --selector "main" --json

# Combine features
mcptools md fetch https://docs.example.com --selector "main" --limit 1000 --page 1 --json
```

#### md toc - Extract table of contents

```bash
# Get TOC with default indented format (shows offset/limit for each section)
mcptools md toc https://docs.example.com

# Get TOC as markdown list
mcptools md toc https://example.com --output markdown

# Get TOC as JSON (includes char_offset and char_limit)
mcptools md toc https://example.com --json

# Extract TOC from specific page section
mcptools md toc https://docs.example.com --selector "main"
mcptools md toc https://example.com --selector "article" --strategy first

# Complete workflow: Get TOC, then fetch specific section
mcptools md toc https://docs.example.com --json > toc.json
# Use char_offset and char_limit from toc.json
mcptools md fetch https://docs.example.com --offset 1234 --limit 580
```

## Best Practices for Web Fetching with Claude Code

When using `md_toc` and `md_fetch` with Claude Code or other LLM coding agents, follow this workflow to efficiently extract web content:

### Two-Step Fetching Workflow

**Step 1: Understand page structure with `md_toc`**

Always start by fetching the table of contents to understand the page layout and identify which sections contain the information you need:

```bash
mcptools md toc https://docs.example.com
```

This returns a hierarchical list of sections with character offsets and limits, allowing you to precisely target content.

**Step 2: Fetch targeted content with `md_fetch`**

After analyzing the TOC, use character offsets to extract specific sections:

```bash
# Fetch a specific section using offset and limit from md_toc
mcptools md fetch https://docs.example.com --offset 1234 --limit 580
```

Or use CSS selectors to filter content:

```bash
# Extract main content only
mcptools md fetch https://docs.example.com --selector "main"
```

### Best Practices

1. **Always use `md_toc` first** - This helps you understand page structure before fetching, reducing unnecessary content extraction
2. **Use CSS selectors** - Narrow down to specific page sections (e.g., `main`, `article`, `div.content`) to avoid noise from sidebars and navigation
3. **Leverage pagination** - For large pages, use the `limit` and `page` parameters or character offsets to fetch content in manageable chunks
4. **Site-specific selectors**:
   - **LocalStack Documentation** (`https://docs.localstack.cloud/*`): Use `selector: "main"` to get main content
5. **Combine tools efficiently** - Use `md_toc` to get metadata, then `md_fetch` with precise offsets for targeted extraction

### Example Workflow

```bash
# Step 1: Get page structure
mcptools md toc https://docs.example.com --json

# Step 2: Use the returned metadata to fetch specific sections
# If the JSON shows an "Installation" section with char_offset: 156, char_limit: 580
mcptools md fetch https://docs.example.com --offset 156 --limit 580

# Alternative: Use CSS selectors for simpler cases
mcptools md fetch https://docs.example.com --selector "main" --limit 1000
```

This approach ensures Claude Code gets focused, relevant content without unnecessary overhead.

### Upgrade

Upgrade mcptools to the latest version.

```bash
# Upgrade to the latest version
mcptools upgrade

# Force upgrade even if already on latest
mcptools upgrade --force
```

## Development

```bash
# Build
cargo build

# Run tests
cargo test

# Run clippy
cargo clippy

# Watch mode (requires bacon)
bacon
```

## License

MIT
