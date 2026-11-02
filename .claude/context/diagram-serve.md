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
- `POST /d/<token>/save`: save `{rev, snapshot, png}`; PNG is standard base64.
- `GET /assets/<file>`: embedded build assets.

Unknown tokens and paths return 404. Other methods return 405. Every request
must contain exactly one Host header whose IP address and port equal the bound
socket; otherwise it returns 403. DNS aliases and reverse proxy Host rewrites
are not accepted. No CORS headers are added. Responses disable caching and
referrers and set nosniff.

Lookup scans `<data_dir>/mcptools/diagrams/*/meta.json` for a matching 32-byte
lowercase hexadecimal token and reads the sibling `plan.tldr`. Here `data_dir`
is `XDG_DATA_HOME`, else the platform data root: `~/Library/Application Support`
on macOS, `$APPDATA` on Windows, or `~/.local/share` on other platforms.
The mcptools data directory is the root plus `mcptools`, matching GUZ-1131.
The shared data-directory helper is `crate::plan_store::data_dir`.
Malformed JSON or metadata without a token is skipped. I/O errors other than missing
files return 500 and are logged without the request token. Files are read on
each request, so plans created or updated after startup require no restart.

Core checks are in `crates/core/src/diagram_serve.rs`. Token comparison visits
all 32 bytes and accumulates XOR differences, with compiler barriers on byte
inputs and the result. Length rejection depends only on public token length.
No I/O occurs in these checks. Axum and storage reads live in
`crates/mcptools/src/diagram/serve.rs`.

## Canvas saves (GUZ-1138)

Save returns 200 with `{rev}` after replacing `plan.tldr`, `latest.png`, and
`meta.json`. A stale revision returns 409, bodies over 10 MiB return 413, and
malformed JSON, snapshot envelopes, or PNG encodings return 400. Declared size
is checked before upload; streamed bodies have the same cap. Unknown tokens
return 404. Rejected saves do not stage or replace files. Baseline stays intact.

Pure save routing, size, body, and revision decisions live in core `plan_store`.
The shell decodes base64, stages and syncs all three files, then renames each.
Replacement is atomic per file, not a multi-file transaction: a crash or rename
failure can leave mixed versions. A daemon mutex serializes saves, including
revision reads, on blocking workers. External `plan_write` processes and other
daemons are not serialized; use one daemon and avoid simultaneous external writes.
The existing retention task and shutdown selection are unchanged.

Failing-first: `cargo test -p mcptools --test plan_edit_acceptance save_ --
--test-threads=1` ran the four newly un-ignored tests before implementation.
All four failed because the endpoint returned 405 instead of 200/409/413/400.
Acceptance assertions were not changed. The same command passed all four after
implementation; pure size-boundary, routing, body, and revision tests also passed.
Final foreground `RUST_TEST_THREADS=1 cargo xtask lint` passed formatting,
check, clippy, all tests, machete, and typos. The added concurrent-save test
observed one 200 and one 409 for competing saves with the same revision.

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

## Offline browser and retention follow-up (GUZ-1147 / GUZ-1148)

The daemon starts the existing retention loop after binding the listener. It
logs loop errors without stopping HTTP service, then aborts and joins the loop
when the server stops. The existing loop sweeps immediately and hourly; its
default TTL is 30 days, overridden by `MCPTOOLS_DIAGRAM_TTL_SECONDS`.

`plan_offline_acceptance.rs` reuses the Chrome selection/version/skip recipe
from `crates/mcptools/tests/e2e_md_chain.rs:15`, the modern headless launch
options from `crates/mcptools/src/md/browser.rs:20`, and CDP network enable and
event-listener setup from `crates/mcptools/src/md/mod.rs:115`. These helpers are
private, so the test uses the same existing `headless_chrome` dependency and
core version validators directly; no new harness or dependency was added.
Chrome absent or older than 156 prints a skip reason. `CHROME` selects a binary.

The runtime test requires seeded `.tl-canvas .tl-shape` elements, waits for
fonts and a two-second settling interval, and inspects outgoing CDP requests.
Only the exact tldraw CDN origin and `/5.6.0/watermarks/watermark-track.svg`
path are allowed off-origin (including its tracking query). Inline data URLs
do not leave the origin and are excluded. The loopback development page sends
no watermark request, so zero external requests is also valid.

Failing-first verification used only foreground `RUST_TEST_THREADS=1 cargo
xtask lint`, with `CHROME` pointing to the installed Chrome for Testing 156.
Before sweep wiring, `daemon_sweeps_expired_plans_on_startup` failed with
`startup sweep must remove expired plans`. The browser test rendered shapes,
then temporarily fetched
`https://cdn.tldraw.com/5.6.0/fonts/IBMPlexSans-Medium.woff2` inside the page.
After excluding inline data URLs, the unchanged network assertion failed with
that font URL as its sole external request. The temporary fetch was removed
before final verification. No bundled asset or other worker's file was edited.

Final foreground lint passed formatting, check, clippy, all tests, machete,
and typos. Both new tests passed; the browser test ran (not skipped) with
Chrome for Testing 156.0.8078.47 selected through `CHROME`.
