---
shaping: true
---

# LINEAR-01 — Shared Linear client and noninteractive output contract — Shaping

Source ticket: GUZ-79, "Add the shared Linear client and noninteractive output contract".
Backlog detail: `docs/linear-cli/README.md` (LINEAR-01). Parent initiative: linear-cli
global agents (LINEAR-00..LINEAR-07, AGENTS-01..03). This ticket is the foundation;
issue list/search/get-full, create/update, comments/relations, MCP exposure, and
global setup belong to later tickets. Do not build them here.

## Requirements (R)

| ID | Requirement | Status |
|----|-------------|--------|
| R0 | Add `mcptools linear` entry point and a shared typed GraphQL client for all future Linear CLI and MCP use | Core goal |
| R1 | Read `LINEAR_API_KEY` at runtime; `auth status` checks viewer access and reports identity without exposing the key; missing/invalid credentials return actionable errors | Must-have |
| R2 | Stable JSON success shapes, resource IDs and URLs, pagination fields, nonzero error exits; stdout carries only requested data, diagnostics go to stderr; commands never require a TTY | Must-have |
| R3 | Use GraphQL variables; reject invalid input before requests; check HTTP errors, GraphQL errors even with HTTP 200, partial data, and mutation `success` flags | Must-have |
| R4 | Set request timeouts and bounded read retries; handle HTTP 429 and GraphQL `RATELIMITED`, including HTTP 400; respect available reset/retry metadata | Must-have |
| R5 | Never blindly retry mutations after an uncertain transport outcome; report the uncertainty and a reconciliation path | Must-have |
| R6 | Offline tests cover missing auth, redaction, timeout, malformed JSON, partial errors, rate limits, and exhausted retries | Must-have |
| R7 | Keep HTTP effects separate from input and response conversion; reuse installed HTTP/JSON dependencies (`reqwest`, `serde_json`, `tokio`, `clap`) | Must-have |

Locked decisions from shaping (do not reopen without the operator):

- Errors follow the existing Jira pattern: success JSON (or human table) to
  stdout, `eyre`/`color-eyre` text to stderr, nonzero exit. No `{ok:...}`
  envelope was invented. See `crates/mcptools/src/atlassian/jira/search.rs`
  handler and `crates/mcptools/src/atlassian/jira/get.rs` handler.
- Pagination is raw cursor passthrough (`end_cursor`, `has_next`), matching
  Linear `pageInfo{hasNextPage,endCursor}`. The Jira hashed-token file store
  (`crates/core/src/pagination.rs`) stays Jira-only; Linear cursors are short
  and statelessness beats hiding them.
- Retry bounds: 10s request timeout, max 3 read attempts, backoff with jitter,
  honor both `x-ratelimit-requests-reset` and `x-ratelimit-complexity-reset`.
  Writes are never auto-retried.
- No `ok:true` booleans anywhere. Success output is the bare resource struct;
  failures are `Err` to stderr. If a JSON error must cross the MCP boundary
  later (LINEAR-06), use a `#[serde(tag = "type")]` enum, never a boolean flag.
- Scope adds one minimal proof read: `linear issue get <identifier>` returning
  `id, identifier, title, url, state`. Full search/filter/detail is LINEAR-03.

## A: Shared typed GraphQL shell plus pure core, noninteractive contract (selected)

| Part | Mechanism |
|------|-----------|
| A1 | New `crates/core/src/linear/` pure core: `Viewer`, `IssueMini`, `PageInfo`, `GraphQLRequest`, `GraphQLResponse`, `LinearError` ADT, redaction, retry classification, pagination normalize. No `reqwest`. Unit tests with fixtures only. |
| A2 | New `crates/mcptools/src/linear/` imperative shell: `LinearConfig::from_env`, `build_client(timeout)`, `execute()` with variables, `auth status` probe (`{viewer{id name email}}`). Mirrors `crates/mcptools/src/atlassian/mod.rs` config plus `jira/get.rs` data-fn pattern. |
| A3 | Output contract: `--json` prints JSON to stdout; human table otherwise; all logs/errors to stderr; nonzero exit on error. Reuses `prelude.rs` `println`/`eprintln` split. |
| A4 | Validation first: parse `LINEAR_API_KEY`, identifiers, pagination args before I/O. No string-interpolated GraphQL; `variables` map only. |
| A5 | Response check pipeline: HTTP status, JSON parse, `errors[]` scan, `data` presence, `success` flag. Partial `data+errors` is an error, not success. |
| A6 | Retry policy: timeout plus bounded retries for safe reads only. 429 / `RATELIMITED` / 400-rate respect `Retry-After` when present and both `x-ratelimit-*-reset` headers. No auto-retry on mutations or timeout-uncertain writes. |
| A7 | Mutation uncertainty rule: transport timeout/error after send returns an uncertainty report with reconciliation (`query by ID`); caller reconciles, never blind-retries. |
| A8 | Offline test matrix in core plus shell with mock HTTP: R1, R3, R4, R5, R6 cases. No network in `cargo test`. |

YAGNI cut: no issue list/create/update, no MCP tools, no OAuth, no caching.
Only the client plus `auth status` plus one minimal `issue get` proof.

## Fit Check: R x A

| Req | Requirement | Status | A |
|-----|-------------|--------|---|
| R0 | Add `mcptools linear` entry and shared typed GraphQL client | Core goal | ✅ |
| R1 | Runtime `LINEAR_API_KEY`; viewer check; no key leak; actionable errors | Must-have | ✅ |
| R2 | Stable JSON shapes, IDs/URLs, pagination, nonzero exit, stdout/stderr split, no TTY | Must-have | ✅ |
| R3 | Variables; pre-request validation; HTTP plus GraphQL-200 errors plus partial plus success flags | Must-have | ✅ |
| R4 | Timeouts, bounded read retries, 429/RATELIMITED/400-rate with reset metadata | Must-have | ✅ |
| R5 | No blind mutation retry; report uncertainty plus reconciliation | Must-have | ✅ |
| R6 | Offline tests for auth, redaction, timeout, bad JSON, partial, rate, exhaust | Must-have | ✅ |
| R7 | Effects separate from conversion; reuse installed deps | Must-have | ✅ |

**Notes:**

- A passes all R by construction. Cost: A defines the contract before the
  first real query. Slower start, faster later ops.
- Live spike evidence: `viewer{id name email}` returns 200 with identity;
  bad field returns HTTP 400 `GRAPHQL_VALIDATION_FAILED`; bad key returns
  HTTP 401 `AUTHENTICATION_ERROR`; issues pagination returns
  `nodes{id identifier title url}` plus `pageInfo{hasNextPage endCursor}`;
  rate state arrives via `x-ratelimit-requests-*`,
  `x-ratelimit-complexity-*`, `x-complexity` headers with no `Retry-After` seen.
