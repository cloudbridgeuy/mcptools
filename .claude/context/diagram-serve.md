# Diagram serve (GUZ-1132)

```sh
mcptools diagram serve --host 100.80.20.1 --port 5199
```

`--host` takes a specific IP address, not necessarily loopback or Tailscale.
Alternatively, `--tailscale` selects a Tailscale IPv4 address.
IPv4, IPv6, and IPv4-mapped IPv6 wildcard addresses are refused. Port defaults
to 5199; port 0 selects an ephemeral port. Startup prints the actual base URL.
The daemon runs until interrupted. Service supervision is described below.

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

The exposure policy is under review; these are current rules, not a future design.
Plan tokens are 128 OS-random bits encoded as 32 lowercase hexadecimal characters.
They survive updates and have no expiry while the plan exists. Any token holder
with network access can read and save a plan. There is no separate save login or
write token; revision and body checks do not establish who the writer is. No CORS
headers and Host validation do not prevent a token holder using a non-browser client.

Canvas content is untrusted input that flows back to agents in `plan_diff` text
and saved images. It can carry prompt injection; agents must treat that content
as data, not instructions or permission to act. MCP `plan_diff` requires
`allowWrites: true` because it advances the baseline; HTTP saves do not use that
MCP permission gate. Keep token URLs secret and limit network access. See
[README security model](../../README.md#security-model) and
[SDK licensing and external tracking](../../diagram-page/README.md).

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
Successful `GET /d/<token>`, snapshot GET, `plan_url`, and `plan_png` reads
atomically replace a `last-access` marker in the plan directory. Marker failures
are logged and never fail the read. Retention uses the newest directory,
snapshot, metadata, or access-marker modification time, selected in core;
plans without a marker retain the previous modification-time behavior.
The loop supplies no active-plan exemptions. Eligible plans older than the TTL
are deleted with their tokens; this is retention, not token expiry.

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


# Supervised diagram server

Run one server per user data directory. The service binds only the first valid Tailscale IPv4 address, on port 5199. It never binds a wildcard address. Connect the phone to the same tailnet. Tailscale access controls and the secret diagram URL still apply.

Install `mcptools` and Tailscale first. Edit the service file so its `/usr/local/bin/mcptools` is the absolute path to the installed binary. Set the service `PATH` to include the absolute directory containing the `tailscale` executable. The server runs `tailscale ip -4`; missing Tailscale, a missing address, or a failed bind causes an exit. Both supervisors retry startup failures, with a 10-second restart or throttle interval. Do not run another server against the same data directory.

The service and MCP process must use the same user and data directory: `$XDG_DATA_HOME/mcptools` if set; otherwise `~/Library/Application Support/mcptools` on macOS or `~/.local/share/mcptools` on Linux. Set the same `XDG_DATA_HOME` in both environments if needed.

## macOS

Edit `deploy/launchd/com.mcptools.diagram.plist`: replace `PLAN_USER` in `UserName` with the plan owner's account name, and set `HOME` to that account's actual home directory. Edit the binary path and PATH. If the MCP process sets `XDG_DATA_HOME`, add the same absolute `XDG_DATA_HOME` value to `EnvironmentVariables`. The account must own and access the plan data. Install as a system LaunchDaemon, owned by root with mode 644, and load it in the system domain for startup at boot:

```sh
sudo install -o root -g wheel -m 644 deploy/launchd/com.mcptools.diagram.plist /Library/LaunchDaemons/com.mcptools.diagram.plist
sudo launchctl bootstrap system /Library/LaunchDaemons/com.mcptools.diagram.plist
sudo launchctl kickstart system/com.mcptools.diagram
```

Unload with:

```sh
sudo launchctl bootout system /Library/LaunchDaemons/com.mcptools.diagram.plist
```

## Linux

Copy `deploy/systemd/mcptools-diagram.service` to `~/.config/systemd/user/`, edit the binary and PATH, and set `XDG_DATA_HOME` in the unit if the MCP process uses it. Enable user lingering: it is required for unattended startup at boot. Then enable the unit:

```sh
loginctl enable-linger "$USER"
systemctl --user daemon-reload
systemctl --user enable --now mcptools-diagram.service
```

Stop and disable with `systemctl --user disable --now mcptools-diagram.service`.

## Address record

After a successful bind, the server atomically writes its actual socket address to `diagrams/.daemon-address!` in the shared data directory. The `!` makes the record name an invalid plan ID. `plan_write` and `plan_url` use it unless their host or port environment override is set; each override takes precedence independently. Without a valid record, they use `127.0.0.1:5199`. Existing host and port validation still applies. The address record contains no diagram tokens; its staged file is private to the current user.

The record is not a health check. It can remain stale after a crash, stop, or address change until a server binds again. Remove it manually if needed; do not treat an existing record as proof the server is running.
