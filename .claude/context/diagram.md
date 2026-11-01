# Plan diagrams

## plan_write and plan_url

`plan_write {id, nodes: [{id, text}], edges: [{from, to}]}` is a destructive
Write tool; direct calls, `call_tool`, and `execute` require `allowWrites: true`.
It returns `{url, rev}`. Duplicate node IDs, unknown edge endpoints, and blank
text fail before any storage writes. Plan IDs are single ASCII names using
letters, digits, `-`, `_`, and `.`; `.` and `..` are rejected.

`plan_url {id}` is Read and returns `{url}`. Missing plans fail without writes.
Both tools use the same persisted token. Updates retain existing node positions,
retain the token, and increment `rev` from its initial value of 1. Revision
overflow or invalid stored metadata fails without replacing the plan.

Storage is `<data_dir>/diagrams/<id>/{plan.tldr, meta.json}`. `data_dir` is
`$XDG_DATA_HOME/mcptools` when set and nonempty; otherwise it is
`~/Library/Application Support/mcptools` on macOS, `$APPDATA/mcptools` on
Windows, or `~/.local/share/mcptools` on other platforms. No prior data-directory
helper exists in this workspace.

Metadata is `{"id":"<id>","token":"<32 lowercase hex characters>","rev":1}`;
`rev` is a u64. The shell obtains 16 bytes from the OS RNG; pure core functions
format the token, build metadata, increment revisions, extract prior positions,
and build URLs. Existing `rand` and `tempfile` dependencies suffice.

URLs are `http://<host>:<port>/d/<token>`. The approved design defines no config
keys for the endpoint, so `MCPTOOLS_DIAGRAM_HOST` and `MCPTOOLS_DIAGRAM_PORT`
are used, defaulting to `127.0.0.1` and `5199`. IPv6 hosts are bracketed in URLs.
Set the host to the daemon's reachable Tailscale IP for phone access.

Both JSON files are staged and synced before replacement with temp-file rename.
Replacement is atomic per file, not a two-file transaction. Simultaneous writers
are not serialized; callers must use one writer per plan. Daemon implementation,
canvas saves, notifications, and retention are outside GUZ-1131.

## GUZ-1131 verification

Before implementation, the four GUZ-1131 acceptance tests were un-ignored and
run through `RUST_TEST_THREADS=1 cargo xtask lint --no-fmt --no-check
--no-clippy --no-machete --no-typos`. All four failed because `plan_write`
was unknown (the success case lacked structured content). The two GUZ-1132
daemon tests remain ignored and unchanged.

After implementation, `RUST_TEST_THREADS=1 cargo xtask lint` passed: all four
GUZ-1131 acceptance tests and four pure plan-store unit tests passed, and Clippy
passed with `-D warnings`. Tool contracts now cover 108 registered tools.
