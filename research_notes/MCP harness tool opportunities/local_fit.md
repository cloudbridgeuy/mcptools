# Local mcptools fit and adjacent MCP aggregators

## What mcptools already provides

### Takeaway

The working checkout is a broad, static, in-process tool server: 77 registered tools, 74 non-meta catalog tools, 11 domains, and three progressive-discovery/composition tools. It already solves tool-schema context pressure, guarded composition, partial result projection, codebase orientation, issue/PR work, and guarded Git-worktree creation; recommendations for another finder, named dispatcher, JS orchestration tool, generic issue graph, PR commenting, or worktree allocator would duplicate current work.

### Cited Findings

- **Exact current inventory:** tests assert 77 registered tools: 49 Read, 25 Write (including `execute` and `call_tool`), and 3 Spend. The 74 non-meta tools span `atlas`, `bitbucket`, `confluence`, `hn`, `images`, `jira`, `lane`, `linear`, `md`, `pdf`, and `ui`; the full oracle is the most concise authoritative name list. [`crates/mcptools/src/mcp/tools/mod.rs:1045-1129,1262-1423`](../../crates/mcptools/src/mcp/tools/mod.rs#L1045-L1129)
- **External-service surface:** Jira has search/get/create/update, comments, sprints, attachments, and saved queries; Bitbucket has PR list/read/create/update/comment plus workspace/repository/branch discovery; Confluence has search. [`crates/mcptools/src/mcp/tools/mod.rs:190-423`](../../crates/mcptools/src/mcp/tools/mod.rs#L190-L423)
- **Linear is already deep:** 26 `linear_*` tools cover identity, issues, bounded dependency graphs/frontier, comments, relations, teams, projects, project statuses/milestones/progress reports, users, states, labels, and cycles. Current working-tree tests include the new project-progress listing tool. [`crates/mcptools/tests/contract_pilot.rs:106-131`](../../crates/mcptools/tests/contract_pilot.rs#L106-L131); [`.claude/context/linear.md:106-174`](../../.claude/context/linear.md#L106-L174)
- **Local and content tools:** three lane tools list, assess cleanup, and create guarded Worktrunk lanes; three Atlas tools browse an annotated code tree, summarize paths/symbols, and report index health; other domains fetch/outline web pages, navigate PDFs, generate/edit/vary images, read Hacker News, and manage UI annotations. [`crates/mcptools/src/mcp/tools/mod.rs:164-189,424-550`](../../crates/mcptools/src/mcp/tools/mod.rs#L164-L189)
- **Progressive discovery already exists:** full mode lists all 77 schemas, discovery mode lists exactly `find_tools` and `call_tool`, and code mode lists `find_tools`, `execute`, and `call_tool`; unlisted real tools remain dispatchable. `find_tools` can rank a task, list a domain, or list domain prefixes and returns TypeScript declarations. [`CONTEXT.md:47-107`](../../CONTEXT.md#L47-L107); [`crates/mcptools/src/mcp/tools/mod.rs:797-870`](../../crates/mcptools/src/mcp/tools/mod.rs#L797-L870)
- **Ranking is resilient but intentionally small:** local weighted token/IDF ranking is always available; configured Jev classification is advisory and falls back locally. Catalog domains derive from the prefix before `_`, so naming controls discovery grouping. [`crates/core/src/catalog.rs:1-30`](../../crates/core/src/catalog.rs#L1-L30); [`crates/core/src/find_tools.rs:68-165`](../../crates/core/src/find_tools.rs#L68-L165); [`.claude/context/mcp-server.md:158-185`](../../.claude/context/mcp-server.md#L158-L185)
- **Composition already exists:** `call_tool` dispatches one named real tool unchanged; `execute` runs fresh QuickJS JavaScript, supports parallel/conditional chains, captures logs/results/errors, and enforces configurable time, memory, and output limits. Read tools bind by default; Write and Spend require separate opt-ins. [`crates/mcptools/src/mcp/tools/call_tool.rs:5-75`](../../crates/mcptools/src/mcp/tools/call_tool.rs#L5-L75); [`crates/mcptools/src/mcp/tools/execute.rs:5-84`](../../crates/mcptools/src/mcp/tools/execute.rs#L5-L84); [`CONTEXT.md:149-187`](../../CONTEXT.md#L149-L187)
- **Safety is layered, not universal authorization:** MCP annotations mark Read versus destructive tools; `call_tool`/`execute` enforce `allowWrites` and `allowSpend`, while a direct `tools/call` reaches the static dispatcher. Lane operations add a separate fail-closed repository allowlist, canonical-path/identity checks, bounded subprocesses, and no delete/merge/push tool. [`crates/mcptools/src/mcp/tools/mod.rs:57-75,873-1018`](../../crates/mcptools/src/mcp/tools/mod.rs#L57-L75); [`.claude/context/lanes.md:3-43`](../../.claude/context/lanes.md#L3-L43)
- **Output control already exists, but only on five large tools:** `pdf_read`, `md_fetch`, `bitbucket_pr_read`, `jira_search`, and `linear_issue_list` accept dotted `fields`; projection occurs after normal shaping and errors on unresolved paths. [`CONTEXT.md:237-261`](../../CONTEXT.md#L237-L261); [`crates/core/src/projection.rs:6-91`](../../crates/core/src/projection.rs#L6-L91)
- **Protocol surface:** the custom JSON-RPC server supports `initialize`, tools list/call, and resources list/read over stdio or SSE; initialization advertises MCP `2025-06-18`. No prompts, sampling, roots, elicitation, or Streamable HTTP handler appears in the request dispatcher. [`crates/mcptools/src/mcp/mod.rs:61-128`](../../crates/mcptools/src/mcp/mod.rs#L61-L128); [`crates/mcptools/src/mcp/tools/mod.rs:144-162`](../../crates/mcptools/src/mcp/tools/mod.rs#L144-L162)

### Inferences

- “Add progressive tool discovery,” “add a generic named call,” “add code-mode batching,” “add output projection,” “add Linear dependency/frontier inspection,” and “add guarded lane creation” are duplicate recommendations, not gaps. [Sources above](#cited-findings)
- README summaries lag the executable registry: for example, the overview names only three Bitbucket tools although eight are registered. Future audits should use `registered_tools` plus contract oracles, then fix documentation separately. [`README.md:7-18`](../../README.md#L7-L18); [`crates/mcptools/tests/contract_pilot.rs:70-77`](../../crates/mcptools/tests/contract_pilot.rs#L70-L77)

### Gaps

- Not found in the registered surface: aggregate readiness/credential diagnostics, static argument validation without dispatch, code/text search in Atlas, bounded Git diff/status detail, invocation traces, structured non-JavaScript multi-call, catalog profiles, or universal output budgeting. [`crates/mcptools/src/mcp/tools/mod.rs:164-794`](../../crates/mcptools/src/mcp/tools/mod.rs#L164-L794)

## Architectural patterns, constraints, and extension points

### Takeaway

Good additions are typed, bounded, domain-specific operations with pure shaping in `mcptools_core`, I/O in `mcptools`, generated schemas, matching structured/text output, explicit ToolKind, and contract tests. Generic proxy, shell, or fleet-management features would fight the current architecture and safety posture.

### Cited Findings

- The intended split is Functional Core/Imperative Shell: `mcptools_core` owns deterministic transformations and reusable domain models; `mcptools` owns I/O/orchestration. [`crates/core/src/lib.rs:1-45`](../../crates/core/src/lib.rs#L1-L45)
- A tool addition currently requires coordinated edits to a large static `registered_tools()` vector and a second large name-to-handler `match`; this is a duplication risk despite tests checking registry/catalog counts and annotation sets. [`crates/mcptools/src/mcp/tools/mod.rs:164-794,873-1018`](../../crates/mcptools/src/mcp/tools/mod.rs#L164-L794); [`crates/mcptools/src/mcp/tools/mod.rs:1057-1103,1316-1423`](../../crates/mcptools/src/mcp/tools/mod.rs#L1057-L1103)
- Schemas derive from Rust types through `schemars`; handlers normally return identical pretty JSON text and `structuredContent`. This makes a small typed tool cheaper and safer than a generic command tunnel. [`crates/mcptools/src/mcp/tools/mod.rs:100-142,849-855`](../../crates/mcptools/src/mcp/tools/mod.rs#L100-L142)
- Context cost is an explicit design constraint: tests cap the three code-mode definitions at 1,200 estimated tokens and the finder definition at 2,400 characters. [`crates/mcptools/src/mcp/tools/mod.rs:1131-1171`](../../crates/mcptools/src/mcp/tools/mod.rs#L1131-L1171)
- The dependency set already includes JSON Schema validation, QuickJS, SQLite, tree-sitter for five languages, glob/ignore support, and Git-oriented lane machinery; additions that reuse these are lower-risk than new runtimes. [`Cargo.toml:31-68`](../../Cargo.toml#L31-L68)
- Lane design explicitly rejects generic command/flag forwarding and says application guards are not an OS sandbox. A shell/test runner would therefore require a new security boundary or an operator-owned fixed command manifest. [`.claude/context/lanes.md:27-43`](../../.claude/context/lanes.md#L27-L43)

### Inferences

- **Best extension seam:** add a domain module, typed args/output, ToolKind, generated schemas, one registry entry, one dispatch arm, and core tests; also consider generating registry and dispatch from one declaration to remove the current double-entry risk. [Sources above](#cited-findings-1)
- **Scope boundary:** mcptools is a curated local product server, not a downstream-MCP client, registry, multi-tenant gateway, or server lifecycle manager. Generic federation would require sessions, collision handling, upstream transports/auth, dynamic capability changes, and policy persistence absent from the current static registry. [`crates/mcptools/src/mcp/mod.rs:40-65`](../../crates/mcptools/src/mcp/mod.rs#L40-L65); [`crates/mcptools/src/mcp/tools/mod.rs:803-847`](../../crates/mcptools/src/mcp/tools/mod.rs#L803-L847)

### Gaps

- No evidence was found for inbound SSE authentication, per-user policy, persistent audit storage, dynamic upstream MCP registration, tool-name collision resolution, or lifecycle supervision. These should not be implied from env-based service credentials or QuickJS limits. [`crates/mcptools/src/mcp/mod.rs:61-128`](../../crates/mcptools/src/mcp/mod.rs#L61-L128)

## Adjacent aggregators and highest-fit opportunities

### Takeaway

Adjacent products primarily aggregate and govern arbitrary downstream servers; mcptools primarily supplies curated agent operations. The useful imports are narrow harness ergonomics—diagnostics, validation, search, bounded repository evidence, profiles, and observability—not wholesale gateway federation.

### Cited Findings

- **1MCP** aggregates static and context-resolved template servers, offers project/session context, presets/filters, health/recovery, and a progressive `instructions → inspect → run` CLI plus a stable lazy MCP surface. This most closely parallels mcptools discovery, but across arbitrary upstream servers. [1MCP README](https://github.com/1mcp-app/agent/blob/main/README.md)
- **MetaMCP** dynamically groups servers into namespaces, enables/disables or overrides tools, applies namespace middleware, exposes authenticated SSE/Streamable HTTP/OpenAPI endpoints, and adds multi-tenancy and rate limits. [MetaMCP README](https://github.com/metatool-ai/metamcp/blob/ai-dev/README.md)
- **Docker MCP Gateway** runs servers in isolated containers and adds profiles, catalogs/registry imports, tool allowlists, secrets/OAuth, lifecycle management, dynamic discovery, logs, and call tracing. [Docker MCP Gateway README](https://github.com/docker/mcp-gateway/blob/main/README.md); [gateway runtime options](https://github.com/docker/mcp-gateway/blob/main/docs/mcp-gateway.md)
- **ToolHive** separates Gateway, Registry, and Runtime; it adds container/Kubernetes isolation, per-request identity policy, OIDC/OAuth, semantic tool search, deterministic workflows, audit/OTel/Prometheus, and provenance-aware catalogs. [ToolHive README](https://github.com/stacklok/toolhive/blob/main/README.md)
- **IBM ContextForge** federates MCP/A2A/REST/gRPC, translates protocols, and adds plugins, retries, rate limits, caching, admin UI, auth, and OpenTelemetry. It is an enterprise gateway rather than a curated local tool server. [ContextForge README](https://github.com/IBM/mcp-context-forge/blob/main/README.md)

### Inferences

Ranked feasible additions (eight):

1. **`atlas_search` (high fit):** symbol/path/text search over the existing Atlas index closes the largest code-navigation hole; it reuses SQLite/tree-sitter and stays read-only/bounded. Atlas currently only tree/peek/status. [`crates/mcptools/src/mcp/tools/mod.rs:528-550`](../../crates/mcptools/src/mcp/tools/mod.rs#L528-L550)
2. **`mcptools_doctor` (high):** return mode, version, enabled domains, non-secret credential presence, external-binary versions, Atlas health, and lane-allowlist state. It consolidates fragmented `linear_auth_status`/`atlas_status` and improves harness setup without exposing secret values. [`crates/mcptools/src/mcp/tools/mod.rs:544-558`](../../crates/mcptools/src/mcp/tools/mod.rs#L544-L558)
3. **`validate_tool_call` (high):** validate `{name,input,allow*}` against the registered schema and report kind/required permission without dispatch. `jsonschema` is installed, schemas are centralized, and current `call_tool` validates only while dispatching. [`Cargo.toml:66-68`](../../Cargo.toml#L66-L68); [`crates/mcptools/src/mcp/tools/call_tool.rs:37-75`](../../crates/mcptools/src/mcp/tools/call_tool.rs#L37-L75)
4. **`repo_status` / `repo_diff_summary` (high with lane guards):** bounded read-only changed-file, staged/unstaged, ahead/behind, and diffstat evidence for an operator-allowed repository. Reuse lane canonicalization/subprocess caps; do not add arbitrary Git flags or file contents by default. [`.claude/context/lanes.md:5-33`](../../.claude/context/lanes.md#L5-L33)
5. **`tool_call_trace` (medium-high):** opt-in, bounded, redacted recent-call metadata—tool, kind, duration, success/error class, output bytes—around central dispatch. Copy adjacent gateways’ observability goal, not argument/result capture, which risks secrets. [`crates/mcptools/src/mcp/tools/mod.rs:873-1018`](../../crates/mcptools/src/mcp/tools/mod.rs#L873-L1018); [Docker monitoring](https://github.com/docker/mcp-gateway/blob/main/README.md)
6. **Structured `call_tools` (medium):** a typed bounded parallel/sequential batch for clients that cannot safely construct nested JavaScript. It must reuse ToolKind gates and output caps. Do not pitch it as new composition: it is only a safer compatibility alternative to existing `execute`. [`crates/mcptools/src/mcp/tools/execute.rs:19-64`](../../crates/mcptools/src/mcp/tools/execute.rs#L19-L64)
7. **Catalog profiles (medium):** operator-defined domain/tool allowlists with a read-only `catalog_profile_status`; profiles would reduce context and accidental exposure for full-mode clients, borrowing the profile concept without becoming an upstream aggregator. Existing modes filter only to fixed meta-tool sets. [`crates/mcptools/src/mcp/tools/mod.rs:803-817`](../../crates/mcptools/src/mcp/tools/mod.rs#L803-L817); [Docker profiles](https://github.com/docker/mcp-gateway/blob/main/README.md)
8. **Universal projection/output budgets (high):** move optional `fields` and/or explicit byte/item budgets into shared handler plumbing for every potentially large structured read. This extends—not duplicates—the proven five-tool projection and aligns with existing execute output caps. [`CONTEXT.md:167-170,237-261`](../../CONTEXT.md#L167-L170)

### Gaps

- **Do not recommend now:** generic downstream MCP federation, container/Kubernetes runtime, multi-tenant auth, OAuth broker, admin UI, or REST/gRPC translation. Adjacent servers cover these, but they conflict with mcptools’ static curated registry and would be product rewrites. [1MCP](https://github.com/1mcp-app/agent/blob/main/README.md); [ContextForge](https://github.com/IBM/mcp-context-forge/blob/main/README.md)
- **Do not recommend without a new boundary:** arbitrary shell/test execution, lane delete/merge/push, or automatic cleanup. Current docs explicitly reject generic forwarding and removal authority. [`.claude/context/lanes.md:3-43`](../../.claude/context/lanes.md#L3-L43)
- Streamable HTTP is a real interoperability gap, but it is transport work rather than a harness tool; treat it separately from the eight tool opportunities. [`crates/mcptools/src/mcp/mod.rs:61-77`](../../crates/mcptools/src/mcp/mod.rs#L61-L77); [MetaMCP transports](https://github.com/metatool-ai/metamcp/blob/ai-dev/README.md)
