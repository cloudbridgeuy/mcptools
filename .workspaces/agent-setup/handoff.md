---
shaping: true
handoff: true
---

# Global agent setup — Orchestration Handoff

## Repo

- Clone: /Users/guzmanmonne/Projects/Rust/mcptools
- Bare repo: none (regular clone, worktrees under .lane/trees)
- Canonical remote / main branch: origin, main
- Trunk lane for this feature: trunk-agent-setup (/Users/guzmanmonne/Projects/Rust/mcptools/.lane/trees/trunk-agent-setup)
- Trunk branch for this feature: trunk-agent-setup

## Documents

- Shaping: ./shaping.md
- Slices: ./slices.md
- Spikes: ./spikes/

## DAG

- V1: no dependencies
- V2: depends on V1
- V3: depends on V2
- V4: depends on V3
- V5: depends on V1, V3, V4

Parallel batches: [V1] → [V2] → [V3] → [V4] → [V5]

## Pattern Policy

Pattern policy: `~/.claude/patterns/POLICY.md` (MUST Functional Core - Imperative Shell; SHOULD the rest when applicable; named principles for steering).

## Notes

- Shape D is locked: top-level `mcptools agent setup|status|uninstall`. Never nest under `atlas`. Repo `atlas setup` stays untouched.
- YAGNI: four targets as ADT, no generic agent plugin system. No new deps; `tempfile` and `serde_json` already present.
- Refuse-all means abort the file write on owned-key bytes mismatch, leave file untouched, print path plus diff. Identical bytes is no-op skip, not refuse.
- Pi gets skill only, never MCP bridge. Absent `pi` binary reports `MissingAgent`, never `Live`.
- Codex managed via `codex mcp add/remove` CLI, never TOML hand-edit.
- `LINEAR_API_KEY` from process env only. Never write secret values to config. Assert with grep in V5 demo.
- Global mode never calls `find_git_root` and never hard-codes the checkout or `target/`.
- Status path does no network. Obsolete means installed `--version` below embedded `CARGO_PKG_VERSION`.
- Test matrix mandatory per slice: temp HOME, rerun, conflicts, rollback, uninstall, paths with spaces.
- Trunk lane started clean from HEAD; unrelated dirty state in the main checkout (`linear/mod.rs`) was left out on purpose.
