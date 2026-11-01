# Plan acceptance fixtures

`spike.tldr` is an unchanged copy of `/tmp/diagram-spike/plan.tldr`.
`spike.short.json` transcribes the six nodes and six edges in that spike's
`gen.mjs`. `spike.positions.json` transcribes its coordinates and dimensions.
These are spike-derived fixtures, not synthesized fallback fixtures.

The spike uses tldraw 5.6.0 and validates records with `createTLSchema` and
`Store.put`. The expansion golden compares parsed JSON, including the complete
schema, fractional indices, rich text, flip flags, and arrow binding records.
Positions are supplied separately so expansion does not prescribe a layout
algorithm. Layout tests independently require deterministic bytes, non-overlap,
and preservation of existing positions.

Shell tests isolate HOME and XDG_DATA_HOME. Storage assertions use
`$XDG_DATA_HOME/mcptools/diagrams/<id>/`, resolving the design's unspecified
`<data_dir>` with the XDG convention. Route tests use the token path returned
by plan_write and an ephemeral loopback daemon port; they do not need Tailscale
or assume a configurable public URL. Hex and unpadded base64url encodings of
the 128-bit token are accepted. Tests do not attempt to prove RNG entropy.
