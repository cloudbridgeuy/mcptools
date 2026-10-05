# Agent Setup

User-level `mcptools` integration for coding agents. Works outside any checkout.

## CLI Usage

```bash
# Show per-target health, no writes
mcptools agent status --target all

# Preview planned paths and changes, writes nothing
mcptools agent setup --target claude --dry-run

# Write MCP entries plus skill files
mcptools agent setup --target all

# Code-mode entries (find_tools + execute, JEV-powered)
mcptools agent setup --target all --mode code

# Remove owned entries only
mcptools agent uninstall --target claude
```

Targets: `codex`, `claude`, `pi`, `opencode` (`opencode2` is an accepted alias), or `all`. Modes: `plain` (default) or `code`.

## Status

`agent status` prints one `name: Health (detail)` line per target and writes no files. Health is one of `Live`, `MissingBinary`, `ObsoleteBinary`, `MissingAgent`, `UnsupportedVersion`, `UnwritableConfig`. An absent `pi` executable reports `MissingAgent`, never `Live`. Status performs no network calls.

## Setup

Setup writes an owned `mcptools` MCP server entry plus a `mcptools` skill file per target:

| Target | MCP entry | Skill file |
| ------ | --------- | ---------- |
| claude | `~/.claude.json` (`mcpServers.mcptools`) | `~/.claude/skills/mcptools/SKILL.md` |
| opencode | `~/.config/opencode/opencode.json` (`mcp.mcptools`) | `~/.config/opencode/skills/mcptools/SKILL.md` |
| codex | via `codex mcp add` CLI, never TOML hand-edit | `~/.codex/skills/mcptools/SKILL.md` |
| pi | none, skill only | `~/.pi/agent/skills/mcptools/SKILL.md` |

Conflicts refuse: an owned entry with different bytes aborts that file, leaves it untouched, and reports path plus diff. Identical bytes are a no-op skip, so rerun changes nothing. Writes are atomic (temp file plus rename) with a backup at `<file>.mcptools-backup.<unix-secs>` and a printed restore command. Secret values from the process environment never reach config files or output.

## Modes

Plain mode writes stdio entries; code mode appends `--code-mode` so the server exposes only `find_tools` and `execute`:

| Target | Plain | Code |
| ------ | ----- | ---- |
| claude | `args: [mcp, stdio]` | `args: [mcp, stdio, --code-mode]`, `env: {JEV_PROVIDER: opencode}` |
| opencode | `command: [exe, mcp, stdio]` | `command: [exe, mcp, stdio, --code-mode]`, `environment: {JEV_PROVIDER: opencode, OPENCODE_API_KEY: {env:OPENCODE_API_KEY}, LINEAR_API_KEY: {env:LINEAR_API_KEY}}` |
| codex | `codex mcp add mcptools -- <exe> mcp stdio` | `codex mcp add mcptools --env JEV_PROVIDER=opencode -- <exe> mcp stdio --code-mode` |
| pi | skill only | skill only, unchanged |

JEV keys always come from process inheritance: claude gets `JEV_PROVIDER` as a literal (safe, non-secret) and reads keys from the host process; opencode stores `{env:VAR}` refs, never values; codex passes `--env JEV_PROVIDER=opencode` literally. Switching modes refuses against the other mode's entry, so remove stale entries (`uninstall`, same `--mode` as the entry) before switching.

## Uninstall

Uninstall removes only byte-identical owned entries: skill files are deleted (empty parent `mcptools` dirs pruned), JSON configs keep the file with the owned key stripped, Codex goes through `codex mcp remove`. User-edited entries stay with a `user-edited, left unchanged` warning. Foreign keys, foreign skill dirs, and files without the owned entry are never touched.
