---
shaping: true
---

# Global agent setup — Shaping

Source: Linear GUZ-80 `Add user-level mcptools setup, status, and uninstall`.
Initiative ref: `linear-cli-global-agents-2026-09-10`, key `AGENTS-01`.

Problem: no global agent install exists. Repo `atlas setup` touches repo only.
`xtask install` installs binary only. No doctor, no uninstall, no per-agent state.

Outcome: user runs setup outside checkout, verifies state, removes owned entries safely.

Decisions locked during shaping:

- Placement: top-level `mcptools agent setup|status|uninstall` (D).
  Rejected A (`atlas setup --global`, overloads repo command),
  B (`setup` with action flags, `--status --uninstall` combos need guards),
  C (flat `setup/status/uninstall`, clashes with `atlas status`).
- Scope: MCP plus skills and docs per target.
- Conflicts: refuse all. Identical bytes gives no-op skip, different bytes aborts
  that file with path plus diff.

## R

| Req | Requirement | Status |
| --- | ----------- | ------ |
| R0 | Setup works at user level outside checkout | Core goal |
| R1 | Provide setup, status/doctor, uninstall for codex, claude, pi, opencode2, all | Must-have |
| R2 | Dry-run shows planned paths and changes | Must-have |
| R3 | Resolve stable exe path. Report missing/obsolete binary, missing agent, unsupported version, unwritable config separately. Absent pi exe never counts as live | Must-have |
| R4 | No dependence on Atlas index, models, repo init. No hard-coded checkout or target dir | Must-have |
| R5 | Merge owned entries only. Preserve unrelated config and formatting. Refuse all conflicts. Atomic replace plus rollback info. Rerun is no-op | Must-have |
| R6 | Uninstall removes owned unchanged entries only. Preserve edits and unrelated skills/servers. Honor env overrides for config roots | Must-have |
| R7 | Discovery guidance comes from shared bundled source. No secrets in output | Must-have |
| R8 | Tests cover temp-home, rerun, conflicts, rollback, uninstall, spaces in paths | Must-have |

## D: Top-level agent command, verbs as subcommands

Reuse `plan_setup` pattern. Pure core in `crates/core`, shell in `crates/mcptools`.

| Part | Mechanism | Flag |
| ---- | --------- | :--: |
| A1 | Top-level `mcptools agent setup\|status\|uninstall --target codex\|claude\|pi\|opencode2\|all --dry-run`. Verbs as subcommands, no flag combos. Repo `atlas setup` untouched. `setup` name stays free |  |
| A2 | Resolve exe via `current_exe` plus PATH probe. Classify health as ADT: `MissingBinary, ObsoleteBinary, MissingAgent, UnsupportedVersion, UnwritableConfig, Live`. `pi` absent maps to `MissingAgent`, never `Live` |  |
| A3 | Shell gathers `GlobalFacts { exe, targets, file contents }`. Core `plan_global_setup` returns `GlobalAction[]`. No repo root, no index, no model calls. Templates via `include_str!` |  |
| A4 | Per target, write MCP server entry plus skill file plus guidance snippet from one bundled source. JSON merge by owned key, TOML/YAML/markdown by markers where format allows. Codex via `codex mcp add/remove` CLI, no TOML hand-edit |  |
| A5 | Refuse all conflicts: owned key exists with different bytes aborts that file, leaves file untouched, reports path plus diff. Identical bytes gives `Skip AlreadyInstalled` |  |
| A6 | Atomic write: tmp file plus rename. Rollback info prints backup path plus restore command. Preserve formatting via text-level splice, not full re-serialize, where parser supports it |  |
| A7 | Uninstall plans `RemoveOwned` only when current bytes equal last-installed bytes. Else `Skip UserEdited`. Resolve roots via HOME plus per-agent env overrides |  |

## Fit Check: R x D

| Req | Requirement | Status | D |
| --- | ----------- | ------ | - |
| R0 | Setup works at user level outside checkout | Core goal | ✅ |
| R1 | Provide setup, status/doctor, uninstall for codex, claude, pi, opencode2, all | Must-have | ✅ |
| R2 | Dry-run shows planned paths and changes | Must-have | ✅ |
| R3 | Resolve stable exe path. Report missing/obsolete binary, missing agent, unsupported version, unwritable config separately. Absent pi exe never counts as live | Must-have | ✅ |
| R4 | No dependence on Atlas index, models, repo init. No hard-coded checkout or target dir | Must-have | ✅ |
| R5 | Merge owned entries only. Preserve unrelated config and formatting. Refuse all conflicts. Atomic replace plus rollback info. Rerun is no-op | Must-have | ✅ |
| R6 | Uninstall removes owned unchanged entries only. Preserve edits and unrelated skills/servers. Honor env overrides for config roots | Must-have | ✅ |
| R7 | Discovery guidance comes from shared bundled source. No secrets in output | Must-have | ✅ |
| R8 | Tests cover temp-home, rerun, conflicts, rollback, uninstall, spaces in paths | Must-have | ✅ |

Ceilings that stay:

- Codex TOML table name unverified. Workaround stands: use `codex mcp` CLI.
- Agent version floors undefined. Default stands: presence check until
  AGENTS-02 and AGENTS-03 record floors.
- `opencode2` config root unverified. Assumes shared `opencode.json`.
  Confirm in AGENTS-03 live check.

## Detail D: Concrete affordances

### Places

| # | Place | Description |
| --- | ----- | ----------- |
| P1 | CLI session | Operator invokes `mcptools agent`, reads stdout |
| P2 | Home configs | User-scope files plus skill dirs, changed only via N |
| P3 | Agent binaries | External `codex`, `claude`, `pi`, `opencode2`, `mcptools` executables probed on PATH |

### Operator Affordances

| # | Place | Component | Affordance | Control | Wires Out | Returns To |
| --- | ----- | --------- | ---------- | ------- | --------- | ---------- |
| U1 | P1 | agent cli | `agent setup --target --dry-run` | type | → N1 | — |
| U2 | P1 | agent cli | `agent status --target` | type | → N1 | — |
| U3 | P1 | agent cli | `agent uninstall --target --dry-run` | type | → N1 | — |
| U4 | P1 | agent cli | plan output, paths plus changes | render | — | — |
| U5 | P1 | agent cli | summary output, executed actions | render | — | — |
| U6 | P1 | agent cli | doctor report, per-target health | render | — | — |
| U7 | P1 | agent cli | conflict report, refused path plus diff | render | — | — |
| U8 | P1 | agent cli | rollback info, backup path plus restore | render | — | — |
| U9 | P2 | home configs | written MCP entries plus skill dirs | perceive | — | — |

### Code Affordances

| # | Place | Component | Affordance | Control | Wires Out | Returns To |
| --- | ----- | --------- | ---------- | ------- | --------- | ---------- |
| N1 | P1 | agent cli | parse args to `Target` plus `Action` ADT | call | → N2 | — |
| N2 | P1 | shell | gather `GlobalFacts`, exe probe, file reads, version probes, writability | call | → N3, → N8 | — |
| N3 | P1 | core planner | `plan_global`, pure, returns `GlobalAction[]` | call | → N4, → N5 | — |
| N4 | P1 | core merge | JSON merge owned `mcptools` key, absent merge, equal skip, different refuse | call | → N6 | → N3 |
| N5 | P1 | core stage | stage skill files from bundled `include_str!` source | call | → N6 | → N3 |
| N6 | P1 | shell | atomic write, tmp plus rename, backup `<file>.mcptools-backup.<ts>` | write | S1, S2 | → N9 |
| N7 | P3 | shell | `codex mcp add/remove`, no TOML hand-edit | call | S1 | → N9 |
| N8 | P1 | core health | classify `Health` ADT, `Live` needs exe plus agent plus writable | call | → N9 | → U6 |
| N9 | P1 | core format | format plan, summary, doctor, conflict, rollback text, no secrets | call | — | → U4, → U5, → U6, → U7, → U8 |

### Data Stores

| # | Place | Store | Description |
| --- | ----- | ----- | ----------- |
| S1 | P2 | target configs | `~/.claude.json`, `opencode.json`, `~/.codex/config.toml` via N7, skill dirs |
| S2 | P2 | backup files | `<file>.mcptools-backup.<ts>`, restore via printed command |
| S3 | P1 | bundled templates | embedded skill plus guidance source, `include_str!` |

Spike evidence: `./spikes/spike-global-agent-paths.md`,
`./spikes/spike-merge-atomic-rollback.md`, `./spikes/spike-exe-path-version.md`.
