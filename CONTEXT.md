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
The `--discovery` flag or `MCPTOOLS_DISCOVERY=true` starts the MCP server in **Discovery mode** on stdio and SSE. `tools/call` dispatches every registered tool by name in both modes.

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
- **WHEN** `MCPTOOLS_DISCOVERY` holds a value other than `true` or `false`, including an empty value
- **THEN** the process exits with a parse error that names the two valid values

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
