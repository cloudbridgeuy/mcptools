# linear-01 — Run context

## Established facts (do not re-derive, do not contradict)
- Error contract follows Jira pattern: success payload to stdout, eyre text to stderr, nonzero exit, no {ok:...} envelope, no ok:true booleans
- Pagination is raw cursor passthrough (end_cursor, has_next); Jira hashed-token file store in crates/core/src/pagination.rs stays Jira-only
- Retry bounds locked: 10s timeout, max 3 read attempts, backoff with jitter, honor both x-ratelimit-requests-reset and x-ratelimit-complexity-reset plus Retry-After when present
- Writes and timeout-uncertain outcomes never auto-retried; report uncertainty plus re-query-by-ID reconciliation path
- LINEAR_API_KEY read at runtime only; key string never appears in Debug or error output, never written to tickets/logs/skills/client config
- cargo test never hits network; all transport failure modes use mock HTTP or fixtures
- Reuse reqwest, serde_json, tokio, clap from workspace; no new HTTP/JSON dependencies
- Module seams: pure transforms in crates/core/src/linear/ (jira.rs style), CLI dispatch and data fns in crates/mcptools/src/linear/ (atlassian/ style), wiring in crates/mcptools/src/main.rs SubCommands
- Future MCP JSON error boundary uses #[serde(tag = "type")], never boolean flag
- `mcptools_core::linear::{Viewer, LinearError, check_response, transform_viewer}` in `crates/core/src/linear/types.rs` — pure, offline-tested
- `LinearConfig::from_env()` in `crates/mcptools/src/linear/config.rs` — runtime-only `LINEAR_API_KEY`, actionable missing-key error, redacted `Debug`
- `build_client(cfg)` / `execute(client, query, variables)` in `crates/mcptools/src/linear/client.rs` — 10s timeout, `https://api.linear.app/graphql`, variables-object-only, never interpolates queries
- `auth_status_data(client)` in `crates/mcptools/src/linear/auth.rs` with `VIEWER_QUERY` constant there (shell, not core — signature needs reqwest::Client)
- CLI shape `mcptools linear auth status [--json]` via `crates/mcptools/src/linear/mod.rs` + `main.rs` `SubCommands::Linear`
- Error display truncation cap is 1000 chars in core `truncate`

## Gotchas
- Concurrent process committing in repo (pdf refactor, planning docs); slice lanes rebase on trunk-linear-01 at start, keep diffs scoped to crates/core/src/linear/* and crates/mcptools/src/linear/* plus main.rs wiring

## Conventions in this repo
- Test: cargo test -p mcptools_core linear (offline only, fixtures/mock HTTP)
- Crate names: mcptools_core (crates/core), mcptools (crates/mcptools)
- Pattern policy: ~/.claude/patterns/POLICY.md (MUST Functional Core-Imperative Shell; unit tests for every pure function)
- No comments of any kind in code; no shaping jargon (slice IDs, shape parts) in code
- Live Linear API: https://api.linear.app/graphql; viewer doc {viewer{id name email}}; issues nodes{id identifier title url} with pageInfo{hasNextPage endCursor}
