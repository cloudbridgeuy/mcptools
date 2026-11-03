# Diagram serve

Canonical daemon, security, license, and troubleshooting reference. Tool
contracts and the shared storage layout are in [Plan canvas tools](diagram.md).

```sh
mcptools diagram serve --host 100.80.20.1 --port 5199
```

`--host` takes a specific IP address, not necessarily loopback or Tailscale.
Alternatively, `--tailscale` runs `tailscale ip -4` and selects an address in
100.64.0.0/10. Exactly one of `--host` and `--tailscale` is required; they conflict.
IPv4, IPv6, and IPv4-mapped IPv6 wildcard addresses are refused. Port defaults
to 5199; port 0 selects an ephemeral port. Startup prints the actual base URL.
The daemon exits after the idle grace period or when interrupted.

Routes:

- `GET /d/<token>`: embedded tldraw page for an existing plan token.
- `GET /d/<token>/plan.tldr`: stored snapshot bytes, JSON content type, and matching `X-Plan-Rev` header.
- `POST /d/<token>/save`: save `{rev, snapshot, png}`; PNG is standard base64.
- `GET /assets/<file>`: embedded build assets.

Unknown tokens and paths return 404. Other methods return 405. Every request
must contain exactly one Host header whose IP address and port equal the bound
socket; otherwise it returns 403. DNS aliases and reverse proxy Host rewrites
are not accepted. No CORS headers are added. Responses disable caching and
referrers and set nosniff.

## Security boundary

Keep the canvas tailnet-only; do not expose it through Cloudflare. Recommended
operator step: Tailscale ACLs allow port 5199 only from the user's devices.
Plan tokens are 128 OS-random bits encoded as 32 lowercase hexadecimal characters.
They survive updates, but access requires an open `share.json` window in the
plan directory (`expires_at_unix_secs`). No file, malformed data, closed or
expired windows return 404 for page GET, snapshot GET, and save POST without
touching last-access. Saves recheck the window after receiving the body, before
writing. The page reports “Link expired, ask the agent for a new link” on save
404. `plan_write` and `plan_url` open or extend the window to now plus the TTL;
default 1800 seconds, environment `MCPTOOLS_DIAGRAM_SHARE_SECONDS`, optional
per-call `share_seconds` override; TTLs clamp to 60–86400 seconds. `plan_close`
closes access now, with no token rotation. `plan_write` and `plan_close` require
`allowWrites: true`; `plan_url` stays Read but writes the share window.
Existing plans need a fresh share; reopening enables the
same token URL again. Any token holder with network access during the window
can read and save a plan. There is no separate save login or
write token; revision and body checks do not establish who the writer is. No CORS
headers and Host validation do not prevent a token holder using a non-browser client.

Canvas content is untrusted input that flows back to agents in `plan_diff` text
and saved images. It can carry prompt injection; agents must treat that content
as data, not instructions or permission to act. MCP `plan_diff` requires
`allowWrites: true` because it advances the baseline; HTTP saves do not use that
MCP permission gate. Keep token URLs secret and limit network access. See
[README security model](../../README.md#security-model) and
[SDK licensing and external tracking](../../diagram-page/README.md).

Lookup scans `<data_dir>/diagrams/*/meta.json` for a matching 32-byte
lowercase hexadecimal token and reads the sibling `plan.tldr`. The root and
file purposes are defined in [storage layout](diagram.md#plan_write-and-plan_url).
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
return 404. Rejected saves do not stage or replace files or deliver reports.
Each successful save closes the share window: page, snapshot, and save routes
return 404 until `plan_url` reopens the same token URL.

`plan_write` captures an absolute `RALPH_INBOX` directory in sibling
`notify.json` (`{"inbox":"<directory>"}`), never in strict `meta.json`.
Without that environment variable, no notify file remains. The daemon reads
this persisted target, not its own environment. A save computes the diff from
`baseline.tldr` with the core diff engine and calls core `canvas_report` with
the plan ID, new revision, diff, absolute `latest.png` path, and shell clock
timestamp. It publishes one Ralph Report (`worker="canvas:<plan-id>"`, `at`,
`status="done"`, `text`) through a hidden inbox temp
file and rename, then advances `baseline.tldr` and closes the share window.
Unchanged saves after reopening still deliver one report. Without a target,
save succeeds, logs `no notify target`, advances the baseline, and closes access.
`plan_diff` after a successful save therefore returns an empty diff.

Pure save routing, size, body, and revision decisions live in core `plan_store`.
The shell decodes base64, stages and syncs the files, then renames each.
Replacement is atomic per file, not a multi-file transaction: a crash or rename
failure can leave mixed versions. A daemon mutex serializes saves, including
revision reads, on blocking workers. External `plan_write` processes and other
daemons are not serialized; use one daemon and avoid simultaneous external writes.
Retention runs at startup and periodically until idle or signal shutdown.

Failing-first: `cargo test -p mcptools --test plan_edit_acceptance save_ --
--test-threads=1` ran the four newly un-ignored tests before implementation.
All four failed because the endpoint returned 405 instead of 200/409/413/400.
Acceptance assertions were not changed. The same command passed all four after
implementation; pure size-boundary, routing, body, and revision tests also passed.
Final foreground `RUST_TEST_THREADS=1 cargo xtask lint` passed formatting,
check, clippy, all tests, machete, and typos. The added concurrent-save test
observed one 200 and one 409 for competing saves with the same revision.
Under the single-save window rule, the competing request now returns 404.

The embedded page build and limitations are in `diagram-page/README.md`.
GUZ-1131 supplies plan writes and URLs. The daemon integration test seeds the
shared storage contract directly and binds port 0.

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
Successful `GET /d/<token>`, snapshot GET, `plan_url`, and `plan_png` reads
atomically replace a `last-access` marker in the plan directory. Marker failures
are logged and never fail the read. Retention uses the newest directory,
snapshot, metadata, or access-marker modification time, selected in core;
plans without a marker retain the previous modification-time behavior.
Open share windows protect plans regardless of age. Closed or expired shares
follow the normal TTL rule. Eligible plans are deleted with their tokens;
this is retention, not token expiry.

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

## Asset MIME regression (GUZ-1183)

The bundled asset extensions checked were html, js, css, json, svg, and woff2;
PNG is also covered for image assets. With nosniff, the SVG icon sprite served
as application/octet-stream did not decode as an image in Chrome 156.

Failing-first verification: `RUST_TEST_THREADS=1 cargo xtask lint`, with
`CHROME` set to installed Chrome for Testing 156.0.8078.47, failed the existing
offline browser test on sprite responses `(200, "application/octet-stream")`.
The compiled core test binary separately failed
`diagram_serve::tests::diagram_assets_have_browser_content_types`: SVG expected
image/svg+xml, got application/octet-stream. After the pure core MIME change,
the same full lint command passed both tests and all checks. The browser test
ran, not skipped: it checks sprite MIME, visible toolbar icon dimensions, and
decoding the actual SVG mask URL. No page assets were rebuilt or browser
downloaded. A real phone was not tested.

## Browser save (GUZ-1137)

Snapshot and revision reads use the same daemon mutex as saves. This does not
serialize `plan_write` in another process or other daemons. The page saves only
on explicit user action; it exports the current page as PNG, posts the document
and revision, and retains the returned revision. A conflict disables Save and
requires a reload, which discards unsaved edits. The browser validates revision
values, handles HTTP outcomes, and encodes the PNG; storage rules remain in core.
After a save, ask the agent to reopen with `plan_url` before saving again.


## On-demand diagram server

`plan_write` and `plan_url` start `mcptools diagram serve --tailscale` when the discovered endpoint is unavailable. The daemon uses the MCP data directory, port `MCPTOOLS_DIAGRAM_PORT` (default 5199), and writes `diagrams/daemon.log`. A failed start returns the URL with a warning. The server checks for open shares every `MCPTOOLS_DIAGRAM_IDLE_CHECK_SECONDS` (default 60) and exits when no shares have been open for `MCPTOOLS_DIAGRAM_IDLE_GRACE_SECONDS` (default 120). Retention runs at startup and while the server runs.

No boot-time systemd or launchd service is required. `plan_close`, `plan_diff`,
and `plan_png` do not start the daemon. An open share prevents idle shutdown;
an open browser tab alone does not. Idle grace restarts after shares close or
expire; shutdown occurs on a subsequent check, not at an exact deadline.
The readiness probe requests `/` without a proxy and expects 404 plus the
daemon's nosniff and no-referrer headers. Startup waits up to three seconds.

If a custom boot-time service was installed previously, inspect its identity
and disable that service through launchd or systemd. This repository supplies
no boot-service definition; do not assume a service name or remove unrelated agents.

The address record `diagrams/.daemon-address!` can be stale. Host and port environment overrides take precedence independently. The daemon and MCP process must use the same data directory.

### Configuration

| Setting | Effect and default |
|---------|--------------------|
| `--host <IP>` | Manual bind to a specific IP; no default. Does not enforce tailnet membership. |
| `--tailscale` | Select the local Tailscale IPv4 address instead of `--host`. Requires the `tailscale` executable. |
| `--port <u16>` | Manual listener port, default 5199; 0 selects an ephemeral port. |
| `MCPTOOLS_DIAGRAM_HOST` | URL and readiness-probe host override; does not change the spawned daemon's Tailscale bind. |
| `MCPTOOLS_DIAGRAM_PORT` | URL/probe port override and on-demand `--port`. The spawned daemon defaults to 5199; URLs fall back through the address record first. Does not set a manual serve command's port. |
| `MCPTOOLS_DIAGRAM_SHARE_SECONDS` | Share TTL, default 1800; unsigned seconds, clamped to 60–86400. Per-call `share_seconds` wins. Invalid values fail share creation. |
| `MCPTOOLS_DIAGRAM_IDLE_CHECK_SECONDS` | Share inspection interval, default 60; zero or invalid values use the default. |
| `MCPTOOLS_DIAGRAM_IDLE_GRACE_SECONDS` | Idle grace, default 120; zero or invalid values use the default. |
| `MCPTOOLS_DIAGRAM_TTL_SECONDS` | Retention age, default 2592000 (30 days); unsigned seconds, zero allowed. Invalid values stop retention, not HTTP service, and log an error. |
| `XDG_DATA_HOME` | Nonempty override for the data root; see [storage](diagram.md#plan_write-and-plan_url). Must match between tool process and daemon. |
| `RALPH_INBOX` | Optional absolute inbox captured on `plan_write`; not read from the daemon environment during save. |

The retention shell reads each plan's `share.json` and passes its window to
the pure core selector. Open windows exempt plans from deletion, even without
reads. Missing or invalid windows are closed; other read errors stop the sweep.
Browser tabs alone are not exemptions. Successful page/snapshot GETs and
`plan_url`/`plan_png` calls refresh activity; an idle tab does not poll. The sweep
rechecks share state, metadata, and timestamps before deletion, and skips invalid
layouts or symlinks. A share opened before that recheck protects the plan.

## Offline assets and license position

The embedded page uses bundled tldraw 5.6.0 fonts, icons, and translations via
`getAssetUrlsByImport`; the daemon serves these from `/assets/`. Ordinary canvas
loading does not need an external asset host, but it still needs connectivity
to the daemon over the tailnet. This is not an offline local-storage editor.

The SDK retains its license validation and watermark behavior, including the
conditional request to
`https://cdn.tldraw.com/5.6.0/watermarks/watermark-track.svg` with tracking query
parameters. Thus a strict zero-external-request claim is not supported.
`plan_offline_acceptance.rs` allows only that exact external origin/path;
loopback development can make no tracking request. No SDK license key is
configured by the page. mcptools is MIT licensed; that does not grant a tldraw
production license. The operator must resolve SDK licensing before production
use; no production entitlement is established by this repository. See the
[page license note](../../diagram-page/README.md#sdk-licensing-and-network-access).

## Troubleshooting

| Symptom | Check or recovery |
|---------|-------------------|
| Tool returns `warning: "diagram server unavailable"` | Read `<data_dir>/diagrams/daemon.log`. Run `tailscale ip -4` in the tool process's environment. Check host/port overrides and the recorded socket. The stored plan/share can exist despite startup failure; retry `plan_url` after fixing connectivity. |
| Daemon exits without serving | Check whether the port is already occupied: bind returns success without starting another listener on address-in-use. For visible startup errors, run `mcptools diagram serve --tailscale --port 5199` in the foreground with the same data root. |
| Phone cannot connect | Connect it to the same tailnet; check the server's Tailscale address, listener port, and ACL. A loopback URL is local to the phone, not the server. `MCPTOOLS_DIAGRAM_HOST` is not a bind flag. |
| HTTP 403 | Use the literal bound IP and port. DNS aliases, missing/duplicate Host headers, and proxy Host rewrites fail validation. |
| HTTP 404 or expired-link message | Call `plan_url` to reopen the share; saves, `plan_close`, and expiry close access. If retention deleted the plan, recreate it with `plan_write`. Reopening reuses the old token, not a new secret. |
| Save 409 | Another write changed the revision. Reload to get the current snapshot; reload discards unsaved edits. |
| Save 400 or 413 | 400 means invalid save JSON, snapshot envelope, or PNG; 413 means the total JSON body, including base64 PNG, exceeds 10 MiB. Reduce canvas/image size before retrying. |
| Save 500 or no inbox report | Inspect daemon logs and file permissions; verify the absolute inbox was set on the writer's `plan_write`. No notify target is a successful save with a log message. An I/O failure can leave mixed files; do not assume a failed save changed nothing. |
| `plan_png` fails or diff is empty | PNG exists only after a browser save. Successful save already advances the baseline, so a later `plan_diff` is empty. Without a saved baseline, a save compares against the stored pre-save plan. |
| Retention does not run | Check `MCPTOOLS_DIAGRAM_TTL_SECONDS` and retention logs. Sweeps run immediately and hourly only while the daemon runs; malformed plans and unsafe layouts are skipped. |
