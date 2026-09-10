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

# Remove owned entries only
mcptools agent uninstall --target claude
```

Targets: `codex`, `claude`, `pi`, `opencode` (`opencode2` is an accepted alias), or `all`.

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

## Uninstall

Uninstall removes only byte-identical owned entries: skill files are deleted (empty parent `mcptools` dirs pruned), JSON configs keep the file with the owned key stripped, Codex goes through `codex mcp remove`. User-edited entries stay with a `user-edited, left unchanged` warning. Foreign keys, foreign skill dirs, and files without the owned entry are never touched.
