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
2. **Locate unknown paths with query.** `mcptools atlas query "provider" --json`
   finds ranked indexed files and symbols, including signatures and line spans.
   `--kind files|symbols|all` selects candidates; matching is case-insensitive
   literal text, not full-text source search. No descriptions or model setup are
   required. Exact symbol names rank above paths, signatures, and descriptions.
   Use `tree [path] --depth 1` for layout, not symbol lookup.
3. **Prefer `peek` over reading whole files.** `mcptools atlas peek <path>`
   shows a file's summary plus its symbols (functions, types, methods), or a
   directory's summary. Only Read the full file when you need exact code.
   Query `--excerpts --context-lines 20` reads bounded current-source excerpts
   only when requested. Indexed line spans may be stale: verify source before edits.
   Tree/query/peek default to 100 results (`--limit`, maximum 1000). Peek limits
   symbols and directory children separately; directory symbols are from immediate
   child files only. Tree depth defaults to 100, maximum 100; depth 0 is empty.
   `--context-chars` defaults to 8192 (maximum 32768), bounds total descriptions
   and signatures and each query excerpt, with `context_truncated` metadata.
   Output is deterministic, with total/returned/truncated bounds. CLI tree/peek
   preserve their existing JSON on stdout; bounds and freshness use stderr.
   CLI query JSON includes bounds and freshness in the document.
4. **Check freshness.** Tree and peek report working-tree freshness on CLI stderr;
   query JSON includes it on stdout (human query uses stderr). MCP exposes the
   `freshness` field, preserving the existing JSON/text result.
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

- Full-text source hunts (error messages, bodies): use grep. Query searches
  indexed identifiers, paths, signatures and available cached descriptions only.
- Files created seconds ago: uncommitted new files can be missing until an explicit
  structural refresh or post-commit hook runs. Fall back to direct reads.

## MCP

When connected, use `atlas_query` for unknown paths, `atlas_tree_view` for layout,
`atlas_peek` for known paths, and `atlas_status` for readiness. MCP content already
contains JSON-serialized text and mirrors it in `structuredContent`. Parse that
envelope, then its `text` JSON; it matches CLI JSON field meanings. Tree/peek
bounds are additive envelope fields; query bounds are also in `text`. Optional
descriptions are null, not required enrichment. Missing index: run
`mcptools atlas init`; stale/no matches: use source reads/grep or explicitly run
`mcptools atlas index`. No query refreshes the index or calls a model. The primer
is an optional resource, not a prerequisite.
