# Images: ChatGPT Images 2.5 Generation via Subscription or API Key

Generates, edits, and varies images with the GPT Image 2.5 models (`gpt-image-2.5-flare` default, `gpt-image-2.5-sunburst` for premium precision). Two backends: the ChatGPT subscription (default, `--api chatgpt`) through the Codex Responses backend with the `image_generation` tool, and the metered OpenAI Image API (`--api openai`) with `OPENAI_API_KEY`.

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
mcptools images generate "a lighthouse" --api openai --model gpt-image-2.5-sunburst -n 2 --output-dir /tmp
```

Output: saved file paths, one per line (or JSON metadata with `--json`). A cyan spinner ticks on stderr during the request. `--open` launches the system viewer after saving. `-n` above 1 works only with `--api openai`; the subscription returns one image per call.

## MCP Tools

Tool names: `images_generate`, `images_edit`, `images_vary`

| Argument | Type | Required | Default |
|----------|------|----------|---------|
| `prompt` | string | yes (generate, edit) | — |
| `images` | array of string | yes (edit, vary) | — |
| `mask` | string | no | — |
| `model` | string | no | `gpt-image-2.5-flare` |
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
| `OPENAI_IMAGES_MODEL` | `gpt-image-2.5-flare` | Image model |
| `OPENAI_API_KEY` | — | Key for `--api openai` |
| `OPENAI_BASE_URL` | `https://api.openai.com/v1` | Base URL for `--api openai` |

## Notes

- The subscription backend normalizes the requested image model to its own managed model; `--model` fully applies only to `--api openai`.
- There is no variations endpoint for GPT image models; `vary` is `edit` with a subject-preserving default prompt.
- The Codex backend requires streaming; the shell sends `stream: true` and reads the image from `output_item.done` events (`response.completed` carries empty output there).
