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

## Slack: pending GUZ-1504
