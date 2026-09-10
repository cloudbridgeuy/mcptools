# Linear CLI and global agent setup

Status: prepared, not created in Linear. On 2026-09-10, `LINEAR_API_KEY` was present, but the authenticated project lookup failed with DNS resolution error. No mutation ran. No global client configuration changed.

11 tickets: 1 parent and 10 implementation tickets. 10 parent links and 15 blocking dependencies. The existing importer validates the backlog and dependency order.

Target: existing MCPTools project. Project/team IDs below come from `../atlas-redesign/backlog.linear-receipt.json`; remote identity and existing overlapping tickets could not be checked in this session. The importer verifies project/team association before writes. Review existing project tickets for semantic overlap when access returns; importer markers only prevent duplicate imports of this backlog.

Repository inspected: `22ee93ccfa00283a3f7526a160326c40868a11e4`. Existing global installation installs the executable only. Existing agent setup is specific to Atlas and local Claude guidance. Reuse these components where applicable, plus the shared CLI/MCP operation pattern.

## Import

Run from the repository root in a session with Linear network access and `LINEAR_API_KEY` already exported. This reuses the existing importer without changes; its historical marker prefix is retained, with a distinct initiative ID for this backlog.

```sh
python3 docs/atlas-redesign/import_linear.py docs/linear-cli/backlog.json
python3 docs/atlas-redesign/import_linear.py docs/linear-cli/backlog.json --apply --project-id 5fe03c79-0c6b-4e49-a56d-f10421143727 --team-id e3b567ba-bcd1-42d8-8ae8-129fd11c97ab
```

A successful import writes `backlog.linear-receipt.json` with issue URLs and dependency receipts. Reruns reconcile existing importer markers. If a mutation outcome is uncertain, inspect Linear before continuing. The importer does not update previously created descriptions.

## Ticket map

| Key | Ticket | Blocked by |
| --- | --- | --- |
| LINEAR-00 | Add Linear CLI and MCP tools with global agent setup | None |
| LINEAR-01 | Add the shared Linear client and noninteractive output contract | None |
| LINEAR-02 | Add Linear workspace discovery and identifier resolution | LINEAR-01 |
| LINEAR-03 | Add paginated Linear issue search and detail commands | LINEAR-02 |
| LINEAR-04 | Add Linear issue creation and targeted updates | LINEAR-02 |
| LINEAR-05 | Add Linear comments and issue relationships | LINEAR-03, LINEAR-04 |
| LINEAR-06 | Expose Linear workflows through the existing MCP server | LINEAR-03, LINEAR-04, LINEAR-05 |
| AGENTS-01 | Add user-level mcptools setup, status, and uninstall | None |
| AGENTS-02 | Integrate mcptools globally with Codex and Claude Code | AGENTS-01, LINEAR-06 |
| AGENTS-03 | Integrate mcptools globally with pi and opencode2 | AGENTS-01, LINEAR-06 |
| LINEAR-07 | Verify and document the full Linear agent workflow | LINEAR-06, AGENTS-02, AGENTS-03 |

## Ticket details

### LINEAR-00: Add Linear CLI and MCP tools with global agent setup

Enable coding agents to discover and use Linear through mcptools from any working directory. Reuse the existing CLI, MCP server, and binary installer. Deliver focused issue workflows first. Proposed command names below may follow existing repository conventions.

Acceptance criteria:

- [ ] An agent can find a team/project, search and read issues, create or update an issue, comment, and manage parent/blocking relations through CLI and MCP.
- [ ] Codex, Claude Code, pi, and opencode2 can discover mcptools after one user-level setup; include status and uninstall.
- [ ] Use LINEAR_API_KEY from the process environment. Never write the key into tickets, logs, skills, or client configuration.
- [ ] Each child issue is complete with offline tests and a documented live verification result or explicit access limitation.
- [ ] Initial scope excludes OAuth hosting, webhooks, autonomous background work, file uploads, arbitrary GraphQL, and full Linear administrative CRUD.

### LINEAR-01: Add the shared Linear client and noninteractive output contract

Add mcptools linear and a shared typed GraphQL client for all Linear CLI and MCP operations. Keep HTTP effects separate from input and response conversion. Reuse installed HTTP/JSON dependencies.

Sources: https://linear.app/developers/graphql and https://linear.app/developers/rate-limiting

Acceptance criteria:

- [ ] Read LINEAR_API_KEY at runtime; auth status checks viewer access and reports identity without exposing the key. Missing or invalid credentials return actionable errors.
- [ ] Define stable JSON success/error shapes, resource IDs and URLs, pagination fields, and nonzero error exits. stdout contains only requested data; diagnostics use stderr. Commands never require a TTY.
- [ ] Use GraphQL variables; reject invalid input before requests. Check HTTP errors, GraphQL errors even with HTTP 200, partial data, and mutation success flags.
- [ ] Set request timeouts and bounded read retries. Handle HTTP 429 and GraphQL RATELIMITED, including HTTP 400; respect available reset/retry metadata.
- [ ] Never blindly retry mutations after an uncertain transport outcome; report the uncertainty and a reconciliation path.
- [ ] Offline tests cover missing auth, redaction, timeout, malformed JSON, partial errors, rate limits, and exhausted retries.

Repository starting points:

- crates/mcptools/src/main.rs
- crates/mcptools/src/atlassian/mod.rs
- crates/core/src/atlassian/jira.rs

### LINEAR-02: Add Linear workspace discovery and identifier resolution

Expose viewer, teams, projects, users, workflow states, labels, and cycles needed to create and update issues. Share resolution code across commands.

Acceptance criteria:

- [ ] Provide bounded list/get commands with cursor pagination and explicit all-pages behavior. Return stable IDs and human-readable names.
- [ ] Resolve team keys and IDs, issue identifiers and UUIDs, and documented resource selectors; reject ambiguous names with candidate IDs.
- [ ] Scope states, cycles, labels, and projects to the selected team where required. Never silently select the first match or team.
- [ ] Allow explicit team/project flags without requiring a repository. If defaults are added, document precedence and require unambiguous selection.
- [ ] Test multi-page results, missing resources, ambiguous names, team mismatch, and empty lists.

Repository starting points:

- crates/mcptools/src/atlassian/jira/mod.rs

### LINEAR-03: Add paginated Linear issue search and detail commands

Implement mcptools linear issue list, search, and get for agent planning and execution.

Acceptance criteria:

- [ ] Filter by team, project, assignee (including current user), workflow state, label, cycle, and updated time; provide text search.
- [ ] Get accepts issue UUID or identifier and returns description, state, assignment, project, cycle, labels, timestamps, and URL.
- [ ] Return compact list rows by default, with explicit detail expansion. Expose next cursor and has-more state; never hide truncation.
- [ ] Paginate nested comments and relations through their dedicated commands or explicit continuation metadata.
- [ ] Test filters, issue identifiers, Unicode search, pagination, empty results, and permission/not-found errors.

Repository starting points:

- crates/mcptools/src/atlassian/jira/search.rs

### LINEAR-04: Add Linear issue creation and targeted updates

Implement issue create and update with structured inputs suitable for agents.

Acceptance criteria:

- [ ] Support title, Markdown description, team, project, assignee, state, priority, labels, cycle, due date, and parent where the API supports them.
- [ ] Accept JSON from stdin or file and Markdown description from file/stdin without shell interpolation or an editor; reject conflicting input sources.
- [ ] Distinguish omitted values from explicit clears. Update only requested fields; define label add/remove/replace behavior. Validate team compatibility.
- [ ] Return the persisted issue ID, identifier, URL, and changed fields. Implement local dry-run validation without a mutation.
- [ ] Repeated create attempts after uncertain failure have a documented reconciliation path; do not claim idempotency from a title match.
- [ ] Test multiline input, invalid fields, clearing values, cross-team selectors, dry-run, and uncertain mutation results.

### LINEAR-05: Add Linear comments and issue relationships

Enable agents to report progress and express ticket dependencies.

Acceptance criteria:

- [ ] Add paginated comment list and comment create with body from JSON/file/stdin and stable comment IDs.
- [ ] Add relation list, add, and remove for blocks/blocked-by and related issues; support parent assignment and clearing via shared issue update behavior.
- [ ] Define relation direction explicitly: A blocks B means B depends on A. Reject self-relations and report server validation errors.
- [ ] Avoid creating a duplicate known relation; distinguish an existing relation from an uncertain create result. Remove only the selected relation.
- [ ] Test direction, parent updates, pagination, duplicate relation handling, comment validation, and remote permission errors.

### LINEAR-06: Expose Linear workflows through the existing MCP server

Register focused Linear tools in the existing tool registry and dispatch each tool to the same operations as the CLI.

Acceptance criteria:

- [ ] Expose workspace discovery, issue list/search/get/create/update, comment list/create, and relation list/add/remove. Use descriptive names and strict input schemas.
- [ ] Describe required fields, selector formats, pagination, empty-versus-omitted values, and mutation effects. Include read-only/destructive/idempotency annotations only where supported and accurate.
- [ ] tools/list and server startup succeed without LINEAR_API_KEY. A Linear call without credentials returns a scoped tool error; unrelated tools remain usable.
- [ ] Return structured success/errors without protocol stdout noise, secrets, or mandatory interactive prompts. Preserve caller approval controls.
- [ ] Test tool discovery and representative read/write/error calls over existing supported transports; assert CLI/MCP behavior parity.

Repository starting points:

- crates/mcptools/src/mcp/tools/mod.rs
- crates/mcptools/src/mcp/tools/atlassian.rs

### AGENTS-01: Add user-level mcptools setup, status, and uninstall

Add a small global agent setup command family. Reuse cargo xtask install for binary installation and existing setup helpers where suitable. This is user-level integration, usable outside this checkout.

Acceptance criteria:

- [ ] Provide install/setup, status/doctor, and uninstall with explicit targets codex, claude, pi, opencode2, or all; support a dry-run that shows planned paths and changes.
- [ ] Resolve a stable executable path. Detect missing/obsolete binary, missing agent, unsupported version, and unavailable writable configuration separately. An absent pi executable must not be reported as a successful live integration.
- [ ] Keep setup independent of Atlas indexing, models, and repository initialization. Never hard-code this checkout or its target directory into global setup.
- [ ] Merge only owned entries, preserve unrelated configuration and formatting where supported, refuse conflicts, and use atomic file replacement plus rollback information. Repeating setup is a no-op.
- [ ] Uninstall removes only owned unchanged entries; preserve user edits and unrelated skills/MCP servers. Honor supported environment overrides for configuration roots.
- [ ] Generate reusable command-discovery guidance from a shared bundled source; no secret values in generated content. Test temporary-home setup, rerun, conflicts, rollback, uninstall, and paths with spaces.

Repository starting points:

- xtask/src/scripts/install.rs
- scripts/install.sh
- crates/mcptools/src/atlas/cli/setup.rs
- crates/core/src/atlas/setup.rs

### AGENTS-02: Integrate mcptools globally with Codex and Claude Code

Use supported user-level MCP registration and skill discovery for Codex and Claude Code. Verify installed client versions and official documentation during implementation.

Sources: https://developers.openai.com/codex/skills/ and https://code.claude.com/docs/en/skills

Acceptance criteria:

- [ ] Register the stable mcptools stdio launch command at user scope for each client. Confirm actual CLI arguments from mcptools help.
- [ ] Install concise discoverable mcptools skill guidance through each supported user-level path, avoiding duplicate discovery. Include CLI/MCP selection, JSON output, pagination, and mutation examples.
- [ ] Use inherited LINEAR_API_KEY without copying its value to configuration. Doctor identifies missing runtime environment and explains a supported launch path.
- [ ] Respect client tool permissions and sandbox settings; setup does not broaden permissions.
- [ ] Test config merge and discovery fixtures for supported versions; record a fresh session from an unrelated directory listing mcptools and making a read call, or state why live verification is blocked.

### AGENTS-03: Integrate mcptools globally with pi and opencode2

Support the exact requested executables. Verify opencode2 version and configuration format before choosing an OpenCode adapter.

Sources: https://github.com/badlogic/pi-mono/blob/main/packages/coding-agent/docs/skills.md and https://opencode.ai/v2/docs/skills and https://opencode.ai/v2/docs/mcp-servers

Acceptance criteria:

- [ ] Expose mcptools CLI through a global pi skill using pi-supported discovery. Do not require an MCP bridge for basic CLI access; make any bridge a separately documented optional integration.
- [ ] For opencode2, register mcptools with the actual supported user-level MCP schema and install discoverable shared skill guidance. Do not infer configuration solely from the executable name.
- [ ] Prevent duplicate skill registrations where clients already scan shared locations. Preserve existing client settings and MCP servers.
- [ ] Inherit LINEAR_API_KEY without storing it; diagnose missing executable, environment, config support, or tool access clearly.
- [ ] Test supported config fixtures and run fresh-session discovery from an unrelated directory on each installed client. Missing clients are explicit verification gaps.

### LINEAR-07: Verify and document the full Linear agent workflow

Publish the final commands, setup procedure, supported client versions, and failure behavior. Extend existing tests at the affected boundaries.

Acceptance criteria:

- [ ] Offline end-to-end test: discover team/project -> search -> get -> create -> update -> comment -> add blocking relation -> read back; use a mock Linear service and assert exact writes.
- [ ] Exercise missing/invalid auth, HTTP 200 GraphQL errors, rate limits, pagination, ambiguous selectors, uncertain mutations, and secret redaction across CLI and MCP.
- [ ] Document executable installation, global setup/status/uninstall, runtime credential inheritance, and working-directory independence for all four agents.
- [ ] Use an explicitly selected test project for any live mutation verification; report created IDs and cleanup disposition. Record unavailable network/client access as unverified.
- [ ] Update README with runnable examples and compact agent guidance. Run relevant Rust tests, formatting, build, and MCP protocol checks; no unrelated source changes.
