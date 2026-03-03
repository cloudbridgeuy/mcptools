# UI Annotations

Integrates with a calendsync dev server overlay that lets developers annotate UI elements directly in the browser. Annotations capture element selectors, computed styles, bounding boxes, and freeform notes.

## Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `CALENDSYNC_DEV_URL` | Dev server base URL | `http://localhost:3000` |

All tools accept an optional `url` parameter that overrides the environment variable.

## MCP Tools

### `ui_annotations_list`

List all annotations from the dev server. Returns selector, component name, note, and resolution status.

### `ui_annotations_get`

Get a single annotation by ID with full details: computed styles, bounding box, position, and optional screenshot.

**Required:** `id` (annotation ID)

### `ui_annotations_resolve`

Mark an annotation as resolved with a summary of changes made.

**Required:** `id`, `summary`

### `ui_annotations_clear`

Delete all annotations from the dev server.

## API Endpoints

The tools communicate with the dev server's annotation API:

| Method | Endpoint | Tool |
|--------|----------|------|
| GET | `/_dev/annotations` | `ui_annotations_list` |
| GET | `/_dev/annotations/:id` | `ui_annotations_get` |
| PATCH | `/_dev/annotations/:id/resolve` | `ui_annotations_resolve` |
| DELETE | `/_dev/annotations` | `ui_annotations_clear` |

## Architecture

- **Core** (`crates/core/src/annotations.rs`): Types (`DevAnnotation`, `BoundingBox`, `ComputedStyles`, `AnnotationSummary`) and pure formatting functions for list and detail views. Fully tested.
- **Shell** (`crates/mcptools/src/mcp/tools/annotations.rs`): HTTP handlers that fetch from the dev server and delegate formatting to core.

### Serde Field Mapping

`DevAnnotation` uses `#[serde(rename)]` to map wire-format names to readable Rust names:

| Rust field | API field | Note |
|------------|-----------|------|
| `selector` | `element_path` | CSS selector / DOM path |
| `note` | `comment` | Freeform annotation text |
| `resolved` | `is_fixed` | Resolution status |

`AnnotationSummary` has `pending`, `acknowledged`, `resolved`, `dismissed` counts from the API. The `unresolved()` method computes `total - resolved` (the API does not provide this field directly).
