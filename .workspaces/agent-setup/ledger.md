# agent-setup — Ledger

Trunk: trunk-agent-setup
Base: f07ffea6dbd632c9cc1f025b2f9662a86050ef95

## Slices

| Slice | State | Branch | Lane | Merge SHA | Verdict | Notes |
| ----- | ----- | ------ | ---- | --------- | ------- | ----- |
| V1 | merged | agent-setup-V1 | /Users/guzmanmonne/Projects/Rust/mcptools/.lane/trees/agent-setup-V1 | 27d639a | verified-live | Doctor reads, no writes. Code c582688, QA 38641dd. Demo exit 0, 4 lines, pi MissingAgent exact, 0 files. Suite 97+4+609+248+16 pass. |
| V2 | merged | agent-setup-V2 | /Users/guzmanmonne/Projects/Rust/mcptools/.lane/trees/agent-setup-V2 | ab74595 | verified-live | Setup dry-run. Code b850ad7 (rebased from bf12d61, content-identical). Demo exit 0, plan lines, 0 newer files. Suite 100+4+632+248+16 pass on trunk. |
| V3 | merged | agent-setup-V3 | /Users/guzmanmonne/Projects/Rust/mcptools/.lane/trees/agent-setup-V3 | 48139a5 | verified-live | JSON write. Code d0b9420 (rebased from 59160d7, content-identical). Write/rerun-skip/refuse-identical/backup-on-overwrite/foreign-preserved all live. Suite 120+4+632+248+16 pass on trunk. |
| V4 | merged | agent-setup-V4 | /Users/guzmanmonne/Projects/Rust/mcptools/.lane/trees/agent-setup-V4 | 002ad5e | verified-live | Skills plus uninstall. Code cba63c9 (rebased from 4849631, content-identical). Uninstall removes dir, edited stays with warning, foreign preserved. Suite 130+4+646+248+16 pass on trunk. |
| V5 | dispatched | agent-setup-V5 | /Users/guzmanmonne/Projects/Rust/mcptools/.lane/trees/agent-setup-V5 | | | Codex plus all-matrix. Model gpt-5.6-terra/openai/medium, base 4bdf04f, attempt 1 |
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
- 2026-09-10: V2 dispatched into lane agent-setup-V2 at base 7ce5965, model gpt-5.6-terra/openai/medium, attempt 1.
- 2026-09-10: V2 reviewed. Diff: qa/V2.md plus plan.rs, cli.rs setup verb, agent/mod export — 729 insertions, zero comments, zero jargon. `plan_global(facts, action)` matches spec exactly; pure `plan_global_with_home` takes explicit home/exe/reader. Demo re-run live exit 0, plan lines, 0 newer files. Suite 100+4+632+248+16 pass. Strict QA gate 0 problems. Verdict verified-live.
- 2026-09-10: INCIDENT lane merge without --base fast-forwarded local main to rebased lane (lane lacked id metadata after timed-out `lane new`). Recovered: main reset to cc8f0dc (verified pre-merge state, untracked-only tree), lane branch reset to c13ee7a, re-merged with `lane merge agent-setup-V2 --base trunk-agent-setup --keep`. Trunk now ab74595, main untouched. RULE: every future `lane merge` in this run passes explicit `--base trunk-agent-setup`.
- 2026-09-10: V2 merged as ab74595 (code b850ad7; rebased from bf12d61, empty crates/ diff, suite plus demo re-run on trunk). V3 unblocked.
- 2026-09-10: V3 dispatched into lane agent-setup-V3 at base 300a05e, model gpt-5.6-terra/openai/medium, attempt 1.
- 2026-09-10: V3 reviewed. Diff: qa/V3.md plus exec.rs, cli.rs setup arm, mod export — 471 insertions, zero comments, zero jargon. Spec signatures exact. Demo re-run live: write, rerun mtime-identical skip, hand-edit refuse md5-identical, merge-owned backup plus foreign keys preserved. Suite 120+4+632+248+16 pass. Strict QA gate 0 problems. Verdict verified-live. Refuse exits 0 per-file (abort that file, others still process).
- 2026-09-10: V3 merged as 48139a5 with explicit --base (code d0b9420; rebased from 59160d7, empty crates/ diff, suite plus demo re-run on trunk). V4 unblocked.
- 2026-09-10: V4 dispatched into lane agent-setup-V4 at base 75ac08f, model gpt-5.6-terra/openai/medium, attempt 1.
- 2026-09-10: V4 reviewed. Diff: qa/V4.md plus plan.rs uninstall planning, exec.rs RemoveOwned, cli.rs uninstall verb — 622 insertions, zero comments, zero jargon. Spec signatures exact. Demo re-run live: setup creates SKILL.md, uninstall removes dir, hand-edit stays md5-identical with user-edited warning, foreign key preserved. Suite 130+4+646+248+16 pass. Strict QA gate 0 problems. Verdict verified-live. Accepted: uninstall JSON strip re-renders pretty (whitespace normalized, keys intact, backup-first) over text splice — disclosed ceiling.
- 2026-09-10: external event — local main advanced cc8f0dc to ec71bbd by another workflow (`wt merge`, linear-02 landing). Not this run; trunk base f07ffea unaffected.
- 2026-09-10: V4 merged as 002ad5e with explicit --base (code cba63c9; rebased from 4849631, empty crates/ diff, suite plus demo re-run on trunk). V5 unblocked.
- 2026-09-10: V5 dispatched into lane agent-setup-V5 at base 4bdf04f, model gpt-5.6-terra/openai/medium, attempt 1.
