# Atlas status and enrichment contract (version 1)

`mcptools atlas status --json` and `mcptools atlas enrich --json` emit one JSON
document on stdout. Progress, retry reports, logs, and errors use stderr. Human
output uses the same status and execution records. There are no live model calls
in status or enrichment dry runs.

## Exit codes and outcomes

| Command result | Exit | JSON `outcome` |
| --- | --- | --- |
| Status read, including an absent index or optional enrichment configuration failure | 0 | `success` |
| Enrichment complete | 0 | `success` |
| Enrichment plan only | 0 | `dry_run` |
| Enrichment unfinished, failed, bounded by a limit, or deferred | 1 | `partial` |
| Enrichment interrupted by SIGINT during execution | 1 | `cancelled` |
| Enrichment preflight input/configuration/runner failure; status structural configuration failure | 1 | `config_failure` |
| Repository/index I/O failure in status; execution infrastructure failure in enrichment | 1 | `execution_failure` |
| CLI argument parsing error (before command execution) | 2 | No JSON document |

These retain the CLI's existing process exit classes. Use `outcome`, not stderr
text, to distinguish exit-1 cases. Failed/cancelled enrichment saves completed
descriptions and retry state. A `resume` command is included for partial and
cancelled results. Forced process termination cannot guarantee a result document;
durable leases remain recoverable by the next enrichment run.

## Status schema

All status results have `schema_version: 1`. Existing count, `last_sync`, and
`primer_hash` fields remain. `primer_excerpt` is removed intentionally: optional
operator context must not appear in status.

- `outcome`: value from the table above.
- `structural`: `uninitialized` (database absent), `not_yet_synced` (database
  exists but neither `last_full_sync` nor `last_update` exists), `ready`, `stale`,
  or `unknown` (read failed). Ready/stale compares stored content hashes with the
  current repository using the same walker, ignores, and optional-primer exclusion
  as structural indexing. This is not a timestamp or Git-only estimate.
- `guidance`: actionable command or error guidance; otherwise null.
- `enrichment`: counts for `current`, `missing`, `stale`, `failed`, `deferred`,
  `in_flight`, and `unknown`. Every indexed file/directory contributes once.
  Saved current identities are checked against current sources, optional context,
  provider/model, settings, and template content without making provider calls.
  Unavailable configuration/templates/context makes compatibility `unknown`,
  rather than asserting that a cached description is current.
  Structural additions, changes, or removals mark saved descriptions for affected
  files and ancestor directories stale, including newly ignored or symlinked files.
- `enrichment_state`: summary with precedence `failed`, `deferred`, `stale`,
  `partial` (missing/in-flight/unknown), then `current`. Absent indexes are partial.
  An empty synchronized index can be current with zero entries.
- `active_enrichment`: tagged object. `state` is `disabled`, `config_failure`, or
  `enabled`. Enabled includes `provider` and `file`/`directory`, each containing
  `model` and `template`. Optional enrichment config failure does not prevent
  structural status.
- `primer_present`: file-presence information only. Absence is informational,
  not an initialization/configuration failure; no context is read for display.
- `retries`: ordered records containing `kind` (`files`/`directories`), `path`,
  `attempts`, `next_eligible_at_ms` (Unix epoch milliseconds), and `exhausted`.
  Deferred records include the active provider/account profile's shared cooldown
  when configured. With no valid active configuration, only the persisted target
  retry time is known. Retry records for superseded identities are excluded.
- `last_execution`: latest completed enrichment invocation's telemetry, or null.
  Historical telemetry retains its meaning; it is not current readiness.

Error status documents use `structural: unknown`; default zero counts are not
observations. Consumers must check `outcome` before consuming counts.

MCP `atlas_status` retains its existing `{format: "status", text: "..."}` output
schema. `text` is the same versioned status JSON as the CLI. The dual text and
structured-content envelopes agree. An absent index returns initialization
guidance without creating a database. Input argument errors use JSON-RPC -32602;
configuration/index errors use -32603 rather than a successful status document.

## Enrichment schema and telemetry

All enrich results have `schema_version`, `outcome`, `plan`, `active_enrichment`,
`telemetry`, `retries`, `resume`, and `guidance`. Preflight/infrastructure failure
documents have null plan/active identity/resume and default telemetry; defaults
are not measured execution results.

`plan.entries` contains `target: {kind: "file"|"directory", path}`, `status`
(`reusable`, `missing`, `stale`, `failed`, `deferred`, `in_flight`),
`context_tokens_estimate`, and `scheduled`. Token estimates are never measured
token usage. Dry runs do not reserve claims, consume retries, or change metadata.

Telemetry fields:

- `total_elapsed_ms`: invocation wall-clock time through execution completion,
  before telemetry persistence/output.
- `preparation_ms`: configuration, scope, and plan preparation wall-clock time.
- `files_ms` / `directories_ms`: execution phase wall-clock time, including waits
  and retries. Concurrent request times are not summed into these phase times.
- `cache_hits`: reusable entries at planning time.
- `completed`, `failed`, `deferred`: final selected-target counts, including cache
  hits and unscheduled targets. They are not numbers of new provider calls.
- `requests`: runner-attempt records with `path`, `attempt`, `elapsed_ms`,
  `outcome` (`completed`, `failed`, `deferred`, `cancelled`, `superseded`),
  `input_bytes` (rendered system + prompt UTF-8 bytes), `output_bytes` (answer
  bytes, null when no successful envelope), and nullable `usage`. Waiting and
  cache-hit events do not create request records. Attempt timing covers the
  runner call, including contract verification and termination, not provider-only
  latency. Failure records can describe a runner attempt that never reached a
  provider.
- `completed_requests`, `failed_requests`, `deferred_requests`: counts of runner
  attempts whose final outcome is exactly `completed`, `failed`, or `deferred`,
  respectively. Cancelled/superseded attempts remain in `requests` with their own
  outcomes and are excluded from these three counts; their sum can be less than
  the number of request records.
  A successful provider envelope with invalid description content counts as a
  failed request; one rejected because inputs changed counts as deferred.
- `usage`: only provider-reported numeric `input_tokens`, `output_tokens`, and
  `total_tokens`, each nullable. `prompt_tokens`/`completion_tokens` map to the
  respective input/output fields. Unsupported/missing usage is null. Totals are
  not inferred. No cost is fabricated. Raw provider usage objects, secrets,
  prompts, and optional primer contents are not emitted or persisted as telemetry.

Descriptions with durable failed/stale/missing states remain excluded from
ordinary tree/peek views. Status also checks current input compatibility; this
read does not change the durable state or rebuild the structural index.
Readiness is independent from enrichment: structural navigation can be ready
while enrichment is partial, deferred, or failed.

## Navigation query and bounds (GUZ-38)

`atlas query TEXT` (`search` alias) and MCP `atlas_query` use the same model-free
index read. `kind` is `all` (default), `files`, or `symbols`. Nonblank text up to
1024 bytes is trimmed and matched as a case-insensitive literal substring. Files
match paths or available short descriptions; symbols match names or signatures.
Scores: symbol name 101-103, path 61-63, signature 31-33, description 11-13;
exact, prefix and substring matches receive +3, +2 and +1 respectively. Sort by
descending score, then path, then symbol line span/name/kind/signature/visibility.
There is one hit per file or symbol, with `matched_on` explaining its score. A file and a
symbol in that file can both match. Structural-only indexes work: descriptions
are nullable, symbol signatures and spans come from the structural index.

All three navigation tools accept `limit` (default 100, range 1-1000). Bounds
contain `limit`, `total`, `returned`, `truncated`, `context_chars`, and
`context_truncated`. `context_chars` / `--context-chars` defaults to 8192 (range
0-32768): a total character budget for returned descriptions and signatures,
allocated in result order. Peek allocates descriptions before signatures;
directory children have their own equal budget. Null context stays null;
truncated strings stay strings. This budget does not change matching/ranking or
drop symbol locations. `context_truncated` reports shortened context fields,
independently of the result-count `truncated` flag. Query totals count matching
hits before truncation. Tree totals count entries inside the requested depth
before the result limit. Tree depth defaults to 100 in CLI and MCP (range 0-100);
0 is empty and 1 contains immediate children. Paths are repository-relative;
`.` and trailing separators normalize, absolute paths and `..` are rejected.
Literal `%`, `_`, and backslash in directory names are not SQL wildcards. Tree
order is path order. Peek symbol order is file path then start/end line, name,
kind, signature, visibility; directory peek aggregates immediate child files
only. Peek applies the limit independently to symbols and immediate children.

Query excerpts are absent by default. `--excerpts` / `excerpts: true` reads
current working-tree source only for returned hits. `context_lines` defaults to
20 (range 0-200); 0 disables reads. A zero `context_chars` also disables reads.
Each hit has at most `context_chars` excerpt characters (8192 by default);
each source read is capped at 1 MiB. Symbol excerpts start at the indexed line
and stop at its indexed end or the line limit; file excerpts start at line 1.
`source: current_worktree`, excerpt line spans, `truncated` and nullable `error`
identify this non-atomic source read. Missing/oversized/non-UTF-8/out-of-repository
or symlinked sources produce an excerpt error, not a lost structural hit. Stale
index ranges are navigation hints, not proof that a symbol still occupies them.
Bounds limit returned output, not database scan work: query scans indexed files
and symbols, without new search dependencies. No source bodies are indexed.

CLI query `--json` equals the JSON inside MCP query `text`, including `freshness`
and `bounds`. CLI tree/peek preserve their existing JSON shapes on stdout; shared
bounds are JSON on stderr (`bounds=...`), alongside freshness. MCP preserves
`{format,text,freshness}` with additive `bounds` and optional `children_bounds`.
MCP `content[0].text` already contains a JSON-serialized envelope; supported
`structuredContent` mirrors it exactly. Parse the envelope, then `text` for view
JSON. There was no missing-JSON defect. Optional descriptions retain their
existing durable-state filtering and cached/unverified freshness semantics.

Tool choice: query locates unknown paths, tree shows layout, peek inspects a
known path, status checks readiness/enrichment. An absent index never creates a
database and reports `mcptools atlas init`. Binding/schema errors retain
actionable `atlas index` guidance. Invalid MCP inputs use -32602; index/I/O
failures use -32603. Stale or absent matches need an explicit `atlas index`
refresh or grep/source reads. Exact source-body searches use grep; verify current
code before edits. None of these tools launches a model or refreshes implicitly.

The offline provider-location contract indexes a small Rust provider file with
no descriptions and no available runner. A name-only query finds its exact
symbol above the prefix match, preserves signatures/spans, agrees across CLI
and MCP, and returns current-source excerpts only on request. It also checks
result/context truncation, missing source, empty matches, literal directory
prefixes, invalid limits/paths, and peek bounds.
