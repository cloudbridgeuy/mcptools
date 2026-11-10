# Gateway recipes

See [Upstream gateway](mcp-server.md#upstream-gateway) for config paths,
transport details, cache behavior, and permissions.

## Context7

Resolve library IDs and query current library documentation through
`context7_resolve_library_id` and `context7_query_docs`.

Add to the config printed by `mcptools config path`:

```toml
[[gateway.upstreams]]
prefix = "context7"
url = "https://mcp.context7.com/mcp"
readOnly = ["resolve-library-id", "query-docs"]
```

No API key is required. For higher rate limits, add this optional table directly
below the upstream:

```toml
[gateway.upstreams.headers]
Authorization = "Bearer ${CONTEXT7_API_KEY}"
```

Export `CONTEXT7_API_KEY` in the process environment that starts mcptools.
The key stays out of config.toml. `${NAME}` references require a nonempty value
at upstream startup; a missing value skips Context7 with a name-only warning.
`mcptools config check` validates syntax and warns about missing variables but
still succeeds, so it can run without secrets. `$$` writes a literal `$`,
`$${NAME}` writes literal `${NAME}`, and unbraced `$NAME` is not expanded.
Defaults, nesting and shell expansion are not supported. Resolved environment
values are redacted from upstream output and never saved in the catalog cache.
Keep keys out of version control and restrict config access:

```bash
chmod 600 "$(mcptools config path)"
```

Restart mcptools after config or environment changes. Check the config and catalog:

```bash
mcptools config check
mcptools mcp catalog | grep context7
```

Expect both tool names above. From your MCP client's `call_tool` interface:

```javascript
call_tool({name: "context7_resolve_library_id", input: {libraryName: "tokio", query: "Tokio async runtime"}})
```

Verified live on 2026-11-06 with mcptools `37157ee`: config check and catalog
passed; resolving `tokio` returned IDs including `/tokio-rs/tokio`, without a key.

**Stdio alternative:** replace `url` with these fields; keep `prefix` and
`readOnly` unchanged. Requires Node.js and `npx` on PATH.

```toml
command = "npx"
args = ["-y", "@upstash/context7-mcp"]
```

**Common errors**

- **401:** remove an invalid optional Authorization header to use keyless access,
  or set `CONTEXT7_API_KEY` to a valid key, then restart mcptools.
- **Missing variable:** export `CONTEXT7_API_KEY` in the harness environment,
  not only in an unrelated terminal, then restart mcptools.
- **Stale catalog:** restart after config changes. If remote tools changed,
  delete `upstreams.cache.json` next to the selected config, then rerun the catalog
  command. See [cache details](mcp-server.md#upstream-gateway).
- **Unknown `url` field:** reinstall from source with `cargo xtask install` (from
  the mcptools checkout), or `mcptools upgrade` once a release includes HTTP
  upstreams, then rerun `mcptools config check`.

## Browse

Use [OpenCode Browse](https://opencode.ai/product/browse) through the gateway
as a stdio upstream. Install its prebuilt binary independently of Node.js or nvm,
without changing agent configs:

```bash
curl -fsSL https://opencode.ai/browse/install | bash -s -- --no-setup
```

This installs `~/.local/bin/browse`. Add the Browse Chrome extension, then run
`browse pair` and enter the one-time code. Confirm pairing with `browse pair --list`.
Use the absolute binary path if `browse` is not on PATH.

Add to the config printed by `mcptools config path`, replacing `<user>` with your
account name (use your platform's absolute home path):

```toml
[[gateway.upstreams]]
prefix = "browse"
command = "/Users/<user>/.local/bin/browse"
args = ["mcp"]
readOnly = ["status", "session_list"]
```

Only `browse_status` and `browse_session_list` are allowed without `allowWrites`.
Every other Browse tool requires `allowWrites: true` through `call_tool` or
`execute`. Keep `browse_secrets_run` gated: it runs a local command with captured
credentials.

Restart the MCP client after `config.toml` changes so its mcptools gateway reloads
the config. Check the config and catalog:

```bash
mcptools config check
mcptools mcp catalog | grep browse
```

Expect `browse_status`, `browse_session_list`, and the other prefixed Browse tools.
From your MCP client's `call_tool` interface, check the running relay:

```javascript
call_tool({name: "browse_status", input: {}})
```

**Cold start**

Observed with Browse 0.0.16, already paired with Chrome, after the relay was
stopped with SIGTERM:

- `browse_execute` through the gateway starts the relay. Chrome reconnects without
  re-pairing.
- `browse_status` does not start the relay; it fails with
  "browse relay is not reachable". This does not mean the install is broken. Run
  `browse_execute` (with `allowWrites: true`) or a relay-backed CLI command first.
- `browse relay` has only `restart`; there is no `stop` subcommand.

Cold `browse_session_list` after pairing is untested; its earlier failure was
observed only before pairing. Cold start after a real reboot is also untested.
These checks were not repeated here to avoid stopping the user's relay or
driving their browser.

**Common errors and limits**

- **No paired browser:** `browse_execute` was observed reaching the gateway's
  upstream timeout instead of returning a clear pairing error. Pair first. The
  stdio upstream request timeout is 30 seconds (`REQUEST_TIMEOUT` in
  `crates/mcptools/src/gateway.rs`), verified from source, not measured live here.
  The no-paired-browser case was not repeated here. A 45-second request gave up
  earlier, while a 25-second run succeeded; these are observations, not the limit.
- **Missing screenshots:** screenshots are saved in a temporary folder named after
  the relay process. They may not outlast it; copy files you need to keep.
- **Version drift:** auto-update is enabled by default, so the binary can change
  while the gateway runs. The Chrome extension version can lag the CLI version.

## Slack

Search Slack and read channel history through Slack's official remote MCP server.
Sending messages is optional and posts as the signed-in user.

**App and HTTPS callback**

At [Slack apps](https://api.slack.com/apps), create or select an app and obtain its
client ID and client secret. The app must also be enabled for Slack MCP access.
Slack requires a confidential OAuth client and an HTTPS redirect URI.

With Tailscale HTTPS enabled and your browser connected to the tailnet, proxy the
fixed local callback port:

```bash
tailscale serve --bg --https=443 http://127.0.0.1:8765
```

Use the HTTPS hostname reported by Tailscale in place of
`<your-mac>.<tailnet>.ts.net` below. Under the app's **OAuth & Permissions**, add
`https://<your-mac>.<tailnet>.ts.net/callback` as a redirect URL and press
**Save URLs**. It must match `redirect_uri` exactly. Keep `redirect_port = 8765`
aligned with the proxy target. See [OAuth transport details](mcp-server.md#oauth-http-upstreams).

**Scopes and config**

Add these **User Token Scopes**, not bot scopes, in the Slack app and list the
same scopes in config. Each login requests only the configured scopes:

```toml
[[gateway.upstreams]]
prefix = "slack"
url = "https://mcp.slack.com/mcp"
auth = "oauth"
client_id = "<slack-client-id>"
client_secret = "${SLACK_CLIENT_SECRET}"
redirect_port = 8765
redirect_uri = "https://<your-mac>.<tailnet>.ts.net/callback"
scopes = ["search:read.public", "search:read.private", "channels:history", "channels:read"]
readOnly = ["slack_search_public"]
```

Add this block to the config printed by `mcptools config path`, replacing all
placeholders. For private-channel history, add `groups:history`; for DM history,
add `im:history`; for sending, add `chat:write`. Add each scope in both places,
accept the app reinstall, and log in again. `search:read` is not a Slack MCP scope.

Client IDs remain literal. Export `SLACK_CLIENT_SECRET` in the environment used
for login and MCP token refresh. `client_secret` supports the same `${NAME}`
rules as HTTP headers: `$$` escapes `$`, unbraced `$NAME` stays literal,
malformed references fail config loading, and expansion is not recursive.
Missing, empty or non-Unicode variables fail the operation with upstream and
variable names only; `config check` warns without printing values. Plain-text
secrets still work. Resolved secrets are redacted from upstream results and are
not saved in credentials or cache. Keep secrets out of version control and
restrict config access:

```bash
chmod 600 "$(mcptools config path)"
mcptools config check
mcptools gateway login slack
```

Finish authorization within 3 minutes. Check the terminal for login success,
then restart the MCP client and inspect the catalog:

```bash
mcptools mcp catalog | grep slack
mcptools mcp declarations slack_slack_search_public slack_slack_send_message
```

Tool names are doubled: `slack_slack_search_public` and
`slack_slack_send_message`. Use the declarations for call inputs. Only public
search is allowed without `allowWrites` in this config. Other tools, including
sending, need `allowWrites: true` through `call_tool` or `execute`. `readOnly`
entries use upstream names, such as `slack_search_public`, without the gateway
prefix; do not add send tools to this list.

Setup verified live on 2026-11-08: HTTPS callback, OAuth login, and Slack calls.

**Common errors and limits**

- **HTTP 400, "App is not enabled for Slack MCP server access.":** enable MCP
  access for the app and accept the reinstall.
- **Missing scope or access:** check User Token Scopes and config `scopes`, accept
  the reinstall, then log in again. Bot scopes do not replace user scopes.
- **Redirect error or timeout:** save the exact HTTPS URL in Slack, check the
  Tailscale proxy and free port 8765, then retry login within 3 minutes.
- **401:** log in again if automatic token refresh fails, then restart the MCP
  client. Restart after config changes to refresh the catalog.
- **Unknown OAuth config field:** install a current binary with
  `cargo xtask install` from the checkout, then rerun `mcptools config check`.
- **File uploads:** one `execute` run can call `file_stat({path})`, check
  `isFile`, pass `size` as the exact `content_length` to
  `slack_slack_get_file_upload_url`, POST the file with
  `http_request({method: "POST", url: signedUrl, bodyPath: path})`, check the
  HTTP status, then call `slack_slack_complete_file_upload`. Inspect the Slack
  declarations for their input shapes. Set `allowWrites: true` on execute
  (and `allowSpend: true` if generating an image in the same script).
  Set `[http].allowed_hosts` in config to the signed URL's exact host, or use
  `MCPTOOLS_HTTP_ALLOWED_HOSTS` as a comma-separated environment override.
  An empty list or empty override denies all; call input cannot authorize
  hosts. No wildcards or redirects. Uploads are limited to 32 MiB
  and 10 seconds, with a 1 MiB UTF-8 response cap. No credentials are added.
  `bodyPath` can upload any readable local regular file, including secrets:
  there is no root restriction. Keep host permissions narrow. The live
  generated-image upload to the user's own Slack DM remains unverified.
- **Untested:** user-group mentions and automatic HTTP 429 retries. Do not rely
  on either.

## Sentry

Search issues, events, traces and docs through Sentry's official remote MCP
server. Tool names are `sentry_<upstream>`, such as `sentry_search_issues`; unlike
Slack's, they are not doubled.

Sentry uses OAuth with dynamic client registration and PKCE S256 as a public
client. No `client_id`, `client_secret` or `redirect_uri` is needed, and the
default loopback callback works.

Add to the config printed by `mcptools config path`:

```toml
[[gateway.upstreams]]
prefix = "sentry"
url = "https://mcp.sentry.dev/mcp"
auth = "oauth"
readOnly = [
  "whoami", "find_organizations", "find_teams", "find_projects", "find_releases",
  "get_release_details", "find_dashboards", "get_dashboard_details", "find_monitors",
  "get_monitor_details", "find_uptime_monitors", "get_uptime_monitor_details",
  "find_metric_monitors", "get_metric_monitor_details", "find_alert_rules",
  "get_alert_rule", "get_alert_options", "get_issue_details", "get_event_stacktrace",
  "get_issue_activity", "get_issue_breadcrumbs", "get_issue_user_reports",
  "get_issue_tag_values", "get_trace_details", "get_span_details", "get_replay_details",
  "get_event_attachment", "search_errors", "search_logs", "search_traces",
  "search_metrics", "search_profiles", "search_replays", "find_dsns", "search_docs",
  "get_doc", "search_issues", "search_issue_events", "find_dropped_events",
  "get_profile", "get_profile_details", "get_sentry_mcp_info", "get_sentry_resource",
  "get_snapshot", "get_snapshot_image", "get_latest_base_snapshot",
  "get_agent_conversation_details", "search_agent_conversations",
]
```

Check the config, then log in and finish authorization within 3 minutes:

```bash
mcptools config check
mcptools gateway login sentry
```

Restart the MCP client, then inspect the catalog:

```bash
mcptools mcp catalog | grep sentry
mcptools mcp declarations sentry_search_issues sentry_get_issue_details
```

`readOnly` entries use upstream names without the gateway prefix. Every other
tool needs `allowWrites: true` through `call_tool` or `execute`: create, update
and delete for monitors, alerts, teams, projects and DSNs; `update_issue`,
`link_issue`, `unlink_issue`, `add_issue_note`, `add_team_to_project`,
`remove_team_from_project`, `onboarding_status_update` and
`submit_feedback_to_sentry_team`. `analyze_issue_with_seer` is deliberately left
out of `readOnly`: it starts a Seer analysis run (a side effect that may consume quota).

Setup verified live on 2026-11-09: OAuth login and catalog listing. Tool calls not
yet verified.

**Common errors and limits**

- **Redirect URI rejected (untested):** fall back to the Slack-style HTTPS
  `redirect_uri` plus `redirect_port` through Tailscale, as in [Slack](#slack).
  See [OAuth transport details](mcp-server.md#oauth-http-upstreams).
- **401:** log in again, then restart the MCP client.
- **AI-powered search (untested):** `search_errors`, `search_logs`, `search_traces`,
  `search_issues` and similar may depend on Sentry's server-side LLM.
- **Self-hosted Sentry:** the hosted remote server targets sentry.io. Untested stdio
  fallback; replace `url` and `auth` with these fields, keeping `prefix` and `readOnly`:

```toml
command = "npx"
args = ["-y", "@sentry/mcp-server@latest"]

[gateway.upstreams.env]
SENTRY_AUTH_TOKEN = "<token>"
SENTRY_HOST = "sentry.example.com"
```

## Honeycomb

Query traces, datasets, SLOs, boards and triggers through Honeycomb's hosted MCP
server. Tool names are `honeycomb_<upstream>`, such as `honeycomb_run_query`.

Honeycomb uses OAuth 2.1 with dynamic client registration. No `client_id`,
`client_secret` or `redirect_uri` is needed, and the default loopback callback
works. Add to the config printed by `mcptools config path`:

```toml
[[gateway.upstreams]]
prefix = "honeycomb"
url = "https://mcp.honeycomb.io/mcp"
auth = "oauth"
readOnly = [
  "find_columns", "find_queries", "get_aiconversation", "get_dataset",
  "get_dataset_columns", "get_environment", "get_query_results", "get_service_map",
  "get_signals", "get_slos", "get_span_details", "get_trace", "get_triggers",
  "get_workspace_context", "list_aiconversations", "list_boards", "list_memories",
  "list_recipients", "list_spans", "migration_guide", "read_memory", "refinery_docs",
  "run_bubbleup", "run_query", "semconv",
]
```

Restrict config access, check it, then log in and finish authorization within
3 minutes:

```bash
chmod 600 "$(mcptools config path)"
mcptools config check
mcptools gateway login honeycomb
```

Restart the MCP client, then inspect the catalog:

```bash
mcptools mcp catalog | grep honeycomb
mcptools mcp declarations honeycomb_get_workspace_context honeycomb_run_query
```

From your MCP client's `call_tool` interface:

```javascript
call_tool({name: "honeycomb_get_workspace_context", input: {}})
```

`readOnly` entries use upstream names without the gateway prefix. Two tools stay
write-gated and need `allowWrites: true`: `honeycomb_feedback` (submits feedback
to Honeycomb) and `honeycomb_record_onboarding_state` (writes onboarding state).
`run_query` and `run_bubbleup` execute queries, which appear in Honeycomb query
history, but do not change configuration. To gate them too, drop them from
`readOnly`.

**EU region:** replace `url` with `https://mcp.eu1.honeycomb.io/mcp`; keep the
rest unchanged. Source:
[Honeycomb MCP configuration guide](https://docs.honeycomb.io/integrations/mcp/configuration-guide).

**Headless alternative (unverified):** a Management API key (team owners only,
with MCP scopes) can replace OAuth. Remove `auth = "oauth"`, since it cannot be
combined with a static Authorization header (see
[mcp-server.md](mcp-server.md#upstream-gateway)), and add:

```toml
[gateway.upstreams.headers]
Authorization = "Bearer <KEY_ID>:<SECRET>"
```

Verified live on 2026-11-09 with mcptools `7b704b4`: OAuth login via dynamic
registration, 27 tools in the catalog, and `honeycomb_get_workspace_context`
returned workspace context through `mcp stdio --discovery --code-mode`.
`honeycomb_feedback` without `allowWrites` was refused by the gateway. Other tool
calls were not exercised.

**Common errors and limits**

- **Stale catalog:** restart after config changes. If remote tools changed, delete
  `upstreams.cache.json` next to the selected config, then rerun the catalog
  command. See [cache details](mcp-server.md#upstream-gateway).
- **Login or registration failure:** rerun `mcptools gateway login honeycomb`
  within 3 minutes, then restart the MCP client. Confirm the region matches your
  Honeycomb account (US versus EU URL).
- **401:** log in again if automatic token refresh fails, then restart the MCP
  client.
- **Plan requirement (unverified):** third-party guides say Honeycomb Intelligence
  must be enabled for the account. Not confirmed here.
