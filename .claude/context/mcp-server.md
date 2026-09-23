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
| `find_tools` | Ranks registered tools for a task description. Returns `none` score, `tools` list with a TypeScript `declaration` per tool, `backend` ("local" or "jev"), optional `fallback` ("unreachable" | "http_status" | "invalid_response" | "invalid_config"), and `usage`. |

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

### Execute

| Tool | Description |
|------|-------------|
| `execute` | Run JavaScript in a sandbox that binds the tools `find_tools` returns |

Inputs:

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `code` | string | (required) | JavaScript to run; the final expression or top-level `return` value becomes `result` |
| `allowWrites` | boolean | `false` | Also bind tools of kind Write |
| `allowSpend` | boolean | `false` | Also bind tools of kind Spend |

Result shape, the `structuredContent` object:

| Field | Type | Description |
|-------|------|-------------|
| `logs` | string[] | Lines captured from `console.log` up to the failure point |
| `result` | any | Final expression or top-level return value; `null` when `error` is set |
| `error` | object \| null | `null` on success; otherwise `{message, name}` |

The envelope carries `isError: true` exactly when `error` is non-null. On
failure `result` is `null` and `logs` still holds the lines printed before the
failure.

Error names:

- `TimeoutError` — the run exceeded `--execute-timeout-secs`
- `OutputLimitError` — the logs and result exceeded `--execute-output-kb`
- JavaScript error names pass through unchanged (`TypeError`, `Error`, and so
  on); memory exhaustion surfaces as `InternalError`

Flag gating is by tool kind only: read tools bind by default, `allowWrites`
adds write tools, `allowSpend` adds spend tools, and `execute` never binds as
a global inside its own sandbox. A missing gated tool rejects with a
`ReferenceError` whose message names the tool, its kind, and the flag to pass
(e.g. `jira_update is a write tool; pass allowWrites: true to execute`); any
other missing global keeps QuickJS's plain `<name> is not defined` message.

Limits come from the server flags `--execute-timeout-secs` (default 30),
`--execute-memory-mb` (default 64), and `--execute-output-kb` (default 256).

## Agent loop

1. Call `find_tools` with a task that names the system (e.g. "mark the Jira ticket done", not "mark it done").
2. Read the returned `declaration` on the tool(s) of interest.
3. Call the tool by name through `tools/call`.
4. For a same-domain follow-up (e.g. `jira_comment_list` then `jira_comment_update`), call the follow-up tool directly; do not call `find_tools` again.
5. `find_tools` is advisory: it never dispatches a tool, and Jev is consulted only inside `find_tools` itself, never in front of or around any other tool call.
6. In discovery mode the loop needs a host that can call a tool absent from `tools/list` — see the Known limitation in [Discovery mode](#discovery-mode) below; this section restates none of that text.

## Discovery mode

Enable with the `--discovery` flag or the `MCPTOOLS_DISCOVERY` environment
variable, accepted values `true` or `false` only. In this mode `tools/list`
returns exactly one tool, `find_tools`; `tools/call` still dispatches every
real tool by name. Without this mode, hosts load all 63 schemas and Jev
saves no context.

Known limitation, planned and undesigned: hosts that declare only listed
tools to the model, such as Claude Code, cannot yet call the tool
`find_tools` returns, because it is absent from `tools/list`. The returned
declarations name the tools/call arguments and results of every tool they
carry. A future `execute` command is planned to close this gap.

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
