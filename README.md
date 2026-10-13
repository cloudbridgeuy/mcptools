# mcptools

Useful MCP Tools to use with LLM Coding Agents

## Overview

`mcptools` is a Model Context Protocol (MCP) server that exposes various tools for LLM agents to interact with external services. Currently provides tools for:

- **Atlassian Jira**: Search, create, update, inspect transitions, and manage issue links and saved queries (`jira_search`, `jira_create`, `jira_get`, `jira_update`, `jira_transition_list`, `jira_link_list`, `jira_link_type_list`, `jira_link_create`, `jira_link_delete`)
- **Atlassian Confluence**: Search, read, create, and update pages (`confluence_search`, `confluence_page_get`, `confluence_page_create`, `confluence_page_update`)
- **Atlassian Bitbucket**: List and read pull requests with diff support; add, list, update, delete, resolve, and reopen comments; create and delete repository branches. See [Bitbucket](.claude/context/bitbucket.md).
- **Linear**: Read a bounded issue dependency graph and ready-to-start frontier (`linear_issue_graph`); see [Linear](.claude/context/linear.md) for all Linear tools.
- **Lanes**: Read Git worktrees, assess cleanup eligibility, and create guarded Worktrunk lanes (`lane_list`, `lane_cleanup_plan`, `lane_create`). Disabled until the server operator sets `MCPTOOLS_LANE_REPOS`. Plans are advisory, not removal permission. No deletion, cleanup execution, merge, or push. See [Lane security and contracts](.claude/context/lanes.md).
- **HackerNews**: Access HN posts, comments, and stories (`hn_read_item`, `hn_list_items`)
- **Web Scraping**: Fetch web pages and convert to Markdown with CSS selector filtering, section extraction, and pagination (`md_fetch`, `md_toc`)
- **PDF Navigation**: Parse PDF documents into navigable trees, read sections, peek at content, and extract images (`pdf_toc`, `pdf_read`, `pdf_peek`, `pdf_images`, `pdf_image`, `pdf_info`)
- **Images**: Generate, edit, and vary images with ChatGPT Images 2.5 via subscription or API key (`images_generate`, `images_edit`, `images_vary`)
- **UI Annotations**: Query and manage UI annotations from a calendsync dev server (`ui_annotations_list`, `ui_annotations_get`, `ui_annotations_resolve`, `ui_annotations_clear`)

## Installation

```bash
# Build and install to ~/.local/bin
cargo xtask install

# Or specify a custom installation path
cargo xtask install --path /usr/local/bin
```

## Usage

### MCP transports

Local agents normally spawn `mcptools mcp stdio` and exchange JSON-RPC over standard input and output. Web clients can instead run `mcptools mcp sse --port 3000 --host 127.0.0.1`; its endpoints are `GET /sse` and `POST /message`.

### How agents invoke mcptools

The stdio server has three invocation modes:

| Mode | Start command or environment | `tools/list` returns | Agent workflow |
|------|------------------------------|----------------------|----------------|
| Plain | `mcptools mcp stdio` | Every registered tool | Call a listed tool directly with MCP `tools/call`. |
| Discovery | `mcptools mcp stdio --discovery` or `MCPTOOLS_DISCOVERY=true` | `find_tools`, `call_tool` | Find declarations for the task, then invoke one catalog tool through `call_tool`. |
| Code | `mcptools mcp stdio --code-mode` or `MCPTOOLS_CODE_MODE=true` | `find_tools`, `execute`, `call_tool` | Use `call_tool` for one catalog call or `execute` to chain calls in one round trip. |

Tool definitions in `tools/list` can consume model context on every turn, depending on how the host presents them. Plain mode avoids discovery indirection but repeatedly exposes every schema. Discovery and code mode reduce that recurring definition cost in exchange for lookup and wrapper calls; code mode can also keep intermediate results inside one `execute` call. The repository has no measured break-even tool count.

`find_tools` ranking uses the Jev classifier when `JEV_PROVIDER` is set (e.g. `opencode` with `OPENCODE_API_KEY` in the process environment); otherwise a local ranker applies. Keys stay in the environment and are never written to config files.

#### Per-call permissions

Each outer invocation must opt in to side effects. Write tools require `allowWrites: true`; Spend tools require `allowSpend: true`; Read tools require neither. Omitted or false permission is denied before the target tool validates its input or performs I/O.

| Invocation path | Where permission goes |
|-----------------|-----------------------|
| Direct MCP `tools/call` | In the tool `arguments`, beside its normal input fields. |
| `call_tool` | On the `call_tool` arguments, beside `name` and `input`, not inside `input`. |
| `execute` | On the `execute` arguments, beside `code`; the flag binds that class of tools into this sandbox only. |

Each `execute` call gets a fresh JavaScript sandbox. Its JavaScript has no direct filesystem, network, module-loading, process, or environment access; external operations are available only through bound mcptools functions. The defaults are a 30-second timeout, 64 MiB memory cap, and 256 KiB combined log/result cap. Configure them with `--execute-timeout-secs`, `--execute-memory-mb`, and `--execute-output-kb`, or their `MCPTOOLS_EXECUTE_*` environment variables.

#### Host wiring

`mcptools agent setup` supports plain and code mode. Plain is the default; add `--mode code` to wire `mcp stdio --code-mode`. Discovery mode must be set directly in the host command or environment.

| Host | Setup command | MCP registration | Installed skill |
|------|---------------|------------------|-----------------|
| Claude | `mcptools agent setup --target claude` | Merges the `mcptools` entry into `~/.claude.json`. | `~/.claude/skills/mcptools/SKILL.md` |
| Codex | `mcptools agent setup --target codex` | Runs `codex mcp add mcptools ...`. | `~/.codex/skills/mcptools/SKILL.md` |
| OpenCode | `mcptools agent setup --target opencode` | Merges the `mcptools` entry into `~/.config/opencode/opencode.json`. | `~/.config/opencode/skills/mcptools/SKILL.md` |

Use `--target all` to configure all three supported hosts. With `--mode code`, setup also writes `JEV_PROVIDER=opencode` for each client.

To register the server manually instead, add it with the Claude Code CLI:

```bash
claude mcp add mcptools -- mcptools mcp stdio
```

For Claude Desktop, add it to the config file (`~/Library/Application Support/Claude/claude_desktop_config.json` on macOS):

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

Append `--discovery` or `--code-mode` to the args to select another mode. See the [MCP server reference](.claude/context/mcp-server.md) for complete configuration, discovery, ranking, sandbox result, and nested execution details.

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

**Example:**

```json
{
  "method": "tools/call",
  "params": {
    "name": "jira_create",
    "arguments": {
      "allowWrites": true,
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

**Example:**

```json
{
  "method": "tools/call",
  "params": {
    "name": "jira_update",
    "arguments": {
      "allowWrites": true,
      "ticketKey": "PROJ-123",
      "status": "In Progress",
      "assignee": "me"
    }
  }
}
```

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
      "allowWrites": true,
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

#### confluence_page_get

Read one Confluence page by numeric ID or by a URL containing its page ID. Returns the title, space ID, version, Markdown body, and web URL.

**Parameters:**

- `idOrUrl` (string, required) - Numeric page ID or Confluence page URL

**Example:**

```json
{
  "method": "tools/call",
  "params": {
    "name": "confluence_page_get",
    "arguments": {
      "idOrUrl": "https://example.atlassian.net/wiki/spaces/ENG/pages/123456/Runbook"
    }
  }
}
```

#### confluence_page_create

Create a published Confluence page from storage-format XHTML. The optional parent accepts a numeric page ID or page URL. Requires `allowWrites`.

**Parameters:**

- `spaceId` (string, required) - Numeric Confluence space ID
- `title` (string, required) - Page title
- `body` (string, required) - Confluence storage-format XHTML
- `parentIdOrUrl` (string, optional) - Parent page ID or URL

#### confluence_page_update

Update a page title, storage-format XHTML body, or both. The tool reads the current page, preserves omitted content, and increments the version. Requires `allowWrites`.

**Parameters:**

- `idOrUrl` (string, required) - Numeric page ID or Confluence page URL
- `title` (string, optional) - Replacement title
- `body` (string, optional) - Replacement Confluence storage-format XHTML

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

### UI Annotation Tools

**Environment Variable:** `CALENDSYNC_DEV_URL` — Base URL of the calendsync dev server (default: `http://localhost:3000`)

#### ui_annotations_list

List all UI annotations from the calendsync dev server. Returns selector, component name, note, and resolution status for each annotation.

**Parameters:**

- `url` (string, optional) — Dev server URL (overrides `CALENDSYNC_DEV_URL`)

**Example:**

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "tools/call",
  "params": {
    "name": "ui_annotations_list",
    "arguments": {}
  }
}
```

#### ui_annotations_get

Get a single UI annotation by ID with full details including computed styles, bounding box, and optional screenshot.

**Parameters:**

- `id` (string, required) — Annotation ID
- `url` (string, optional) — Dev server URL (overrides `CALENDSYNC_DEV_URL`)

**Example:**

```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "tools/call",
  "params": {
    "name": "ui_annotations_get",
    "arguments": {
      "id": "abc-123"
    }
  }
}
```

#### ui_annotations_resolve

Mark a UI annotation as resolved with a summary of the changes made.

**Parameters:**

- `id` (string, required) — Annotation ID
- `summary` (string, required) — Description of what was fixed
- `url` (string, optional) — Dev server URL (overrides `CALENDSYNC_DEV_URL`)

**Example:**

```json
{
  "jsonrpc": "2.0",
  "id": 3,
  "method": "tools/call",
  "params": {
    "name": "ui_annotations_resolve",
    "arguments": {
      "allowWrites": true,
      "id": "abc-123",
      "summary": "Increased font size to 24px for mobile"
    }
  }
}
```

#### ui_annotations_clear

Clear all UI annotations from the dev server.

**Parameters:**

- `url` (string, optional) — Dev server URL (overrides `CALENDSYNC_DEV_URL`)

**Example:**

```json
{
  "jsonrpc": "2.0",
  "id": 4,
  "method": "tools/call",
  "params": {
    "name": "ui_annotations_clear",
    "arguments": {
      "allowWrites": true
    }
  }
}
```

### PDF Tools

#### pdf_toc

Parse a PDF document and return its table of contents as a navigable tree with section IDs, headings, content previews, image counts, and page ranges.

**Parameters:**

- `path` (string, required) - Absolute path to the PDF file

**Example:**

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "tools/call",
  "params": {
    "name": "pdf_toc",
    "arguments": {
      "path": "/absolute/path/to/document.pdf"
    }
  }
}
```

#### pdf_read

Read section content as rendered Markdown with image references. Omit `sectionId` to read the entire document.

**Parameters:**

- `path` (string, required) - Absolute path to the PDF file
- `sectionId` (string, optional) - Section ID from `pdf_toc` (e.g., `s-1-0`). Omit for whole document.

**Example:**

```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "tools/call",
  "params": {
    "name": "pdf_read",
    "arguments": {
      "path": "/absolute/path/to/document.pdf",
      "sectionId": "s-1-0"
    }
  }
}
```

#### pdf_peek

Sample a text snippet from a section without reading the full content. Returns the snippet, position, and total character count.

**Parameters:**

- `path` (string, required) - Absolute path to the PDF file
- `sectionId` (string, optional) - Section ID from `pdf_toc`. Omit for whole document.
- `position` (string, optional) - Where to sample: "beginning" (default), "middle", "ending", "random"
- `limit` (number, optional) - Maximum characters to return (default: 500)

**Example:**

```json
{
  "jsonrpc": "2.0",
  "id": 3,
  "method": "tools/call",
  "params": {
    "name": "pdf_peek",
    "arguments": {
      "path": "/absolute/path/to/document.pdf",
      "sectionId": "s-1-0",
      "position": "middle",
      "limit": 300
    }
  }
}
```

#### pdf_images

List images in a section or the entire document. Returns image IDs, formats, and alt text.

**Parameters:**

- `path` (string, required) - Absolute path to the PDF file
- `sectionId` (string, optional) - Section ID from `pdf_toc`. Omit for all images.

**Example:**

```json
{
  "jsonrpc": "2.0",
  "id": 4,
  "method": "tools/call",
  "params": {
    "name": "pdf_images",
    "arguments": {
      "path": "/absolute/path/to/document.pdf",
      "sectionId": "s-1-0"
    }
  }
}
```

#### pdf_image

Extract a specific image by ID or pick a random one. Returns base64-encoded image data with format and size.

**Parameters:**

- `path` (string, required) - Absolute path to the PDF file
- `imageId` (string, optional) - Image ID (XObject name). Required unless `random` is true.
- `sectionId` (string, optional) - Section ID to scope image selection (used with `random`)
- `random` (boolean, optional) - Pick a random image. Cannot be used with `imageId`.

**Example:**

```json
{
  "jsonrpc": "2.0",
  "id": 5,
  "method": "tools/call",
  "params": {
    "name": "pdf_image",
    "arguments": {
      "path": "/absolute/path/to/document.pdf",
      "imageId": "Im1"
    }
  }
}
```

#### pdf_info

Get document metadata including title, author, page count, and creator.

**Parameters:**

- `path` (string, required) - Absolute path to the PDF file

**Example:**

```json
{
  "jsonrpc": "2.0",
  "id": 6,
  "method": "tools/call",
  "params": {
    "name": "pdf_info",
    "arguments": {
      "path": "/absolute/path/to/document.pdf"
    }
  }
}
```

### Images Tools

Generate, edit, and vary images with the GPT Image 2.5 models (`gpt-image-2.5-flare` default, `gpt-image-2.5-sunburst` for premium precision). Defaults to the ChatGPT subscription (llm-stream `auth.json`); pass `api: "openai"` with `OPENAI_API_KEY` for the metered API.

#### images_generate

Generate images from a text prompt. Saves files and returns paths plus metadata.

**Parameters:**

- `prompt` (string, required) - Text description of the image
- `model` (string, optional) - Image model (default: `gpt-image-2.5-flare`)
- `size` (string, optional) - `auto`, `1024x1024`, `1536x1024`, `1024x1536`, or custom `WIDTHxHEIGHT`
- `quality` (string, optional) - `auto`, `low`, `medium`, `high`, `xhigh`, `max`
- `outputFormat` (string, optional) - `png`, `jpeg`, `webp` (default: `png`)
- `outputCompression` (integer, optional) - 0-100 for jpeg/webp
- `background` (string, optional) - `auto`, `transparent`, `opaque`
- `moderation` (string, optional) - `auto` or `low`
- `n` (integer, optional) - 1-10 images (default: 1; subscription always returns 1)
- `outputDir` (string, optional) - Directory for output files (default: `.`)

**Example:**

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "tools/call",
  "params": {
    "name": "images_generate",
    "arguments": {
      "allowSpend": true,
      "prompt": "a fox reading a newspaper, watercolor",
      "quality": "low"
    }
  }
}
```

#### images_edit

Edit 1-16 reference image files with a prompt and optional mask. Saves generated files and returns their paths.

**Parameters:**

- `prompt` (string, required) - Edit instruction
- `images` (array of strings, required) - Input image file paths (1-16)
- `mask` (string, optional) - Mask PNG path (alpha channel marks edit area)
- Same output options as `images_generate`, plus `inputFidelity` (string, optional) - `high` or `low`

#### images_vary

Create variations anchored to 1-16 reference images. Same as `images_edit` with an optional prompt (defaults to preserving subject and style).

**Parameters:**

- `images` (array of strings, required) - Input image file paths (1-16)
- `prompt` (string, optional) - Variation instruction
- Same output options as `images_edit`

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

Executes a specific tool. Direct Write calls require `allowWrites: true` in `arguments`; direct Spend calls require `allowSpend: true`. The server strips these permission fields before tool input validation. Read calls need neither field.

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

# List available transitions
mcptools atlassian jira transitions list PROJ-123

# PROJ-123 blocks PROJ-456
mcptools atlassian jira link add PROJ-123 --type Blocks PROJ-456

# List links and configured link types
mcptools atlassian jira link list PROJ-123
mcptools atlassian jira link types

# Output as JSON
mcptools atlassian jira search "project = PROJ" --json

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

#### Confluence

```bash
# Search for pages
mcptools atlassian confluence search "text ~ 'deployment'"

# Search in a specific space
mcptools atlassian confluence search "space = WIKI AND text ~ 'api'" --limit 10

# Output as JSON
mcptools atlassian confluence search "text ~ 'guide'" --json

# Read one page by ID or URL; read is an alias for get
mcptools atlassian confluence get 123456
mcptools atlassian confluence read 'https://example.atlassian.net/wiki/spaces/ENG/pages/123456/Runbook' --json

# Create and update published pages with Confluence storage-format XHTML
mcptools atlassian confluence create --space-id 9988 --title 'Runbook' --body '<p>Deploy</p>' --parent 123456 --json
mcptools atlassian confluence update 123456 --title 'Deployment Runbook' --body '<p>Deploy and verify</p>' --json
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

### PDF

```bash
# Get document table of contents
mcptools pdf toc document.pdf

# Read a specific section
mcptools pdf read document.pdf s-1-0

# Read the whole document
mcptools pdf read document.pdf

# Peek at a section (quick preview)
mcptools pdf peek document.pdf s-1-0

# Peek at middle of document with custom limit
mcptools pdf peek document.pdf --position middle --limit 300

# List all images in the document
mcptools pdf images document.pdf

# List images in a specific section
mcptools pdf images document.pdf s-1-0

# Extract an image by ID
mcptools pdf image document.pdf Im1 --output photo.jpg

# Pick a random image
mcptools pdf image document.pdf --random

# Random image from a specific section
mcptools pdf image document.pdf --random --section s-1-0

# Get document metadata
mcptools pdf info document.pdf
```

### Images

```bash
# Generate with subscription (first sign in once: llm-stream --login)
mcptools images generate "a fox reading a newspaper, watercolor" --quality low -o /tmp/fox.png --open

# Edit a reference image
mcptools images edit "make the fox wear glasses" --image /tmp/fox.png -o /tmp/fox-glasses.png

# Variation anchored to a reference image
mcptools images vary --image /tmp/fox.png --prompt "same fox, cyberpunk style" -o /tmp/fox-cyber.png

# Metered API path with multiple images
mcptools images generate "a lighthouse" --api openai --model gpt-image-2.5-sunburst -n 2 --output-dir /tmp
```

**Environment Variables:**

| Variable | Default | Description |
|----------|---------|-------------|
| `MCPTOOLS_IMAGES_API` | `chatgpt` | Backend: `chatgpt` (subscription) or `openai` (API key) |
| `LLM_STREAM_CONFIG_DIR` | `~/.config/llm-stream` | Config dir holding subscription `auth.json` |
| `MCPTOOLS_IMAGES_MAINLINE` | `gpt-5.6-sol` | Chat model fronting the image tool (subscription) |
| `OPENAI_IMAGES_MODEL` | `gpt-image-2.5-flare` | Image model |
| `OPENAI_API_KEY` | — | Key for `--api openai` |
| `OPENAI_BASE_URL` | `https://api.openai.com/v1` | Base URL for `--api openai` |

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
