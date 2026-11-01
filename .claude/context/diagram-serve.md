# Diagram serve (GUZ-1132)

```sh
mcptools diagram serve --host 100.80.20.1 --port 5199
```

`--host` is a required IP address. Bind a specific loopback or Tailscale IP.
IPv4, IPv6, and IPv4-mapped IPv6 wildcard addresses are refused. Port defaults
to 5199; port 0 selects an ephemeral port. Startup prints the actual base URL.
The daemon runs until interrupted; the operator supplies service supervision.

Routes:

- `GET /d/<token>`: embedded tldraw page for an existing plan token.
- `GET /d/<token>/plan.tldr`: stored snapshot bytes, JSON content type.
- `GET /assets/<file>`: embedded build assets.

Unknown tokens and paths return 404. Other methods return 405. Every request
must contain exactly one Host header whose IP address and port equal the bound
socket; otherwise it returns 403. DNS aliases and reverse proxy Host rewrites
are not accepted. No CORS headers are added. Responses disable caching and
referrers and set nosniff. Requests are read-only; no save endpoint exists yet.

Lookup scans `<data_dir>/mcptools/diagrams/*/meta.json` for a matching 32-byte
lowercase hexadecimal token and reads the sibling `plan.tldr`. Here `data_dir`
is `XDG_DATA_HOME`, else the platform data root: `~/Library/Application Support`
on macOS, `$APPDATA` on Windows, or `~/.local/share` on other platforms.
The mcptools data directory is the root plus `mcptools`, matching GUZ-1131.
No reusable data-directory helper was found in the existing checkout.
Malformed JSON or metadata without a token is skipped. I/O errors other than missing
files return 500 and are logged without the request token. Files are read on
each request, so plans created or updated after startup require no restart.

Core checks are in `crates/core/src/diagram_serve.rs`. Token comparison visits
all 32 bytes and accumulates XOR differences, with compiler barriers on byte
inputs and the result. Length rejection depends only on public token length.
No I/O occurs in these checks. Axum and storage reads live in
`crates/mcptools/src/diagram/serve.rs`.

The embedded page build and limitations are in `diagram-page/README.md`.
GUZ-1131 supplies plan writes and URLs. Its MCP-backed token-route acceptance
test remains ignored here; the daemon integration test seeds the shared
storage contract directly and binds port 0.

## Red-first evidence

Before the daemon existed, `RUST_TEST_THREADS=1 cargo xtask lint --fix` compiled
the new integration test and failed at
`seeded_token_routes_enforce_host_and_serve_embedded_page`: daemon exited before
serving. The baseline binary rejected the missing `diagram` command.
The newly unignored `serve_refuses_wildcard_host` test was also run against
that pre-implementation test binary and failed because the CLI returned
`unrecognized subcommand 'diagram'`, not the required wildcard policy error.

Final verification: `RUST_TEST_THREADS=1 cargo xtask lint` passed all checks,
including clippy with warnings denied, tests, machete, and typos. A headless
Chromium smoke test loaded the seeded spike at a 390×844 viewport: 12 rendered
shape elements, no load alert, and no page errors. A real phone over Tailscale
was not tested.
