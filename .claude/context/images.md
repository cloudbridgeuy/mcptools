# Images: Generation via Subscription or API Key

Generates, edits, and varies images with `gpt-image-2.5-flare`, `gpt-image-2.5-sunburst`, `gpt-image-2.6-flare` (default), and `gpt-image-2.6-sunburst`. The 2.6 models work with both the ChatGPT subscription and the API key (`--api openai`, MCP: `api=openai`). Two backends: the ChatGPT subscription (default, `--api chatgpt`) through the Codex Responses backend with the `image_generation` tool, and the metered OpenAI Image API (`--api openai`) with `OPENAI_API_KEY`.

Subscription auth reuses llm-stream credentials: `mcptools` reads `<config_dir>/auth.json` (default `~/.config/llm-stream`), refreshes the access token when near expiry, and writes back with mode `0600`. First sign-in happens via `llm-stream --login`; there is no `images login` command.

## CLI Usage

```bash
# Generate (subscription, opens result in Preview on macOS)
mcptools images generate "a fox reading a newspaper, watercolor" --quality low -o /tmp/fox.png --open

# Landscape with JSON output
mcptools images generate "neon tokyo alley at night" --size 1536x1024 --quality medium --json

# Edit with reference images and optional mask
mcptools images edit "make the fox wear glasses" --image /tmp/fox.png -o /tmp/fox-glasses.png
mcptools images edit "replace sky with sunset" --image in.png --mask mask.png --quality high -o out.png

# Variation anchored to reference images (default prompt preserves subject/style)
mcptools images vary --image /tmp/fox.png --quality low -o /tmp/fox-vary.png
mcptools images vary --image /tmp/fox.png --prompt "same fox, cyberpunk style" -o /tmp/fox-cyber.png

# Metered API path (multiple images per call)
mcptools images generate "a lighthouse" --api openai --model gpt-image-2.6-sunburst -n 2 --output-dir /tmp
```

Output: saved file paths, one per line (or JSON metadata with `--json`). A cyan spinner ticks on stderr during the request. `--open` launches the system viewer after saving. `--preview` displays each saved PNG inline in Kitty or Ghostty when stdout is a terminal and `--json` is absent. In tmux, enable passthrough with `set -g allow-passthrough on`. Other formats and unsupported output skip the preview with a notice; saved files remain available. `-n` above 1 works only with `--api openai`; the subscription returns one image per call.

## MCP Tools

Tool names: `images_generate`, `images_edit`, `images_vary`

| Argument | Type | Required | Default |
|----------|------|----------|---------|
| `prompt` | string | yes (generate, edit) | — |
| `images` | array of string | yes (edit, vary) | — |
| `mask` | string | no | — |
| `model` | string | no | `gpt-image-2.6-flare` |
| `size` | string | no | `auto` |
| `quality` | string | no | `auto` |
| `outputFormat` | string | no | `png` |
| `outputCompression` | integer | no | — |
| `background` | string | no | `auto` |
| `moderation` | string | no | — |
| `inputFidelity` | string | no | — |
| `n` | integer | no | `1` |
| `outputDir` | string | no | `.` |
| `api` | string | no | `chatgpt` |
| `configDir` | string | no | `~/.config/llm-stream` |
| `mainline` | string | no | `gpt-5.6-sol` |

Size accepts `auto`, `1024x1024`, `1536x1024`, `1024x1536`, or custom `WIDTHxHEIGHT` (multiples of 16). Quality accepts `auto`, `low`, `medium`, `high`, `xhigh`, `max`. Returns saved paths plus model/size/quality metadata as text JSON (no base64 inline).

## Architecture

Follows the Functional Core - Imperative Shell pattern:

- **Core** (`crates/core/src/images.rs`): Pure functions — option validation (`validate_size`, `validate_quality`, `validate_n`, ...), Image API body builder (`build_generate_body`), Responses tool/body builders (`build_image_tool`, `build_responses_body`), response extractors (`extract_image_b64`, `extract_stream_image`), SSE parsing (`parse_sse_payloads`), JWT expiry classification (`classify_tokens`), config dir resolution (`resolve_config_dir`), response types (`ImagesResponse`, `TokenSet`)
- **Shell** (`crates/mcptools/src/images/`): `mod.rs` — CLI, dual-backend dispatch (`Backend::ChatGpt` / `Backend::OpenAi`), Codex SSE transport, multipart edit uploads, file output, spinner, `--open`; `auth.rs` — credential load/save (0600) and OAuth refresh against `auth.openai.com`
- **MCP** (`crates/mcptools/src/mcp/tools/images.rs`): Tool handlers bridging MCP to the shell data functions

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `MCPTOOLS_IMAGES_API` | `chatgpt` | Backend: `chatgpt` (subscription) or `openai` (API key) |
| `LLM_STREAM_CONFIG_DIR` | `~/.config/llm-stream` | Config dir holding subscription `auth.json` |
| `MCPTOOLS_IMAGES_MAINLINE` | `gpt-5.6-sol` | Chat model fronting the image tool (subscription) |
| `OPENAI_IMAGES_MODEL` | `gpt-image-2.6-flare` | Image model |
| `OPENAI_API_KEY` | — | Key for `--api openai` |
| `OPENAI_BASE_URL` | `https://api.openai.com/v1` | Base URL for `--api openai` |

## Notes

- The subscription request sends the requested image model unchanged in the image tool. The subscription accepted the default gpt-image-2.6-flare in a live generate run on 2026-11-05.
- There is no variations endpoint for GPT image models; `vary` is `edit` with a subject-preserving default prompt.
- The Codex backend requires streaming; the shell sends `stream: true` and reads the image from `output_item.done` events (`response.completed` carries empty output there).

Markup edits: CLI `images edit --markup PATH` and MCP `images_edit` accept a transparent PNG overlay with `api=openai`. It must match the first reference image dimensions. Each of those images is limited to 16,777,216 pixels; markup is limited to 50,000,000 bytes. Markup and mask cannot be combined. Other reference images remain supported. ChatGPT subscription edits reject markup before authentication or requests. GPT Image 2.5 and 2.6 support markup.

Example: `mcptools images edit "Change the marked area" --image input.png --markup overlay.png --api openai`. Validation reads image headers and does not decode image data or inspect transparency. The API validates image content and transparency.
