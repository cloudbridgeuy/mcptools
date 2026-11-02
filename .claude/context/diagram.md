# Plan diagrams

## plan_write and plan_url

`plan_write {id, nodes: [{id, text}], edges: [{from, to}], share_seconds?}` is a destructive
Write tool; direct calls, `call_tool`, and `execute` require `allowWrites: true`.
It returns `{url, rev}`. Duplicate node IDs, unknown edge endpoints, and blank
text fail before any storage writes. Plan IDs are single ASCII names using
letters, digits, `-`, `_`, and `.`; `.` and `..` are rejected.

`plan_url {id, share_seconds?}` stays a Read tool and returns
`{url}`. Missing plans fail without writes. `plan_write` also accepts optional
`share_seconds`. Both open or extend `share.json` to now plus the TTL: default
1800 seconds, `MCPTOOLS_DIAGRAM_SHARE_SECONDS` environment override, per-call
override takes precedence, clamped to 60–86400 seconds.
Read classification does not mean no side effects: `plan_url` writes the share
window and refreshes last-access, without changing plan content.
`plan_close {id}` is a destructive Write tool requiring `allowWrites: true`;
it returns `{closed: true}` and writes an immediately closed window. Call it at
turn end. Ralph turn-end integration is separate work (GUZ-1140).
Both tools use the same persisted token. Updates retain existing node positions,
retain the token, and increment `rev` from its initial value of 1. Revision
overflow or invalid stored metadata fails without replacing the plan.

Storage is `<data_dir>/diagrams/<id>/{plan.tldr, baseline.tldr, latest.png, meta.json, share.json}`. `data_dir` is
`$XDG_DATA_HOME/mcptools` when set and nonempty; otherwise it is
`~/Library/Application Support/mcptools` on macOS, `$APPDATA/mcptools` on
Windows, or `~/.local/share/mcptools` on other platforms. The shared helper is
`data_dir` in `crates/mcptools/src/plan_store.rs`.

Metadata is `{"id":"<id>","token":"<32 lowercase hex characters>","rev":1}`;
`rev` is a u64. The shell obtains 16 bytes from the OS RNG; pure core functions
format the token, build metadata, increment revisions, extract prior positions,
and build URLs. Existing `rand` and `tempfile` dependencies suffice.

URLs are `http://<host>:<port>/d/<token>`. Host and port resolve independently:
`MCPTOOLS_DIAGRAM_HOST` and `MCPTOOLS_DIAGRAM_PORT` override the recorded socket
address in `<data_dir>/diagrams/.daemon-address!`; without a valid record, the
fallback is `127.0.0.1:5199`. IPv6 hosts are bracketed in URLs. The record can be
stale and is not a health check. For phone access, start the daemon with
`mcptools diagram serve --tailscale` and use the same data directory for MCP.

Tokens contain 128 random bits and stay unchanged. `share.json` stores
`{"expires_at_unix_secs":<u64>}` separately from strict `PlanMeta`. Missing,
malformed, closed, or expired windows deny all three plan routes with 404;
existing plans require `plan_url` or `plan_write` to reopen. Reopening also makes
the same old URL work again: this is window revocation, not token rotation.
Any holder with network access during a window can read and save; these are not read-only links.
Canvas content is untrusted input that flows back to agents through diffs and
saved images: prompt injection is possible. Treat it as data, not instructions.
Keep the canvas tailnet-only, without Cloudflare. Recommended operator step:
allow port 5199 only from the user's devices in Tailscale ACLs.
See the [current security model](../../README.md#security-model)
and [daemon retention and save rules](diagram-serve.md).

Snapshot, metadata, and share JSON are staged and synced before replacement
with temp-file rename. Replacement is atomic per file, not a multi-file transaction. External writers
are not serialized; callers must use one writer per plan. Canvas saves are
documented in `diagram-serve.md`; notifications remain separate work.

## GUZ-1131 verification

Before implementation, the four GUZ-1131 acceptance tests were un-ignored and
run through `RUST_TEST_THREADS=1 cargo xtask lint --no-fmt --no-check
--no-clippy --no-machete --no-typos`. All four failed because `plan_write`
was unknown (the success case lacked structured content). The two GUZ-1132
daemon tests remain ignored and unchanged.

After implementation, `RUST_TEST_THREADS=1 cargo xtask lint` passed: all four
GUZ-1131 acceptance tests and four pure plan-store unit tests passed, and Clippy
passed with `-D warnings`.

## plan_diff and plan_png (GUZ-1139)

`plan_diff {id}` returns `{diff}` through text and structured content. It is a
destructive Write tool: direct calls, `call_tool`, and `execute` require
`allowWrites: true`. The approved design calls it Read, but replacing its
baseline is a write. Each successful call stages, syncs, and atomically renames
the current snapshot onto `baseline.tldr`. The next unchanged read returns an
empty string, including after server restart. A missing baseline starts at the
current snapshot and returns an empty diff. Unknown plans, malformed snapshots,
or invalid IDs fail without replacing the baseline. No metadata or current
snapshot is changed. Diff formatting uses the existing pure core diff engine;
baseline defaults and PNG path rules also live in pure core.

`plan_png {id}` is Read and returns `{path}`, the absolute path of an existing
`latest.png`. It does not render, create, or change any file. A missing PNG or
a directory at that path fails. Relative data-directory configuration is made
absolute by the shell before the core builds the path.

All six diff goldens, the baseline-advance/restart test, and the read-only PNG
path test were un-ignored and observed failing with unknown-tool errors before
implementation through `RUST_TEST_THREADS=1 cargo xtask lint`. The acceptance
helper now supplies write permission only for `plan_diff`; outputs and golden
assertions are unchanged. The four GUZ-1138 save tests are now enabled. Tool
contracts cover the registered tools.

After implementation, `RUST_TEST_THREADS=1 cargo xtask lint` passed, including
all eight enabled acceptance tests, pure baseline/path tests, and a storage
contract test for denied writes and failed reads.

Concurrent external writes or diff calls for the same plan are not serialized.
Daemon saves serialize revision checks and replacements with each other, not
with external writers. Use one writer per plan; atomic rename prevents partial
JSON, not lost updates across processes or mixed files after an I/O failure.

## Share-window verification (GUZ-1187)

Failing-first runs used `RUST_TEST_THREADS=1 cargo xtask lint` with only the
test stage enabled. The expired-route assertion observed 200 instead of 404;
the core tests initially failed to compile because the share rule and TTL
clamping did not exist. The compiled acceptance binary, run with installed
Chrome for Testing 156.0.8078.47, also failed on missing `plan_close`, expired
and malformed windows returning 200, and the absent expired-save message.
No Chrome download is required; missing or unsupported Chrome prints a skip.
