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

## Slack: pending GUZ-1504
