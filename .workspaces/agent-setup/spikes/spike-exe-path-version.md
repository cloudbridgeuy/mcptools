---
shaping: true
---

## Spike 3: Executable path and version detection

### Context

GUZ-80 R3 demands stable exe path plus separate detection of missing or obsolete binary, missing agent, unsupported version, unwritable config. Absent pi must never report live. Shape part A2 was flagged.

### Goal

Describe exe resolution plus health taxonomy using existing repo code.

### Questions

| # | Question |
| --- | --- |
| **S3-Q1** | How does the repo resolve install paths today? |
| **S3-Q2** | How does the repo detect versions today? |
| **S3-Q3** | How does status report health today? |
| **S3-Q4** | What agent executables exist on PATH? |

### Acceptance

Spike complete when we can describe resolution order, version sources, health states, and live-check rules.

### Findings

Install paths diverge. `xtask/src/scripts/install.rs:14` uses `args.path` else `$HOME/.local/bin`. `xtask/src/scripts/install_binary.rs:227` uses `args.install_dir` else `/usr/local/bin` when writable else `$HOME/.local/bin`. `scripts/install.sh:17` defaults to `/usr/local/bin`. Live `mcptools` on this machine is `/Users/guzmanmonne/.local/bin/mcptools`, version `1.11.0`.

Version logic: `env!("CARGO_PKG_VERSION")` is the source (`upgrade.rs:27`, `main.rs:19` clap version). Compare via `mcptools_core::upgrade::is_version_up_to_date` (`core/src/upgrade.rs:28`), `parse_version_tag` (`core/src/upgrade.rs:20`). Upgrade flow fetches GitHub latest, downloads to `<exe>.download`, backs up to `<exe>.backup`, renames over current (`upgrade.rs:89`, `upgrade.rs:113`). OS and arch mapping in `core/src/upgrade.rs:68`. No Windows support.

Status today is index-only. `atlas/cli/status.rs:16` opens repo DB and prints file, dir, symbol counts. No binary, agent, or version state. No `doctor` command exists. Name `doctor` appears only in `docs/linear-cli/backlog.json:77`.

PATH facts on this machine: `codex` at `.vite-plus/bin/codex`, version `0.153.4`. `claude` at `.local/bin/claude`, version `2.1.267`. `opencode` at `.opencode/bin/opencode`, version `1.18.29`. `opencode2` binary exists at same dir. `pi` absent. PATH order puts `.opencode/bin` before `.local/bin`.

Resolution order for GUZ-80:

1. `std::env::current_exe` first. Canonicalize. Use when file exists and responds to `--version`.
2. Else PATH probe for `mcptools` via `which` equivalent. No shell string. Use `Path` search.
3. Else report `MissingBinary` with install hint pointing at `cargo xtask install`, not at this checkout or `target/`.

Health ADT, one variant per R3 clause: `MissingBinary, ObsoleteBinary, MissingAgent, UnsupportedVersion, UnwritableConfig, Live`. `Live` needs all true: exe resolves, `--version` parses, agent binary found on PATH, agent version passes floor, config dir writable. Pi absent gives `MissingAgent`, never `Live`.

Obsolete rule: installed `mcptools --version` below embedded `CARGO_PKG_VERSION` gives `ObsoleteBinary`. No network check in status path. Upgrade stays a separate command.

Version floors for agents remain unknown. No constants in code. Record floors at implementation time from `AGENTS-02` and `AGENTS-03` live verification, default to presence check until floors land.
