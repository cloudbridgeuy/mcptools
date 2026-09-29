# MCPTools

CLI and MCP tools that expose Linear, Jira, Atlas, and related services to coding agents.

## Language

**Issue identifier**:
Linear `KEY-NUMBER` handle for an issue, such as `GUZ-22`.
_Avoid_: ticket ID, issue UUID

**Team key**:
Linear team's short uppercase code. It is the prefix of an **Issue identifier**.
_Avoid_: team name, team id

**Activity**:
Time-ordered issue events from Linear history, always starting with created.
_Avoid_: history, timeline, audit log

**Discovery mode**:
MCP server mode in which `tools/list` returns only `find_tools`.
_Avoid_: finder mode, list gating

## Relationships

- An **Issue identifier** contains exactly one **Team key**

## Example dialogue

> **Dev:** "Does `issue update GUZ-22 --state Done` need `--team GUZ`?"
> **Domain expert:** "No. `GUZ-22` is an **Issue identifier**, so the **Team key** is `GUZ`. Pass `--team` only to override that, or when the id is a UUID."

## Behavior

Linear command reference: [Linear](.claude/context/linear.md).
MCP server reference: [MCP Server](.claude/context/mcp-server.md).

### Requirement: Discovery mode tool list
The `--discovery` flag or `MCPTOOLS_DISCOVERY=true` starts the MCP server in **Discovery mode** on stdio and SSE. `MCPTOOLS_DISCOVERY` also accepts `y`, `yes`, `t`, `true`, `on`, `1`, `n`, `no`, `f`, `false`, `off`, and `0`, in upper or lower case. `tools/call` dispatches every registered tool by name in every mode.

#### Scenario: Flag or env enables the mode
- **WHEN** the server starts with `--discovery`, or with `MCPTOOLS_DISCOVERY=true` and no flag
- **THEN** `tools/list` returns exactly one tool, `find_tools`

#### Scenario: Default list
- **WHEN** the server starts with neither the flag nor the env var
- **THEN** `tools/list` returns every registered tool

#### Scenario: Unlisted tool call
- **WHEN** a client in **Discovery mode** sends `tools/call` with the name of a tool that `tools/list` did not return
- **THEN** the server dispatches that tool and returns its result

#### Scenario: Invalid env value
- **WHEN** `MCPTOOLS_DISCOVERY` holds a value outside the accepted list, including an empty value
- **THEN** the process exits with code 2 and an error that says `value was not a boolean`

### Requirement: Code mode tool list
The `--code-mode` flag or `MCPTOOLS_CODE_MODE=true` starts the MCP server in code mode on stdio and SSE. `MCPTOOLS_CODE_MODE` accepts the same values as `MCPTOOLS_DISCOVERY`; any other value exits with code 2. `tools/call` dispatches every registered tool by name in every mode.

#### Scenario: Flag or env enables the mode
- **WHEN** the server starts with `--code-mode`, or with `MCPTOOLS_CODE_MODE=true` and no flag
- **THEN** `tools/list` returns exactly two tools, in order: `find_tools`, then `execute`

#### Scenario: Code mode wins over discovery
- **WHEN** the server starts with `--discovery` and `--code-mode`, or with both env vars enabled
- **THEN** `tools/list` returns `find_tools` and `execute`, not one tool

#### Scenario: Unlisted tool call
- **WHEN** a client in code mode sends `tools/call` with the name of a tool that `tools/list` did not return
- **THEN** the server dispatches that tool and returns its result

#### Scenario: Code-mode usage text
- **WHEN** `find_tools` runs in code mode, including a nested call inside `execute`
- **THEN** its `usage` field is `Call each declared function as an async global inside tools.mcptools.execute.`

#### Scenario: Usage text without code mode
- **WHEN** `find_tools` runs in any other mode
- **THEN** its `usage` field is `Each declaration is the call signature: the input interface is the tools/call arguments object, the Promise type is the result.`

### Requirement: Issue get payload
`linear issue get` and `linear_issue_get` return the same payload: snapshot fields, comments, and **Activity**. **Activity** always includes a created row. Comments stay on `comments`.

#### Scenario: Human get
- **WHEN** an issue is fetched without `--json`
- **THEN** stdout prints snapshot fields, then `Comments (N):`, then `Activity (N):`
- **AND** **Activity** includes a created row
- **AND** a missing comment author or **Activity** actor prints as `unknown`

#### Scenario: JSON and MCP match
- **WHEN** an issue is fetched with `--json` or via `linear_issue_get`
- **THEN** the object has `comments` and `activity` arrays at the top level
- **AND** CLI `--json` and MCP structuredContent serialize the same type

### Requirement: Issue-update state team
Issue update resolves a workflow state name against the **Team key** in the **Issue identifier**. An explicit team argument overrides that key. A state UUID skips team resolution.

#### Scenario: Identifier supplies team
- **WHEN** an issue is updated by **Issue identifier** with a state name and no team argument
- **THEN** the state name resolves against the **Team key** in that identifier

#### Scenario: Explicit team overrides identifier
- **WHEN** an issue is updated by **Issue identifier** with a state name and an explicit team argument
- **THEN** the state name resolves against the explicit team

#### Scenario: Issue UUID with state name
- **WHEN** an issue is updated by UUID with a state name and no team argument
- **THEN** the update fails and asks for a team argument or a state UUID

### Requirement: Comment create body source
`linear issue comments create` takes exactly one of `--body TEXT` or `--body-file PATH`. `--body-file -` reads stdin to EOF. `--json` prints JSON and does not select stdin. `--body -` is the body text `-`.

#### Scenario: Body file with open stdin
- **WHEN** comments create runs with `--body-file` a real path and stdin is a non-TTY stream that does not EOF
- **THEN** the process does not wait on stdin
- **AND** the comment body is the file contents

#### Scenario: Stdin sentinel
- **WHEN** `--body-file -`
- **THEN** the process reads stdin to EOF for the body

### Requirement: Execute tool
The `execute` MCP tool runs JavaScript in a sandbox and returns `logs`, `result`, and `error` in `structuredContent`. Limits come from the `--execute-timeout-secs`, `--execute-output-kb`, and `--execute-memory-mb` server flags. Gating is by tool kind: read tools bind by default, `allowWrites` adds write tools, `allowSpend` adds spend tools, and `execute` never binds as a global inside its own sandbox. A missing gated tool keeps `error.name` at `ReferenceError` and reports a message naming the tool, its kind, and the flag to pass; any other missing global keeps the plain `is not defined` message.

#### Scenario: Success payload
- **WHEN** the script finishes without throwing
- **THEN** `structuredContent` holds `logs`, the final value as `result`, and `error: null`
- **AND** the envelope has no `isError`

#### Scenario: Script throws
- **WHEN** the script throws a JavaScript error
- **THEN** `error.name` is the error's own name and `error.message` is its message
- **AND** `result` is `null`, `logs` keeps the lines printed before the throw, and the envelope has `isError: true`

#### Scenario: Timeout
- **WHEN** the run exceeds `--execute-timeout-secs`
- **THEN** `error.name` is `TimeoutError`
- **AND** `logs` keeps the lines printed before the deadline

#### Scenario: Output cap
- **WHEN** the logs and result exceed `--execute-output-kb`
- **THEN** `error.name` is `OutputLimitError`
- **AND** `logs` holds the lines that fit under the cap

#### Scenario: Memory exhaustion
- **WHEN** the script exhausts `--execute-memory-mb`
- **THEN** `error.name` is `InternalError`

#### Scenario: Kind gating
- **WHEN** `execute` runs with neither `allowWrites` nor `allowSpend`
- **THEN** only read tools bind as globals in the sandbox

#### Scenario: Gated tool message
- **WHEN** the script references a write or spend tool that did not bind
- **THEN** `error.name` is `ReferenceError` and `error.message` names the tool, its kind, and the flag to pass
- **AND** a missing global that is not a gated tool keeps the plain `is not defined` message

#### Scenario: Missing code
- **WHEN** `tools/call` invokes `execute` without `code`
- **THEN** the server returns a JSON-RPC error with code `-32602`

### Requirement: Linear chart rendering
`linear chart` takes one or more **Issue identifier**s **or a `--project` selector (project names need `--team`)**, fetches the transitive closure of sub-issues, parents, `blocked_by` blockers, and forward `blocks` targets (cap `--limit`, default 300 issues, stderr warning when the cap stops expansion), drops canceled issues, and classifies the rest as complete, in progress, frontier (no incomplete blocker), or fog (at least one incomplete blocker). `--exclude-completed` hides completed issues and drops their blocker edges so dependents render as frontier. It writes a full-viewport dark-themed HTML Mermaid page — one subgraph per parent issue, state legend — to `<temp dir>/mcptools-linear-chart-<project-or-first-id>.html` unless `--out` overrides it, prints the output path and a stats line, and opens the page in the default browser unless `--no-open`. `--serve` instead starts a foreground loopback server (default `--port 0` assigns a random open port, `PORT` env also read) that serves an interactive page plus JSON APIs: `GET /api/chart` refreshes the closure, `GET /api/issues/:id` returns full details, `GET /api/states?team=` lists workflow states, `POST /api/issues/:id/state` transitions state. The page has a Refresh button plus `r` keybinding, click-drag pans and scrollwheel zooms the diagram, ticket clicks open a detail modal with markdown-rendered description and comments, and a state select that auto-posts a transition then refreshes the diagram only. `--out` conflicts with `--serve`.

#### Scenario: Closure and classification
- **WHEN** chart runs on a set of **Issue identifier**s
- **THEN** every transitive sub-issue, parent, blocker, and forward-blocked issue appears as a node, grouped into one subgraph per parent issue
- **AND** completed issues are complete, started issues are in progress, not-yet-started issues with no incomplete blocker are frontier, and issues with at least one incomplete blocker are fog
- **AND** canceled issues are absent and do not block dependents

#### Scenario: Exclude completed
- **WHEN** chart runs with `--exclude-completed`
- **THEN** completed issues are absent, their parent links and blocker edges are dropped, and dependents of completed blockers render as frontier with no missing-blocker warning for completed blockers

#### Scenario: Project seed
- **WHEN** chart runs with `--project` (and `--team` when the selector is a project name)
- **THEN** every issue in the project is seeded into the chart with the same closure and classification rules
- **AND** the page title is the project selector

#### Scenario: Cap stops expansion
- **WHEN** the closure exceeds `--limit` (default 300 issues)
- **THEN** expansion stops, stderr prints a cap warning, and the partial chart still renders

#### Scenario: Blocker outside the fetched set
- **WHEN** an issue is blocked by an issue outside the fetched set
- **THEN** stderr warns and the blocked issue renders as fog

#### Scenario: Output location and browser launch
- **WHEN** chart runs with `--out PATH`
- **THEN** the page is written to PATH; without `--out` it is written to `<temp dir>/mcptools-linear-chart-<project-or-first-id>.html`
- **AND** the default browser opens it unless `--no-open` is set
- **AND** stdout prints the output path, then `<N> issues: <n> complete, <n> in progress, <n> frontier, <n> fog`

#### Scenario: Blank issue id
- **WHEN** chart runs with an empty or blank issue id
- **THEN** the command fails with an error naming the problem before any Linear request

#### Scenario: No input
- **WHEN** chart runs with neither issue ids nor `--project` (or with `--team` alone)
- **THEN** it fails before any Linear request

#### Scenario: Serve loop
- **WHEN** chart runs with `--serve`
- **THEN** a foreground loopback server serves an interactive page; `GET /api/chart` rebuilds the closure, click-drag pans and scrollwheel zooms the diagram, ticket clicks open a detail modal with markdown-rendered description plus comments and activity, state select auto-posts to Linear and refreshes the diagram only, stale responses never touch a moved-on modal, Refresh button and `r` keybinding reload, and `--out` rejects

#### Scenario: Deterministic output
- **WHEN** chart runs twice on the same issue set
- **THEN** both runs write byte-identical HTML

### Requirement: Tool output field projection
`pdf_read`, `md_fetch`, `bitbucket_pr_read`, `jira_search`, and `linear_issue_list` accept an optional `fields` argument: an array of dotted paths. When present and non-empty, the tool returns only the named fields. Projection runs after the handler builds its output and after all existing output shaping (lineLimit truncation, pagination assembly), before serialization. `content[0].text` and `structuredContent` carry the same projected value. Each tool's `outputSchema` keeps the full output shape with every field optional, and each tool description documents `fields`.

#### Scenario: Named fields only
- **WHEN** `tools/call` invokes `pdf_read` with `fields: ["id", "title"]`
- **THEN** `structuredContent` contains exactly the keys `id` and `title`
- **AND** `content[0].text` parses back to the same value as `structuredContent`

#### Scenario: Array outputs project per element
- **WHEN** `tools/call` invokes `linear_issue_list` with `fields: ["nodes.identifier", "nodes.title", "pageInfo"]`
- **THEN** every element of `nodes` contains exactly `identifier` and `title`, and `pageInfo` is still present

#### Scenario: Absent or empty fields
- **WHEN** `tools/call` invokes any of the five tools without `fields`, or with `fields: []`
- **THEN** the full output returns unchanged, byte-identical to the pre-`fields` behavior

#### Scenario: Unknown field path
- **WHEN** `tools/call` invokes any of the five tools with a `fields` path that resolves to no field in the actual output, e.g. `["bogus"]`
- **THEN** the server returns a JSON-RPC error with code `-32602` and message `Unknown field path: "bogus"`

#### Scenario: Catalog advertises the parameter
- **WHEN** `tools/list` returns any of the five tools
- **THEN** the input schema shows `fields` as an optional array of strings
- **AND** the description documents `fields`
- **AND** the output schema contains no `required` key anywhere
