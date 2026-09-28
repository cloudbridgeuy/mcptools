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

# Update fields. State names use the identifier team; --team overrides
mcptools linear issue update GUZ-22 --state Done
mcptools linear issue update GUZ-22 --state Done --team GUZ
```

Selector rules:

- `--team` accepts id, key, or name.
- `--project` accepts id or name. Names need `--team`.
- `--assignee` accepts a user UUID or `me` (current viewer). Find UUIDs with `linear users list --query NAME`.
- `--state` and `--label` match by name. `--cycle` accepts a cycle number or id.
- `--query` matches a title substring. `--updated-after` needs RFC3339 and is rejected before any request when malformed.
- `--sort` accepts `priority` or `updatedAt`. Omitted, the order stays newest-first.
- Issue update `--state` names resolve from the issue identifier team (`GUZ-22` → `GUZ`). `--team` overrides. UUID issue ids still need `--team` or a state UUID.

### Chart

```bash
# Render an HTML Mermaid chart of issues, sub-issues, and blockers, then open it
mcptools linear chart GUZ-185 GUZ-186 GUZ-188
mcptools linear chart GUZ-185 --out /tmp/linear-chart.html
mcptools linear chart GUZ-185 --no-open
mcptools linear chart --project modelops-cycles --team GUZ
```

`chart` walks the transitive closure of children and `blocked_by` blockers (cap 300 issues, warning on the cap), drops canceled issues, and classifies the rest as complete, in progress, frontier (ready to start), or fog (prerequisite incomplete). Output defaults to `<temp dir>/mcptools-linear-chart-<project-or-first-id>.html`. Empty issue ids reject before any request. `--project` seeds the chart with every issue in the project (project names need `--team`); issue ids and `--project` are mutually alternative inputs.

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
- `issue get` and `issue list` include `Parent` (parent issue identifier, empty when none) and `BlockedBy` (comma-separated identifiers of issues blocking this one). `--json` carries the same data as `parent` and `blocked_by` fields.
- `issue get` and `issue list` include `priority`, the label string (`Urgent`, `High`, `Medium`, `Low`, `No priority`), never the 0–4 number. `--json` carries it as `priority`. A `--sort` table run adds a `Priority` column between `State` and `Parent`; the default table keeps its original columns.
- `issue get` prints snapshot fields, then `Comments (N):`, then `Activity (N):`. A missing comment author or activity actor prints as `unknown`. Activity always starts with a created row. History after created is capped at 50 events.
- CLI `issue get --json` and MCP `linear_issue_get` serialize the same object, with top-level `comments` and `activity` arrays.
- `--json` returns `{"nodes": [...], "pageInfo": {...}}`.
- `--all` follows cursors until no pages remain.

## MCP Tools

18 `linear_*` tools mirror the CLI over MCP (`mcptools mcp stdio` or `mcptools mcp sse`). All return JSON text content. Single reads return one page (default 25); `all: true` follows cursors until no pages remain.

```bash
# Reads
linear_auth_status, linear_issue_get, linear_issue_list
linear_comment_list, linear_relation_list
linear_team_list, linear_team_get
linear_project_list, linear_project_get
linear_user_list, linear_state_list, linear_label_list, linear_cycle_list

# Writes
linear_issue_create (needs team, title)
linear_issue_update (needs id plus one of title, description, state, assignee, parent, clearParent; state names resolve from the issue identifier team, or team)
linear_comment_create (needs id, body; body trims, empty rejects)
linear_relation_add / linear_relation_remove (source, related, type triple; type is blocks or related)
```

`linear_relation_add` returns `created` or `already_exists`. `linear_relation_remove` matches the `(source, related, type)` triple and returns the deleted relation id.
