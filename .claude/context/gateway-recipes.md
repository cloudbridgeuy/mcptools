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
below the upstream, replacing `<key>` with your Context7 API key:

```toml
[gateway.upstreams.headers]
Authorization = "Bearer <key>"
```

Header values are literal: environment-variable references are not expanded
(GUZ-1494). Keep keys out of version control and restrict config access:

```bash
chmod 600 "$(mcptools config path)"
```

Restart mcptools after config changes. Check the config and catalog:

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
  or replace it with a valid `Bearer <key>` value, then restart mcptools.
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
client_secret = "<slack-client-secret>"
redirect_port = 8765
redirect_uri = "https://<your-mac>.<tailnet>.ts.net/callback"
scopes = ["search:read.public", "search:read.private", "channels:history", "channels:read"]
readOnly = ["slack_search_public"]
```

Add this block to the config printed by `mcptools config path`, replacing all
placeholders. For private-channel history, add `groups:history`; for DM history,
add `im:history`; for sending, add `chat:write`. Add each scope in both places,
accept the app reinstall, and log in again. `search:read` is not a Slack MCP scope.

Client IDs and secrets are literal values, not environment references. Keep the
secret out of version control and restrict config access:

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
- **File uploads unsupported:** `slack_slack_get_file_upload_url` and
  `slack_slack_complete_file_upload` exist, but uploading bytes to the signed URL
  requires a plain HTTP POST that the gateway cannot perform.
- **Untested:** user-group mentions and automatic HTTP 429 retries. Do not rely
  on either.
