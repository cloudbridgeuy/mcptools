# PDF Document Navigation

Converts PDF files into navigable tree structures for AI agents. Agents fetch the table of contents, pick a section, read its content, and request images individually — without consuming the entire document.

## Architecture

The `crates/pdf` crate is the Functional Core (pure, no I/O). The shell lives in `crates/mcptools/src/pdf/` (CLI) and `crates/mcptools/src/mcp/tools/pdf.rs` (MCP).

Key modules:
- `parser/backend.rs` — lopdf wrapper, `PdfBackend` trait
- `parser/layout.rs` — text extraction, font-size-based heading detection
- `parser/table.rs` — spatial alignment table detection
- `tree.rs` — stack-based nesting algorithm, builds `DocumentTree`
- `images.rs` — image extraction, format detection, raw-to-PNG re-encoding, CCITT fax decoding
- `render/markdown.rs` — section content to Markdown
- `render/cleanup.rs` — text normalization (ligatures, hyphenation, CJK)
- `lib.rs` — public API: `ParsedDocument`, `parse()`, `read_section()`, `peek_section()`, `list_section_images()`, `get_image()`, `info()`, `extract_window()`

## CLI Commands

### Table of Contents

```bash
mcptools pdf toc document.pdf
```

Returns the full document tree as JSON with section IDs, headings, content previews, image counts, and page ranges.

### Read a Section

```bash
# Read a specific section
mcptools pdf read document.pdf s-1-0

# Read the whole document
mcptools pdf read document.pdf
```

Returns the section content as rendered Markdown with image references. Section IDs come from the `pdf toc` output (format: `s-{depth}-{index}`). Omit the section ID to read the entire document.

### Peek at a Section

```bash
# Peek at beginning of a section (default)
mcptools pdf peek document.pdf s-1-0

# Peek at middle of whole document with custom limit
mcptools pdf peek document.pdf --position middle --limit 300

# Random sample from a section
mcptools pdf peek document.pdf s-1-0 --position random --limit 200
```

Samples a text snippet from a section without reading the full content. Returns the snippet, the position it was taken from, and total character count. Useful for quickly assessing content before committing to a full read.

Options:
- `--position` / `-p`: Where to sample from — `beginning` (default), `middle`, `ending`, `random`
- `--limit` / `-l`: Maximum characters to return (default: 500)

### List Images

```bash
# List all images in the document
mcptools pdf images document.pdf

# List images in a specific section
mcptools pdf images document.pdf s-1-0
```

Returns image IDs, formats, section IDs, section titles, and page numbers for all images in the specified scope.

### Extract an Image

```bash
# Save to file by ID
mcptools pdf image document.pdf Im1 --output photo.jpg

# Print base64 to stdout
mcptools pdf image document.pdf Im1

# Random image from the document
mcptools pdf image document.pdf --random

# Random image from a specific section
mcptools pdf image document.pdf --random --section s-1-0
```

Image IDs are XObject names from the PDF (visible in section image references and `pdf images` output). All image formats are supported for export: JPEG and JPEG2000 images are extracted as-is, while FlateDecode (raw pixel data) and CCITTFaxDecode (fax) images are automatically re-encoded as PNG.

Options:
- `--output` / `-o`: Save to file instead of printing base64
- `--section` / `-s`: Scope image selection to a section (used with `--random`)
- `--random` / `-r`: Pick a random image (cannot be used with an image ID)

### Document Info

```bash
mcptools pdf info document.pdf
```

Returns title, author, page count, and creator.

## Best Practice Workflow

1. **Get document structure:**
   ```bash
   mcptools pdf toc document.pdf
   ```

2. **Peek at sections to assess content:**
   ```bash
   mcptools pdf peek document.pdf s-1-0
   ```

3. **Read specific sections of interest:**
   ```bash
   mcptools pdf read document.pdf s-1-0
   ```

4. **List and extract images:**
   ```bash
   mcptools pdf images document.pdf s-1-0
   mcptools pdf image document.pdf Im1 --output image.jpg
   ```

### Filtering Decorative Images

PDFs frequently reuse decorative images (logos, backgrounds, page headers) across many pages. When `pdf_images` returns results, the same image ID appearing on multiple pages is almost certainly decorative. To find meaningful content images (screenshots, diagrams, charts):

1. **List images for a section** — scope with a `sectionId` to reduce noise.
2. **Identify recurring IDs** — image IDs that appear on nearly every page (e.g., a company logo or page background) are decorative. Ignore these.
3. **Extract unique IDs** — images that appear only within the target section are the actual content. These are the screenshots, diagrams, and figures worth extracting.

Example: a section spanning pages 27-29 returns 9 images (3 per page). Two IDs repeat on every page (logo + background) — skip those. The 3 unique IDs are the actual screenshots.

## MCP Tools

### pdf_toc

```json
{
  "method": "tools/call",
  "params": {
    "name": "pdf_toc",
    "arguments": { "path": "/absolute/path/to/document.pdf" }
  }
}
```

**Arguments:**
- `path` (required): Absolute path to the PDF file

### pdf_read

```json
{
  "method": "tools/call",
  "params": {
    "name": "pdf_read",
    "arguments": {
      "path": "/absolute/path/to/document.pdf",
      "sectionId": "s-1-0"
    }
  }
}
```

**Arguments:**
- `path` (required): Absolute path to the PDF file
- `sectionId` (optional): Section ID from `pdf_toc` (e.g., `s-1-0`). Omit for whole document.

### pdf_peek

```json
{
  "method": "tools/call",
  "params": {
    "name": "pdf_peek",
    "arguments": {
      "path": "/absolute/path/to/document.pdf",
      "sectionId": "s-1-0",
      "position": "middle",
      "limit": 300
    }
  }
}
```

**Arguments:**
- `path` (required): Absolute path to the PDF file
- `sectionId` (optional): Section ID from `pdf_toc`. Omit for whole document.
- `position` (optional): Where to sample — `beginning` (default), `middle`, `ending`, `random`
- `limit` (optional): Maximum characters to return (default: 500)

### pdf_images

```json
{
  "method": "tools/call",
  "params": {
    "name": "pdf_images",
    "arguments": {
      "path": "/absolute/path/to/document.pdf",
      "sectionId": "s-1-0"
    }
  }
}
```

**Arguments:**
- `path` (required): Absolute path to the PDF file
- `sectionId` (optional): Section ID from `pdf_toc`. Omit for all images.

### pdf_image

```json
{
  "method": "tools/call",
  "params": {
    "name": "pdf_image",
    "arguments": {
      "path": "/absolute/path/to/document.pdf",
      "imageId": "Im1"
    }
  }
}
```

**Arguments:**
- `path` (required): Absolute path to the PDF file
- `imageId` (optional): Image ID (XObject name). Required unless `random` is true.
- `sectionId` (optional): Section ID to scope image selection (used with `random`)
- `random` (optional): Pick a random image. Cannot be used with `imageId`.

Returns base64-encoded image data with format and size.

### pdf_info

```json
{
  "method": "tools/call",
  "params": {
    "name": "pdf_info",
    "arguments": { "path": "/absolute/path/to/document.pdf" }
  }
}
```

**Arguments:**
- `path` (required): Absolute path to the PDF file

## Domain Types

- `SectionId` — validated format `s-{depth}-{index}` (e.g., `s-1-0`, `s-2-3`)
- `HeadingLevel` — 1 through 6
- `ImageId` — XObject name string
- `ImageFormat` — Jpeg, Png, Jpeg2000, Gif, Tiff, Bmp, WebP, Unknown
- `EnrichedImageRef` — image ID, format, section ID, section title, page number (returned by `list_section_images`)
- `PeekPosition` — Beginning, Middle, Ending, Random
- `PeekContent` — snippet with position, total chars, section info
- `DocumentTree` — nested sections with metadata and flat index
- `SectionContent` — rendered Markdown text with image references
- `ParsedDocument` — holds intermediate state for efficient repeated queries

## Heading Detection

Headings are detected by font size analysis:
1. Build a histogram of font sizes across all text spans
2. The most frequent size is the body baseline
3. Text larger than baseline + 1.5pt is classified as a heading
4. Heading levels are assigned by size rank (largest = H1)

If no headings are found, sections are created per-page as a fallback.
