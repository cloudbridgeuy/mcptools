# agent-setup — Ledger

Trunk: trunk-agent-setup
Base: f07ffea6dbd632c9cc1f025b2f9662a86050ef95

## Slices

| Slice | State | Branch | Lane | Merge SHA | Verdict | Notes |
| ----- | ----- | ------ | ---- | --------- | ------- | ----- |
| V1 | merged | agent-setup-V1 | /Users/guzmanmonne/Projects/Rust/mcptools/.lane/trees/agent-setup-V1 | 27d639a | verified-live | Doctor reads, no writes. Code c582688, QA 38641dd. Demo exit 0, 4 lines, pi MissingAgent exact, 0 files. Suite 97+4+609+248+16 pass. |
| V2 | ready | | | | | Setup dry-run, unblocked by V1 merge |
| V3 | pending | | | | | JSON write, blocked by V2 |
| V4 | pending | | | | | Skills plus uninstall, blocked by V3 |
| V5 | pending | | | | | Codex plus all-matrix, blocked by V1,V3,V4 |

## Deferred decisions

- V1 → V2: `UnsupportedVersion` never emitted (no agent version floors; presence check stands until AGENTS-02/03 record floors). Routed to V5 version-floor work.
- V1 → V2: `AgentAction` defined in core (`crates/core/src/agent/health.rs`), unconsumed until V2 planner. Routed to V2.
- V1 → V2/V4: `agent setup`/`agent uninstall` stubs exit 1 with plain unavailable message; V2+ replace with real verbs. Routed to V2 (setup) and V4 (uninstall).

## Events

- 2026-09-10: run started. Plan check clean (handoff 0, slices 0). Trunk lane exists clean at 68d8a90, base f07ffea. context.md created. Mode not yet selected.
- 2026-09-10: mode Auto-DAG selected. V1 dispatched into lane agent-setup-V1 at base 458723a, model gpt-5.6-terra/openai/medium, attempt 1.
- 2026-09-10: V1 reviewed. Diff: qa/V1.md plus 7 code files, 746 insertions, zero comments, zero jargon. Demo re-run live exit 0 with exact pi line and 0 files. Suite 97+4+609+248+16 pass on final tree. Strict QA gate 0 problems (expected-sha c582688, crates/ identical to HEAD). Verdict verified-live. Lane scaffolding reverted before merge.
- 2026-09-10: V1 merged as 27d639a (code c582688). V2 unblocked.
