# Linear Integration

## Auth

```bash
export LINEAR_API_KEY="lin_api_..."
mcptools linear auth status
mcptools linear auth status --json
```

Missing or invalid credentials return actionable errors. The key never appears in output.

## CLI Commands

### Issues

```bash
# Get one issue by UUID or identifier
mcptools linear issue get GUZ-79

# List issues, newest first
mcptools linear issue list --limit 5

# Order by urgency (priority first, No priority last) or by recency
mcptools linear issue list --sort priority
mcptools linear issue list --sort updatedAt

# Filter by team, state, label, cycle, assignee, title text
mcptools linear issue list --team GUZ --state "In Progress"
mcptools linear issue list --team GUZ --project "MCPTools"
mcptools linear issue list --assignee me
mcptools linear issue list --label Bug --query "auth"
mcptools linear issue list --cycle 3
mcptools linear issue list --updated-after 2026-01-01T00:00:00Z

# Pagination
mcptools linear issue list --cursor <cursor>
mcptools linear issue list --all

# Output as JSON
mcptools linear issue list --json

# Create one issue (labels repeatable or comma-separated)
mcptools linear issue create --team GUZ --title "Wire the thing" --label docs
mcptools linear issue create --team GUZ --title "Wire the thing" --label docs,api --label urgent

# Update fields. State and label names use the identifier team; --team overrides
mcptools linear issue update GUZ-22 --state Done
mcptools linear issue update GUZ-22 --state Done --team GUZ
mcptools linear issue update GUZ-22 --label docs,api
mcptools linear issue update GUZ-22 --clear-labels
```

Selector rules:

- `--team` accepts id, key, or name.
- `--project` accepts id or name. Names need `--team`.
- `--assignee` accepts a user UUID or `me` (current viewer). Find UUIDs with `linear users list --query NAME`.
- `--state` and `--label` match by name (labels also accept UUIDs). `--cycle` accepts a cycle number or id.
- `--query` matches a title substring. `--updated-after` needs RFC3339 and is rejected before any request when malformed.
- `--sort` accepts `priority` or `updatedAt`. Omitted, the order stays newest-first.
- Issue update `--state` and `--label` names resolve from the issue identifier team (`GUZ-22` → `GUZ`). `--team` overrides. UUID issue ids still need `--team` or a state/label UUID.
- Issue update `--label` replaces the full label set; `--clear-labels` (alias `--clear-label`) empties it. The two flags reject when combined.

### Chart

```bash
# Render an HTML Mermaid chart of issues, sub-issues, parents, and blockers, then open it
mcptools linear chart GUZ-185 GUZ-186 GUZ-188
mcptools linear chart GUZ-185 --out /tmp/linear-chart.html
mcptools linear chart GUZ-185 --no-open
mcptools linear chart --project modelops-cycles --team GUZ
mcptools linear chart --project modelops-cycles --team GUZ --exclude-completed --limit 1000
# Interactive server: refresh button + `r` key, click ticket for detail modal + state transition
mcptools linear chart GUZ-185 --serve
mcptools linear chart GUZ-185 --serve --port 8080 --no-open
```

`chart` walks the transitive closure of children, parents, `blocked_by` blockers, and forward `blocks` targets (cap `--limit`, default 300, warning on the cap), drops canceled issues, and classifies the rest as complete, in progress, frontier (ready to start), or fog (prerequisite incomplete). Nodes group into one subgraph per parent issue; top-level issues without sub-issues share a Standalone subgraph laid out as a grid instead of floating. `--exclude-completed` hides completed issues and drops their blocker edges so dependents render as frontier. Output defaults to `<temp dir>/mcptools-linear-chart-<project-or-first-id>.html`. Empty issue ids reject before any request. `--project` seeds the chart with every issue in the project (project names need `--team`); positional issue ids can be combined with `--project`. `--serve` starts a foreground loopback server instead of writing a file (default `--port 0` assigns a random open port, `PORT` env also read; `--out` rejects): `GET /api/chart` rebuilds the closure, click-drag pans and scrollwheel zooms the diagram, ticket clicks open a detail modal with markdown-rendered description and comments plus activity, a Refresh issue button reloads one ticket, the state select optimistically posts to `POST /api/issues/:id/state` and refreshes the diagram only, Refresh button and `r` keybinding reload.

### Discovery

```bash
mcptools linear teams list
mcptools linear teams get GUZ
mcptools linear projects list --team GUZ
mcptools linear projects get "MCPTools" --team GUZ
mcptools linear users list --query "Ada"
mcptools linear states list --team GUZ
mcptools linear labels list --team GUZ
mcptools linear cycles list --team GUZ
```

`users list` needs `--query`. `projects`, `states`, `labels`, `cycles` need `--team`. Ambiguous names fail with candidate IDs instead of picking a match.

## Output

- Tables show compact rows plus a `hasMore/endCursor` line. Truncation is never hidden.
- `issue get` and `issue list` include `Project` (project name, empty when none), `Parent` (parent issue identifier, empty when none) and `BlockedBy` (comma-separated identifiers of issues blocking this one). `--json` carries the same data as `project` (`{id, name}`, absent when none), `parent` and `blocked_by` fields. Issue `labels` (name list, empty when none) ride along in `--json` and in create/update output.
- `issue get` and `issue list` include `priority`, the label string (`Urgent`, `High`, `Medium`, `Low`, `No priority`), never the 0–4 number. `--json` carries it as `priority`. A `--sort` table run adds a `Priority` column between `State` and `Project`; the table otherwise shows `ID Identifier Title State [Priority] Project Parent BlockedBy`. `issue get` prints a `Project:` line and create/update tables show a `Project` column.
- `issue get` prints snapshot fields, then `Comments (N):`, then `Activity (N):`. A missing comment author or activity actor prints as `unknown`. Activity always starts with a created row. History after created is capped at 50 events.
- CLI `issue get --json` and MCP `linear_issue_get` serialize the same object, with top-level `comments` and `activity` arrays.
- `--json` returns `{"nodes": [...], "pageInfo": {...}}`.
- `--all` follows cursors until no pages remain.

## MCP Tools

19 `linear_*` tools expose Linear over MCP (`mcptools mcp stdio` or `mcptools mcp sse`). All return JSON text and matching structured content. List reads return one page (default 25); `all: true` follows cursors until no pages remain. The graph tool has separate hard bounds.

```bash
# Reads
linear_auth_status, linear_issue_get, linear_issue_list, linear_issue_graph
linear_comment_list, linear_relation_list
linear_team_list, linear_team_get
linear_project_list, linear_project_get
linear_user_list, linear_state_list, linear_label_list, linear_cycle_list

# Writes
linear_issue_create (needs team, title; accepts labels string-or-array)
linear_issue_update (needs id plus one of title, description, state, assignee, parent, clearParent, labels, clearLabels; state and label names resolve from the issue identifier team, or team; labels replaces, clearLabels empties, the two reject when combined)
linear_comment_create (needs id, body; body trims, empty rejects)
linear_relation_add / linear_relation_remove (source, related, type triple; type is blocks or related)
```

### Structured issue graph

`linear_issue_graph({ids: ["GUZ-185"], limit: 100, maxPages: 2})` returns the chart's dependency closure without rendering Mermaid/HTML, opening a browser, writing a file, or starting a chart server. It is a read-only, non-spend tool in full mode, discovery via `call_tool`, declarations, and code-mode `execute`.

- Input: nonempty `ids` array, at most 300 entries; each trimmed selector is nonempty and at most 128 bytes. Roots are sorted and deduplicated. `limit` defaults to 100, range 1–300, and must cover all unique roots. `maxPages` defaults to 2, range 1–10. Unknown properties, null bounds, negative/fractional bounds, and invalid input reject before configuration or HTTP.
- Traversal: breadth-first through children, incoming `blocks` blockers (`blocked_by`), outgoing `blocks` targets, and parents. Other relation types are not dependencies. Each issue query fetches at most 50 children and 25 relations per direction, with independent cursors. Completed connections are not fetched again. Fetch attempts, including missing selectors and aliases, cannot exceed `limit`; each attempt uses at most `maxPages` queries. The existing HTTP client's bounded read retries still apply. No workspace-wide issue list is fetched.
- `roots`: `{selector, identifier}` pairs; canonical identifier is null when unresolved. `nodes`: sorted by identifier, each with `identifier`, `title`, `url`, `state_type`, `parent`, sorted/deduplicated `children`, `blocked_by`, `blocks`, and `class`. Relation identifiers can refer outside the fetched set. Classes reuse the CLI's `complete`, `inprogress`, `frontier`, `fog`; canceled nodes remain in this structured graph with class null. Completed and canceled blockers satisfy dependencies. Parent/child containment alone does not block an issue.
- `frontier`: ready-to-start identifiers from chart classification, excluding issues whose incoming blocker connection is incomplete. `inprogress`: started identifiers. `stats`: chart counts; canceled nodes are excluded. Per-node classes and stats remain provisional when blocker paging is incomplete, so `stats.frontier` can exceed `frontier.length`.
- `missing_blockers`: `{issue, blocker}` pairs whose blocker is outside the fetched set. `unresolved_blockers`: all observed blockers that are missing or neither completed nor canceled, including blockers of started/completed issues. These lists describe observed relations, not unseen pages.
- `limits`: `{issues, pages_per_issue, children_per_page, relations_per_page}`; `fetched_count` counts distinct returned nodes, `attempted_count` counts selector fetches. `cap_hit` means the selector bound left work in sorted `pending`. `truncated` means pending selectors or incomplete connection pages remain. `missing_issues` contains sorted selectors whose lookup returned null; missing issues are reported separately and do not alone set `truncated`.
- `incomplete_connections`: `{identifier, connection, reason, cursor}` records. Connections are `children`, `blocks`, `blocked_by`; reasons are `page_limit` or `invalid_cursor` (missing, empty, or repeated cursor). A page-limit cursor is the last fetched page's end cursor. Root/dependency null lookups are reported, but authentication, transport, GraphQL, malformed response, and disappearing-while-paging failures are errors, never a successful partial graph.

For a truncated graph, fetch `pending` as roots or raise the bounds within the hard limits. Use the issue list and relation list tools for larger inventories and explicit paging. Code-mode's own time/output limits still apply; use `call_tool` for graphs that exceed its envelope.

`linear_relation_add` returns `created` or `already_exists`. `linear_relation_remove` matches the `(source, related, type)` triple and returns the deleted relation id.
