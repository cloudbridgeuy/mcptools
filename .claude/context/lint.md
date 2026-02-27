# Code Quality: cargo xtask lint

Unified lint pipeline for all code quality checks. Runs fmt, check, clippy, test, and machete in sequence. Stops at the first failure.

## Agent Usage

Agents must use `cargo xtask lint` exclusively. Never call individual tools (`cargo fmt`, `cargo test`, etc.) directly.

**Default behavior (agent-optimized):**
- Silent on success (no stdout)
- Prints only actionable errors on failure
- Full output always written to `target/xtask-lint.log`

```bash
cargo xtask lint                # Run all checks, silent on success
cargo xtask lint --verbose      # Human/CI mode: print all output
cargo xtask lint --fix          # Auto-fix fmt + clippy issues
```

## Skip Flags

```bash
cargo xtask lint --no-fmt       # Skip cargo fmt
cargo xtask lint --no-check     # Skip cargo check
cargo xtask lint --no-clippy    # Skip cargo clippy
cargo xtask lint --no-test      # Skip cargo test
cargo xtask lint --no-machete   # Skip cargo machete
```

## Git Hook Management

Hook management is integrated into the lint subcommand:

```bash
cargo xtask lint --install-hooks    # Install pre-commit hook
cargo xtask lint --uninstall-hooks  # Remove pre-commit hook
cargo xtask lint --hooks-status     # Show hook installation status
cargo xtask lint --test-hooks       # Test hook executability
```

The pre-commit hook runs `cargo xtask lint --staged-only`, which implies `--fix` and re-stages formatted `.rs` files.

## Pipeline Order

1. `cargo fmt --check` (or `cargo fmt` with `--fix`)
2. `cargo check --all-targets`
3. `cargo clippy --all-targets -- -D warnings` (or with `--fix --allow-dirty`)
4. `cargo test --all-targets`
5. `cargo machete` (optional: skipped if not installed)

## Architecture

Located in `xtask/src/scripts/lint/`:
- `mod.rs` — Pipeline logic (functional core) + orchestration (imperative shell)
- `hooks.rs` — Git hook install/uninstall/status/test
