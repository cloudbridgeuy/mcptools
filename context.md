# linear-02 — Run context

## Established facts (do not re-derive, do not contradict)
- trunk branch: trunk-linear-02 (file:linear/mod.rs, linear/types.rs, linear/discover.rs)
- Base SHA: f07ffea (workspace: remove linear-01)
- V1: Teams + projects + core paging — implements teams_list_data, teams_get_data, projects_list_data, projects_get_data, resolve_team, Paginated<T>, transform_*
- V2: Users + states + labels + cycles complete — depends on V1 interfaces
- Pattern: Functional Core-Imperative Shell; parse don't validate; no defaults for --team/--project; Users --query required; explicit --team for scoped lists
- Errors → stderr + nonzero exit, no TTY, no key leak
- V1 merged 40c88d4: Team{id,key,name}, Project{id,name}, Paginated<T> in core types (crates/core/src/linear/types.rs:transform_teams/transform_team/transform_projects/transform_team_projects)
- V1 merged 40c88d4: TeamSelector{Id,Key,Name}+match_teams, ProjectSelector{Id,Name}+match_projects in core resolve (crates/core/src/linear/resolve.rs:parse_team_selector/parse_project_selector)
- V1 merged 40c88d4: teams_list_data/limit/cursor, teams_get_data/selector→Team, projects_list_data/team/limit/cursor, projects_get_data/id/team→Project in shell discover (crates/mcptools/src/linear/discover.rs)
- V1 merged 40c88d4: projects scoping resolves team key to id first; ProjectFilter has no team field; team.projects nested query used
- V1 merged 40c88d4: --all pages capped at 50 items; --team required with no default

## Gotchas
- Two unsuccessful repair rounds on same issue → diagnose before final dispatch
- Browser serialization: only one agent drives MCP browser at a time
- Port conflicts handled by random free ports for network apps

## Conventions in this repo
- Test command: `cargo test -p mcptools-core linear`
- Lint/typecheck: `cargo check -p mcptools-core` and `cargo check -p mcptools`
- No jargon rule: no slice IDs, shape parts, or shaping vocabulary in code
- Comments: code is truth; no comments in diff means acceptance