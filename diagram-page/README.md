# Diagram page

The daemon embeds `dist/`. Node is required only to rebuild the page, not to
compile or run mcptools. Use Node 22.12+ (or Node 20.19+).

```sh
cd diagram-page
npm ci
npm run build
```

Commit source, lockfile, and the rebuilt `dist/` together. `package.json` contains
the build script. Vite emits hashed JavaScript and CSS under `/assets/`.
Generated bundles and the npm lockfile are excluded from the workspace typo
check in `typos.toml`; source remains checked. Cargo machete checks Rust
dependencies and requires no bundle exclusion.

The page fetches `/d/<token>/plan.tldr` from the same origin, converts `.tldr`
records to a tldraw document store, loads it, and fits the camera. The response
includes the matching revision in `X-Plan-Rev`. The Save button exports the current
page to PNG and sends the document, PNG, and revision to the same-origin save
route. There is no autosave. A successful save updates the revision. Errors are
shown on the page. A conflict disables Save; reload to see the newer plan, which
discards unsaved edits. Exports contain the current page, not all pages.

tldraw 5.6.0 fonts, icons, and translations are bundled for same-origin use.
The SDK retains its original license tracking request when applicable, at
`https://cdn.tldraw.com/5.6.0/watermarks/watermark-track.svg` with query parameters.
License validation and watermark rendering remain unchanged. This does not grant
a production license; the owner must resolve licensing before production use.

`include_dir` 0.7.4 embeds the complete directory at Rust compile time. Neither
candidate embed crate existed in the workspace. `include_dir` needs only its
macro crate, supports static byte slices, and does not add hashing or runtime
filesystem fallback. A single binary serves the checked-in build.
