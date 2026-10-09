---
name: atlas-navigation
description: Use when exploring, navigating, or searching this codebase - atlas maintains an indexed mental model (primer, annotated tree, file summaries) that answers structure questions without reading whole files
---

<!-- atlas-template v1 -->

# Atlas Navigation

This repository is indexed by atlas (`mcptools atlas`). Use the index before
manual exploration — it answers "where is X / what does Y do" questions in one
command instead of many file reads.

## Workflow

1. **Read optional operator context if present.** `.mcptools/atlas/primer.md` holds the repo's
   mental model: architecture, conventions, key paths. The path can be changed
   with `primer_path` in `.mcptools/config.toml` or `ATLAS_PRIMER_PATH`.
   If absent, continue without it. Never create or rewrite it automatically.
2. **Prefer `tree` over `ls`/`find`.** `mcptools atlas tree [path]` shows the
   directory tree with symbols and any cached descriptions.
3. **Prefer `peek` over reading whole files.** `mcptools atlas peek <path>`
   shows a file's summary plus its symbols (functions, types, methods), or a
   directory's summary. Only Read the full file when you need exact code.
4. **Check freshness.** Tree and peek report working-tree freshness on CLI stderr
   or the MCP `freshness` field, preserving the existing JSON/text result.
   A raw, non-symlink-following preflight limits the whole checkout, including
   ignored entries and ignore files, to 10,000 entries and 16 MiB of regular-file
   sizes. Database, primer, and sidecar bytes are excluded. The ignore-aware hash
   walk runs only after that check succeeds and retains byte and deadline checks.
   The shared 250 ms deadline is checked between filesystem operations and after
   reads; a synchronous operation can overrun it. External ignore sources,
   symlink/special-file ignore sources, and Git-directory reference files report `unknown`
   rather than permit unchecked ignore processing.
   Incomplete or failed scans report `unknown`, never infer deletions, and never
   refresh automatically or launch a model. `ready` is a best-effort snapshot,
   not an atomic view of concurrent filesystem edits. Semantic context is cached
   and unverified; missing, stale, failed, deferred, and in-flight counts are shown.
   Run `mcptools atlas index` or `update` for an authoritative byte-hash refresh.
   These commands detect dirty edits even with unchanged size and timestamps.
   Use `mcptools atlas status` for detailed enrichment checks.

Each database is bound to the canonical checkout root and Git directory. Linked
worktrees use separate default databases. Configured databases must not be shared
between repositories or worktrees. A moved checkout or unbound legacy database
fails closed: choose a new database path and run `mcptools atlas index`.

## When NOT to use atlas

- Exact-string hunts (error messages, identifiers): use grep — atlas
  complements it, it does not replace it.
- Files created seconds ago: uncommitted new files can be missing until an explicit
  structural refresh or post-commit hook runs. Fall back to direct reads.

## MCP

When the `mcptools` MCP server is connected, use the `atlas_tree_view`,
`atlas_peek`, and `atlas_status` tools instead of shelling out; the primer is
exposed as an MCP resource.
