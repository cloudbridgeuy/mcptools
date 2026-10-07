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
| `bitbucket_pr_create` | Create a Bitbucket PR |
| `bitbucket_pr_update` | Update a Bitbucket PR: metadata, reviewers, state |
| `bitbucket_pr_comment_add` | Add a general or single-line inline PR comment (write; `allowWrites` required in discovery/Code Mode) |

### Linear

| Tool | Description |
|------|-------------|
| `linear_auth_status` | Show Linear viewer identity |
| `linear_issue_get` | Get one Linear issue |
| `linear_issue_list` | List Linear issues with filters |
| `linear_issue_graph` | Bounded dependency closure, frontier, blocker status, and explicit truncation; no chart rendering or browser |
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
| `linear_project_status_list` | List workspace project statuses; limit 1–250, cursor, all |
| `linear_project_milestone_list` | List milestones scoped to project UUID or name with team; nullable description/date; limit 1–250, cursor, all; Read |
| `linear_project_milestone_create` | Create a milestone in project UUID or name with team; name required, verbatim Markdown description, YYYY-MM-DD targetDate, finite sortOrder; absent/null optionals omitted; Write |
| `linear_project_create` | Create a project in one team with optional description and Markdown content |
| `linear_project_update` | Partially update project properties by UUID or name with team; explicit clear flags; Write (`allowWrites` required in discovery/Code Mode) |
| `linear_project_update_create` | Create a progress report, not a property edit; project UUID or name with team, nonblank verbatim Markdown body, optional onTrack/atRisk/offTrack health; MCP-only Write (`allowWrites` required) |
| `linear_project_update_list` | List published progress reports, not property changes; project UUID or name with team; verbatim body, health, timestamps, URL, project and pageInfo; createdAt API order; limit 1–250, cursor, all (1000 report pages maximum); MCP-only Read (no `allowWrites` required) |
| `linear_user_list` | List users matching a query |
| `linear_state_list` | List workflow states in a team |
| `linear_label_list` | List labels in a team |
| `linear_cycle_list` | List cycles in a team |

### Lanes

| Tool | Description |
|------|-------------|
| `lane_list` | Read native Git worktree state and registered ownership in an operator-permitted repository |
| `lane_cleanup_plan` | Read-only advisory removal eligibility with typed blockers and bounded ignored-path evidence; no approval token |
| `lane_create` | Create one guarded Worktrunk lane from an existing local base branch; write tool |

All three deny access when the operator-controlled server environment `MCPTOOLS_LANE_REPOS` is unset or empty. Set an exact canonical path list, or explicitly set the whole value to `'*'` to authorize all otherwise-supported repository roots. Mixed wildcard/path lists and whitespace wildcards are invalid; repository-root, containment, identity, and executable-filter guards remain unchanged. Requests and repository configuration cannot opt in. `lane_create` needs `allowWrites: true` in `call_tool` and `execute`; `lane_cleanup_plan` is Read and needs neither write nor spend permission. Direct `tools/call` uses the existing write dispatch policy and the same repository guards. Omit plan `laneIds` to assess all registered worktrees, or pass 1 to 100 unique managed IDs; unknown IDs fail. Plans do not authorize deletion. No lane removal, cleanup execution, merge, or push tools exist. See [Lanes](lanes.md).

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
| `find_tools` | Ranks for a task, lists domains via listDomains, or lists declarations for a domain. Returns tagged kind (rank|domain|domains) plus appropriate fields. |

`find_tools` is advisory: it never dispatches a tool, and a Jev outage never blocks the caller.

**Rank score semantics**

- `none` = `1.0 - top1`. An empty catalog reports `none = 1.0`.
- Local backend: each score is matched-weighted-IDF over twice the task IDF sum, with name 2.0 / domain 1.5 / summary 1.0 weights, so values near 0.2 are normal. Compare tools relative to each other, not against an absolute cutoff.
- Cross-domain leakage is expected: Jira tools can surface for a Linear task when the top-1 match is right but `none` stays near 0.8.
- Jev backend contrast: scores are verbatim classifier probabilities, kept at or above 0.05 with at most 3 tools returned, so extremes like 1.0 or 0 are normal.
- Jev enables via `JEV_PROVIDER` plus `JEV_API_KEY` or the provider's conventional key var (see table).

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
adds write tools, `allowSpend` adds spend tools, and `execute` and `call_tool` never bind as
globals inside its own sandbox. A missing gated tool rejects with a
`ReferenceError` whose message names the tool, its kind, and the flag to pass
(e.g. `jira_update is a write tool; pass allowWrites: true to execute`); any
other missing global keeps QuickJS's plain `<name> is not defined` message. A ReferenceError from calling a declared name outside execute means that name was never a host tool, not a TTL or an expired binding. That is separate from the in-sandbox gated-tool ReferenceError above.

Limits come from the server flags `--execute-timeout-secs` (default 30),
`--execute-memory-mb` (default 64), and `--execute-output-kb` (default 256).

## Agent loop

1. Call `find_tools` with a task that names the system (e.g. "mark the Jira ticket done", not "mark it done").
2. Read the returned `declaration` on the tool(s) of interest.
3. Call the tool by name through `tools/call`.
4. For a same-domain follow-up (e.g. `jira_comment_list` then `jira_comment_update`), call the follow-up tool directly; do not call `find_tools` again.
5. `find_tools` is advisory: it never dispatches a tool, and Jev is consulted only inside `find_tools` itself, never in front of or around any other tool call.
6. In discovery mode, call the tool through `call_tool({ name, input })`; it is listed beside `find_tools`.

## Discovery mode

Enable with the `--discovery` flag or the `MCPTOOLS_DISCOVERY` environment
variable. Env values are case-insensitive booleans: `y`, `yes`, `t`, `true`,
`on`, `1` enable the mode; `n`, `no`, `f`, `false`, `off`, `0` keep it off;
any other value, including an empty one, exits with code 2 and the error
`value was not a boolean`. In this mode `tools/list` returns exactly two tools, in order,
`find_tools` then `call_tool`; `tools/call` still dispatches every real tool by name. Without
either discovery or code mode, hosts load all 77 schemas and Jev saves no context.

Hosts that declare only listed tools to the model, such as Claude Code, call
any catalog tool through the listed `call_tool`, which takes `name`, `input`,
`allowWrites` and `allowSpend`. `execute` is not listed in this mode.

## Code mode

Enable with the `--code-mode` flag or the `MCPTOOLS_CODE_MODE` environment
variable, with the same accepted values as `MCPTOOLS_DISCOVERY`. In this mode
`tools/list` returns exactly three tools, in order: `find_tools`,
`execute`, then `call_tool`. Agents call one tool directly with
`await tools.mcptools.call_tool({ name: "linear_issue_list", input: { team: "GUZ" } })`
(`allowWrites` and `allowSpend` gate write and spend tools; the meta tools
cannot be called through it) and chain many calls with `execute`. Code mode wins over Discovery mode when both are enabled.
`tools/call` still dispatches every real tool by name. The `find_tools`
response `usage` field, also for nested `find_tools` calls inside `execute`,
is `Call a single tool with tools.mcptools.call_tool, e.g. await tools.mcptools.call_tool({ name: "linear_issue_list", input: { team: "GUZ" } }). To chain many calls in one round trip, pass code as a string to tools.mcptools.execute, where each declared function is an async global (one fresh sandbox per call). The outer Code Mode scope has only find_tools, execute and call_tool; a declared name called there is a ReferenceError, not a TTL or expiry.`; other
modes keep `Each declaration is the call signature: the input interface is
the tools/call arguments object, the Promise type is the result.`

### Nested execute (outer harness -> inner mcptools)

When the outer host is itself Code Mode (opencode `default.execute`), the
outer scope binds only `find_tools`, `execute`, and `call_tool`. Declared
tool names are never bound there. Sequence two writes by nesting
`tools.mcptools.execute` inside the outer `execute`, with each inner `code`
as a plain string. This is the value of the outer `code` argument:

```javascript
const updateArgs = JSON.stringify({ id: issueId, state: doneState });
const commentArgs = JSON.stringify({ issueId: issueId, body: commentBody });
const updateCode = "await linear_issue_update(" + updateArgs + ")";
const commentCode = "await linear_comment_create(" + commentArgs + ")";
await tools.mcptools.execute({ code: updateCode, allowWrites: true });
return await tools.mcptools.execute({ code: commentCode, allowWrites: true });
```

Escaping rules for the inner `code` string:

- Build inner code with string concatenation plus `JSON.stringify`, never a
  raw template literal inside a template literal.
- `JSON.stringify` every argument object and every user text value before
  interpolating it into the inner string.
- Keep one sandbox call per `execute`; sequence with `await` between calls.

Failures here are runtime-only: a broken inner string surfaces as a
`ReferenceError` or a syntax error from inside `execute`, never as a type
or lint error. When that happens, log the exact inner `code` string first;
the break is in the string building, not in the tool binding.

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
