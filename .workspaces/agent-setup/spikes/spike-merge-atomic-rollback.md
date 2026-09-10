---
shaping: true
---

## Spike 2: Merge, atomic write, rollback, uninstall

### Context

GUZ-80 R5 and R6 demand owned-entry merge, format preservation, refuse-all conflicts, atomic replace plus rollback, idempotent rerun, safe uninstall. Shape parts A4, A6, A7 were flagged.

### Goal

Describe steps to implement merge plus uninstall using existing repo patterns.

### Questions

| # | Question |
| --- | --- |
| **S2-Q1** | What merge and write patterns exist in the repo? |
| **S2-Q2** | What atomic and rollback patterns exist? |
| **S2-Q3** | What uninstall-owned patterns exist? |
| **S2-Q4** | What deps support format-preserving JSON? |

### Acceptance

Spike complete when we can describe merge steps, atomic steps, rollback shape, uninstall rule, and dep gaps.

### Findings

Merge base: `crates/core/src/atlas/setup.rs` owns pure planning. `splice_hook_block` (`setup.rs:211`), `splice_claude_md` (`setup.rs:402`), `template_status` (`setup.rs:437`) compare with `trim_end` only. `plan_setup` (`setup.rs:251`) maps facts to `SetupAction` (`setup.rs:193`). Executor `crates/mcptools/src/atlas/cli/setup.rs:113` writes full file content via `write_with_parents` (`setup.rs:131`), plain `fs::write`. No tmp file. No rename. No backup. No transaction.

JSON merge: none exists. `serde_json` used for API parse and pretty print only. No `mcpServers` merge. No conflict-refuse logic. TOML merge in `core/src/atlas/config.rs:299` is flat-vs-nested config merge, not reusable.

Atomic plus rollback analogs: `crates/mcptools/src/upgrade.rs:113` copies current to `<binary>.backup`, renames new over current, restores backup on rename failure. `xtask/src/scripts/lint/hooks.rs:82` renames existing hook to `.backup.<timestamp>`. `atlas/cli/init.rs:110` uses `tempfile::NamedTempFile` only for editor buffer, final save is direct `fs::write`.

Uninstall analog: only `xtask/src/scripts/lint/hooks.rs:221` `uninstall_hooks` removes symlink only when `read_link` equals owned source, skips foreign files. No marker-strip function. No checksum compare. `atlas/cli/mod.rs:1` has no uninstall module.

Deps: `serde 1.0.228`, `serde_json 1.0.145` plain, no `preserve_order` feature. `toml 0.8`, `toml_edit 0.22.27` transitive only. `tempfile 3.8` direct in `crates/mcptools/Cargo.toml:52`. No `toml_edit::DocumentMut` use in source.

Plan steps:

1. Core defines `GlobalFacts { files: Map<Path, Option<String>> }`, `plan_global` returns `Create, MergeOwned, Skip AlreadyInstalled, Refuse Conflict, RemoveOwned, Skip UserEdited`.
2. JSON targets (`~/.claude.json`, `opencode.json`): parse to `serde_json::Value`, read `mcpServers` or `mcp` map. Owned key `mcptools`. Absent key gives merge. Equal value gives skip. Different value gives refuse, no write.
3. Format preserve: text-level splice keeps key order and whitespace where parser supports it. Else full re-serialize and document the limit. No new dep. YAGNI holds.
4. Shell writes via tmp file in same dir plus `rename`, keeps `<file>.mcptools-backup.<timestamp>` on overwrite, prints restore command. Reuse `upgrade.rs:113` shape.
5. Uninstall removes owned key only when bytes equal last-installed template. Else skip with warning. Skill dirs removed only when dir contains only owned files.
6. Codex uses `codex mcp add/remove` CLI from shell, not TOML edit. Avoids unknown table name.
7. Pi writes skill dir only, no MCP step.

Test hooks: temp HOME via env, rerun gives skip, conflict fixture gives refuse with no write, backup file exists after overwrite, uninstall removes owned key and keeps foreign keys, paths with spaces pass through `Path` without shell interpolation.
