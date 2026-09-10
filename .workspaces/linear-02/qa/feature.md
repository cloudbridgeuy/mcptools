# QA — linear-02: Linear workspace discovery and identifier resolution

**Tested SHA:** 11bef45194a403f4edfa07a6018ff6eb98e8797b

## Agent-run

| # | Slice | Scenario | Command / steps | Expected | Result | Evidence |
| - | ----- | -------- | --------------- | -------- | ------ | -------- |
| 1 | V1 | Core unit tests | `cargo test -p mcptools_core linear` | all pass | pass | 68 passed, 0 failed (V1 lane; 76 with V2 on trunk) |
| 2 | V1 | Shell unit tests | `cargo test -p mcptools linear` | all pass | pass | 14 passed, 0 failed (V1 lane; 15 with V2 on trunk) |
| 3 | V1 | Teams list JSON | `cargo run -q -p mcptools -- linear teams list --json` | nodes with id/key/name + pageInfo | pass | GUZ team, hasNextPage false |
| 4 | V1 | Projects list scoped | `cargo run -q -p mcptools -- linear projects list --team GUZ --json` | 5 nodes + pageInfo | pass | modelops-cycles, MCPTools, LLM-Stream, Wayfind, Summoners |
| 5 | V1 | Teams get by key | `cargo run -q -p mcptools -- linear teams get GUZ --json` | single team object | pass | id e3b567ba…, key GUZ |
| 6 | V1 | Projects get by name | `cargo run -q -p mcptools -- linear projects get MCPTools --team GUZ --json` | single project object | pass | id 5fe03c79… |
| 7 | V1 | Unknown team errors | `cargo run -q -p mcptools -- linear teams get NoSuchTeamXYZ` | stderr + exit 1 | pass | "Linear team not found", exit 1 |
| 8 | V1 | Teams list table | `cargo run -q -p mcptools -- linear teams list` | table + hasMore/endCursor | pass | 1 row + hasMore false |
| 9 | V1 | Team required | `cargo run -q -p mcptools -- linear projects list --json` | clap error, no default | pass | missing --team error |
| 10 | V1 | Ambiguous resolution | unit test `reports_ambiguous_team_name` | Ambiguous, 2 hits | pass | cargo test ok |
| 11 | V1 | Empty selector pre-IO | discover.rs unit tests | "must not be empty", no request | pass | cargo test ok |
| 12 | V2 | Core unit tests | `cargo test -p mcptools_core linear` | all pass | pass | 76 passed, 0 failed |
| 13 | V2 | Shell unit tests | `cargo test -p mcptools linear` | all pass | pass | 15 passed, 0 failed |
| 14 | V2 | Users list JSON | `cargo run -q -p mcptools -- linear users list --query guz --json` | 1 node + pageInfo | pass | guzman monne |
| 15 | V2 | States list JSON | `cargo run -q -p mcptools -- linear states list --team GUZ --json` | nodes with type | pass | Todo/In Progress/Done etc. |
| 16 | V2 | Labels list JSON | `cargo run -q -p mcptools -- linear labels list --team GUZ --json` | 8 nodes | pass | docs, terraform, infra, handler, cycles, Improvement, Feature, Bug |
| 17 | V2 | Cycles empty | `cargo run -q -p mcptools -- linear cycles list --team GUZ` | empty table + exit 0 | pass | header only |
| 18 | V2 | Users mismatch empty | `cargo run -q -p mcptools -- linear users list --query nonexistentxyz123` | empty table + exit 0 | pass | header only |
| 19 | V2 | Query required | `cargo run -q -p mcptools -- linear users list --json` | clap error, no dump | pass | missing --query error |
| 20 | V2 | V1 regression | teams + projects list rerun on V2 tree | unchanged | pass | GUZ + 5 projects |

## User-run

| # | Slice | Scenario | Command / steps | Expected | Result | Evidence | Why an agent cannot judge this |
| - | ----- | -------- | --------------- | -------- | ------ | -------- | ------------------------------ |
