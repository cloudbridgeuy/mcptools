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

32 `linear_*` tools expose Linear over MCP (`mcptools mcp stdio` or `mcptools mcp sse`). All return JSON text and matching structured content. List reads return one page (default 25); `all: true` follows cursors until no pages remain. The graph tool has separate hard bounds.

```bash
# Reads
linear_auth_status, linear_issue_get, linear_issue_list, linear_issue_graph
linear_comment_list, linear_relation_list
linear_team_list, linear_team_get
linear_initiative_list (workspace initiatives; optional name query; limit/cursor/all)
linear_initiative_get (required initiative UUID; Markdown content and all directly linked projects)
linear_document_get (required document UUID; Markdown content and nullable owner/project)
linear_document_create (existing project UUID or name with team, nonblank title; optional Markdown content)
linear_project_list, linear_project_get
linear_project_status_list
linear_project_milestone_list
linear_project_update_list (published progress reports; project UUID or name with team; limit/cursor/all)
linear_user_list, linear_state_list, linear_label_list, linear_cycle_list

# Writes
linear_initiative_project_add (needs initiative UUID, project UUID or name with team; links existing entities only)
linear_project_milestone_create (needs project, name; names require team; accepts Markdown description, targetDate, sortOrder)
linear_project_milestone_update (needs milestone UUID; partial update with explicit clear flags)
linear_project_create (needs team, name; accepts description and Markdown content)
linear_project_update (project UUID, or name with team; partial property updates and explicit clear flags)
linear_project_update_create (needs project, nonblank Markdown body; names require team; optional health)
linear_issue_create (needs team, title; accepts labels string-or-array)
linear_issue_update (needs id plus one of title, description, state, assignee, parent, clearParent, labels, clearLabels; state and label names resolve from the issue identifier team, or team; labels replaces, clearLabels empties, the two reject when combined)
linear_comment_create (needs id, body; body trims, empty rejects)
linear_relation_add / linear_relation_remove (source, related, type triple; type is blocks or related)
```

### Project progress reports (MCP only)

`linear_project_update_create({project: "project-UUID", body: "# Progress\nShipping soon.", health: "onTrack"})` creates a progress report, not a project property edit. Names require `team` and use the same all-page resolution with missing/ambiguous matches rejected. Nonblank Markdown body preserves exact whitespace. Optional health is `onTrack`, `atRisk`, or `offTrack`; omitted or null health is not sent, with no invented default. Invalid selectors, blank body, malformed types and unknown health reject before configuration or HTTP. Returns report `id`, `body`, required API `health`, `createdAt`, `url`, and `project: {id, name}`. Requires strict success and complete well-typed fields; HTTP/GraphQL errors, including partial data, fail. MCP-only Write tool; `call_tool` and `execute` require `allowWrites: true`. No CLI command added.

`linear_project_update_list({project: "project-UUID"})` lists published progress reports, not project property changes. It uses the same selectors and strict inputs as milestone listing. It returns `project: {id, name}`, explicit `nodes` (including `[]`), and `pageInfo`. Each node has required `id`, verbatim Markdown `body`, `health` (`onTrack`, `atRisk`, `offTrack`), `createdAt`, `updatedAt`, `url`, and `project: {id, name}`. Null, missing, or malformed non-null fields and mismatched envelope/report project UUIDs fail; project names must be nonblank but may differ after a rename. Names and body whitespace are not changed. The query explicitly requests `orderBy: createdAt` and preserves API order across pages; direction is not asserted. `all` traverses remaining pages with missing, repeated, and non-progressing cursor checks (also checked on a single page) and a 1000-report-page cap. HTTP/GraphQL errors, including partial data, fail without partial output. MCP-only Read; read-only `call_tool` and `execute` need no `allowWrites`. No CLI command added.

### Project property updates (MCP only)

`linear_project_update({id: "project-UUID", content: "    indented Markdown\n", priority: 2})` changes only supplied properties. `id` accepts a UUID without team, or a case-insensitive project name with `team` (id, key, or name). Name resolution reads every team and team-project page and rejects missing or ambiguous matches before mutation. UUID lookup must return the requested project identity before mutation.

Optional properties: nonblank `name`, `description`, Markdown `content` (exact indentation and whitespace preserved), `status` (workspace project status UUID or case-insensitive name), `lead` (user UUID or `me`), `startDate`, `targetDate` (valid `YYYY-MM-DD` calendar dates), and `priority` (integer 0–4; 0 resets priority). Status resolution reads all workspace project-status pages; never issue workflow states. Missing or ambiguous statuses fail before mutation.

Explicit clears: `clearDescription` and `clearContent` set empty strings; `clearLead`, `clearStartDate`, and `clearTargetDate` set null. A property plus its clear flag rejects. Status cannot clear because project status is non-null. Null input values reject; use clear flags. Omitted fields are not sent. Empty updates, blank names/selectors, malformed UUID-shaped selectors, invalid dates, and invalid priority reject before configuration or HTTP.

Output returns `id`, `name`, `url`, non-null `description`, nullable `content`, `status: {id, name, type}`, nullable `lead: {id, name, email}`, nullable `startDate` and `targetDate`, and numeric `priority`. HTTP/GraphQL errors (including partial data), unsuccessful mutations, and missing/malformed selected response fields fail; no false success. This is a Write tool: `call_tool` and `execute` require `allowWrites: true`. No CLI update command is added.

### Project milestones (MCP only)

`linear_project_milestone_update({id: "milestone-UUID", name: "Release"})` partially edits name, verbatim description (including empty/blank strings), valid YYYY-MM-DD targetDate, or finite sortOrder. Omission leaves fields unchanged. `clearDescription` and `clearTargetDate` set null; set/clear conflicts and no-op requests fail before HTTP. Explicit null inputs, unknown fields, blank names and non-UUID ids fail. No project reassignment. Returns the complete milestone with strict success, complete typed properties and case-insensitive matching milestone UUID; project names may change concurrently. MCP-only Write; `call_tool` and `execute` require `allowWrites: true`.

`linear_project_milestone_list({project: "project-UUID"})` reads one project's milestones. Project names require `team` (UUID, key, or name); resolution scans all team and project pages and rejects missing or ambiguous matches. Unknown fields, null inputs, blank selectors/cursors, malformed UUID-shaped selectors, and invalid limits reject before HTTP. `limit` defaults to 25, range 1–250; `cursor` starts at that page; `all` defaults to false and traverses remaining pages.

Output is `{project: {id, name}, nodes, pageInfo}`. Nodes contain usable milestone UUID `id`, `name`, nullable `description` and `targetDate` (explicit nulls), `status` (`done`, `next`, `overdue`, `unstarted`), numeric `sortOrder`, and `project: {id, name}`. Markdown whitespace and API order remain unchanged. Existing empty projects return empty nodes; missing projects fail. Each returned project and milestone project identity must match the requested UUID; malformed payloads or pagination cursors fail without partial success. All-page output carries the final pageInfo, including endCursor. Read-only in direct calls, discovery and code mode; no `allowWrites` required. No CLI command added.

`linear_project_milestone_create({project: "project-UUID", name: "Release"})` is an MCP-only Write tool. Project names require `team` and use the same all-page selector resolution. Optional `description` preserves Markdown and whitespace verbatim, including empty and whitespace-only strings; `targetDate` must be a valid YYYY-MM-DD calendar date; `sortOrder` must be finite. Absent or null optional inputs are omitted, with no invented defaults. Blank names or selectors, unknown fields and malformed types reject before HTTP. Returns milestone id, name, nullable description/targetDate, status, sortOrder and project identity. Requires strict mutation success and complete well-typed output; GraphQL errors fail even with partial data. `call_tool` and `execute` require `allowWrites: true`; no CLI command is added.

### Workspace initiatives (MCP only)

`linear_initiative_list({query: "Roadmap", limit: 25})` lists workspace initiatives, not team projects. Omit arguments for the first page. Optional `query` is a case-sensitive name substring, sent as `InitiativeFilter.name.contains`; no client-side filtering. No team selector or CLI command. Unknown fields, explicit null inputs, blank query/cursor, control characters in cursors, malformed types and limits outside 1–250 reject before HTTP. `limit` defaults to 25; `all` defaults to false. `cursor` starts after that cursor; `all: true` reads remaining pages, with a 1000-page cap. Pages with `hasNextPage: true` require a new cursor, including single-page reads. Terminal pages do not need cursor progress.

Output is `{nodes, pageInfo}`; empty results include `nodes: []`. Each node has initiative UUID `id`, `name`, nullable `description`, `status` (`Proposed`, `Planned`, `Active`, `Completed`, `Canceled`), nullable `targetDate`, nullable `owner: {id, name, email}`, and API `url`. Null optional output fields remain explicit. UUIDs are suitable for initiative detail and project association. No large `content`, documents or nested projects are selected. Text and API order remain unchanged; the query requests `orderBy: createdAt`, without claiming sort direction. All-page output carries final pageInfo. HTTP/GraphQL/permission errors and malformed output fail without partial success. Read-only direct calls, discovery `call_tool`, and code-mode `execute` need no `allowWrites`.

`linear_initiative_get({id: "initiative-UUID"})` queries `initiative(id)` and returns one initiative with the same metadata plus nullable verbatim Markdown `content` and `projects: [{id, name}]`. Required `id` must be an exact UUID; blank, malformed, missing or null IDs, wrong types and unknown fields reject before HTTP. Missing initiatives fail, distinct from an existing initiative with `projects: []`. Nullable selected fields must be present and remain explicit nulls. Malformed fields, mismatched UUIDs and HTTP/GraphQL errors fail without partial output. Projects include archived directly linked projects (`includeArchived: true`, `includeSubInitiatives: false`), preserve API createdAt order, and always traverse pages of 250 with missing/repeated/invalid cursor checks and a 1000-page cap. Overflow fails instead of truncating. Terminal pages need no cursor progress. No team selector, documents, association mutations or CLI command. MCP-only Read in direct, discovery and code mode; no `allowWrites` needed. Schema verified 2026-10-08 from the public Linear SDK schema: `Query.initiative(id: String!)`, nullable `Initiative.content`, and `Initiative.projects(first, after, includeArchived, includeSubInitiatives, orderBy)`. No authenticated live API call.

`linear_initiative_project_add({initiative: "initiative-UUID", project: "project-UUID"})` links an existing project to an existing initiative; it creates neither entity. Required `initiative` is a UUID; required `project` is a UUID or project name with `team` (UUID, key, or name). Both entities resolve before `initiativeToProjectCreate`; missing or ambiguous selectors fail before mutation. Invalid inputs and unknown fields reject before HTTP. Returns association `id`, `initiative: {id, name}`, and `project: {id, name}` with strict mutation success and matching identities. HTTP/GraphQL errors, duplicate links and hierarchy conflicts fail without mutation retry or claimed success. MCP-only Write; `call_tool` and `execute` require `allowWrites: true`. No CLI command added.

### Project documents (MCP only)

`linear_document_create({project: "project-UUID", title: "Delivery plan", content: "# Plan\n"})` creates a document attached to an existing project via `documentCreate(input: DocumentCreateInput)`. Project names require `team` (UUID, key, or name); UUIDs do not. Nonblank title and valid selectors are checked before HTTP; missing or ambiguous projects fail before mutation. Title is trimmed; Markdown content is preserved exactly, including indentation and empty strings. Omitted or null content is omitted from the mutation. Returns `id`, `title`, nullable `content`, `url`, `createdAt`, `updatedAt`, and non-null `project: {id, name}`. Mutation success, complete selected output, and matching project identity are checked. HTTP/GraphQL/permission errors fail without partial success or automatic mutation retry. MCP-only Write; discovery `call_tool` and code-mode `execute` require `allowWrites: true`. No initiative/issue/cycle associations, rich-text JSON, attachments, or CLI command. No live API writes used for verification.

`linear_document_get({id: "document-UUID"})` queries `document(id)` and returns `id`, `title`, nullable verbatim Markdown `content`, `url`, `createdAt`, `updatedAt`, nullable `owner: {id, name, email}`, and nullable `project: {id, name}`. Documents outside projects return explicit `project: null`; null content and owner also remain explicit. Empty titles are valid and preserved. Required `id` must be an exact UUID; slugs, blank or malformed IDs, missing/null/wrong-typed inputs, and unknown fields reject before HTTP. Missing documents, mismatched IDs, malformed selected fields, and HTTP/GraphQL/permission errors fail without partial output. No document listing, attachments, export/file generation, or CLI command. MCP-only Read in direct, discovery and code mode; no `allowWrites` needed. Schema verified 2026-10-08 from the public Linear SDK schema: `Query.document(id: String!): Document!`; nullable `Document.content`, `owner`, and `project`; non-null selected identity, title, URL, and timestamps. No authenticated live API call.

Schema basis verified 2026-10-08: [Linear SDK GraphQL schema](https://github.com/linear/linear/blob/master/packages/sdk/src/schema.graphql), `Query.initiatives(filter: InitiativeFilter, first, after, orderBy)`, `InitiativeFilter.name: StringComparator`, `StringComparator.contains`, and the selected `Initiative`/`User` fields. This is schema verification, not an authenticated live API test. Initiative listing excludes archived resources by API default.

### Workspace project statuses

`linear_project_status_list({limit: 25, cursor: "cursor", all: false})` reads the workspace `projectStatuses` connection, not team workflow states. MCP only; no CLI command or team selector. Omit arguments for the first page. `limit` defaults to 25, range 1–250; `all` defaults to false. Unknown fields, invalid types or bounds, and blank cursors reject before configuration or HTTP.

Output is `{nodes, pageInfo}`. Each node has returned `id`, `name`, `type`, `color`, floating-point `position`, and optional `description` (omitted when null). Pages retain API order with explicit `orderBy: createdAt`; no category names or per-page sorting are imposed. `all: true` accumulates pages and returns the final pageInfo. Missing, blank, unchanged or repeated continuation cursors fail instead of looping or returning partial success. HTTP, GraphQL and malformed-response errors also fail. This is a read-only tool in direct calls, discovery, and code mode; no `allowWrites` needed.

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
