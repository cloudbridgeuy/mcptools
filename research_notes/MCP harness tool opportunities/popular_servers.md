# Popular MCP servers: adoption signals and harness opportunities

All sources accessed 2026-10-07. Counts are point-in-time observations, not durable rankings.

## Which servers have credible adoption signals?

### Takeaway

Twelve servers have useful evidence, but only package downloads approximate execution. GitHub stars/forks show awareness and contributor interest; release dates show maintenance; neither proves production use.

### Cited Findings

| Server | Directly observed signal | Capability relevant to a harness |
|---|---|---|
| **Playwright MCP** | 37,901 stars, 3,222 forks; 28,609,891 npm downloads during 2026-09-05–10-04; v0.0.83 released 2026-09-28. [GitHub API](https://api.github.com/repos/microsoft/playwright-mcp) [npm API](https://api.npmjs.org/downloads/point/last-month/%40playwright%2Fmcp) [releases](https://api.github.com/repos/microsoft/playwright-mcp/releases?per_page=5) | Structured accessibility snapshots give deterministic element handles; isolated/persistent contexts, permission grants, capability flags, secret masking, output directories, and idle timeout make browser state explicit. Its README now says CLI+skills can be more token-efficient while MCP suits persistent, introspective loops. [README](https://github.com/microsoft/playwright-mcp#readme) |
| **Chrome DevTools MCP** | 53,084 stars, 5,772 forks; 7,949,247 npm downloads in the same period; five releases from 2026-08-10 through 09-23. [GitHub API](https://api.github.com/repos/ChromeDevTools/chrome-devtools-mcp) [npm API](https://api.npmjs.org/downloads/point/last-month/chrome-devtools-mcp) [releases](https://api.github.com/repos/ChromeDevTools/chrome-devtools-mcp/releases?per_page=5) | Separate tools expose page snapshots, screenshots, console and network records, performance traces/insights, and heap-snapshot analysis; `includeSnapshot` avoids automatic bulky output. [tool reference](https://github.com/ChromeDevTools/chrome-devtools-mcp/blob/main/docs/tool-reference.md) |
| **Context7** | 62,774 stars, 3,054 forks; 2,861,909 npm downloads in the same period; multiple releases on 2026-10-07. [GitHub API](https://api.github.com/repos/upstash/context7) [npm API](https://api.npmjs.org/downloads/point/last-month/%40upstash%2Fcontext7-mcp) [releases](https://api.github.com/repos/upstash/context7/releases?per_page=5) | Two-stage `resolve-library-id` then `query-docs` narrows retrieval to an exact library/version and topic instead of injecting a whole corpus. [README](https://github.com/upstash/context7#available-tools) |
| **GitHub MCP Server** | 33,433 stars, 5,104 forks; GitHub released v2.0.0 and v2.0.1 on 2026-10-06/07. [GitHub API](https://api.github.com/repos/github/github-mcp-server) [releases](https://api.github.com/repos/github/github-mcp-server/releases?per_page=5) | Composable toolsets, individual allow/exclude lists, read-only mode, and lockdown reduce schema context and risk; read-only and exclusion override other selections. OAuth keeps the token in memory. [configuration](https://github.com/github/github-mcp-server/blob/main/docs/server-configuration.md) [README](https://github.com/github/github-mcp-server#readme) |
| **Serena** | 30,085 stars, 2,044 forks; five releases from 2026-05-26 through 08-09. [GitHub API](https://api.github.com/repos/oraios/serena) [releases](https://api.github.com/repos/oraios/serena/releases?per_page=5) | Symbol overview, declarations, references, diagnostics, rename, safe delete, and body-level edits replace broad text reads/search-replace; project memories provide explicit long-lived context. [tools](https://oraios.github.io/serena/01-about/035_tools.html) |
| **Context Mode** | 25,617 stars, 1,844 forks; 92,856 npm downloads in the same period; the repository was pushed on 2026-10-07. [GitHub API](https://api.github.com/repos/mksglu/context-mode) [npm API](https://api.npmjs.org/downloads/point/last-month/context-mode) | Sandbox execution keeps raw output outside model context; SQLite/FTS5 event search restores selected session facts; `stats`, `doctor`, `purge`, and indexed search make context behavior observable and repairable. Its “98% reduction” is an author benchmark, not independent evidence. [README](https://github.com/mksglu/context-mode#how-context-mode-solves-it) |
| **n8n MCP** | 23,048 stars, 3,662 forks; 405,013 npm downloads in the same period; v2.89.0–v2.92.1 shipped from 2026-09-23 through 10-06. [GitHub API](https://api.github.com/repos/czlonkowski/n8n-mcp) [npm API](https://api.npmjs.org/downloads/point/last-month/n8n-mcp) [releases](https://api.github.com/repos/czlonkowski/n8n-mcp/releases?per_page=5) | A “start here” tool documents other tools; node details have minimal/full modes; workflow validation precedes deployment; snapshots support diff/rollback; resource discovery resolves real IDs instead of guessing. Credentials are origin-confined. [README](https://github.com/czlonkowski/n8n-mcp#available-mcp-tools) |
| **AWS MCP servers / AWS Documentation server** | The AWS Labs suite has 9,761 stars and 1,799 forks; its documentation package had 419,120 PyPI downloads in the reported last month; five suite releases landed during 2026-09-01–30. [GitHub API](https://api.github.com/repos/awslabs/mcp) [PyPI Stats](https://pypistats.org/api/packages/awslabs.aws-documentation-mcp-server/recent) [releases](https://api.github.com/repos/awslabs/mcp/releases?per_page=5) | AWS’s managed server combines docs with prebuilt SOPs, syntactic API validation, IAM authorization, and CloudTrail audit logs; its successor toolkit adds condition keys that distinguish agent actions from human actions. [README](https://github.com/awslabs/mcp#readme) |
| **Firecrawl MCP** | 7,565 stars, 901 forks; 445,335 npm downloads in the same period; repository pushed 2026-10-07. [GitHub API](https://api.github.com/repos/mendableai/firecrawl-mcp-server) [npm API](https://api.npmjs.org/downloads/point/last-month/firecrawl-mcp) | Bounded crawl depth/path filters, schema-shaped JSON, and separate map/search/scrape tools constrain output. A three-tool keyless profile and nine-tool search profile provide progressive disclosure for clients with tool-slot limits. [README](https://github.com/mendableai/firecrawl-mcp-server#readme) |
| **Filesystem reference server** | Its package had 2,616,154 npm downloads in the same period. The containing reference repository has 91,067 stars, but that monorepo count is only an indirect signal for this server. [npm API](https://api.npmjs.org/downloads/point/last-month/%40modelcontextprotocol%2Fserver-filesystem) [GitHub API](https://api.github.com/repos/modelcontextprotocol/servers) | Runtime MCP Roots replace allowed directories; `list_allowed_directories` exposes the boundary; edits support dry-run diff; annotations identify read-only, idempotent, destructive, and closed-world tools. [README](https://github.com/modelcontextprotocol/servers/blob/main/src/filesystem/README.md) |
| **mcp-atlassian** | 5,973 stars, 1,389 forks; v0.21.1–v0.23.1 shipped from 2026-04-10 through 08-19. PyPI download statistics were unavailable due to HTTP 429. [GitHub API](https://api.github.com/repos/sooperset/mcp-atlassian) [releases](https://api.github.com/repos/sooperset/mcp-atlassian/releases?per_page=5) [PyPI](https://pypi.org/project/mcp-atlassian/) | One server supports Jira and Confluence Cloud plus Server/Data Center, with paired get/create/update tools and documented token/PAT/OAuth choices. [README](https://github.com/sooperset/mcp-atlassian#key-tools) |
| **Sentry MCP** | Only 917 stars and 159 forks, but 500,185 npm downloads in the same period and five releases from 2026-09-24 through 10-05. This is the clearest case where repository popularity understates package activity. [GitHub API](https://api.github.com/repos/getsentry/sentry-mcp) [npm API](https://api.npmjs.org/downloads/point/last-month/%40sentry%2Fmcp-server) [releases](https://api.github.com/repos/getsentry/sentry-mcp/releases?per_page=5) | Skill-selectable tool groups target inspect/triage/Seer workflows; natural-language error, trace, log, and issue searches compile to Sentry query syntax. Explicit bearer forwarding avoids server-side token storage. [README](https://github.com/getsentry/sentry-mcp#readme) |

### Inferences

- Strongest combined adoption cases are Playwright, Chrome DevTools, Context7, n8n, and Firecrawl because each has substantial repository interest, package traffic, and current maintenance. [GitHub search API](https://api.github.com/search/repositories?q=topic%3Amcp-server&sort=stars&order=desc&per_page=100)
- Filesystem and Sentry show why package traffic must be checked separately from repository stars. [Filesystem npm API](https://api.npmjs.org/downloads/point/last-month/%40modelcontextprotocol%2Fserver-filesystem) [Sentry npm API](https://api.npmjs.org/downloads/point/last-month/%40sentry%2Fmcp-server)

### Gaps

- No public source established unique active users, successful tool-call counts, retention, or production deployment counts for any server.
- npm/PyPI counts include installs, updates, CI, mirrors, and repeated `npx`/`uvx` execution; they are not user counts.
- No reliable package-download count was obtained for GitHub MCP, Serena, or mcp-atlassian; AWS’s suite-level stars do not isolate its documentation server.

## Which capabilities should inform mcptools?

### Takeaway

The repeated pattern is not “more tools.” It is a small searchable surface, bounded results, explicit state and authority, previewable mutation, and diagnostics.

### Cited Findings

- **Progressive discovery:** add a compact tool finder plus on-demand full schema/help. Context7 resolves then queries; n8n starts with tool documentation and minimal/full detail; Firecrawl offers three- and nine-tool profiles. [Context7](https://github.com/upstash/context7#available-tools) [n8n](https://github.com/czlonkowski/n8n-mcp#available-mcp-tools) [Firecrawl](https://github.com/mendableai/firecrawl-mcp-server#readme)
- **Policy-filtered catalog:** support named toolsets/profiles, exact allow/exclude lists, read-only enforcement, and precedence rules before schemas enter context. [GitHub configuration](https://github.com/github/github-mcp-server/blob/main/docs/server-configuration.md)
- **Output budgets:** standardize `limit`, cursor/page token, projection/detail level, schema-shaped output, optional snapshots, and artifact handles for oversized payloads. [Firecrawl README](https://github.com/mendableai/firecrawl-mcp-server#readme) [Chrome tool reference](https://github.com/ChromeDevTools/chrome-devtools-mcp/blob/main/docs/tool-reference.md) [n8n README](https://github.com/czlonkowski/n8n-mcp#available-mcp-tools)
- **Safety contract:** surface MCP annotations, scope roots, origin-bind credentials, offer dry-run/diff, and let read-only mode override requested writes. [Filesystem README](https://github.com/modelcontextprotocol/servers/blob/main/src/filesystem/README.md) [GitHub configuration](https://github.com/github/github-mcp-server/blob/main/docs/server-configuration.md) [n8n README](https://github.com/czlonkowski/n8n-mcp#important-safety-warning)
- **Reliable mutation:** add validate-before-execute and checkpoint/diff/rollback patterns for stateful tools. [n8n README](https://github.com/czlonkowski/n8n-mcp#available-mcp-tools)
- **Observable harness:** expose health/doctor, invocation latency/success, context bytes saved/returned, active profile, versions, and purge controls. Context Mode exposes these controls, while Chrome DevTools records success/latency telemetry with opt-out. [Context Mode](https://github.com/mksglu/context-mode#readme) [Chrome DevTools](https://github.com/ChromeDevTools/chrome-devtools-mcp#readme)
- **Typed state handles:** return stable page, request, trace, symbol, workflow-version, or artifact IDs and retrieve details separately. [Chrome tool reference](https://github.com/ChromeDevTools/chrome-devtools-mcp/blob/main/docs/tool-reference.md) [Serena tools](https://oraios.github.io/serena/01-about/035_tools.html) [n8n README](https://github.com/czlonkowski/n8n-mcp#available-mcp-tools)
- **Auditable actor identity:** distinguish agent actions from human actions and preserve an operation audit trail. [AWS README](https://github.com/awslabs/mcp#readme)

### Inferences

- Highest-value mcptools sequence: (1) filtered/searchable catalog, (2) uniform output budgets and continuations, (3) safety metadata plus preview, (4) diagnostics/telemetry, then (5) durable artifact/session retrieval.
- Semantic code operations are valuable but domain-specific; mcptools should first supply generic routing, policy, result-shaping, and observability primitives that any server can use.

### Gaps

- This research did not inspect local mcptools code, so each opportunity requires a duplicate/fit check before implementation.
- No independent benchmark compared these interaction patterns on task success, latency, or total tokens.

## How strong is the evidence?

### Takeaway

Use the signals for prioritization, not market-share claims. Direct API counts and release records are reproducible; usage interpretation remains uncertain.

### Cited Findings

- **Direct observations:** GitHub API star/fork/push metadata, release timestamps, and registry download totals. Example endpoints return machine-readable values. [GitHub API](https://api.github.com/repos/microsoft/playwright-mcp) [npm API](https://api.npmjs.org/downloads/point/last-month/%40playwright%2Fmcp) [PyPI Stats](https://pypistats.org/api/packages/awslabs.aws-documentation-mcp-server/recent)
- **Direct capability evidence:** repository documentation and tool references describe exposed interfaces and configuration. [Chrome tools](https://github.com/ChromeDevTools/chrome-devtools-mcp/blob/main/docs/tool-reference.md) [Filesystem tools](https://github.com/modelcontextprotocol/servers/blob/main/src/filesystem/README.md)
- **Weak proxies:** stars, forks, monorepo popularity, directory placement, testimonials, and vendor performance claims indicate attention or intent, not sustained use. Context Mode’s reduction claim is explicitly treated only as a design lead. [Context Mode README](https://github.com/mksglu/context-mode#readme)

### Inferences

- Require at least two independent signal types before calling a server “strongly adopted”; package traffic plus active releases is better than stars alone.
- Prefer borrowing interaction contracts seen across unrelated servers over copying one vendor’s claimed benchmark.

### Gaps

- Directory rankings and community recommendation frequency were not used because no stable, auditable count was found within the research budget.
- Download APIs do not expose how many calls completed successfully after installation.
