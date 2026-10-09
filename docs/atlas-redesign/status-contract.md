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
