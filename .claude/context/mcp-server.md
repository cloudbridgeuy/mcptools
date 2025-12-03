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
| `jira_fields` | List custom field values |
| `jira_query_list` | List saved queries |
| `jira_query_save` | Save a JQL query |
| `jira_query_delete` | Delete a saved query |
| `jira_query_load` | Load a saved query |
| `confluence_search` | Search Confluence pages |
| `bitbucket_pr_list` | List Bitbucket PRs |
| `bitbucket_pr_read` | Read PR details/diff |

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
