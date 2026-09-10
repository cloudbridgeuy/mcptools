---
shaping: true
handoff: true
---

# LINEAR-01 — Orchestration Handoff

## Repo

- Clone: /Users/guzmanmonne/Projects/Rust/mcptools
- Bare repo: none (standard checkout, not a bare-repo worktree)
- Canonical remote / main branch: git@github.com:cloudbridgeuy/mcptools.git / main
- Trunk lane for this feature: trunk-linear-01 (/Users/guzmanmonne/Projects/Rust/mcptools/.lane/trees/trunk-linear-01)
- Trunk branch for this feature: trunk-linear-01

## Documents

- Shaping: ./shaping.md
- Slices: ./slices.md

## DAG

- V1: no dependencies
- V2: depends on V1
- V3: depends on V2

Parallel batches: [V1] → [V2] → [V3]

## Pattern Policy

Pattern policy: `~/.claude/patterns/POLICY.md` (MUST Functional Core - Imperative Shell; SHOULD the rest when applicable; named principles for steering).

## Notes

- Source ticket is GUZ-79 (LINEAR-01 in docs/linear-cli/README.md). Only the
  shared client plus output contract plus `auth status` plus one minimal
  `issue get` proof. Full search/filter/detail is LINEAR-03, writes are
  LINEAR-04, comments/relations LINEAR-05, MCP LINEAR-06. Reject scope creep
  into those tickets.
- Existing error pattern wins: success payload to stdout, `eyre` text to
  stderr, nonzero exit. No `{ok:...}` envelope was invented. No `ok:true`
  booleans anywhere; use sum types. A future MCP JSON error boundary uses
  `#[serde(tag = "type")]`, never a boolean flag.
- Pagination is raw cursor passthrough (`end_cursor`, `has_next`). Do not
  reuse the Jira hashed-token file store (`crates/core/src/pagination.rs`);
  it stays Jira-only.
- Retry bounds are locked: 10s timeout, max 3 read attempts, backoff with
  jitter, honor both `x-ratelimit-requests-reset` and
  `x-ratelimit-complexity-reset`. Writes and timeout-uncertain outcomes are
  never auto-retried; report uncertainty plus the re-query-by-ID
  reconciliation path.
- `LINEAR_API_KEY` is read at runtime only. Assert in tests that the key
  string never appears in `Debug` or error output. Never write the key into
  tickets, logs, skills, or client configuration.
- `cargo test` must never hit the network. All transport failure modes use
  mock HTTP or fixtures.
- Live spike facts: viewer query `{viewer{id name email}}` returns 200; bad
  field returns HTTP 400 `GRAPHQL_VALIDATION_FAILED`; bad key returns HTTP
  401 `AUTHENTICATION_ERROR`; issues return
  `nodes{id identifier title url}` with `pageInfo{hasNextPage endCursor}`;
  rate state uses `x-ratelimit-requests-*`, `x-ratelimit-complexity-*`,
  `x-complexity` headers; no `Retry-After` was observed, so still honor it
  when present.
- Follow the existing module seams: pure transforms in
  `crates/core/src/atlassian/jira.rs` style (new `crates/core/src/linear/`),
  CLI dispatch and data fns in `crates/mcptools/src/atlassian/` style (new
  `crates/mcptools/src/linear/`), wiring in `crates/mcptools/src/main.rs`
  `SubCommands`. Reuse `reqwest`, `serde_json`, `tokio`, `clap` from the
  workspace. No new HTTP/JSON dependencies (YAGNI, Subtract Before You Add).
- A concurrent process is committing in this repo (pdf refactor, planning
  docs). Slice lanes are cheap; rebase on trunk-linear-01 at slice start and
  keep diffs scoped to `crates/core/src/linear/*` and
  `crates/mcptools/src/linear/*` plus `main.rs` wiring.
