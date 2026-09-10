# agent-setup — Run context

## Established facts (do not re-derive, do not contradict)
- Shape D locked: top-level `mcptools agent setup|status|uninstall`, never nested under `atlas`. Repo `atlas setup` stays untouched.
- V1 on trunk: core ADTs in `crates/core/src/agent/health.rs` (`AgentTarget`, `AgentAction`, `Health`, `GlobalFacts`, `TargetHealth`, `classify_health`, `classify_target`); shell in `crates/mcptools/src/agent/cli.rs` (`gather_global_facts`, `format_doctor`, `ShellTarget`, `run`); `Agent` registered beside `Atlas` in `crates/mcptools/src/main.rs`.
- V1 format: doctor prints one `name: Health (detail)` line per target; pi-absent line is `pi: MissingAgent (executable not found)`.
- V1 probing: `classify_health` keeps spec signature and probes PATH/HOME internally; all branching lives in pure `classify_target` (consequence of spec-pinned `GlobalFacts` shape). `Live` needs exe plus agent binary plus writable config.
- V1 CLI: `ShellTarget` maps 1:1 to core `AgentTarget`; `opencode2` accepted as alias, both binaries probed.
- V1 shell gotcha: call `crate::prelude::println!` explicitly (anstream glob collides with std `println`).
- V2 on trunk: planner in `crates/core/src/agent/plan.rs` (`GlobalAction`, `SkipReason`, `plan_global`, `plan_global_with_home`, `format_plan`, `plan_config`, `plan_skill`, `config_path`, `skill_path`, `skill_content`, `desired_server_value`); setup verb in `crates/mcptools/src/agent/cli.rs` (`--dry-run` flag, prints plan, never writes).
- V2 decisions: skill body is minimal discovery snippet naming target (no reusable bundled global skill source in repo); Codex plans skill-plus-guidance only, TOML entry deferred to V5 `codex mcp` CLI; Pi skill only, never MCP entry; `RemoveOwned` plus `UserEdited`/`NotApplicable` spec-shaped but unreturned until V4.
- V2 paths: Claude `~/.claude.json` (`mcpServers.mcptools`) plus `.claude/skills`; Opencode `~/.config/opencode/opencode.json` (`mcp.mcptools`) plus skills; Codex `.codex/skills`; Pi `.pi/agent/skills`. V1 probe roots are parents of these write paths.
- V3 on trunk: executor in `crates/mcptools/src/agent/exec.rs` (`BackupInfo`, `atomic_write`, `execute_global`, pure `backup_path_for`, `restore_command`, LCS `line_diff`, `format_outcome`); setup verb executes when no `--dry-run`, prints wrote/backup/restore plus already-installed plus refuse lines.
- V3 semantics: `Create`/`MergeOwned` write via tmp-plus-rename with parents created; existing file copied to `<file>.mcptools-backup.<unix-secs>`; new files get `rm '<target>'` restore cmd; `Skip`/`Refuse`/`RemoveOwned` mutate nothing. Skill `Refuse` renders true line diff (staged re-derived from path suffix); config `Refuse` prints owned-value detail only, never existing file lines (no secret leak). Refuse is per-file with exit 0.
- V3 ceilings: `line_diff` LCS uncapped (fine for small files); backup stamp unix seconds (same-second collision unreachable — second run always skips); single-quote quoting, no single-quote escaping.
- V4 on trunk: `plan_uninstall`/`plan_uninstall_with_home`/`plan_uninstall_target`/`plan_uninstall_config`/`plan_uninstall_skill` plus `stage_skill(template)` (single-arg spec shape, `{target}` substitution, `skill_content` rebuilt on top byte-identical) in `plan.rs`; `execute_remove` in `exec.rs`; `agent uninstall --target --dry-run` verb (dry-run prints plan, live prints outcomes).
- V4 semantics: missing path → `Skip NotApplicable`; skill bytes equal staged → `RemoveOwned` (delete file, prune `mcptools` dir only when empty); skill edited → `Skip UserEdited` (`user-edited, left unchanged`); JSON owned value equal → `RemoveOwned` as strip-owned-key-plus-rewrite with backup (file never deleted, foreign keys intact, whitespace normalized — ceiling); JSON owned different or invalid → `Skip UserEdited`; JSON without owned key → `Skip NotApplicable`. No per-agent env overrides exist; HOME is the only root switch.
- V5 on trunk: `crates/mcptools/src/agent/codex.rs` (`codex_mcp_add`, `codex_mcp_remove`, pure argv builders plus plan-line renderers); setup/uninstall execute paths call codex CLI best-effort with honest skip/fail lines, exit 0 preserved; dry-run prints codex intent without invoking CLI.
- V5 live versions: codex-cli 0.154.0, claude 2.1.267, opencode/opencode2 1.18.29, pi absent. No version floors defined; presence-check default stands. Real codex surface: `codex mcp add <NAME> (--url | -- <CMD>...)`, `codex mcp remove <NAME>`; `--env` never used. Codex CLI honors HOME (hermetic temp-HOME runs).
- Orchestrator rule: every `lane merge` passes explicit `--base trunk-agent-setup` (timed-out `lane new` leaves lanes without id metadata, default base is wrong).

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
