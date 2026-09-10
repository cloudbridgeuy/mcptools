# linear-02 — Run context

## Established facts (do not re-derive, do not contradict)
- trunk branch: trunk-linear-02 (file:linear/mod.rs, linear/types.rs, linear/discover.rs)
- Base SHA: f07ffea (workspace: remove linear-01)
- V1: Teams + projects + core paging — implements teams_list_data, teams_get_data, projects_list_data, projects_get_data, resolve_team, Paginated<T>, transform_*
- V2: Users + states + labels + cycles complete — depends on V1 interfaces
- Pattern: Functional Core-Imperative Shell; parse don't validate; no defaults for --team/--project; Users --query required; explicit --team for scoped lists
- Errors → stderr + nonzero exit, no TTY, no key leak

## Gotchas
- Two unsuccessful repair rounds on same issue → diagnose before final dispatch
- Browser serialization: only one agent drives MCP browser at a time
- Port conflicts handled by random free ports for network apps

## Conventions in this repo
- Test command: `cargo test -p mcptools-core linear`
- Lint/typecheck: `cargo check -p mcptools-core` and `cargo check -p mcptools`
- No jargon rule: no slice IDs, shape parts, or shaping vocabulary in code
- Comments: code is truth; no comments in diff means acceptance