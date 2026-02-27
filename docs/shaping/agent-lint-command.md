---
shaping: true
---

# Agent-Friendly Lint Command — Shaping

## Requirements (R)

| ID | Requirement | Status |
|----|-------------|--------|
| R0 | AI agents use a single command (`cargo xtask lint`) for all code quality checks | Core goal |
| R1 | Successful runs produce minimal stdout (only the log file path) | Must-have |
| R2 | Failed runs print only actionable errors to stdout, then the log file path | Must-have |
| R3 | Full verbose output from all tools is stored in `target/xtask-lint.log` | Must-have |
| R4 | `--verbose` flag streams full output to stdout (for humans/CI) | Must-have |
| R5 | Runs all checks by default: fmt, check, clippy, test, machete | Must-have |
| R6 | Individual checks can be skipped via flags (`--no-test`, `--no-machete`, etc.) | Must-have |
| R7 | Pre-commit hook is a thin shell that calls `cargo xtask lint --staged-only` | Must-have |
| R8 | Hooks management (install/uninstall/status/test) lives under `cargo xtask lint` | Must-have |
| R9 | CLAUDE.md instructs agents to use only `cargo xtask lint` | Must-have |

## Selected Shape: A — Rust-native lint with captured output

| Part | Mechanism |
|------|-----------|
| **A1** | **Output capture** — Each check runs via `duct` with `.stderr_to_stdout().stdout_capture().unchecked().run()`. On success: nothing printed. On failure: captured output printed to stdout. Full output always appended to log file. |
| **A2** | **Log file** — All verbose output written to `target/xtask-lint.log` (deterministic, gitignored). Path printed as last line of every run. |
| **A3** | **`--verbose` mode** — When set, print captured output to stdout after each check regardless of success/failure. Log file still written. |
| **A4** | **Check pipeline** — Runs in order: (1) `cargo fmt --check`, (2) `cargo check --all-targets`, (3) `cargo clippy --all-targets -- -D warnings`, (4) `cargo test --all-targets`, (5) `cargo machete`. Stops on first failure. |
| **A5** | **Skip flags** — `--no-fmt`, `--no-check`, `--no-clippy`, `--no-test`, `--no-machete` each skip their respective check. |
| **A6** | **`--fix` flag** — Auto-fix where possible: `cargo fmt` (without `--check`), `cargo clippy --fix`. |
| **A7** | **`--staged-only` flag** — For pre-commit hook use. Re-stages `.rs` files after fmt fix. |
| **A8** | **Hooks management** — `--install-hooks`, `--uninstall-hooks`, `--hooks-status`, `--test-hooks`. Pre-commit hook is a thin bash script: `exec cargo xtask lint --staged-only`. |
| **A9** | **CLAUDE.md update** — Agents must use `cargo xtask lint`, never call individual cargo commands directly. Document log file path. |

## Fit Check: R × A

| Req | Requirement | Status | A |
|-----|-------------|--------|---|
| R0 | AI agents use a single command for all code quality checks | Core goal | ✅ |
| R1 | Successful runs produce minimal stdout | Must-have | ✅ |
| R2 | Failed runs print only actionable errors + log file path | Must-have | ✅ |
| R3 | Full verbose output stored in `target/xtask-lint.log` | Must-have | ✅ |
| R4 | `--verbose` flag streams full output for humans/CI | Must-have | ✅ |
| R5 | Runs all checks by default | Must-have | ✅ |
| R6 | Individual checks skippable via flags | Must-have | ✅ |
| R7 | Pre-commit hook is thin shell calling `cargo xtask lint` | Must-have | ✅ |
| R8 | Hooks management lives under `cargo xtask lint` | Must-have | ✅ |
| R9 | CLAUDE.md instructs agents to use only `cargo xtask lint` | Must-have | ✅ |

## Spike Summary

All unknowns resolved:

- **Output capture**: `duct` capture → write to log → conditionally print. No streaming needed.
- **Error extraction**: None needed — tools produce self-documenting errors. Pass through raw output on failure.
- **Hooks migration**: Reuse existing Rust functions from `scripts/hooks.rs`. Replace bash script with thin shell.
- **cargo-machete**: CLI-only, invoke as subprocess. Skip gracefully if not installed.
- **Module structure**: `scripts/lint/mod.rs` + `scripts/lint/hooks.rs`.
