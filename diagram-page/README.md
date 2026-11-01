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
records to a tldraw document store, loads it, and fits the camera. Load errors
appear in an accessible alert. Edits are local to this tab and are not saved.
Save routes belong to the next epic.

tldraw 5.6.0 still fetches fonts, icons, and translations from its CDN. Offline
assets belong to the unattended/offline epic in the approved design. The HTML
and server use a no-referrer policy to keep the token out of CDN requests.
The approved HTTP deployment keeps tldraw's unlicensed watermark; the owner
must resolve licensing before production use.

`include_dir` 0.7.4 embeds the complete directory at Rust compile time. Neither
candidate embed crate existed in the workspace. `include_dir` needs only its
macro crate, supports static byte slices, and does not add hashing or runtime
filesystem fallback. A single binary serves the checked-in build.
