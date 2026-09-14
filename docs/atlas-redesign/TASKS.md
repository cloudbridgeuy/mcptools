# Atlas redesign task dump

These are local import keys, not existing Linear issue IDs. Publication status: **not published**.

| Key | Task | Priority | Prerequisites |
|---|---|---|---|
| ATLAS-00 | Make Atlas fast to start and move MCPTools model calls to llm-stream | High | — |
| ATLAS-01 | Record Atlas baseline and verify relevant Graft mechanisms | Normal | — |
| ATLAS-02 | Expose a reliable automation contract from llm-stream | High | — |
| ATLAS-03 | Ship and install Atlas templates for llm-stream | High | ATLAS-02 |
| ATLAS-04 | Add the shared llm-stream process runner | High | ATLAS-02 |
| ATLAS-05 | Replace Atlas Ollama configuration with template and runner settings | High | ATLAS-02, ATLAS-03 |
| ATLAS-06 | Handle 429 with bounded retries and shared cooldowns | High | ATLAS-02, ATLAS-04, ATLAS-05 |
| ATLAS-07 | Make Atlas initialization and structural refresh model-free | High | — |
| ATLAS-08 | Make the primer optional and preserve operator text | High | ATLAS-07 |
| ATLAS-09 | Track enrichment provenance, stale state, and durable progress | High | ATLAS-05, ATLAS-08 |
| ATLAS-10 | Plan scoped enrichment before making model calls | High | ATLAS-03, ATLAS-05, ATLAS-07, ATLAS-09 |
| ATLAS-11 | Execute and resume explicit Atlas enrichment | High | ATLAS-04, ATLAS-06, ATLAS-08, ATLAS-09, ATLAS-10 |
| ATLAS-12 | Bound prompt size and remove unnecessary enrichment serialization | Normal | ATLAS-01, ATLAS-11 |
| ATLAS-13 | Expose structural readiness, enrichment progress, and retry state | High | ATLAS-09, ATLAS-11 |
| ATLAS-14 | Keep structural data fresh across edits, deletions, and worktrees | High | ATLAS-07, ATLAS-09 |
| ATLAS-15 | Provide a focused Atlas query and predictable CLI/MCP results | High | ATLAS-13, ATLAS-14 |
| ATLAS-16 | Install Atlas guidance for Codex and Claude harnesses | High | ATLAS-03, ATLAS-07, ATLAS-08, ATLAS-15 |
| ATLAS-17 | Migrate GrepRAG to llm-stream and an explicit template | High | ATLAS-03, ATLAS-04, ATLAS-05 |
| ATLAS-18 | Migrate Strand to llm-stream with a portable prompt contract | High | ATLAS-03, ATLAS-04, ATLAS-05 |
| ATLAS-19 | Remove obsolete Ollama clients, assets, and dependency wiring | Normal | ATLAS-11, ATLAS-17, ATLAS-18 |
| ATLAS-20 | Verify the complete workflow under failures and concurrency | High | ATLAS-06, ATLAS-11, ATLAS-12, ATLAS-13, ATLAS-14, ATLAS-16, ATLAS-17, ATLAS-18 |
| ATLAS-21 | Measure Atlas usefulness and enrichment cost after redesign | Normal | ATLAS-01, ATLAS-12, ATLAS-15, ATLAS-16, ATLAS-20 |
| ATLAS-22 | Publish migration instructions and the complete operator flow | Normal | ATLAS-19, ATLAS-20, ATLAS-21 |

## ATLAS-00: Make Atlas fast to start and move MCPTools model calls to llm-stream

Parent: none. Priority: high.

Area: Initiative

Deliver the agreed Atlas workflow and replace direct Ollama integrations in Atlas, GrepRAG, and Strand with a shared llm-stream process runner. Use Atlas llm-stream templates, not presets, for prompt definitions. All work, including prerequisites in the llm_stream repository, belongs in the existing MCPTools Linear project.

Acceptance criteria:

- [ ] init/index/update/sync build or refresh structural data without an editor, primer, model binary, or model request. Read/query operations never start paid or free model work implicitly.
- [ ] Primer is optional operator-owned context; MCPTools never rewrites its contents.
- [ ] enrich [path] is explicit, scoped, previewable, bounded, resumable, and tolerant of rate limits without hiding failure.
- [ ] Atlas file/directory instructions are versioned llm-stream templates; provider/model/auth choices are independent.
- [ ] All three model consumers use the shared runner; no direct Ollama client, special Ollama model, or Rig dependency remains for these commands.
- [ ] CLI/MCP/harness integration is usable with structural data alone, exposes freshness and partial completion, and passes the measured evaluation.
- [ ] Release includes migration guidance, implementation in both repositories where required, offline failure coverage, and recorded before/after evidence.

Prerequisites: none.

Evidence:

- Session design decisions, 2026-09-08. Graft is a source of patterns, not an assumed performance baseline.
- Scope excludes building a Graft clone, adding embeddings/vector storage, LSP services, or call-graph/concept-graph features without separate evidence and approval.

## ATLAS-01: Record Atlas baseline and verify relevant Graft mechanisms

Parent: ATLAS-00. Priority: normal.

Area: MCPTools / evaluation

Create a reproducible baseline on ~/Projects/Rust/llm_stream before changing behavior. Compare mechanisms and workload, not product claims. Inspect a pinned Graft revision for structural/enrichment separation, checkpointing, scope, and harness discovery.

Acceptance criteria:

- [ ] Record repository revisions, uncommitted input differences, binary version/build, file counts, hardware, provider/model, template/context hashes, concurrency, and cache state.
- [ ] Keep the observed structural run separate from full enrichment: 89 files, 22 directories, 516 symbols; 0.262 s initial, 0.051 s unchanged, 0.015 s tree, 0.075 s peek. These are single local measurements with /tmp database, no primer, and no model access.
- [ ] Measure current full enrichment when an explicitly selected provider is accessible; record blocked measurements as missing, never zero.
- [ ] Define comparable coding tasks, correct target files/symbols, repeated trial counts, and baseline metrics for time to useful context, total enrichment, request count, token usage when available, and task correctness.
- [ ] Pin Graft source and document verified mechanisms, unsupported claims, and language/extraction limits. Do not compare Graft structural timing against Atlas enriched timing.

Prerequisites: none.

Evidence:

- mcptools revision bd25315ec0f761925a214c2ce51a749f0a4a07e0; llm_stream revision bdfadd75a2e076b7b9c371507618fdc4bbbfd496.
- https://github.com/trailhq/Graft/blob/main/src/graph/build.ts
- https://github.com/trailhq/Graft/blob/main/src/cli.ts
- Existing measurement artifact: /tmp/atlas-llm-stream-yp9js0qh/measurements.json (session-local).

## ATLAS-02: Expose a reliable automation contract from llm-stream

Parent: ATLAS-00. Priority: high.

Area: llm_stream / required prerequisite

Add a minimal versioned machine interface over existing provider dispatch. The caller renders prompts in-process (GUZ-24); the contract carries rendered prompt text, not template identity. MCPTools must receive structured success/error information without parsing human stderr. Preserve the ordinary interactive CLI.

Acceptance criteria:

- [ ] Specify and test one request containing rendered prompt text, explicit provider/model options, and optional context. Carry large input through stdin or a file, not an unbounded command-line argument.
- [ ] Return answer text plus resolved provider/model identity on success. Expose usage only when the provider supplies it; absent usage is unknown.
- [ ] Return typed failure metadata: category, provider HTTP status if available, retryability, Retry-After raw/normalized value when available, and a safe message. Preserve nonzero exit for failed requests, including failures after partial streamed output.
- [ ] Carry HTTP 429 and Retry-After from HTTP providers through existing error types. Distinguish temporary limits from known permanent quota exhaustion. Mark unavailable metadata explicitly for CLI-backed providers; do not invent a status from arbitrary model text.
- [ ] Use a fresh conversation with no response-history writes. Preserve configured credentials/auth refresh and explicit provider settings; prevent unrelated presets, history, or user system prompts from contaminating the request.
- [ ] Test malformed requests, 401/403/404/429/5xx, connection errors, partial output then failure, and child provider failure even when that child exits zero.
- [ ] Preserve machine stdout framing and keep diagnostics separate. A pure text request cannot enable provider tools, hooks, or repository edits implicitly; verify effective tool-disable controls for CLI-backed providers.

Prerequisites: none.

Evidence:

- llm_stream crates/llm_stream/src/main.rs, prelude.rs, config.rs, error.rs, claude.rs, and provider adapters.
- Current top-level main exits 1 on errors; Claude result.is_error is already parsed. Preserve this behavior.
- Current --no-cache prevents conversation writes but startup still creates config/templates/cache directories. An isolated config directory alone would lose existing authentication.
- Reopened 2026-09-14: validation audit found no contract work on llm_stream 6d4270c9. Scope narrowed per GUZ-20 decision: prompt-text-in; template rendering stays caller-side.

## ATLAS-03: Ship and install Atlas templates for llm-stream

Parent: ATLAS-00. Priority: high.

Area: MCPTools + llm_stream / templates

Define provider-neutral atlas-file and atlas-directory templates using llm-stream's existing TOML template format. Move prompt instructions into these assets. Model choice does not belong in template definitions; presets are not required.

Acceptance criteria:

- [ ] Template files use the actual schema: name, description, template, default_vars, and system. Avoid the stale README's prompt field.
- [ ] Document explicit variables for optional operator context, path, structural symbols, bounded source text, and child summaries. Define missing context as valid input and keep source text as data.
- [ ] Use one validated description response contract. Include stable examples and test rendering for quotes, Unicode, empty context, braces, and large source input.
- [ ] Provide idempotent installation/discovery in a selected llm-stream template location with preview. Detect malformed assets and name collisions; never silently overwrite operator-edited templates.
- [ ] Track effective template content/version for cache identity. Template updates invalidate only applicable enrichment.
- [ ] Verify system precedence, {{prompt}} versus {{stdin}} behavior, and vars/default_vars precedence against runtime rendering. Do not pass a separate Atlas system prompt that silently overrides the selected template.
- [ ] Use the minimal template features required; do not use conversation templates unless their empty-conversation panic path is fixed and tested.

Prerequisites: ATLAS-02.

Evidence:

- llm_stream crates/llm_stream/src/config.rs:42; prelude.rs template loading/rendering.
- mcptools crates/core/src/atlas/prompts.rs; crates/mcptools/src/atlas/templates/ currently contains harness instructions, not model templates.

## ATLAS-04: Add the shared llm-stream process runner

Parent: ATLAS-00. Priority: high.

Area: MCPTools / shared runtime

Create one small subprocess adapter for Atlas, GrepRAG, and Strand. Reuse the existing async runtime and standard process facilities. Delegate provider transport/auth to llm-stream.

Acceptance criteria:

- [ ] Spawn the executable directly with argument arrays, never a shell. Support a trusted executable override and detect missing or incompatible versions before model work.
- [ ] Use the machine contract and pipe request input; concurrently drain stdout and stderr to avoid pipe deadlocks. Bound captured output and redact credentials from errors/logs.
- [ ] Enforce per-attempt timeout, cancellation, and cleanup/reaping. Terminate request-owned descendants for CLI-backed providers without affecting unrelated sessions.
- [ ] Reject nonzero exit, malformed/truncated response, empty invalid output, or a failed final event even if partial text was emitted. Do not commit partial output.
- [ ] Preserve access to operator-selected configuration/auth while disabling request history and accidental prompt overrides.
- [ ] Keep execution and classification separate from Atlas scheduling/retry policy; do not layer automatic hidden retries in both tools.
- [ ] Offline subprocess fixtures cover executable paths with spaces, shell metacharacters as data, large stdin, saturated stderr, timeout, cancellation, descendant cleanup, and exit-zero protocol failure.

Prerequisites: ATLAS-02.

Evidence:

- Existing direct clients: crates/mcptools/src/atlas/llm.rs, greprag/mod.rs, strand/mod.rs.
- tokio::process and async infrastructure are already installed.

## ATLAS-05: Replace Atlas Ollama configuration with template and runner settings

Parent: ATLAS-00. Priority: high.

Area: MCPTools / configuration

Define the smallest configuration for llm-stream execution, template selection, provider/model overrides, concurrency, timeouts, and retry limits. Keep credentials in llm-stream/environment.

Acceptance criteria:

- [ ] File and directory template names are independent from provider/model choices. Permit different model overrides where useful without making presets mandatory.
- [ ] Set and document configuration precedence and validate positive concurrency, finite timeouts, bounded attempts, and nonempty explicit names.
- [ ] Structural commands do not require a working llm-stream configuration, executable, authentication, or model.
- [ ] Before enrichment, show the resolved provider/model/templates and planned scope. Existing local-only users are never silently moved to a paid remote provider.
- [ ] Detect obsolete kind=ollama/base_url/model configuration and OLLAMA_URL assumptions; provide a concrete migration message instead of silently ignoring settings.
- [ ] Migration preserves unrelated .mcptools/config.toml fields and operator context. Standard Ollama remains usable only through llm-stream when explicitly selected; it is no longer a required dependency.

Prerequisites: ATLAS-02, ATLAS-03.

Evidence:

- crates/core/src/atlas/config.rs; crates/mcptools/src/atlas/config.rs.

## ATLAS-06: Handle 429 with bounded retries and shared cooldowns

Parent: ATLAS-00. Priority: high.

Area: MCPTools / Atlas scheduler

Implement Atlas retry policy using llm-stream's structured errors. Free-tier limits must delay or stop enrichment predictably while retaining completed work.

Acceptance criteria:

- [ ] Retry only classified temporary failures: 429 and the selected transient transport/server conditions. Do not retry authentication, invalid requests/models/templates, unsupported protocol, or known exhausted quota automatically.
- [ ] Honor valid Retry-After seconds and HTTP dates. For absent/invalid values use capped exponential backoff with jitter; never retry earlier than a valid server deadline.
- [ ] Bound attempts and total elapsed retry time. If the server deadline exceeds the run budget, persist the next eligible time and stop/defer rather than truncating the required wait.
- [ ] Share a cooldown across workers using the same configured provider/account rate scope so one 429 does not trigger a burst from the rest. Do not store raw credentials in that scope identifier.
- [ ] Retries requeue only the failed item, release active execution slots while waiting, remain cancellable, and do not repeat successful items.
- [ ] Report attempts, waiting reason, next attempt time, and exhausted/deferred count. No automatic provider/model fallback or hidden spend.
- [ ] Use fake clock/randomness and subprocess errors to verify 429 storms, both Retry-After forms, missing headers, permanent quota errors, resume during cooldown, cancellation, and retry ceilings.

Prerequisites: ATLAS-02, ATLAS-04, ATLAS-05.

Evidence:

- Current Atlas has no retry mechanism and converts errors to generic messages in atlas/llm.rs.

## ATLAS-07: Make Atlas initialization and structural refresh model-free

Parent: ATLAS-00. Priority: high.

Area: MCPTools / Atlas CLI

Separate structural indexing from enrichment in init, index, update, and sync. Preserve existing Tree-sitter extraction; optimize startup flow rather than rewriting the parser.

Acceptance criteria:

- [ ] init creates required local configuration/index and gives next-step guidance without editor launch, primer creation/refinement, model binary checks, or network requests.
- [ ] Repeated init preserves existing configuration, primer bytes, and usable cached enrichment.
- [ ] index/update/sync perform structural work only. Structural rebuilds retain reusable semantic cache records and invalidate affected records through the cache model rather than clearing all descriptions.
- [ ] Update CLI help and internal callers/hooks for removed LLM parallelism or primer flags; obsolete flags give actionable errors.
- [ ] True dry-run performs no directory/database/config writes. Missing index status is actionable, not a raw SQLite failure.
- [ ] Tests run all structural commands with model executables absent and network disabled; fresh and existing repositories remain queryable.

Prerequisites: none.

Evidence:

- crates/mcptools/src/atlas/cli/{init,index,update,sync}.rs.
- index currently opens/creates the DB before --dry-run; init currently edits/refines primer then runs full index.

## ATLAS-08: Make the primer optional and preserve operator text

Parent: ATLAS-00. Priority: high.

Area: MCPTools / Atlas context

Retain the primer only as optional operator-authored context. Remove its generation/refinement from the normal Atlas lifecycle and remove unused refinement code.

Acceptance criteria:

- [ ] No Atlas command or automatic harness hook rewrites, reformats, truncates on disk, or synthesizes the operator's primer.
- [ ] Enrichment works with no primer; permission/read/encoding failures are reported distinctly from an absent optional file.
- [ ] Document optional primer path and a short manually authored example; do not require an editor or offer automatic rewrite during init.
- [ ] Primer changes affect enrichment identity but do not invalidate/rebuild unchanged structural data or trigger model calls.
- [ ] Update atlas://primer listing/read behavior to describe optional context; missing context does not tell users to run init to generate it.
- [ ] Regression checks preserve exact bytes across init/update/sync/enrich, including Unicode, whitespace, and custom paths.

Prerequisites: ATLAS-07.

Evidence:

- crates/mcptools/src/atlas/cli/init.rs; crates/core/src/atlas/prompts.rs; crates/mcptools/src/mcp/resources.rs.

## ATLAS-09: Track enrichment provenance, stale state, and durable progress

Parent: ATLAS-00. Priority: high.

Area: MCPTools / Atlas database

Separate structural records from enrichment completion state. Fix the current failure-marker cache bug and make resume safe across changes and interruptions.

Acceptance criteria:

- [ ] Distinguish missing, current, stale, failed/deferred, and in-flight work without using human strings such as [description failed] as valid descriptions.
- [ ] Cache identity covers actual content, effective template/prompt/variables, optional context, provider/model and output-affecting settings. Directory identity includes the child inputs actually used.
- [ ] Save each validated success atomically with its input identity; interrupted runs retain successes and recover abandoned in-flight work.
- [ ] Before committing a response, check input identity still matches current source/context/template state. Late results cannot overwrite newer data.
- [ ] Edits, deletions, renames, ignore changes, and template/context/model changes invalidate only appropriate records/ancestors. Structural rebuilds do not discard reusable results.
- [ ] Migrate existing SQLite data transactionally without losing operator files. Legacy failure markers become retryable failures; unverifiable legacy descriptions are marked stale.
- [ ] Document what cache reuse can guarantee when a remote model alias changes without a supplied revision, and provide explicit refresh control.

Prerequisites: ATLAS-05, ATLAS-08.

Evidence:

- crates/mcptools/src/atlas/db.rs: files_needing_descriptions/directories_needing_descriptions select only NULL.
- atlas/cli/index.rs writes [description failed] after generation and parsing failures, making them appear completed to incremental selection.

## ATLAS-10: Plan scoped enrichment before making model calls

Parent: ATLAS-00. Priority: high.

Area: MCPTools / Atlas enrich

Add atlas enrich [path] with a read-only plan/dry-run. Resolve a file or subtree against the structural index and cache before scheduling any generation.

Acceptance criteria:

- [ ] With no path select the repository; a file selects that file; a directory selects its subtree. Validate normalized paths, symlinks, ignored files, unknown paths, and repository boundaries.
- [ ] Plan shows exact files/directories, reusable/stale/missing records, expected model-call count, templates/model selection, and context size estimates.
- [ ] Honor scope in every semantic pass. Ancestors outside the selected subtree may be marked stale but are not silently enriched; any wider scope is explicit in the plan.
- [ ] Support bounded work per invocation and explicit refresh. Use stable ordering and filter completed cache hits before rendering/calling models.
- [ ] Preflight runner/templates/provider configuration before starting requests. Do not publish artificial success when no valid configuration exists.
- [ ] Dry-run does not call a model, create/update index state, install templates, or edit the primer. Tests enforce exact scope and no hidden whole-repository work.

Prerequisites: ATLAS-03, ATLAS-05, ATLAS-07, ATLAS-09.

Evidence:

- crates/mcptools/src/atlas/cli/index.rs currently combines scanning and describing all indexed paths.
- Graft scope regression/fix motivates end-to-end selection tests: https://github.com/trailhq/Graft/issues/252

## ATLAS-11: Execute and resume explicit Atlas enrichment

Parent: ATLAS-00. Priority: high.

Area: MCPTools / Atlas enrich

Connect the plan to the shared runner, retry policy, and durable cache. This becomes Atlas's only normal entry point for model generation.

Acceptance criteria:

- [ ] Run only planned misses/stale work with bounded active requests; render the selected llm-stream templates using structural data and optional operator context.
- [ ] Validate the full description format and content before saving. Failed or malformed output never becomes a successful cache record.
- [ ] Resume reuses every compatible saved success and retries eligible failures. Retry-exhausted/deferred work remains visible and resumable.
- [ ] Directory work obeys child dependencies; failed child summaries are not passed as factual content. Represent missing child context explicitly and retain correct input identity.
- [ ] Cancellation stops new dispatch, ends active requests safely, persists completed work, and reports partial completion.
- [ ] Return a documented nonzero/partial exit result when requested enrichment is incomplete, with success/failure/deferred totals and a resume command.
- [ ] Exercise cold enrich, exact-file/subtree enrich, unchanged resume, interrupted resume, mixed successes/429s, invalid model responses, and source edits during a request.

Prerequisites: ATLAS-04, ATLAS-06, ATLAS-08, ATLAS-09, ATLAS-10.

Evidence:

- crates/mcptools/src/atlas/cli/index.rs and update.rs currently duplicate model orchestration.

## ATLAS-12: Bound prompt size and remove unnecessary enrichment serialization

Parent: ATLAS-00. Priority: normal.

Area: MCPTools / Atlas performance

Reduce expensive model work after the baseline exists. Use a bounded ready-work queue and deterministic prompt budgets; avoid building every prompt in memory before dispatch.

Acceptance criteria:

- [ ] Apply a documented total-input budget across source, symbols, path context, primer, and child summaries, not only file source text. State whether tokens are measured or estimated.
- [ ] Handle oversized files/directories and multi-byte text deterministically; report omitted/truncated context without editing the primer or source.
- [ ] Avoid duplicate file reads and repeated directory symbol queries where measurements show material cost; cache only within bounded run memory.
- [ ] Run independent files/directories concurrently while respecting child-before-parent dependencies, provider cooldown, and the global worker limit.
- [ ] Do not create an unrequested model call for every node if a valid compatible result exists. Report request reductions separately from throughput changes.
- [ ] Compare peak memory, prompt size distribution, request count, and file/directory phase times against ATLAS-01; retain only optimizations with clear benefit.

Prerequisites: ATLAS-01, ATLAS-11.

Evidence:

- index.rs prebuilds prompts, defaults file workers to 1, and processes directories sequentially.
- crates/core/src/atlas/prompts.rs limits file content but not the complete prompt.

## ATLAS-13: Expose structural readiness, enrichment progress, and retry state

Parent: ATLAS-00. Priority: high.

Area: MCPTools / CLI and MCP status

Make partial usefulness and delayed work clear to users and harnesses. Structural readiness is independent of optional descriptions.

Acceptance criteria:

- [ ] status distinguishes uninitialized, structurally ready/stale, and enrichment current/partial/stale/failed/deferred with counts.
- [ ] Show active provider/model/template identities, cache hits, completed/failed/deferred requests, retry attempts and next eligible times without secrets or raw source prompts.
- [ ] Record phase durations, total elapsed time, input/output sizes, and provider usage if available. Do not label estimates as measured tokens or fabricate cost.
- [ ] Human progress and JSON results agree; machine stdout stays parseable while logs/progress use stderr.
- [ ] Uninitialized repository returns actionable init guidance. Missing optional primer is informational. Failures do not appear as normal descriptions in tree/peek.
- [ ] Provide stable documented exit and JSON schema semantics for success, partial completion, configuration failure, and cancellation.

Prerequisites: ATLAS-09, ATLAS-11.

Evidence:

- crates/mcptools/src/atlas/cli/status.rs; atlas/data.rs; crates/core/src/atlas output formatters; MCP handlers.

## ATLAS-14: Keep structural data fresh across edits, deletions, and worktrees

Parent: ATLAS-00. Priority: high.

Area: MCPTools / Atlas freshness

Ensure harness navigation reflects the working tree without launching enrichment. Reuse content-hash indexing and make any automatic structural refresh bounded and visible.

Acceptance criteria:

- [ ] Check Git worktrees where .git is a file, normal clones, nested invocation paths, and configured DB paths; isolate the right repository/worktree index.
- [ ] Detect edits, additions, deletions, renames, and ignore changes; same-size edits and preserved timestamps cannot silently survive an authoritative refresh.
- [ ] Choose and document query freshness behavior: cheaply report stale data or perform bounded structural refresh. Neither path launches a model.
- [ ] Handle concurrent refresh/enrichment safely; avoid duplicate refresh storms, busy errors, and stale model results overwriting current source identity.
- [ ] Queries remain usable during enrichment and report any stale or incomplete semantic context.
- [ ] Test working-tree changes before commits as well as post-commit hooks; do not rely solely on committed Git state.

Prerequisites: ATLAS-07, ATLAS-09.

Evidence:

- atlas/cli/index.rs find_git_root currently tests .git as a directory; filesystem walk and file hashing already exist.
- https://github.com/trailhq/Graft/blob/main/src/graph/build.ts

## ATLAS-15: Provide a focused Atlas query and predictable CLI/MCP results

Parent: ATLAS-00. Priority: high.

Area: MCPTools / harness navigation

Make Atlas useful when the harness does not already know the file path. Reuse indexed symbols, paths, and available descriptions for one focused search/query surface, alongside tree and peek.

Acceptance criteria:

- [ ] Provide matching/ranked file and symbol locations with signatures and line spans; work on a structural-only index. Start with existing SQLite/index capabilities before adding dependencies.
- [ ] Bound tree/search/peek output using explicit depth/result/context limits with stable ordering and truncation metadata. Offer source excerpts only when requested.
- [ ] Keep CLI JSON and MCP field semantics aligned, including optional descriptions, freshness, and actionable setup/errors.
- [ ] Document that current MCP content already contains JSON-serialized text. Add structuredContent only if supported by the actual server/client contract, preserving compatible text content; do not fix a nonexistent absence-of-JSON bug.
- [ ] Tool descriptions explain when to use tree, query, peek, and status and how to fall back to source reads.
- [ ] Use small representative tasks to verify the harness can locate a provider implementation without prior path knowledge. No call graph, embeddings, or LSP is required for this issue.

Prerequisites: ATLAS-13, ATLAS-14.

Evidence:

- crates/mcptools/src/mcp/tools/atlas.rs; mcp/tools/mod.rs; atlas/cli/{tree,peek}.rs.
- Current Atlas exposes tree, peek and status but no task-oriented symbol/file query.

## ATLAS-16: Install Atlas guidance for Codex and Claude harnesses

Parent: ATLAS-00. Priority: high.

Area: MCPTools / setup

Update atlas setup and bundled navigation instructions for the new command lifecycle and supported harnesses. Keep setup local, idempotent, and compatible with existing hook managers.

Acceptance criteria:

- [ ] Generate/maintain the appropriate project guidance and skill locations for explicitly supported Codex and Claude harnesses; verify current harness conventions during implementation.
- [ ] Explain initial status/tree/query/peek use, source verification, optional context, stale results, and explicit user-controlled enrich. Do not instruct a harness to run whole-repository enrichment by default.
- [ ] Session/edit/commit integration, where supported, only exposes or refreshes structural context. No automatic LLM work.
- [ ] Preview lists exact files/hooks/settings changed. Preserve operator instructions, existing MCP servers, executable hooks, symlinks, and managed sections.
- [ ] Respect existing hook-manager detection and provide manual instructions when managed hooks cannot be edited safely.
- [ ] Repeat setup without duplicate instructions. Test empty and preconfigured repositories, custom hooksPath, and both supported harnesses.

Prerequisites: ATLAS-03, ATLAS-07, ATLAS-08, ATLAS-15.

Evidence:

- atlas/cli/setup.rs currently targets .claude/skills/atlas-navigation/SKILL.md and CLAUDE.md.
- crates/core/src/atlas/setup*.rs and bundled atlas/templates harness assets.

## ATLAS-17: Migrate GrepRAG to llm-stream and an explicit template

Parent: ATLAS-00. Priority: high.

Area: MCPTools / GrepRAG

Replace GrepRAG's direct Ollama/Rig calls with the shared runner. Preserve retrieval behavior by making its model instructions explicit and consistent with its output parser.

Acceptance criteria:

- [ ] Create a versioned llm-stream template for query generation; inspect the existing fine-tuned/Modelfile assumptions rather than copying a model name.
- [ ] Resolve the current instruction mismatch: models/greprag/Modelfile asks for regex patterns while code parses rg commands. Select one documented output contract and update prompt/parser fixtures together.
- [ ] Move provider/model selection to the runner configuration; remove required greprag Ollama model and Ollama URL coupling from CLI/MCP.
- [ ] Preserve repository scope, allowed rg execution, ranking, deduplication and token-budget behavior. Model text never becomes unrestricted shell execution.
- [ ] Surface runner/timeouts/provider failures honestly. Reuse shared classification without introducing a second hidden retry layer.
- [ ] Compare representative retrieval outputs and correct-file recall before/after with at least the chosen non-Ollama provider when accessible; retain offline contract fixtures.

Prerequisites: ATLAS-03, ATLAS-04, ATLAS-05.

Evidence:

- crates/mcptools/src/greprag/mod.rs; crates/core/src/greprag.rs or greprag modules; models/greprag/Modelfile; docs/GREPRAG_SETUP.md.

## ATLAS-18: Migrate Strand to llm-stream with a portable prompt contract

Parent: ATLAS-00. Priority: high.

Area: MCPTools / Strand

Replace Strand's direct Ollama/Rig calls with the shared runner. Extract the intended behavior from its current specialized model and caller/parser contract.

Acceptance criteria:

- [ ] Define a versioned llm-stream template reproducing the requested Strand behavior with a general provider; document any fine-tuned-model behavior that cannot be assumed portable.
- [ ] Preserve expected output and existing optional user instruction semantics without silently overriding the template system prompt.
- [ ] Migrate CLI and MCP provider/model configuration and return errors through the shared runner.
- [ ] Remove the mandatory maternion/strand-rust-coder/Ollama setup assumption; require explicit model selection rather than hidden remote fallback.
- [ ] Test prompt rendering, output contract, malformed/partial responses, timeout, and provider errors offline.
- [ ] Compare the relevant existing Strand workload using the selected non-Ollama provider when accessible, and report quality limits rather than claiming equivalence without evidence.

Prerequisites: ATLAS-03, ATLAS-04, ATLAS-05.

Evidence:

- crates/mcptools/src/strand/mod.rs; crates/mcptools/src/mcp/tools/strand.rs and registration callers.

## ATLAS-19: Remove obsolete Ollama clients, assets, and dependency wiring

Parent: ATLAS-00. Priority: normal.

Area: MCPTools / cleanup

After all callers migrate, delete the legacy direct-client code and obsolete setup assumptions. Avoid maintaining two provider stacks.

Acceptance criteria:

- [ ] Search all callers and remove Atlas RigProvider, duplicated Ollama client/error helpers, obsolete provider enums/env vars, and dead primer-refinement helpers.
- [ ] Remove rig-core from affected manifests and update Cargo.lock only after confirming no remaining consumer needs it.
- [ ] Retire or clearly relocate obsolete required Modelfiles/model-download setup and stale flags; preserve unrelated product functionality and user changes.
- [ ] Update CLI/MCP registration signatures and examples together; compilation catches missed consumers.
- [ ] Verify a clean install can initialize/query Atlas without Ollama and can enrich/run GrepRAG/Strand with llm-stream using an explicitly selected non-Ollama provider.
- [ ] Run relevant crate tests and build checks; no new code comments or unrelated sweeps.

Prerequisites: ATLAS-11, ATLAS-17, ATLAS-18.

Evidence:

- Cargo.toml; crates/mcptools/Cargo.toml; models/atlas; models/greprag; direct clients identified across three commands.

## ATLAS-20: Verify the complete workflow under failures and concurrency

Parent: ATLAS-00. Priority: high.

Area: MCPTools + llm_stream / integration

Add a focused cross-process regression suite that proves the new lifecycle and failure handling end to end. Use offline fixtures by default; real provider runs remain explicitly enabled.

Acceptance criteria:

- [ ] Exercise fresh init -> structural query -> install templates -> scoped enrich -> edit -> structural update -> stale query -> resume enrichment.
- [ ] Verify no model calls during init/index/update/sync/status/query/setup hooks and no primer-byte changes anywhere.
- [ ] Inject 429 with and without Retry-After, permanent quota failure, authentication failure, server errors, malformed descriptions, partial stdout then failure, timeout, interruption, and missing executable/template.
- [ ] Assert bounded attempts, no retry burst across workers, exact scope, durable saved successes, failed work still eligible, and no leaked child processes.
- [ ] Cover concurrent queries/update/enrich, stale input commits, DB migration, deletion/rename, worktree roots, Unicode paths, and oversized prompt input.
- [ ] Keep fixtures independent of model wording and avoid timing-flaky wall-clock sleeps. Live provider validation records provider/model and any usage/cost; unavailable access is reported as a limitation.

Prerequisites: ATLAS-06, ATLAS-11, ATLAS-12, ATLAS-13, ATLAS-14, ATLAS-16, ATLAS-17, ATLAS-18.

## ATLAS-21: Measure Atlas usefulness and enrichment cost after redesign

Parent: ATLAS-00. Priority: normal.

Area: MCPTools / acceptance evaluation

Repeat the pinned baseline and coding tasks from ATLAS-01. Judge success by usable context, task correctness, and avoided/reused model work, not index throughput alone.

Acceptance criteria:

- [ ] On llm_stream and one representative larger fixture/repository, report repeated cold structural, cold enriched, unchanged, single-file edit, context/template change, and interrupted-resume results.
- [ ] Record median and spread for time to first useful query, total enrichment, request count, cache hit rate, 429 wait/retries, memory, and token usage where available.
- [ ] Compare supported harness task completion/correct file selection with source tools alone and Atlas assistance. Record actual tool usage and whether instructions cause unnecessary enrichment.
- [ ] Compare Graft only on aligned capabilities, source revision, repository, provider/model, cache state, and correctness task; publish raw logs with source/secrets excluded as needed.
- [ ] Verify zero requests for structural operations and unchanged compatible enrichment, exact scoped request counts, and bounded retries as hard invariants.
- [ ] Set/report numeric improvement targets from the measured baseline before claiming speedups. Identify remaining limits and separately propose optional graph/LSP/semantic-search experiments only if results justify them.

Prerequisites: ATLAS-01, ATLAS-12, ATLAS-15, ATLAS-16, ATLAS-20.

## ATLAS-22: Publish migration instructions and the complete operator flow

Parent: ATLAS-00. Priority: normal.

Area: MCPTools + llm_stream / release

Update product and harness documentation around the final implementation. Provide a complete path from no Atlas setup to useful navigation and optional enrichment.

Acceptance criteria:

- [ ] Document init, optional manually authored primer, template installation, explicit provider/model selection, enrich preview, scoped enrich, status, retries, resume, update and query examples.
- [ ] Explain atlas-file/atlas-directory templates using the actual llm-stream schema and variable precedence. No preset setup or mandatory Ollama download remains.
- [ ] Provide migration examples for existing Atlas config/database/template overrides and GrepRAG/Strand callers, including removed flags and environment variables.
- [ ] Document free-tier 429 behavior, Retry-After, retry/run budgets, partial exits, process cancellation, model-output validation, and what is cached.
- [ ] State minimum llm-stream version and cross-repository release order; test examples against built binaries.
- [ ] Update README, docs/ATLAS_SETUP.md, docs/GREPRAG_SETUP.md, bundled harness instructions, root guidance, and stale .claude/context/atlas.md references.
- [ ] Attach measured results and remaining limitations; confirm the initiative acceptance criteria and all prerequisite issues are complete.

Prerequisites: ATLAS-19, ATLAS-20, ATLAS-21.
