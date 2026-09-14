# Atlas redesign — Linear backlog

Prepared 2026-09-08 for the existing **MCPTools** Linear project.

**Publication status: not published.** The session has `LINEAR_API_KEY`, but its shell cannot resolve `api.linear.app`. No project ID, team ID, existing project issues, or remote mutations could be verified. This is a complete local task dump plus an offline-tested GraphQL importer, not a claim that Linear has been updated.

## Agreed outcome

- Structural `init`, `index`, `update`, and `sync` require no primer, editor, or model.
- The optional primer belongs to the operator. MCPTools does not rewrite it.
- `enrich [path]` performs explicit, scoped model work with preview, saved progress, cache reuse, and clear partial failure.
- `atlas-file` and `atlas-directory` are **llm-stream templates**, not presets. Provider/model selection is separate.
- One dedicated llm-stream process runner replaces the direct Ollama calls in Atlas.
- Atlas owns bounded retries and scheduling. llm-stream exposes reliable error metadata, including 429 and Retry-After when available.
- CLI/MCP and supported harness instructions work with structural data alone and expose stale or incomplete enrichment.

There are **21 issues: 20 implementation/evaluation tasks plus one parent initiative**, with **46 blocking dependencies**. Each task contains scope, acceptance checks, prerequisite keys, priority, and source evidence where applicable. Cross-repository llm_stream prerequisites stay in the MCPTools project, as requested. Task keys below are local import keys; Linear will assign its own issue identifiers.

No new project, team, labels, assignees, dates, or workflow states are invented. Existing project selection is resolved at import time. Priority 2 means high; priority 3 means normal.

## Files

- [backlog.json](backlog.json): complete issue payloads and dependency graph.
- [TASKS.md](TASKS.md): the same task content in readable form.
- [import_linear.py](import_linear.py): standard-library GraphQL importer.
- [test_import_linear.py](test_import_linear.py): offline importer checks.

## Import

Run from the mcptools checkout. The default command is offline validation only:

```sh
python3 docs/atlas-redesign/import_linear.py docs/atlas-redesign/backlog.json
```

On a host/session with network access and `LINEAR_API_KEY` already set:

```sh
python3 docs/atlas-redesign/import_linear.py docs/atlas-redesign/backlog.json --apply
```

The importer reads the key from the environment. Do not put it in this dump or in the command line. It resolves the exact project name without case sensitivity and the project's associated team. If those are ambiguous, use verified `--project-id` and `--team-id` values.

Issues are created under one parent. Each description includes a stable initiative/key marker. The importer reconciles those markers on rerun, preserves existing issue bodies/statuses/assignees, and creates native prerequisite → dependent `blocks` relationships. It saves confirmed results to `backlog.linear-receipt.json`. Keep that file when resuming.

An uncertain mutation stops the import. Rerun to reconcile remote state; do not blindly repeat a create request. The importer is designed to resume sequential runs; do not run concurrent importers. Markers prevent duplicates from this dump, but semantic duplicates among unrelated pre-existing issues still require project review when access is available.

The runtime Linear schema and real publishing remain unverified in this session. API basis: [Linear GraphQL documentation](https://linear.app/developers/graphql), including API-key authentication, pagination, mutation success, and GraphQL errors returned with HTTP 200.

The importer limits each nested project-issue snapshot page to 25 issues. Under Linear's documented query-complexity accounting, the former 100×100 issue/relation query exceeds the 10,000 cap before its other field selections. The live HTTP 400 body cannot be reproduced from this DNS-restricted session, so this does not prove the user's exact failure. HTTP failures now report the operation, status, and safe GraphQL message/code when Linear supplies them; the API key is redacted.

## Evidence and limits

- Inspected mcptools revision: `bd25315ec0f761925a214c2ce51a749f0a4a07e0`. Existing unrelated image-feature edits were present and preserved.
- Inspected llm_stream revision: `bdfadd75a2e076b7b9c371507618fdc4bbbfd496`.
- The installed Atlas binary indexed llm_stream structurally in 0.262 s: 89 files, 22 directories, 516 symbols. Unchanged incremental index: 0.051 s. These single runs used a /tmp database and no primer; no full model build was measured.
- Shell network restrictions blocked both Ollama access in the earlier run and Linear access in this turn. Do not infer provider throughput from structural timings.
- Graft's separate structural/enrichment flow, content-hash reuse, checkpointing, and scope handling were checked in [build.ts](https://github.com/trailhq/Graft/blob/main/src/graph/build.ts) and [CLI source](https://github.com/trailhq/Graft/blob/main/src/cli.ts). These were mutable main-branch sources; task ATLAS-01 pins a revision before formal comparison.
- Graft's benchmark numbers are not independently reproduced. No issue assumes that Graft is faster or more correct.
- Current Atlas MCP responses already carry JSON inside text content. The task is predictable/bounded output and better discovery, not correcting an alleged total lack of JSON.
- Current llm-stream accepts `--quiet true`, not bare `--quiet`. Existing template schema uses `template`, not `prompt`. Source inspection and an offline prompt-assembly run informed the backlog.

## Deliberate scope limits

No new vector store, embeddings pipeline, daemon, LSP dependency, full call graph, or concept graph is required for this delivery. Graft-inspired graph/search expansions remain separate experiments if the measured harness tasks justify them. Structural extraction already exists and should be reused.

A new automatic primer-writing feature is not included. If requested later, it must produce a separate suggestion without modifying operator text.

This session prepares project work; it does not implement the product redesign or modify llm_stream.
