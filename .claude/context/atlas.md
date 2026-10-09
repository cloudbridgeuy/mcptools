# Atlas — Codebase Navigator for AI Agents

Atlas scans a repository, extracts symbols using tree-sitter, and stores them in a local SQLite index. Agents use the index to understand codebase structure without reading every file. Structural commands never call a model or edit operator context.

## V1 Scope

- **Symbol index**: Extract functions, classes, structs, enums, traits, interfaces, types, constants, methods, and modules from source files
- **Tree view**: Annotated directory listing from the index
- **Peek**: File summary with symbol signatures and line ranges
- **Languages**: Rust, TypeScript/TSX, JavaScript/JSX, Python, Go

## Optional Context and Descriptions

- **Project primer**: Optional operator-authored text. No command creates, reformats, or rewrites it.
- **Cached descriptions**: Structural refresh preserves reusable results. Source changes invalidate the affected file and ancestors; stale descriptions remain stored but are not displayed or counted as current.
- **Model execution**: Explicit, scoped, resumable `atlas enrich [path]`; structural commands never call models. Add `--dry-run` for a read-only plan.
- **Config file**: `.mcptools/config.toml` for per-project paths, templates, provider/model overrides, concurrency, timeout, attempt limits, and token limits
- **Environment variables**: Override config via `ATLAS_*` env vars

## CLI Commands

```bash
mcptools atlas init                   # Prepare repository and run index
mcptools atlas index                  # Refresh structural index, retain cached descriptions
mcptools atlas index --incremental    # Skip unchanged files
mcptools atlas tree [path]            # Directory tree with descriptions (optional: filter by path)
mcptools atlas tree --depth 2         # Limit depth
mcptools atlas tree --json            # JSON output
mcptools atlas peek <path>            # File or directory summary with symbols/children
mcptools atlas peek <path> --json     # JSON output
mcptools atlas update                 # Incremental update (changed files only)
mcptools atlas sync                   # Force full re-index
mcptools atlas status                 # Index health summary
mcptools atlas status --json          # JSON output
mcptools atlas index --dry-run        # Show what would be indexed (by tier)
mcptools atlas update --dry-run       # Show what would change (adds/mods/deletes)
mcptools atlas enrich [path]
mcptools atlas enrich [path] --json    # Versioned machine result; progress on stderr
mcptools atlas enrich [path] --dry-run  # Plan exact scoped enrichment without side effects
mcptools atlas enrich src --dry-run --limit 20 --refresh  # Bound work and bypass cache hits
git status --porcelain | mcptools atlas index --stdin   # Index only the listed paths
mcptools atlas setup                  # Install hook + guidance + llm-stream templates (atlas-file/dir)
mcptools atlas setup --dry-run        # Preview setup actions without applying them
mcptools atlas setup --no-hooks       # Skip git hook installation
mcptools atlas setup --no-skills      # Skip installing .claude/skills/atlas-navigation/SKILL.md
mcptools atlas setup --no-claude-md   # Skip the CLAUDE.md atlas section
mcptools atlas setup --no-templates   # Skip installing llm-stream templates (atlas-file/dir)
```

The `index` and `update` commands report structural work and elapsed time. Removed flags `--with-primer` and `--parallel` are rejected by the CLI.

### `atlas enrich` — Scoped Execution and Read-Only Planning

`atlas enrich` selects every indexed file and directory. A file path selects only that file; a directory path selects only that indexed subtree. Paths are normalized against the current directory, must remain inside the repository, and must not traverse symlinks. Missing, ignored, reserved, stale-index, and non-normalized records fail before a plan is published.

Planning requires explicit provider/model configuration, an available llm-stream runner, and both selected templates. It reads the index through a read-only SQLite connection, renders inputs to calculate complete cache identities, and reports every target, cache state, estimated input tokens, model/template selection, and expected call count. Files sort lexically before directories; directories sort deepest first. Cache hits do not consume `--limit`. `--refresh` marks matching cache hits as work in the plan. Matching active leases remain visible and are not scheduled; a lease for changed input is stale work.

Without `--dry-run`, the command opens the writable index and executes pending work. `--concurrency N` bounds active requests; `--limit N` bounds selected work items per invocation, not retry attempts. Files finish before directories; directories run deepest first after selected children settle. Active child leases block parents. Only compatible successful child summaries become context; other children use `[missing current description]`. File context uses structural ancestor paths, not ancestor summaries.

Responses require `SHORT:` followed by `LONG:` with nonempty content; malformed output and the short-description failure marker are rejected. Before committing success, Atlas rereads source bytes, templates, configuration, and optional primer, and rechecks path boundaries, ignore rules, and reserved files. Changed identities or invalid inputs reject late responses.

Compatible successes are reused. Eligible unfinished work retries within durable attempt limits and shared cooldowns. `--refresh` regenerates the selected scope and resets its attempt state, without bypassing cooldowns. Exhausted or malformed responses require refresh. The run retry budget is `timeout_secs × max_attempts`; longer Retry-After waits persist for a later invocation.

Exit `0` means all selected entries have compatible saved successes. Errors and partial runs exit `1`; partial output includes success/failure/deferred totals and a shell-quoted resume command. Ctrl-C stops dispatch, cancels active subprocesses, and retains completed work. Cancelled attempts do not consume the attempt budget; expired abandoned leases recover as retryable work.

With `--dry-run`, planning remains read-only: no model calls, index writes, or template installation. Neither mode edits the optional primer.

### `atlas init` Workflow

1. Adds `index.db` to `.gitignore` if not already present, unless `.gitignore` is the configured primer.
2. Runs structural indexing. No editor, primer read, or model request occurs.

### Optional Operator Primer

Default path: `.mcptools/atlas/primer.md`. Set `[atlas].primer_path` in `.mcptools/config.toml` or `ATLAS_PRIMER_PATH` for a custom path. Relative paths resolve from the repository root; absolute paths are supported.

If useful, manually author a short file, for example:

```markdown
# Project context
This CLI exposes repository navigation to coding agents.
Pure logic lives in crates/core; I/O lives in crates/mcptools.
```

`init`, `index`, `update`, and `sync` leave exact primer bytes untouched, even if the file is absent or is not UTF-8. The configured primer is excluded from structural indexing. Primer-only changes do not rebuild structural records or update the stored enrichment `primer_hash`. Enrichment templates accept optional primer text, and the planner includes its exact bytes in input identity.

### `atlas index --stdin` — Index an Explicit Path List

`atlas index --stdin` reads file paths from standard input, one per line, instead of walking the repository. Each line may be a plain path (`src/foo.ts`) or `git status --porcelain` output (`M  src/foo.ts`, `?? new.ts`, `R  old.ts -> new.ts` — the new path wins). Blank lines and `#` comments are ignored, as are paths that don't resolve to a file.

Only the listed files are indexed; unchanged hashes are reused. Other records are retained. `skip_patterns` are not applied, but the configured primer is excluded.

### `atlas peek` for Files and Directories

`atlas peek <path>` accepts either a file or directory path. For files it shows the file summary and symbol signatures. For directories it shows the directory description, child entries with their descriptions, and aggregated symbols.

### `atlas update` — Incremental Update

`atlas update` compares source hashes against the index. It extracts symbols for added/modified files and removes deleted files. It retains cached results, invalidates changed inputs and ancestors, and never calls a model.

### `atlas sync` — Full Structural Scan

`atlas sync` runs the full structural scan with `atlas index`. Unchanged records and cached descriptions are retained, not cleared. Deleted files and their symbols are removed.

### Setup

`mcptools atlas setup` installs a `post-commit` git hook that runs structural `mcptools atlas update` in the background, plus agent-facing guidance and two llm-stream prompt templates (`atlas-file.toml`, `atlas-dir.toml`) under the llm-stream config directory (`LLM_STREAM_CONFIG_DIR`, else `~/.config/llm-stream`). It does not install a primer-generation template or remove old operator files.

- The hook is a marker-delimited block (`# >>> mcptools atlas >>>` ... `# <<< mcptools atlas <<<`) appended to `.git/hooks/post-commit` (or the path from `core.hooksPath`). If no hook file exists, one is created with a shebang; if one exists without the marker, the block is appended, preserving existing content.
- The skill and CLAUDE.md section come from templates embedded in the binary via `include_str!` — no network access needed. The CLAUDE.md section uses its own marker pair (`<!-- >>> mcptools atlas >>> -->` ... `<!-- <<< mcptools atlas <<< -->`); if the file doesn't exist, it's created; if it exists without the marker, the section is appended after a blank line.
- Re-running setup skips installed content. User-edited guidance and templates remain unchanged, with a warning. Each template reports its own status.
- `--dry-run` prints the plan without touching the filesystem.
- `--no-hooks` skips hook installation; `--no-skills` skips the skill file; `--no-claude-md` skips the CLAUDE.md section; `--no-templates` skips the llm-stream templates. Each step is independently skippable.
- Setup never touches a hook it doesn't own. It detects: a symlinked `post-commit` hook, a non-UTF8 hook file, or a repo managed by a third-party hook manager (`.husky/` dir, `lefthook.yml`/`lefthook.yaml`, `.pre-commit-config.yaml`). In these cases the hook is left untouched and manual instructions for wiring in `mcptools atlas update` are printed instead (both in `--dry-run` and normal runs) — this never blocks the skill/CLAUDE.md steps.

## Configuration

### Config File (`.mcptools/config.toml`)

```toml
[atlas]
db_path = ".mcptools/atlas/index.db"
primer_path = ".mcptools/atlas/primer.md"
max_file_tokens = 10000
api = "ollama"
model = "atlas"
concurrency = 2
timeout_secs = 120
max_attempts = 2
skip_patterns = ["*.test.ts", "*.md", "*.json", "*.yml"]

[atlas.file_llm]
template = "atlas-file"

[atlas.directory_llm]
template = "atlas-dir"
```

Structural fields are optional. Enrichment stays disabled until both `api` and a model are explicit. Set one shared `model`, or set both role models. Template names are independent from provider/model selection.

Precedence is environment variables, nested `[atlas]` values, flat top-level values, and defaults. Fields may be written flat, including top-level `[file_llm]` and `[directory_llm]`. Nested `[atlas]` values win field by field.

`concurrency`, `timeout_secs`, and `max_attempts` must be positive integers. `max_attempts` cannot exceed `10`. Explicit provider, model, and template names cannot be blank. Retries apply only to temporary rate-limit, connection, timeout, and HTTP 408, 429, 500, 502, 503, or 504 failures. Explicit non-retryable metadata overrides this list. Authentication, protocol/request errors, and known exhausted quota are permanent.

All structural and query commands work without llm-stream, authentication, or a selected model. Status inspects strict enrichment settings without calling a runner; invalid enrichment settings do not prevent structural status. Other structural/query commands do not load strict enrichment settings.

The [versioned status and enrichment contract](../../docs/atlas-redesign/status-contract.md) defines freshness, counts, safe identities, telemetry, JSON documents, and process exits. Status does not create an absent index; it returns `atlas init` guidance. Status can migrate an existing legacy index, as before.

Legacy `kind`, `base_url`, `OLLAMA_URL`, and `ATLAS_BASE_URL` settings are rejected before enrichment with a migration message. Keep unrelated `.mcptools/config.toml` fields, remove those settings, set `api = "ollama"` with a shared or per-role model, and configure the Ollama endpoint in llm-stream. Atlas never rewrites the config file.

### Ignore Patterns

The `skip_patterns` field accepts glob patterns (powered by the `globset` crate), matched against repository-relative paths. Files matching any pattern are skipped by the `index` and `update` repo walks, so they never enter the index and never appear in tree, peek, or status output. `atlas index --stdin` bypasses these patterns. An invalid pattern fails the command.

Common patterns:
- `"*.test.ts"` — skip test files by extension
- `"*.md"` — skip markdown files
- `"packages/mom/**"` — skip an entire directory tree
- `"*.{json,yml,yaml}"` — skip multiple extensions

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `ATLAS_DB_PATH` | `.mcptools/atlas/index.db` | Database location |
| `ATLAS_PRIMER_PATH` | `.mcptools/atlas/primer.md` | Primer file location |
| `ATLAS_MAX_FILE_TOKENS` | `10000` | Max tokens per file for LLM |
| `ATLAS_API` | unset | Explicit llm-stream provider |
| `ATLAS_MODEL` | unset | Shared model |
| `ATLAS_FILE_MODEL` | shared model | File model override |
| `ATLAS_DIR_MODEL` | shared model | Directory model override |
| `ATLAS_FILE_TEMPLATE` | `atlas-file` | File template name |
| `ATLAS_DIR_TEMPLATE` | `atlas-dir` | Directory template name |
| `ATLAS_CONCURRENCY` | `1` | Maximum active enrichment requests |
| `ATLAS_TIMEOUT_SECS` | `120` | Timeout for each attempt |
| `ATLAS_MAX_ATTEMPTS` | `1` | Attempts per item, maximum `10` |

### LLM runner settings

Enrichment uses llm-stream with explicit provider/model selection. `[atlas.file_llm]` and `[atlas.directory_llm]` can override the shared model and select templates. Atlas passes prompts, provider, and model to llm-stream; it does not own credentials or endpoint configuration.

## Supported Symbol Types by Language

| Language | Symbols Extracted |
|----------|-------------------|
| Rust | `function_item`, `struct_item`, `enum_item`, `trait_item`, `type_item`, `const_item`, `static_item`, `mod_item`, `impl` methods |
| TypeScript/JavaScript | `function_declaration`, named arrow functions, `class_declaration`, `method_definition`, `interface_declaration` (TS), `type_alias_declaration` (TS), top-level `const`, exports |
| Python | `function_definition`, `class_definition`, class methods, `UPPER_CASE` assignments (const), decorated definitions |
| Go | `function_declaration`, methods (with receiver), `type_declaration` (struct/interface/other), `const_declaration`, package-level `var` |

**Visibility detection**: Rust (`pub`/`pub(crate)`/private), TS/JS (export/private), Python (underscore prefix), Go (capitalization).

## Index Tiers

| Tier | Extensions | Description |
|------|-----------|-------------|
| **Full** | `rs`, `ts`, `tsx`, `js`, `jsx`, `py`, `go` | Tree-sitter symbol extraction |
| **Light** | `md`, `txt`, `json`, `yaml`, `yml`, `toml`, `cfg`, `ini`, `xml`, `html`, `css`, `scss`, `sql`, `sh`, `bash`, `zsh` | Indexed without symbols; explicit enrichment can add descriptions |
| **Skip** | Everything else | Not indexed |

## Module Layout

### Core (`crates/core/src/atlas/`) — Pure functions, no I/O

| File | Purpose |
|------|---------|
| `types.rs` | `SymbolKind`, `Visibility`, `ContentHash`, `Language`, `IndexTier`, `Symbol`, `FileEntry`, `TreeEntry`, `PeekView` |
| `config.rs` | Structural `AtlasConfig`, validated `RunnerConfig`, provider/model/template names, precedence, limits, and legacy migration errors |
| `hash.rs` | `content_hash(bytes) -> ContentHash` (SHA-256) |
| `symbols.rs` | `extract_symbols(tree, source, language, path) -> Vec<Symbol>` |
| `tree_view.rs` | `format_tree(entries, json)`, `format_peek(peek, json)`, `format_status(status, json)`, `IndexStatus` |
| `parse.rs` | `parse_description(response) -> FileDescription` — parse SHORT/LONG format from LLM output |
| `prompts.rs` | Symbol formatting and bounded source-text helpers used to populate installed templates |

### Shell (`crates/mcptools/src/atlas/`) — I/O operations

| File | Purpose |
|------|---------|
| `config.rs` | Loads structural and strict enrichment config separately from `.mcptools/config.toml` plus environment overrides |
| `db.rs` | SQLite storage: read-write migration/open, read-only planning open, and structural queries |
| `db/enrichment.rs` | Transactional migration, input reconciliation, claims, conditional completion, recovery, invalidation, and refresh |
| `enrichment.rs` | Pure deterministic input hashing, typed cache states, stable work planning, claims, and validated descriptions |
| `fs.rs` | Gitignore-aware file walker excluding the configured primer; optional primer reads with distinct read/encoding errors |
| `llm.rs` | `LlmStreamProvider::attempt`, cancellable slot-free retry waiting, typed outcomes and retry reports |
| `retry.rs` | Pure retry classification, jittered backoff, server deadlines, run-budget decisions and hashed rate scopes |
| `db/retry.rs` | Durable attempt reservations, per-item eligibility, monotonic shared cooldowns and retry counts |
| `parser.rs` | `parse_and_extract(path, source)` — tree-sitter grammar registry and extraction bridge |
| `cli/init.rs` | `atlas init` handler: gitignore setup and initial structural index |
| `cli/enrich.rs` | Read-only preflight, bounded dependency-ordered execution, input rechecks, validated commits, cancellation, totals, and resume output |
| `cli/index.rs` | `atlas index` handler: structural scan, hash reuse, progress reporting |
| `cli/tree.rs` | `atlas tree` handler: path filtering, depth, JSON output |
| `cli/peek.rs` | `atlas peek` handler: file or directory lookup, symbol display |
| `cli/update.rs` | `atlas update` handler: incremental update via hash-based change detection |
| `cli/sync.rs` | `atlas sync` handler: full structural scan without clearing semantic data |
| `cli/status.rs` | `atlas status` handler: index health display |
| `data.rs` | Shared data functions: `atlas_tree_data`, `atlas_peek_data`, `atlas_status_data` (used by CLI + MCP) |

## SQLite Schema

Seven tables: `symbols`, `files`, `directories`, `metadata`, `enrichment`, `enrichment_retry`, `enrichment_cooldown`. Indexes on `symbols(file_path)` and `files(path)`. Database at `.mcptools/atlas/index.db` relative to git root.

`enrichment` is keyed by `(kind, path)`, where kind is `files` or `directories`. It stores the desired input digest, last successful input digest, state, random claim token, lease deadline, and failure/deferral reason. Description bytes stay in the structural tables. `described_files` and `described_directories` expose description bytes only for current results; tree, peek, directory-child input queries, and status use these views.

`Database::open` transactionally installs schema version `1` in metadata, reconstructs missing parent directories, and migrates legacy records. The known `[description failed]` marker becomes a retryable failed record and is removed from description columns. Other unverifiable legacy descriptions remain stored as stale, not current. Missing descriptions remain missing. Migration never reads or rewrites primer/config/template files. Unknown enrichment schema versions fail without committing changes.

### Enrichment Persistence Contract (GUZ-29)

The read-only planner, per-item runner, and durable retry policy are integrated into executable `atlas enrich`. Command orchestration supplies dependency ordering, input rechecks, response validation/commit, cancellation, partial totals, and exit status.

`EnrichmentInput::identity` hashes versioned canonical JSON with SHA-256. The caller supplies full source bytes or a sorted map of the actual directory-child inputs used, effective template bytes, rendered system and user prompts, effective variables, optional context bytes, provider, model, optional immutable model revision, and all output-affecting settings. Map insertion order does not matter. Missing context differs from empty context. Child map values must encode every child attribute actually consumed, including summaries, source/provenance, or symbols when used; map keys must distinguish paths/kinds. Do not substitute timestamps, template names, or parent paths for actual inputs. Include endpoint/backend identity in settings when it can affect output; never include credentials.

States are `Missing`, `Current(identity)`, `Stale`, `Failed(reason)`, `Deferred(reason)`, and `InFlight { identity, lease_until }`. Legacy failure strings cannot become validated descriptions. Callers enumerate indexed files/directories and inspect `enrichment_state`; the old NULL-only selectors and unconditional description writers are removed.

Caller sequence:

1. Collect current inputs, derive their identity, and call `set_enrichment_input` for an indexed target. An unchanged identity preserves its state. A different identity cancels its claim and invalidates ancestors. A retained successful result can become current again only when its complete identity matches.
2. Low-level claims use `claim_enrichment(target, identity, now, lease_until)`. Only non-current work with matching desired identity can be claimed. Unexpired claims are exclusive across connections. Use nonnegative Unix seconds and a deadline after `now`; long-running callers must supply a sufficient lease. For model execution, use the GUZ-28 runner below instead of calling this claim API directly.
3. Generate output outside the database. Recollect all inputs before completion. Pass the rechecked identity and `ValidatedDescription::parse` output to `commit_enrichment`. Success atomically saves descriptions and provenance only if the identity, claim token, and unexpired lease still match. A rejected completion returns `false`; it must not be reported as saved. Call `set_enrichment_input` with newly collected inputs to reconcile changed configuration/context.
4. Use `finish_enrichment` for explicit failures or deferrals. It applies the same identity/token/lease guards. Use `recover_enrichment(now)` for expired abandoned claims; it preserves completed results and makes abandoned work retryable. Claiming an expired entry also replaces its token. Opening a database does not cancel another process's active claims.

Structural insert/content-change/delete triggers invalidate only affected ancestors (literal path ancestry, not SQL wildcard matching); deletions remove that target's provenance. Unchanged structural upserts retain current results and ignore description fields supplied through structural `FileEntry`. Changed directory summaries invalidate ancestors. Ignore changes and renames are represented by the structural walk's additions/deletions. `index --stdin` still retains unlisted paths; callers must explicitly delete removed paths when using a targeted path list.

`invalidate_enrichment(targets)` cancels targeted claims and marks their results/ancestors for identity reconciliation, retaining reusable successful provenance. `refresh_enrichment(targets)` additionally discards targeted successful identities and retry state, so even identical inputs require regeneration. Both are transactional, targeted APIs. Execution with `--refresh` calls `refresh_enrichment` for the selected scope; `--dry-run --refresh` only previews the effect. Provider cooldowns remain intact. For template/context/model/settings changes, callers recompute affected identities; structural/query commands deliberately do not load enrichment configuration or read templates/context to evaluate freshness.

Remote model alias limit: without a supplied immutable model revision, cache reuse proves only that the configured alias and all other supplied inputs match. It cannot detect a provider changing what that alias points to. Supply a revision when available, or explicitly refresh the affected targets to force regeneration.

### Retry Integration Contract (GUZ-28)

Use one writable Atlas database and execution semaphore across workers. Create scopes with `LlmStreamProvider::rate_scope` from a stable account/profile identifier, not a model name. File/directory models sharing an account must share the scope. Only the SHA-256 scope digest is stored. Coordination covers workers sharing this database, not unrelated repository databases.

Keep one absolute epoch-millisecond retry deadline for the whole run; do not renew it per attempt. `AttemptContext` accepts an injected current-time function. Use `jitter: None` for runtime randomness and `Some(value)` for deterministic tests. Database claim/commit leases still use Unix seconds.

Set the desired input identity, then call `attempt` instead of directly claiming/generating. Each call reserves at most one durable attempt and checks cooldown before and after acquiring an execution slot. Valid Retry-After seconds and HTTP dates are never truncated; missing/invalid headers use capped exponential backoff with jitter. Attempts survive resume. Input changes or explicit refresh reset item retry state, not account cooldowns.

For `Response`, validate the answer, recheck inputs, then commit its returned claim. `Reusable` needs no call. Requeue only the failed target for `Retry(Waiting)`; `wait_for_retry` is cancellable and owns no execution permit. Recheck eligibility after waiting because another worker can extend the cooldown. `Retry(Deferred)` keeps its next-attempt time for a later run with a new run deadline. `Failed` and `Retry(Exhausted)` must not retry automatically. `Busy`/`Superseded` must not overwrite another claim/input. Cancellation during a subprocess persists deferred work and releases its attempt reservation; abandoned claims recover after lease expiry.

Report `AttemptReport::format` or its fields: attempts, reason, next-attempt time, exhausted count and deferred count. No provider/model fallback occurs. Recheck the remaining budget after durable attempt reservation before starting the subprocess. Process cleanup can extend the wall-clock run budget by the existing two-second termination grace; synchronous database lock waits can add the existing five-second busy timeout per operation.

## MCP Tools (V5)

Atlas exposes three MCP tools and one MCP resource for AI agent access.

### Tools

| Tool | Description | Input |
|------|-------------|-------|
| `atlas_tree_view` | Browse annotated directory tree | `path` (optional), `depth` (optional, default 1) |
| `atlas_peek` | Get file/directory summary + symbols | `path` (required) |
| `atlas_status` | Check index health | (none) |

All tools return JSON. They call the same shared data functions as the CLI commands.

### Primer Resource

Optional operator-authored context is exposed at `atlas://primer`. Missing text returns a clear optional-absence error, not an init instruction. Other read failures and invalid UTF-8 return distinct errors. Valid text retains all Unicode and whitespace.

### Agent Navigation Workflow

1. Read optional primer (MCP resource) if present — understand purpose and architecture
2. `atlas_tree_view` at root — pick a direction
3. `atlas_tree_view` with path — drill into directories of interest
4. `atlas_peek` with file path — get summary + symbols before reading
5. Use grep/read to pull the actual code

## Planned Features

- **V2**: Durable enrichment persistence, read-only planning, and resumable scoped execution implemented
- **V3**: ~~MCP tool integration~~ Implemented (V5)
- **V4**: ~~Incremental updates~~ Implemented
- **V5**: ~~MCP tools + status + primer resource~~ Implemented
- **V6**: Cross-file relationship tracking
- **V7**: Semantic search over symbols and descriptions
