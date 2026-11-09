# MCP Server Configuration

mcptools can run as an MCP (Model Context Protocol) server.

The built-in registry has 114 tools: 111 catalog tools and 3 meta tools.
Configured upstream tools extend the catalog, not `tools/list`.
`plan_write` stores a diagram (Write; requires `allowWrites: true`);
`plan_url` reads its persisted URL (Read). `plan_diff` reads changes and advances
the baseline (destructive Write; requires `allowWrites: true`). `plan_png`
returns the latest saved PNG path (Read). See [Plan diagrams](diagram.md).

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

### JSON-RPC notifications

Messages without an `id` are notifications. Both transports ignore them before
method dispatch, including `notifications/initialized`, `notifications/cancelled`,
and unknown methods. Stdio writes nothing; `/message` returns HTTP 202 with an
empty body, not a JSON-RPC response. An explicit `id: null` is still a request.
Unknown requests keep error `-32601`.

Regression evidence for GUZ-1228: before the fix, `RUST_TEST_THREADS=1 cargo xtask lint`
failed at `crates/mcptools/tests/mcp_notifications.rs:31` (three responses, expected
two), and `crates/mcptools/src/mcp/mod.rs:41` (unknown notification returned
`-32601`, expected no response). The protocol decision is the pure `parse_request`
function in `mcp/mod.rs`; stdio and SSE consume its optional response.

## Upstream gateway

Configure once in `~/.config/mcptools/config.toml`; all harnesses running
mcptools with the same environment use it automatically. An absolute
`XDG_CONFIG_HOME` changes the base to `$XDG_CONFIG_HOME/mcptools/config.toml`.
An unset, empty or relative `XDG_CONFIG_HOME` uses `$HOME/.config` instead.
`MCPTOOLS_CONFIG` selects a different file instead of the default file; a
relative override resolves from the working directory, an empty override is
ignored. Missing files, including an explicit override, mean defaults and no
upstreams; nothing is created. Missing HOME without an absolute XDG base or
file override is an actionable error. The path convention is the same on macOS
and Linux, not macOS Application Support.

The sectioned TOML configuration layers defaults with one global file. There
is no per-store or repository layer. Supported sections are `[gateway]`, `[atlassian]`, `[linear]`, `[jev]`, `[images]`, `[general]`, `[mcp]`, `[diagram]`, and `[md]`.
Unknown sections and keys fail validation. CLI flags and per-call arguments override environment values, then file values, then built-in defaults.

| File keys | Environment variables |
|-----------|-----------------------|
| `[general].verbose` | `MCPTOOLS_VERBOSE` |
| `[mcp].discovery`, `code_mode` | `MCPTOOLS_DISCOVERY`, `MCPTOOLS_CODE_MODE` |
| `[mcp].execute_timeout_secs`, `execute_memory_mb`, `execute_output_kb` | `MCPTOOLS_EXECUTE_TIMEOUT_SECS`, `MCPTOOLS_EXECUTE_MEMORY_MB`, `MCPTOOLS_EXECUTE_OUTPUT_KB` |
| `[diagram].host`, `port`, `share_seconds`, `ttl_seconds`, `idle_check_seconds`, `idle_grace_seconds` | Corresponding `MCPTOOLS_DIAGRAM_*` variables |
| `[md].timeout` | `MD_TIMEOUT` |

`MCPTOOLS_LANE_REPOS` remains operator-controlled environment only: an agent that can write user config must not grant itself lane access. Unknown-field validation rejects root `lane-repos` or `lane_repos`, `[mcp].lane_repos`, and `[lane]`. `CHROME` remains the executable override; `RALPH_INBOX` remains operator-controlled because it grants an external notification write target. `LLM_STREAM_BIN` is test-only. Platform selectors such as `MCPTOOLS_CONFIG`, `XDG_CONFIG_HOME`, `HOME`, `XDG_DATA_HOME`, `APPDATA`, and `PATH` remain environment only, as do terminal autodetection variables `TERM`, `TERM_PROGRAM`, `KITTY_WINDOW_ID`, and `TMUX`. `PORT` sets the Linear chart `--serve` port per invocation (default 0, ephemeral) and is outside this migration; `CALENDSYNC_DEV_URL` is development-only. `HN_ITEM`, `HN_LIMIT`, `MD_URL`, `MD_SELECTOR`, `MD_STRATEGY`, `MD_INDEX`, `MD_PAGINATED`, `MD_OFFSET`, `MD_LIMIT`, `MD_PAGE`, and `MD_OUTPUT` remain per-invocation environment arguments.

```toml
[[gateway.upstreams]]
prefix = "context7"
command = "npx"
args = ["-y", "@upstash/context7-mcp"]
readOnly = ["resolve-library-id", "query-docs"]
```

`mcptools config path` prints the resolved path without reading the file.
`mcptools config check` checks TOML, keys, types and static gateway policy;
it prints the path plus `ok:` or `defaults apply`. Errors name the path and
invalid key or TOML line/column and exit nonzero without printing source lines.
It never starts upstream commands, discovers tools or writes the cache.
For a remote Streamable HTTP server, configure a URL instead of a command:

```toml
[[gateway.upstreams]]
prefix = "remote"
url = "https://example.com/mcp"
readOnly = ["search"]
[gateway.upstreams.headers]
Authorization = "Bearer token"
```

URLs must use HTTPS, except loopback HTTP. Set exactly one of `command` or `url`;
an empty `command` with a URL is invalid. Header values are literal. HTTP
sessions support JSON and event-stream responses, retain the initialization
session ID, and send best-effort DELETE on normal exit and request errors.
Protocol headers override configured headers. OAuth requires explicit login;
legacy SSE is not supported. The gateway uses its own client for both transports. Protect the
configuration file because it can contain credentials.

HTTP tool results are redacted for credential headers: `Authorization`,
`Proxy-Authorization`, `Cookie`, or any name containing `token`, `key`,
`secret`, `password` or `auth` (case-insensitive). The full value is redacted,
and for `<scheme> <credential>` values such as `Bearer abc` the bare credential
is redacted too. Header-derived candidates shorter than 8 characters are not
collected. Other headers (for example `X-Team`) do not supply redaction
candidates. OAuth access tokens, refresh tokens and client secrets are also
redacted when nonempty, without the header minimum length. The client retains
old and new secrets across refresh. Redacted result keys that collide fail the
call.

`args`, `env` and `readOnly` default to empty. Environment entries override
the inherited environment; values are literal, not variable substitutions.
Add environment entries with `[gateway.upstreams.env]` below an upstream.
Commands are executed directly, not through a shell. Relative paths use the
mcptools working directory; use absolute paths for stable configuration.
Prefixes must be unique ASCII letters/digits, starting with a letter; no
underscores. Exposed tool names are `<prefix>_` followed by the upstream name
with each character outside `[A-Za-z0-9_]` replaced by `_`. For example,
`resolve-library-id` becomes `context7_resolve_library_id`, and `query.docs`
becomes `context7_query_docs`. Each non-ASCII character also becomes one `_`.
Declarations use plain `declare function`; `execute` calls plain globals.
Forwarding always sends the original upstream name. Empty names, malformed
object input schemas, duplicate names after normalization (such as `a-b` and
`a_b`), and collisions with built-in tools or hidden aliases fail startup.

Every startup, catalog and declarations command performs live initialization and discovery; core policy skips runtime connection/catalog failures with one secret-free stderr warning per prefix (including the OAuth login hint), while invalid configuration and tool-name clashes remain fatal.
Skipped tools are omitted even when cached, no cache is written after any upstream failure, and fully successful discovery writes an atomic owner-only `upstreams.cache.json` beside the selected config; live clients are reused for calls.
EOF, handled termination signals, errors and abrupt owner death release an
independent reaper lease. Each completed request acknowledges a process-tree
snapshot before returning. The reaper checks process identities and stops the
observed process tree, escalating TERM to KILL, using the Chrome reaper helpers,
even if the root exits on stdin EOF before the reaper runs.
Commands that detach/reparent descendants before shutdown are not supported.

`context7_resolve_library_id` appears in `find_tools`, `mcptools mcp catalog`, and
`mcptools mcp declarations context7_resolve_library_id`. It is never advertised by plain
`tools/list`, including full mode. Call through the existing interfaces:

```javascript
find_tools({domain: "context7"})
call_tool({name: "context7_resolve_library_id", input: {libraryName: "tokio", query: "Tokio async runtime"}})
execute({code: 'return await context7_resolve_library_id({libraryName: "tokio", query: "Tokio async runtime"})'})
```

**Security:** Every upstream tool needs `allowWrites` unless its original,
unprefixed name is explicitly in that upstream's `readOnly` list. Normalized
aliases and prefixed names do not grant read-only access; an unchanged name
still matches its original spelling. Upstream
safety annotations are ignored. `allowSpend` does not authorize upstream writes.
This permission gate does not sandbox the executable: discovery itself runs
trusted operator-configured code with the user's permissions and inherited
environment. Keep the config and its directory private. Do not accept config
paths or read-only policy from an agent or upstream response.

Upstream output, descriptions and schemas are **untrusted input**, including
possible prompt injection; never treat them as agent instructions. `call_tool`
returns the upstream `tools/call` result unchanged, including content blocks,
`isError`, structured content and metadata. `execute` uses its normal result
unwrapping: structured content when present, otherwise content. Declarations
describe the input schema and an open object output; upstream `outputSchema`
is deliberately not interpreted as the raw call-result envelope.

The minimal client uses existing Tokio and serde_json dependencies and fits
the existing hand-written JSON-RPC server. `rmcp` provides a full client SDK
and `transport-child-process`, but adds another protocol model/transport layer
and dependencies; it does not replace our custom lifecycle, cache or policy.
The stdio client adds no SDK dependency; TOML configuration uses the `toml`
crate. Supported protocol
versions: 2024-11-05, 2025-03-26, 2025-06-18. Each request has a 30-second
deadline and a 4-MiB message cap; discovery has a 100-page cap. Calls to each
upstream are serialized. Client `ping` is supported; other server-initiated
requests receive method-not-found. No sampling, roots, elicitation or progress
relay. Failed/uncertain calls are never retried automatically; a later explicit
call starts a fresh connection after a failed request.

### Regression evidence (GUZ-1443)

Before implementation, `RUST_TEST_THREADS=1 cargo xtask lint --no-fmt --no-check
--no-clippy --no-machete --no-typos` failed at
`crates/mcptools/tests/acceptance_gateway.rs:109`, `:285`, `:303`, `:321`,
`:337`, and `:351` (pre-format locations): normalized catalog/cache, discovery,
plain declarations, plain-global execution, original-name forwarding, and
normalization collisions. Core tests also failed at
`crates/core/src/gateway.rs:234` and `:251`: arbitrary-character mapping with
original-name read-only policy, and normalized/reserved-name collisions.
The fake upstream provides both hyphenated and dotted names without network.
The manual Context7 check with a temporary `MCPTOOLS_CONFIG` returned Tokio
library matches, including `/tokio-rs/tokio`, from plain-global
`context7_resolve_library_id` inside `execute` with no write permission.

The generic declaration renderer retains quoted callable interfaces for
non-identifier inputs: the read-only `acceptance_config.rs` regression directly
requires that behavior. Gateway names no longer reach that branch or need
`globalThis` guidance.

### Regression evidence (GUZ-1414)

Before implementation, `RUST_TEST_THREADS=1 cargo xtask lint --no-fmt --no-check
--no-clippy --no-machete --no-typos` ran the fake-server tests red at
`crates/mcptools/tests/acceptance_gateway.rs:88`, `:105`, `:120`, `:138`, and
`:150` (original pre-format locations): catalog discovery/cache, declarations
and hidden listing, write gating/forwarding, read-only override, and exit
cleanup. The fixture uses Python's standard library and no network.

Before the TOML revision, the same lint test command ran five config tests red
at `crates/mcptools/tests/acceptance_config.rs:14`, `:26`, `:34`, `:51`, and `:69`
(original pre-format locations): default/XDG path, override, missing-file
defaults, TOML sections, and actionable check errors. `config` was not a CLI
command. The live Context7 smoke check is manual, never part of lint.

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

### Hidden tool-name aliases

Aliases are opt-in entries in `crates/mcptools/src/mcp/tools/aliases.rs`.
The only alias is `linear_issue_view` → `linear_issue_get`. It accepts the same
arguments and returns the same result through direct `tools/call`, `call_tool`,
and the inner `execute` sandbox. Resolution precedes dispatch and policy checks;
permissions and policy errors use the canonical name. Aliases are absent from
`tools/list`, `mcptools mcp catalog`, declarations, and `find_tools` results.
Alias names cannot collide with registered tools; targets must be real,
non-meta tools. No CLI aliases or general `view` → `get` rewrite exists.

### Atlassian

| Tool | Description |
|------|-------------|
| `jira_search` | Search Jira issues using JQL |
| `jira_get` | Get Jira ticket details |
| `jira_create` | Create a new Jira ticket |
| `jira_update` | Update Jira ticket fields |
| `jira_transition_list` | List available transitions for an issue |
| `jira_link_list` | List links on an issue |
| `jira_link_type_list` | List configured link types and direction text |
| `jira_link_create` | Create a directed issue link; Write (`allowWrites` required) |
| `jira_link_delete` | Delete an issue link by ID; destructive Write (`allowWrites` required) |
| `jira_board_list` | List visible Jira Software boards and IDs; Read |
| `jira_query_list` | List saved queries |
| `jira_query_save` | Save a JQL query |
| `jira_query_delete` | Delete a saved query |
| `jira_query_load` | Load a saved query |
| `confluence_search` | Search Confluence pages |
| `confluence_page_get` | Read one Confluence page by ID or URL |
| `confluence_page_create` | Create a page from storage XHTML; additive Write (`allowWrites` required) |
| `confluence_page_update` | Update title and/or storage body with version increment; destructive Write (`allowWrites` required) |
| `bitbucket_pr_list` | List Bitbucket PRs |
| `bitbucket_pr_read` | Read PR details/diff |
| `bitbucket_pr_create` | Create a Bitbucket PR |
| `bitbucket_pr_update` | Update PR metadata/reviewers; approve, unapprove, decline, or merge |
| `bitbucket_pr_comment_add` | Add a general or single-line inline PR comment (Write; `allowWrites` required) |
| `bitbucket_pr_comment_list` | List PR comments and thread resolution state; Read |
| `bitbucket_pr_comment_update` | Replace PR comment Markdown; destructive Write (`allowWrites` required) |
| `bitbucket_pr_comment_delete` | Delete a PR comment; destructive Write (`allowWrites` required) |
| `bitbucket_pr_comment_resolve` | Resolve a PR comment thread; destructive Write (`allowWrites` required) |
| `bitbucket_pr_comment_reopen` | Reopen a PR comment thread; destructive Write (`allowWrites` required) |
| `bitbucket_repo_branch_create` | Create a branch from a target; additive Write (`allowWrites` required) |
| `bitbucket_repo_branch_delete` | Delete a non-main branch; destructive Write (`allowWrites` required) |
| `bitbucket_deploy_key_list` | List repository deploy keys; Read |
| `bitbucket_deploy_key_add` | Add a repository deploy key; additive Write (`allowWrites` required) |
| `bitbucket_deploy_key_remove` | Remove a repository deploy key; destructive Write (`allowWrites` required) |

### Linear

| Tool | Description |
|------|-------------|
| `linear_auth_status` | Show Linear viewer identity |
| `linear_issue_get` | Get one Linear issue |
| `linear_issue_list` | List Linear issues with filters |
| `linear_issue_graph` | Bounded dependency closure, frontier, blocker status, and explicit truncation; no chart rendering or browser |
| `linear_issue_create` | Create a Linear issue |
| `linear_issue_update` | Update a Linear issue |
| `linear_issue_delete` | Move a Linear issue to trash; never permanently delete; Write (`allowWrites` required) |
| `linear_issue_restore` | Restore a Linear issue from trash; Write (`allowWrites` required) |
| `linear_comment_list` | List comments on an issue |
| `linear_comment_create` | Create a comment on an issue |
| `linear_comment_update` | Replace a comment body by comment id; requires allowWrites |
| `linear_comment_delete` | Delete a comment by comment id; no API trash/restore operation documented; requires allowWrites |
| `linear_relation_list` | List relations on an issue |
| `linear_relation_add` | Add a relation between issues |
| `linear_relation_remove` | Remove a relation by triple |
| `linear_team_list` | List Linear teams |
| `linear_team_get` | Get one Linear team |
| `linear_project_list` | List projects in a team |
| `linear_project_get` | Get one project in a team |
| `linear_project_status_list` | List workspace project statuses; limit 1–250, cursor, all |
| `linear_initiative_list` | List workspace initiatives with case-sensitive name query; nullable description, targetDate and owner; limit 1–250, cursor, all (1000 pages maximum); MCP-only Read |
| `linear_initiative_get` | Get initiative by required UUID; nullable metadata and Markdown content; every directly linked project including archived projects; 250 projects/page, 1000-page cap, no silent truncation; MCP-only Read |
| `linear_document_list` | List documents by project, issue, or title; limit 1–250, cursor, all; Read |
| `linear_document_get` | Get document by required UUID; title, nullable verbatim Markdown, timestamps, URL, nullable owner and project; MCP-only Read |
| `linear_document_create` | Create a document attached to exactly one existing project or issue; nonblank title, optional verbatim Markdown; Write, requires `allowWrites` |
| `linear_document_update` | Update document UUID with partial nonblank title or verbatim Markdown; empty content clears body, omitted fields unchanged; nullable project identity; MCP-only Write, requires `allowWrites` |
| `linear_initiative_project_add` | Link existing initiative UUID to existing project UUID or name with team; returns association id and initiative/project identities; duplicate links and hierarchy conflicts fail without mutation retry; MCP-only Write (`allowWrites` required) |
| `linear_project_milestone_list` | List milestones scoped to project UUID or name with team; nullable description/date; limit 1–250, cursor, all; Read |
| `linear_project_milestone_create` | Create a milestone in project UUID or name with team; name required, verbatim Markdown description, YYYY-MM-DD targetDate, finite sortOrder; absent/null optionals omitted; Write |
| `linear_project_milestone_update` | Partial milestone UUID update: name, verbatim description, targetDate, sortOrder; omission unchanged, explicit clearDescription/clearTargetDate set null; no project reassignment; Write |
| `linear_project_create` | Create a project in one team with optional description and Markdown content |
| `linear_project_update` | Partially update project properties by UUID or name with team; explicit clear flags; Write (`allowWrites` required) |
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
| `lane_cleanup_apply` | Replan, then remove only eligible worktrees, merged local branches, and matching ownership records without force; destructive write tool |
| `lane_create` | Create one guarded Worktrunk lane from an existing local base branch; write tool |

All four deny access when the operator-controlled server environment `MCPTOOLS_LANE_REPOS` is unset or empty. Set an exact canonical path list, or explicitly set the whole value to `'*'` to authorize all otherwise-supported repository roots. Mixed wildcard/path lists and whitespace wildcards are invalid; repository-root, containment, identity, and executable-filter guards remain unchanged. Requests and repository configuration cannot opt in. `lane_create` and `lane_cleanup_apply` need `allowWrites: true` in direct `tools/call`, `call_tool`, and `execute`; apply has `destructiveHint: true`. `lane_cleanup_plan` is Read and needs neither write nor spend permission. Omit `laneIds` to assess all registered worktrees, or pass 1 to 100 unique managed IDs; unknown IDs fail. Apply always replans and skips every blocked lane. It uses non-force Git worktree and merged-branch deletion, removes the matching ownership record last, and reports possible partial mutation on failure. No merge or push tools exist. CLI exposes the same apply operation as `mcptools lane cleanup --repo <absolute-root> [--lane-id <id> ...]`. See [Lanes](lanes.md).

### HackerNews

| Tool | Description |
|------|-------------|
| `hn_read_item` | Read post and comments |
| `hn_list_items` | List stories |

### Web Scraping

| Tool | Description |
|------|-------------|
| `md_fetch` | Fetch page as Markdown or HTML with CSS selection and character pagination |
| `md_toc` | Extract table of contents |
| `md_render` | Write self-contained offline HTML with strict Mermaid diagrams; requires `allowWrites` |

### PDF

| Tool | Description |
|------|-------------|
| `pdf_toc` | Parse document tree / table of contents |
| `pdf_read` | Read section content as Markdown |
| `pdf_peek` | Sample text from a section or whole document at a chosen or random position |
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
- Jev enables with a provider and API key from the file or environment (see table).

**Jev environment variables**

Jev also reads `[jev]` fields `provider`, `endpoint`, `model`, and `api_key` from `~/.config/mcptools/config.toml` (or `MCPTOOLS_CONFIG`). Environment values override file values; `JEV_API_KEY` takes precedence over the selected provider's key variable, then `api_key`. Run `chmod 600` on files containing keys.

| Variable | Default | Description |
|----------|---------|-------------|
| `JEV_PROVIDER` | unset | `opencode`, `openrouter`, `vercel` or `typesafe`; unset in env and file disables Jev |
| `JEV_ENDPOINT` | provider preset | Override endpoint URL |
| `JEV_MODEL` | provider preset | Override model name |
| `JEV_API_KEY` | (none) | API key; falls back to the provider's conventional key var |
| `OPENCODE_API_KEY` | - | Conventional key var when provider is `opencode` |
| `OPENROUTER_API_KEY` | - | Conventional key var when provider is `openrouter` |
| `AI_GATEWAY_API_KEY` | - | Conventional key var when provider is `vercel` |
| `TYPESAFE_API_KEY` | - | Conventional key var when provider is `typesafe` |

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

Direct `tools/call` extracts and removes `allowWrites` and `allowSpend` before tool input validation. A Write tool runs only with `allowWrites: true`; a Spend tool runs only with `allowSpend: true`. Omitted or false permission denies the call before tool validation or I/O. Read tools need neither field. Plain `tools/list` schemas advertise the applicable optional field. Hidden aliases use the canonical tool kind. The `call_tool` and `execute` permission contracts are unchanged.

1. Call `find_tools` with a task that names the system (e.g. "mark the Jira ticket done", not "mark it done").
2. Read the returned `declaration` on the tool(s) of interest.
3. Call the tool by name through `tools/call`; include `allowWrites: true` or `allowSpend: true` in the arguments for Write or Spend tools.
4. For a same-domain follow-up (e.g. `jira_comment_list` then `jira_comment_update`), call the follow-up tool directly; do not call `find_tools` again.
5. `find_tools` is advisory: it never dispatches a tool, and Jev is consulted only inside `find_tools` itself, never in front of or around any other tool call.
6. In discovery mode, call the tool through `call_tool({ name, input })`; it is listed beside `find_tools`.

## Discovery mode

Enable with the `--discovery` flag or the `MCPTOOLS_DISCOVERY` environment
variable. Env values are case-insensitive booleans: `y`, `yes`, `t`, `true`,
`on`, `1` enable the mode; `n`, `no`, `f`, `false`, `off`, `0` keep it off;
any other value, including an empty one, exits with code 2 and the error
`value was not a boolean`. In this mode `tools/list` returns exactly two tools, in order,
`find_tools` then `call_tool`; `tools/call` still accepts every real tool name and applies direct-call permission gating. Without
either discovery or code mode, hosts load all 90 schemas and Jev saves no context.

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
`tools/call` still accepts every real tool name and applies direct-call permission gating. The `find_tools`
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

### OAuth HTTP upstreams

For compatibility, callback pages acknowledge redirect validation before token exchange; the terminal command remains the source of truth for successful credential storage.

For an HTTP upstream, set `auth = "oauth"` and optionally `client_id`, `client_secret`, `scopes`, `redirect_port`, and `redirect_uri` in its `[[gateway.upstreams]]` entry. OAuth cannot be used with a stdio command or a static `Authorization` header. Run `mcptools gateway login <prefix>` in a terminal before starting MCP. The command discovers the authorization server, with OpenID discovery as fallback, opens a browser, and uses a loopback callback with PKCE. Without `client_id`, the server must support dynamic client registration. Registration requests a public client (`token_endpoint_auth_method = "none"`); an explicit `client_secret_post` registration response is also supported. If registration is unavailable, configure `client_id` from a client registered with the authorization server. `client_secret` requires `client_id`; both are literal values, not environment references. Code exchange and refresh use `client_secret_post`, or `client_secret_basic` when metadata supports basic but not post. Refresh reads the current configured secret, so rotating it does not require login again. Configured client secrets are not copied into credential files or cache and are redacted from upstream results. Protect the config with `chmod 600`.

`redirect_port` must be 1–65535. It binds exactly `127.0.0.1:<port>`; a busy port is an error, with no random-port fallback. Without it, login uses a random port. The default redirect is `http://127.0.0.1:<port>/callback`. Optional `redirect_uri` must use HTTPS, requires `redirect_port`, and cannot contain credentials, a query, or a fragment. It is sent unchanged in authorization and code-exchange requests. The local listener remains plain HTTP and accepts the redirect URI's path. The operator must provide TLS termination and preserve that path and query when forwarding.

[Slack MCP](https://docs.slack.dev/ai/mcp-server/) requires a registered app and confidential OAuth, with no dynamic registration. [Slack OAuth v2](https://docs.slack.dev/authentication/installing-with-oauth/#asking) requires HTTPS redirects; HTTP loopback is not a documented exception. Register an HTTPS URL reachable by the user's browser, then proxy to the fixed local port. For example, with Tailscale HTTPS enabled and the browser connected to the tailnet:

```sh
tailscale serve --bg --https=443 http://127.0.0.1:8765
```

```toml
[[gateway.upstreams]]
prefix = "slack"
url = "https://mcp.slack.com/mcp"
auth = "oauth"
client_id = "YOUR_SLACK_CLIENT_ID"
client_secret = "YOUR_SLACK_CLIENT_SECRET"
redirect_port = 8765
redirect_uri = "https://YOUR_NODE.YOUR_TAILNET.ts.net/callback"
scopes = ["search:read.public"]
```

Use the actual hostname reported by `tailscale serve`, register that exact redirect with Slack, then run `mcptools gateway login slack`. No TLS listener is built into mcptools. [Tailscale Serve](https://tailscale.com/kb/1242/tailscale-serve) terminates TLS using its provisioned certificate. Slack's [`oauth.v2.user.access`](https://docs.slack.dev/reference/methods/oauth.v2.user.access/) returns the user `access_token` at the top level with `token_type = "user"`; `authed_user` contains identity and scopes. The parser accepts this variant and rejects `ok: false` even when HTTP status is 200.

Protected-resource metadata may identify the exact upstream URL or a same-scheme, same-host, same-port path-segment prefix, including Slack's host-level resource; authorization, token exchange, stored credentials, and refresh use that metadata resource unchanged, while MCP calls retain the configured upstream URL. Stored credentials must satisfy the same resource-matching rule.

Credentials are stored per upstream in `XDG_DATA_HOME/mcptools/oauth` (or `~/.local/share/mcptools/oauth`) in files restricted to the user. Run `mcptools gateway logout <prefix>` to remove credentials. Expired tokens and HTTP 401 responses trigger one refresh; if refresh fails, run login again. MCP requests never start interactive login. Use HTTPS for remote endpoints; loopback HTTP is allowed for local development.

`http_request` sends requests only to exact comma-separated hosts in `MCPTOOLS_HTTP_ALLOWED_HOSTS`, an environment-only setting. Empty denies all. All methods require `allowWrites`. Redirects are returned without following. Supply `body` or `bodyPath`; the latter streams a regular file. Responses are UTF-8, limited to 1 MiB and 10 seconds. `file_stat` returns a regular file size in bytes for upload preparation.
`bodyPath` can read any regular file the user can read; the allowlist controls destinations, not file access. Set narrow exact hosts; wildcard entries such as `*.slack.com` do not grant access.
