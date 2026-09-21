# MCP Server Configuration

mcptools can run as an MCP (Model Context Protocol) server.

## Transport Modes

### stdio (Local Agents)

```bash
mcptools mcp stdio
```

For local agents like Claude Desktop that communicate via stdin/stdout.

### SSE (Web Clients)

```bash
mcptools mcp sse --port 3000 --host 127.0.0.1
```

For web-based clients using Server-Sent Events over HTTP.

mcptools mcp catalog # Print the compact tool catalog (domain, name, summary)
mcptools mcp declarations [NAME...] # Print TypeScript declarations for named tools (all tools without names)

## Claude Desktop Configuration

Add to `~/Library/Application Support/Claude/claude_desktop_config.json`:

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

## Claude Code Configuration

Using CLI:

```bash
claude mcp add mcptools -- mcptools mcp stdio
```

Or manually add to `~/Library/Application Support/Claude/claude_code_config.json`:

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

## Available MCP Tools

### Atlassian

| Tool | Description |
|------|-------------|
| `jira_search` | Search Jira issues using JQL |
| `jira_get` | Get Jira ticket details |
| `jira_create` | Create a new Jira ticket |
| `jira_update` | Update Jira ticket fields |
| `jira_query_list` | List saved queries |
| `jira_query_save` | Save a JQL query |
| `jira_query_delete` | Delete a saved query |
| `jira_query_load` | Load a saved query |
| `confluence_search` | Search Confluence pages |
| `bitbucket_pr_list` | List Bitbucket PRs |
| `bitbucket_pr_read` | Read PR details/diff |

### Linear

| Tool | Description |
|------|-------------|
| `linear_auth_status` | Show Linear viewer identity |
| `linear_issue_get` | Get one Linear issue |
| `linear_issue_list` | List Linear issues with filters |
| `linear_issue_create` | Create a Linear issue |
| `linear_issue_update` | Update a Linear issue |
| `linear_comment_list` | List comments on an issue |
| `linear_comment_create` | Create a comment on an issue |
| `linear_relation_list` | List relations on an issue |
| `linear_relation_add` | Add a relation between issues |
| `linear_relation_remove` | Remove a relation by triple |
| `linear_team_list` | List Linear teams |
| `linear_team_get` | Get one Linear team |
| `linear_project_list` | List projects in a team |
| `linear_project_get` | Get one project in a team |
| `linear_user_list` | List users matching a query |
| `linear_state_list` | List workflow states in a team |
| `linear_label_list` | List labels in a team |
| `linear_cycle_list` | List cycles in a team |

### HackerNews

| Tool | Description |
|------|-------------|
| `hn_read_item` | Read post and comments |
| `hn_list_items` | List stories |

### Web Scraping

| Tool | Description |
|------|-------------|
| `md_fetch` | Fetch page as Markdown |
| `md_toc` | Extract table of contents |

### PDF

| Tool | Description |
|------|-------------|
| `pdf_toc` | Parse document tree / table of contents |
| `pdf_read` | Read section content as Markdown |
| `pdf_peek` | Sample text snippet from section |
| `pdf_images` | List images in section or document |
| `pdf_image` | Extract specific image by ID |
| `pdf_info` | Get document metadata |

### UI Annotations

| Tool | Description |
|------|-------------|
| `ui_annotations_list` | List all annotations |
| `ui_annotations_get` | Get annotation by ID |
| `ui_annotations_resolve` | Mark annotation as resolved |
| `ui_annotations_clear` | Clear all annotations |

### Find tools

| Tool | Description |
|------|-------------|
| `find_tools` | Ranks registered tools for a task description. Returns `none` score, `tools` list, `backend` ("local" or "jev"), and optional `fallback` ("unreachable" | "http_status" | "invalid_response" | "invalid_config"). |

`find_tools` is advisory: it never dispatches a tool, and a Jev outage never blocks the caller.

**Jev environment variables**

| Variable | Default | Description |
|----------|---------|-------------|
| `JEV_PROVIDER` | unset | `opencode`, `openrouter`, `vercel` or `typesafe` to enable Jev classifier |
| `JEV_ENDPOINT` | provider preset | Override endpoint URL |
| `JEV_MODEL` | provider preset | Override model name |
| `JEV_API_KEY` | (none) | API key; falls back to the provider's conventional key var |
| `OPENCODE_API_KEY` | - | Conventional key var when `JEV_PROVIDER=opencode` |
| `OPENROUTER_API_KEY` | - | Conventional key var when `JEV_PROVIDER=openrouter` |
| `AI_GATEWAY_API_KEY` | - | Conventional key var when `JEV_PROVIDER=vercel` |
| `TYPESAFE_API_KEY` | - | Conventional key var when `JEV_PROVIDER=typesafe` |

## Discovery mode

Enable with the `--discovery` flag or the `MCPTOOLS_DISCOVERY` environment
variable, accepted values `true` or `false` only. In this mode `tools/list`
returns exactly one tool, `find_tools`; `tools/call` still dispatches every
real tool by name. Without this mode, hosts load all 62 schemas and Jev
saves no context.

Known limitation, planned and undesigned: hosts that declare only listed
tools to the model, such as Claude Code, cannot yet call the tool
`find_tools` returns, because it is absent from `tools/list`. A future
`execute` command is planned to close this gap.

## Testing with curl

```bash
# Start MCP server
mcptools mcp sse --port 3000

# Test tools/list
curl -X POST http://127.0.0.1:3000/message \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}'

# Test a tool call
curl -X POST http://127.0.0.1:3000/message \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"hn_list_items","arguments":{"limit":5}}}'
```
