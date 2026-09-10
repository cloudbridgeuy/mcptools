---
shaping: true
---

## Spike 1: Global agent paths, formats, env overrides

### Context

GUZ-80 needs user-level setup for codex, claude, pi, opencode2. Shape A parts A2, A4, A7 were flagged. Need concrete config roots, MCP schemas, skill dirs, env overrides.

### Goal

Describe exact user-scope paths and formats per target, plus ownership and secret rules.

### Questions

| # | Question |
| --- | --- |
| **S1-Q1** | Where does each target read user-scope MCP servers? |
| **S1-Q2** | Where does each target discover user-scope skills? |
| **S1-Q3** | Which env overrides change config roots? |
| **S1-Q4** | What MCP entry shape does each target expect? |

### Acceptance

Spike complete when we can describe per-target roots, formats, skill dirs, and overrides, with unknowns marked.

### Findings

Verified on this machine plus official docs.

| Target | MCP root | Skill roots (user) | Entry shape |
| --- | --- | --- | --- |
| codex | `~/.codex/config.toml`, managed via `codex mcp add/remove/list`. No `[mcp_servers]` section on this machine yet. CLI exists: `codex mcp --help` | `~/.codex/skills/<name>/SKILL.md`, plus shared `~/.agents/skills/<name>/SKILL.md`. Docs: `developers.openai.com/codex/skills`. Local `~/.codex/skills/` holds 10 entries | TOML via CLI. Exact table name unverified, use CLI not hand-edit. `codex-cli 0.153.4` installed |
| claude | `~/.claude.json` top-level `mcpServers` dict. Verified: holds `playwright`. CLI: `claude mcp add/get/list`, `claude 2.1.267` installed. Desktop legacy: `~/Library/Application Support/Claude/claude_desktop_config.json`, shape `{"mcpServers":{"mcptools":{"command":"mcptools","args":["mcp","stdio"]}}}` per `README.md:65` | `~/.claude/skills/<name>/SKILL.md`. Verified dir with shared entries. Docs: `code.claude.com/docs/en/skills` | JSON `mcpServers` map. Same shape as README for Code and Desktop |
| pi | None. Ticket AGENTS-03 forbids MCP bridge for basic access. `pi` executable absent on this machine, confirmed | `~/.pi/agent/skills/`, shared `~/.agents/skills/`, plus `~/.claude/skills` via `~/.pi/agent/settings.json` `skills` array. Verified settings file. Docs: `pi` repo `skills.md` | Skill dir with `SKILL.md` only. No MCP entry |
| opencode | `~/.config/opencode/opencode.json` key `mcp.<name>` with `{type:"local", command:[...], enabled}`. Verified live file with `context7` entry. Binary `opencode2` exists beside `opencode 1.18.29` | `~/.config/opencode/skills/<name>/SKILL.md`, plus `~/.claude/skills`, `~/.agents/skills`. Verified docs `opencode.ai/docs/skills`. Local `~/.config/opencode/skills/` absent | JSONC `mcp` map, local type with command array. No secrets: `environment` holds only non-secret values |

Env overrides: no `CODEX_HOME`, `CLAUDE_HOME`, `PI_HOME`, `OPENCODE_CONFIG` found in repo or on machine. `~/.pi/agent/settings.json` `skills` array is the only supported root redirect found. All other roots fixed. HOME is the only root switch. Missing override counts as no override.

Secrets: `LINEAR_API_KEY` comes from process env only. Never write value to config. Matches `docs/linear-cli/backlog.json:8`, `README.md:48`.

Ownership rule: owned key is `mcptools` inside each MCP map, plus owned skill dirs named `mcptools-*`. All other keys and dirs foreign. Refuse-all means abort file write when owned key exists with different bytes.

Remaining unknown: Codex TOML table name for hand-merge. Resolution: call `codex mcp add/remove` from shell instead of editing TOML. Keeps merge logic to JSON targets only.
