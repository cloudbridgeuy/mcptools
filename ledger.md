# linear-02 — Ledger

Trunk: trunk-linear-02
Base: f07ffea

## Slices

| Slice | State   | Branch | Lane | Merge SHA | Verdict | Notes |
| ----- | ------- | ------ | -------- | --------- | ------- | ----- |
| V1    | merged | V1 | .worktrees/V1 | 40c88d4 | verified-live | teams+projects discovery, live demo pass |
| V2    | merged | V2 | .worktrees/V2 | 11bef45 | verified-live | users+states+labels+cycles, live demo pass |

## Deferred decisions

- V1: `teams_get_data` returns single `Team` (not `Paginated<Team>`); ambiguous/not-found candidates go in error message. V2 unaffected (consumes `teams_list_data` + `parse_team_selector`/`match_teams`).
- V1: `Team` uses `key` field (Linear API name), not `identifier`. `Project` is `{id, name}`.
- V1: `projects_list_data` resolves team key to id first (2 round trips: teams list + team.projects nested), because `ProjectFilter` has no team field.
- V1: `--all` loops pages capped at 50 total items per shaping notation.
- V1: ambiguous-team live path not exercisable (single GUZ team); covered by unit test `reports_ambiguous_team_name`.
- V2: empty users query and empty cycles print empty table + exit 0 (list queries return collections); spec also lists a mismatch exit-1 line, resolved toward empty-table exit 0 for consistency with the empty-results rule.
- V2: users filter uses `name.contains`; states/labels/cycles use team-nested connections (first 50, no paging flags per spec).

## Events

- trunk created at Base f07ffea; ledger/context committed as df06893
- V1 implemented in lane .worktrees/V1, reviewed (verified-live: 68 core + 14 shell tests, live teams/projects list/get, not-found exit 1, --team required), merged 40c88d4 into trunk-linear-02
- V2 implemented in lane .worktrees/V2, reviewed (verified-live: 76 core + 15 shell tests, live users/states/labels/cycles, query/team required, empty exit 0), merged 11bef45 into trunk-linear-02