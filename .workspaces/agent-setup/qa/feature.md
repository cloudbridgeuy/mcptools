# QA — agent-setup: Global agent setup

**Tested SHA:** 09dff66f6d250115348174bce14dadb67ac7595d

Consolidated from `qa/V1.md` through `qa/V5.md` after all slices merged. Every item below re-ran on the trunk tree; slice evidence logs live outside the repo.

## Agent-run

| # | Slice | Scenario | Command / steps | Expected | Result |
| - | ----- | -------- | --------------- | -------- | ------ |
| 1 | V1 | Doctor reads, no writes, no git repo | `rm -rf /tmp/t1 && mkdir -p /tmp/t1 && HOME=/tmp/t1 target/debug/mcptools agent status --target all`, `find /tmp/t1 -mindepth 1 \| wc -l` | exit 0, 4 lines incl `pi: MissingAgent (executable not found)`, 0 files | PASS |
| 2 | V1 | HOME with spaces | `HOME="/tmp/t 2" target/debug/mcptools agent status --target all` | exit 0, 4 lines | PASS |
| 3 | V1 | Hermetic missing-agent matrix plus `opencode2` alias | `cargo test -p mcptools --test agent_status` | 4 passed | PASS |
| 4 | V2 | Dry-run plans, writes nothing | `touch /tmp/t2.marker && HOME=/tmp/t2 target/debug/mcptools agent setup --target claude --dry-run`, `find /tmp/t2 -newer /tmp/t2.marker` | exit 0, plan names config path plus owned change, `find` empty | PASS |
| 5 | V2 | Edited owned key refuses, identical skip | seed edited config, rerun dry-run; seed identical bytes, rerun | `refuse` with path plus diff; `already installed`, `find -newer` empty | PASS |
| 6 | V3 | Setup writes entry, rerun no-op | `HOME=/tmp/t3 target/debug/mcptools agent setup --target claude`, rerun, `stat` compare | exit 0, `created` then `already installed`, mtimes identical | PASS |
| 7 | V3 | Hand-edit refuses, bytes identical; overwrite backs up | hand-edit owned value, md5, rerun, md5; seed foreign-only config, setup, backup glob | `refuse`, md5 identical; `wrote` plus `backup` plus `restore:`, foreign key kept | PASS |
| 8 | V3 | Dry-run still plans, secrets absent | `--dry-run` plus `find -newer`; `LINEAR_API_KEY=secret-value` runs plus `grep -r` sweep | plan lines, 0 newer files, no matches | PASS |
| 9 | V4 | Setup creates SKILL.md, uninstall removes dir | `HOME=/tmp/t4` setup, uninstall | `created`, then owned strip with backup plus skill removal | PASS |
| 10 | V4 | Hand-edited skill survives, foreign untouched | hand-edit SKILL.md plus foreign key/dir seeds, uninstall, md5 compare | `user-edited, left unchanged`, md5 identical | PASS |
| 11 | V4 | Pi and opencode uninstall, spaced HOME | per-target setup/uninstall, `HOME="/tmp/t4 demo"` end to end | entries removed, spaced paths work | PASS |
| 12 | V5 | All-matrix setup on spaced HOME with secret canary | `LINEAR_API_KEY=canary-9f31-v5 HOME="/tmp/t 5" target/debug/mcptools agent setup --target all` | exit 0, 6 file lines plus codex registered line | PASS |
| 13 | V5 | Pi skill lands, codex via CLI hermetic, real HOME untouched | `ls` pi skill, `cat` temp codex config, `stat` real config | skill exists, `[mcp_servers.mcptools]` in temp HOME, real mtime Sep 9 | PASS |
| 14 | V5 | Status honesty, secret sweeps, empty-PATH matrix | status all; `grep -r` canary over HOME and stdout; `PATH`=empty-dir status | pi `MissingAgent` plus JSON `Live`, no matches, all-`MissingAgent` | PASS |
| 15 | V5 | Missing codex binary degrades, uninstall all removes | empty-PATH setup/uninstall all | exit 0 with honest skip lines; skills deleted, keys stripped, codex entry removed | PASS |
| 16 | All | Full suite | `cargo test --workspace` | 140 + 4 + 646 + 248 + 16 passed, 0 failed | PASS |
| 17 | All | Typecheck plus clippy | `cargo check --workspace`; `cargo clippy --workspace -- -D warnings` | check clean; clippy only pre-existing `core/src/images.rs:196`, `md.rs:172` | PASS |
| 18 | All | No comments in diff | grep new files for comment markers | only `#[test]`/`#[cfg(test)]` directives | PASS |

## User-run

None. Every scenario is agent-checkable; no visual, UX, or external-account judgment involved.
