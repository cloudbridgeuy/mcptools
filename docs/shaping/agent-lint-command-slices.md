---
shaping: true
---

# Agent-Friendly Lint Command — Slices

## Slice Overview

| Slice | Title | Parts | Demo |
|-------|-------|-------|------|
| **V1** | Core lint pipeline with output capture | A1, A2, A3, A4, A9 | `cargo xtask lint` → silent on success, errors on failure, log file written |
| **V2** | Skip flags and auto-fix | A5, A6 | `cargo xtask lint --no-test --fix` → selective checks with auto-fix |
| **V3** | Hooks migration | A7, A8 | `cargo xtask lint --install-hooks` → install hook, commit to verify |

## V1: Core lint pipeline with output capture

The minimum viable agent tool. After this slice, agents can use `cargo xtask lint`.

### Affordances

**CLI:**
- `LintArgs` struct: `--verbose` flag
- `Lint(LintArgs)` variant in `Commands` enum
- Route in `main.rs`

**Lint runner (`scripts/lint/mod.rs`):**
- `run(args: &LintArgs) -> Result<()>` — entry point
- `run_check(name, cmd, verbose, log_file) -> Result<bool>` — runs one check with output capture pattern
- Check pipeline: fmt → check → clippy → test → machete (in order, stop on first failure)
- Machete: skip gracefully if `cargo-machete` not installed

**Output capture pattern:**
```
For each check:
  1. Run with .stderr_to_stdout().stdout_capture().unchecked().run()
  2. Append captured output to log file
  3. If --verbose: print captured output to stdout
  4. If exit code != 0 and not --verbose: print captured output to stdout
  5. If exit code != 0: return failure
Always print log file path as last line
```

**Log file:** `target/xtask-lint.log` (overwritten each run)

**CLAUDE.md update:** Add agent instructions — use only `cargo xtask lint`, document log file path.

### Files touched

| File | Action |
|------|--------|
| `xtask/src/cli.rs` | Add `Lint(LintArgs)` to `Commands`, define `LintArgs` |
| `xtask/src/main.rs` | Add route for `Lint` |
| `xtask/src/scripts/mod.rs` | Add `pub mod lint;` |
| `xtask/src/scripts/lint/mod.rs` | **New** — core pipeline + output capture |
| `CLAUDE.md` | Add agent lint instructions |

---

## V2: Skip flags and auto-fix

Adds granular control over which checks run and auto-fix capability.

### Affordances

**CLI additions to `LintArgs`:**
- `--no-fmt`, `--no-check`, `--no-clippy`, `--no-test`, `--no-machete`
- `--fix` — runs `cargo fmt` (without `--check`) and `cargo clippy --fix`

**Pipeline change:**
- Each check gated by its skip flag
- When `--fix`: fmt runs without `--check`, clippy runs with `--fix --allow-dirty`

### Files touched

| File | Action |
|------|--------|
| `xtask/src/cli.rs` | Add skip flags and `--fix` to `LintArgs` |
| `xtask/src/scripts/lint/mod.rs` | Gate checks on flags, add fix mode |

---

## V3: Hooks migration

Replaces the bash pre-commit hook with the lint command. Subsumes `cargo xtask hooks`.

### Affordances

**CLI additions to `LintArgs`:**
- `--staged-only` (hidden) — for pre-commit hook use
- `--install-hooks`, `--uninstall-hooks`, `--hooks-status`, `--test-hooks`

**Hooks module (`scripts/lint/hooks.rs`):**
- Migrate hook management functions from `scripts/hooks.rs`
- New pre-commit hook template: thin shell calling `exec cargo xtask lint --staged-only`
- Re-stage `.rs` files after fmt fix in staged-only mode

**Cleanup:**
- Remove `scripts/hooks/pre-commit` (bash script)
- Remove `scripts/install-hooks.sh` (legacy)
- Remove `cargo xtask hooks` subcommand
- Remove `xtask/src/scripts/hooks.rs` (logic moved to `lint/hooks.rs`)

### Files touched

| File | Action |
|------|--------|
| `xtask/src/cli.rs` | Add hooks flags and `--staged-only` to `LintArgs`. Remove `Hooks` command. |
| `xtask/src/main.rs` | Remove `Hooks` route |
| `xtask/src/scripts/mod.rs` | Remove `pub mod hooks;` |
| `xtask/src/scripts/hooks.rs` | **Delete** — migrated to lint/hooks.rs |
| `xtask/src/scripts/lint/mod.rs` | Route hooks flags, add staged-only logic |
| `xtask/src/scripts/lint/hooks.rs` | **New** — migrated hooks management |
| `scripts/hooks/pre-commit` | **Replace** with thin shell: `exec cargo xtask lint --staged-only` |
| `scripts/install-hooks.sh` | **Delete** |
