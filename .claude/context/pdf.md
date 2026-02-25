# PDF Document Navigation

Converts PDF files into navigable tree structures for AI agents. Agents fetch the table of contents, pick a section, read its content, and request images individually — without consuming the entire document.

## Architecture

The `crates/pdf` crate is the Functional Core (pure, no I/O). The shell lives in `crates/mcptools/src/pdf/` (CLI) and `crates/mcptools/src/mcp/tools/pdf.rs` (MCP).

Key modules:
- `parser/backend.rs` — lopdf wrapper, `PdfBackend` trait
- `parser/layout.rs` — text extraction, font-size-based heading detection
- `parser/table.rs` — spatial alignment table detection
- `tree.rs` — stack-based nesting algorithm, builds `DocumentTree`
- `images.rs` — image extraction, magic byte format detection
- `render/markdown.rs` — section content to Markdown
- `render/cleanup.rs` — text normalization (ligatures, hyphenation, CJK)
- `lib.rs` — public API: `ParsedDocument`, `parse()`, `read_section()`, `get_image()`, `info()`

## CLI Commands

### Table of Contents

```bash
mcptools pdf toc document.pdf
```

Returns the full document tree as JSON with section IDs, headings, content previews, image counts, and page ranges.

### Read a Section

```bash
mcptools pdf read document.pdf s-1-0
```

Returns the section content as rendered Markdown with image references. Section IDs come from the `pdf toc` output (format: `s-{depth}-{index}`).

### Extract an Image

```bash
# Save to file
mcptools pdf image document.pdf Im1 --output photo.jpg

# Print base64 to stdout
mcptools pdf image document.pdf Im1
```

Image IDs are XObject names from the PDF (visible in section image references).

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

2. **Read a specific section by ID:**
   ```bash
   mcptools pdf read document.pdf s-1-0
   ```

3. **Extract images referenced in the section:**
   ```bash
   mcptools pdf image document.pdf Im1 --output image.jpg
   ```

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
- `sectionId` (required): Section ID from `pdf_toc` (e.g., `s-1-0`)

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
- `imageId` (required): Image ID (XObject name from the PDF)

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
