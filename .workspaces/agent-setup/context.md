# agent-setup — Run context

## Established facts (do not re-derive, do not contradict)
- Shape D locked: top-level `mcptools agent setup|status|uninstall`, never nested under `atlas`. Repo `atlas setup` stays untouched.
- V1 on trunk: core ADTs in `crates/core/src/agent/health.rs` (`AgentTarget`, `AgentAction`, `Health`, `GlobalFacts`, `TargetHealth`, `classify_health`, `classify_target`); shell in `crates/mcptools/src/agent/cli.rs` (`gather_global_facts`, `format_doctor`, `ShellTarget`, `run`); `Agent` registered beside `Atlas` in `crates/mcptools/src/main.rs`.
- V1 format: doctor prints one `name: Health (detail)` line per target; pi-absent line is `pi: MissingAgent (executable not found)`.
- V1 probing: `classify_health` keeps spec signature and probes PATH/HOME internally; all branching lives in pure `classify_target` (consequence of spec-pinned `GlobalFacts` shape). `Live` needs exe plus agent binary plus writable config.
- V1 CLI: `ShellTarget` maps 1:1 to core `AgentTarget`; `opencode2` accepted as alias, both binaries probed.
- V1 shell gotcha: call `crate::prelude::println!` explicitly (anstream glob collides with std `println`).

## Gotchas
- Global mode never calls `find_git_root` and never hard-codes checkout or `target/`.
- `LINEAR_API_KEY` from process env only. Never write secret values to config.
- Codex managed via `codex mcp add/remove` CLI, never TOML hand-edit.
- Pi gets skill only, never MCP bridge. Absent `pi` binary reports `MissingAgent`, never `Live`.
- Refuse-all means abort file write on owned-key bytes mismatch, leave file untouched, print path plus diff. Identical bytes is no-op skip, not refuse.
- Status path does no network. Obsolete means installed `--version` below embedded `CARGO_PKG_VERSION`.

## Conventions in this repo
- Test: `cargo test --workspace` from lane root. Per-slice add temp-HOME tests plus paths with spaces.
- Lint: `cargo clippy --workspace -- -D warnings` if available; typecheck via `cargo check --workspace`.
- Pure core in `crates/core/src/agent/`, shell in `crates/mcptools/src/agent/`. Register `Agent` subcommand in `crates/mcptools/src/main.rs` `SubCommands` beside `Atlas`.
- Pattern policy `~/.claude/patterns/POLICY.md`: MUST Functional Core-Imperative Shell, unit tests for every pure function.
- No comments of any kind in diff. No shaping jargon in code (no slice IDs, no shape parts).
- No network listeners, no browser. Random temp HOME per demo (`HOME=/tmp/tN`). No secrets in output.
