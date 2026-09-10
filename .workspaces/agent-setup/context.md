# agent-setup — Run context

## Established facts (do not re-derive, do not contradict)
- Shape D locked: top-level `mcptools agent setup|status|uninstall`, never nested under `atlas`. Repo `atlas setup` stays untouched.

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