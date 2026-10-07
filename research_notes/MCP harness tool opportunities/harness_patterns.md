# MCP harness workflow primitives

Research current through **2026-10-07**. All linked sources were accessed **2026-10-07**. “Observed” means a user report, issue, implementation, or measured example exists; “author claim” means the project or vendor reports the result; “inference” is a proposed transfer to mcptools, not an observed mcptools result.

## What tools help a harness choose tools, compress outputs, manage context, inspect failures, checkpoint work, or safely perform writes?

### Takeaway

The strongest primitives are control-plane operations around tools: progressive discovery, result indirection, typed lifecycle state, structured failure artifacts, policy gates, and end-to-end traces. Twelve defensible patterns follow.

### Cited Findings

1. **Progressive tool disclosure.** Anthropic says most clients load every definition up front and describes filesystem or `search_tools` discovery with selectable detail levels; its worked example reports 150,000 to 2,000 tokens, but this is an author example, not an independent benchmark. Atlassian’s implemented proxy exposes `get_tool_schema` and `invoke_tool` (optionally `list_tools`) instead of the whole catalog. — [Anthropic](https://www.anthropic.com/engineering/code-execution-with-mcp); [Atlassian Labs](https://github.com/atlassian-labs/mcp-compressor)

2. **Code-mode composition.** Anthropic proposes generated code APIs so filtering, joins, loops, retries, and inter-tool data transfer happen outside model context; it explicitly warns that generated code needs sandboxing, limits, and monitoring. A Hacker News implementation report independently describes the same two-tool design: discover tools, then execute code against them. — [Anthropic](https://www.anthropic.com/engineering/code-execution-with-mcp); [HN implementation discussion](https://news.ycombinator.com/item?id=45994560)

3. **Queryable result handles, not giant responses.** A user tested a compression hook and found it could not intercept MCP JSON-RPC responses after the fact; the suggested server-side remedy was compact summaries plus indexed full output and drill-down tools. Playwright implements the same shape with `browser_find`, which returns only matching accessibility nodes and nearby context instead of a full page snapshot. — [HN empirical report](https://news.ycombinator.com/item?id=47193074); [Playwright snapshots](https://playwright.dev/mcp/snapshots)

4. **Budgeted structural context.** Graft parses ASTs, builds dependency graphs, ranks files, incrementally caches changes, and renders to a caller-supplied token budget; its “100K LOC to ~2K tokens” result is an author claim. Context-MCP exposes `tokens_used`, dropped counts/reasons, and path metadata, making omission inspectable rather than silent. — [Graft](https://github.com/amaar-mc/graft); [Context-MCP Subgraph](https://github.com/yesheng-oss/context-mcp-subgraph)

5. **Typed, stewarded memory and lifecycle checkpoints.** Agent Memory MCP combines episodic/semantic/procedural/working types, hybrid lexical/vector retrieval, pre-compaction checkpoints, start/end hooks, provenance/freshness, supersession, drift scans, duplicate/conflict review, and low-risk-only session consolidation. It reports that indiscriminate age decay reduced its own Hit@5 from 0.7217 to 0.1942, evidence that generic recency heuristics can damage recall. — [Agent Memory MCP](https://github.com/ipiton/agent-memory-mcp)

6. **Persistent task DAG with leases and evidence.** AgentPlanner separates briefing, task claim, context pull, completion, and blocked writeback; claims expire after 30 minutes. Agent Task Manager adds dependencies, task locks, named checkpoints, structured handoff notes, and “evidence-based verification”; these are implementation descriptions, not comparative effectiveness studies. — [AgentPlanner](https://github.com/TAgents/agent-planner-mcp); [Agent Task Manager](https://github.com/mlnima/agent-task-manager-mcp)

7. **Test outcomes as typed data, not shell text.** MCP Pytest Runner treats normal test failures as successful tool execution while reserving tool errors for pytest/configuration failures; it returns structured summaries, traces, and capture controls. MCP Server Tester separately checks direct calls, model discoverability/use, and protocol compliance. MCPChecker records trajectories and verifies final outcomes with scripts or judges. — [MCP Pytest Runner](https://pypi.org/project/mcp-pytest-runner); [MCP Server Tester](https://github.com/steviec/mcp-server-tester); [MCPChecker](https://github.com/mcpchecker/mcpchecker)

8. **Artifact-first browser control.** Playwright MCP uses stable references over accessibility snapshots, targeted snapshot search, incremental/full/none modes, console/network inspection, traces, and persistent sessions; its docs now recommend CLI+Skills for coding agents because MCP schemas and snapshots cost more tokens, while retaining MCP for stateful exploratory loops. Arbitrary `browser_run_code_unsafe` is explicitly RCE-equivalent. — [Playwright MCP](https://github.com/microsoft/playwright-mcp); [Playwright coding-agent guidance](https://playwright.dev/docs/getting-started-cli); [Playwright MCP guide](https://playwright.dev/docs/getting-started-mcp)

9. **Trace propagation and cost trees.** mcp-agent emits OpenTelemetry spans for workflows, tool calls, LLM requests, MCP traffic, and durable activities; its token tree attributes usage and cost to child branches and supports threshold watchers. Langfuse’s example propagates W3C trace context through MCP `_meta`, permitting one trace across client, server, and external calls. — [mcp-agent observability](https://docs.mcp-agent.com/mcp-agent-sdk/advanced/observability); [Langfuse MCP tracing example](https://github.com/langfuse/langfuse-examples/tree/main/applications/mcp-tracing)

10. **Write mediation as a transparent proxy.** Seal relays MCP unchanged but evaluates calls against allow/deny/approval policy and writes a hash-chained audit log. MCP Audit Proxy additionally records schema changes and detects rug pulls/tool poisoning. UniFi’s server pairs dry-run previews with scrubbed audit records and captures pre-state for rollback of partially failed composite writes. — [Seal local gateway](https://useseal.dev/docs/local-mode); [MCP Audit Proxy](https://pkg.go.dev/github.com/firatmio/mcp-audit-proxy@v0.1.0); [mcp-unifi safety design](https://github.com/pete-builds/mcp-unifi)

11. **Coordination uses leases plus enforcement, not prose.** Agent-coord offers TTL path-glob leases, atomic task claims, status messages, and a pre-tool hook that blocks edits conflicting with another agent. MACP 2.0 proposes durable addressed messages delivered at inference boundaries, explicit interrupt priority, default-deny steering grants, and project isolation; it is early-stage (six commits at search time), so treat it as design evidence, not proven adoption. — [agent-coord](https://github.com/ThatHunky/agent-coord); [MACP 2.0](https://github.com/multiagentcognition/macp)

12. **Schema pinning and semantic change control.** OWASP recommends content-addressed/signed schemas, provenance per invocation, revalidation on changes, semantic policies, and approval when impact crosses a threshold. This makes tool metadata part of the executable trust boundary, not documentation. — [OWASP MCP03](https://owasp.org/www-project-mcp-top-10/2025/MCP03-2025%E2%80%93Tool-Poisoning)

### Inferences

- **Best mcptools transfer:** keep discovery/projection as the front door, but add a session-scoped projection: `search → describe → execute`, with risk, estimated response size, and required capability in search results. Cache schemas by hash and require re-approval on hash change. Patterns 1 and 12 support this shape.
- Add an opaque result store: every call can return a bounded preview, content hash, byte/token estimate, expiry, and handle; generic `search_result`, `slice_result`, and `export_result` tools preserve exact raw data without forcing it through the model. Pattern 3 supports this.
- Add a policy sidecar inside the aggregator: classify tools/read-write risk, validate paths/domains, support dry-run where upstreams allow it, approve by exception, and emit tamper-evident decisions. Pattern 10 supports this.
- Emit OpenTelemetry spans and a token/bytes tree keyed by session, agent, server, tool, schema hash, and result handle. Expose a read-only `inspect_failure` projection that joins the failed call, stderr, traces, test artifacts, and prior attempts. Patterns 7 and 9 support this.
- Keep project plans, memory, and multi-agent locks as optional narrow stores, not one universal “brain.” Use leases, evidence links, freshness/provenance, and explicit lifecycle events. Patterns 5, 6, and 11 support this.

### Gaps

- No independent study found comparing these twelve primitives within the same coding harness and task suite.
- No reliable evidence found that a general desktop-control MCP outperforms native computer-use APIs; browser evidence is much stronger and more reproducible.

## What patterns recur in community discussions, agent frameworks, and production server implementations?

### Takeaway

Three patterns recur across vendors, repositories, and user reports: disclose capabilities only when relevant, keep bulk data outside model context, and make state transitions explicit and auditable.

### Cited Findings

- **Observed user cost:** one Claude Code user reported five MCPs consuming 45K tokens, including approximately 12.9K for Linear and 9.8K for Playwright. This is anecdotal and client/version-specific, but it matches Anthropic’s architectural diagnosis. — [Reddit report](https://www.reddit.com/r/ClaudeCode/comments/1nntrkh/mcps_consume_too_much_context); [Anthropic](https://www.anthropic.com/engineering/code-execution-with-mcp)
- **Observed boundary:** the HN hook experiment found built-in/CLI output could be indexed before context, but third-party MCP output could not; compression must be in the server or proxy response path. — [HN empirical report](https://news.ycombinator.com/item?id=47193074)
- **Production direction:** Playwright itself now distinguishes token-efficient CLI+Skills for high-throughput coding from MCP for persistent, introspective browser sessions. — [Playwright](https://playwright.dev/docs/getting-started-cli)
- **Implementation convergence:** Atlassian’s compressed proxy, code-mode implementations, and community gateways all converge on a tiny stable meta-tool surface rather than API-per-operation exposure. — [Atlassian Labs](https://github.com/atlassian-labs/mcp-compressor); [HN Code Mode](https://news.ycombinator.com/item?id=45994560)
- **State convergence:** memory, planners, and coordinators independently use typed records, checkpoints, leases, review queues, and explicit complete/block/resolve events instead of relying on chat history. — [Agent Memory MCP](https://github.com/ipiton/agent-memory-mcp); [AgentPlanner](https://github.com/TAgents/agent-planner-mcp); [agent-coord](https://github.com/ThatHunky/agent-coord)

### Inferences

- mcptools should optimize for a **small permanent control surface plus ephemeral projections**, not merely shorter static descriptions.
- MCP adds most value where it owns durable state, permissions, or a live connection. For stateless high-volume operations, generated CLI/code clients may be the better projection.
- Evaluation should measure task success, wrong-tool rate, input tokens, output tokens, latency, approval burden, and recovery quality together; token reduction alone can hide worse behavior.

### Gaps

- Community reports rarely publish client versions, cache behavior, model settings, or repeated trials; their token numbers are directional only.
- No mature interoperability evidence found for inference-boundary multi-agent interrupts across major harnesses.

## What attractive ideas fail in practice or create security/context-cost risks?

### Takeaway

The main traps are compressing too late, lossy summarization without retrieval, broad “do everything” tools, autonomous memory writes, and permission-by-prompt.

### Cited Findings

- **Late compression cannot recover spent context.** Once a native MCP result has entered model history, post-tool hooks can only do damage control. — [HN empirical report](https://news.ycombinator.com/item?id=47193074)
- **More tools can reduce usable context and selection quality.** Users report tens of thousands of schema tokens; Anthropic identifies up-front definitions and intermediate copies as separate costs. — [Reddit report](https://www.reddit.com/r/ClaudeCode/comments/1nntrkh/mcps_consume_too_much_context); [Anthropic](https://www.anthropic.com/engineering/code-execution-with-mcp)
- **Automatic memory is not automatically correct.** Agent Memory MCP requires trust/freshness metadata, drift detection, review queues, and low-risk-only auto-apply; its measured recency-decay regression shows seemingly sensible heuristics can sharply hurt retrieval. — [Agent Memory MCP](https://github.com/ipiton/agent-memory-mcp)
- **Tool metadata and output are hostile input.** OWASP documents description/schema poisoning, hidden characters, sensitive-path references, runtime rug pulls, and the need for signed/versioned schemas plus execution-layer enforcement. — [OWASP MCP03](https://owasp.org/www-project-mcp-top-10/2025/MCP03-2025%E2%80%93Tool-Poisoning)
- **Code mode moves rather than removes risk.** It saves context and composes well, but agent-generated code can become an RCE/exfiltration path unless sandbox, network, filesystem, secret, CPU, and time limits are deterministic. Playwright labels its arbitrary-code tool RCE-equivalent. — [Anthropic](https://www.anthropic.com/engineering/code-execution-with-mcp); [Playwright MCP guide](https://playwright.dev/docs/getting-started-mcp)
- **Cooperative locks are insufficient.** agent-coord needs a harness pre-tool hook for hard edit blocking; clients without such a hook remain cooperative. — [agent-coord](https://github.com/ThatHunky/agent-coord)

### Inferences

- Do not add opaque LLM summarization as mcptools’ default compressor. Prefer structural filters, reversible handles, exact hashes, and user-selectable loss budgets.
- Do not collapse all upstreams into one unconstrained `execute(anything)` tool. Progressive discovery must retain per-tool validation, identity, provenance, and policy.
- Do not let an MCP server approve another server’s writes. Approval authority belongs to the harness or an independently trusted proxy.
- Pin tool schemas per session; diff changes; quarantine new imperative text; fail closed for write-class calls when policy or provenance is unavailable.

### Gaps

- No reliable production incident rate found for schema rug pulls versus response-based prompt injection.
- No independent evidence found that semantic prompt compressors preserve coding-agent constraints across long, adversarial tasks.
