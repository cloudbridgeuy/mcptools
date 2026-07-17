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

1. **Read the primer first.** `.mcptools/atlas/primer.md` holds the repo's
   mental model: architecture, conventions, key paths. Read it once per
   session before exploring.
2. **Prefer `tree` over `ls`/`find`.** `mcptools atlas tree [path]` shows the
   annotated directory tree — every entry carries a one-line description.
3. **Prefer `peek` over reading whole files.** `mcptools atlas peek <path>`
   shows a file's summary plus its symbols (functions, types, methods), or a
   directory's summary. Only Read the full file when you need exact code.
4. **Check freshness when results look wrong.** `mcptools atlas status`
   reports index health. If the index lags the working tree, run
   `mcptools atlas update`.

## When NOT to use atlas

- Exact-string hunts (error messages, identifiers): use grep — atlas
  complements it, it does not replace it.
- Files created seconds ago: the index updates post-commit; uncommitted new
  files may be missing. Fall back to direct reads.

## MCP

When the `mcptools` MCP server is connected, use the `atlas_tree_view`,
`atlas_peek`, and `atlas_status` tools instead of shelling out; the primer is
exposed as an MCP resource.
