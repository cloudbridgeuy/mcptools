# Web Scraping (md commands)

Uses headless Chrome to fetch web pages and convert to Markdown.

## CLI Commands

### Fetch Web Pages

```bash
# Basic fetch
mcptools md fetch https://example.com

# With CSS selector
mcptools md fetch https://docs.example.com --selector "main"

# With pagination
mcptools md fetch https://example.com --limit 500 --page 2

# Extract specific section using offset (from md toc)
mcptools md fetch https://docs.example.com --offset 1234 --limit 580

# Selection strategies for multiple matches
mcptools md fetch https://example.com --selector "article" --strategy all
mcptools md fetch https://example.com --selector "article" --strategy last
mcptools md fetch https://example.com --selector "p" --strategy n --index 2

# Get raw HTML instead of Markdown
mcptools md fetch https://example.com --raw-html

# Include metadata (title, URL, HTML size, fetch time)
mcptools md fetch https://example.com --include-metadata

# Custom timeout
mcptools md fetch https://example.com --timeout 60

# Output as JSON
mcptools md fetch https://example.com --json
```

### Complete CLI Flags Reference

| Flag | Env Var | Default | Description |
|------|---------|---------|-------------|
| `<URL>` | `MD_URL` | required | URL to fetch |
| `--timeout`, `-t` | `MD_TIMEOUT` | 30 | Timeout in seconds |
| `--json` | - | false | Output as JSON |
| `--raw-html` | - | false | Output raw HTML instead of Markdown |
| `--include-metadata` | - | false | Include title, URL, HTML size, fetch time |
| `--selector` | `MD_SELECTOR` | - | CSS selector to filter content |
| `--strategy` | `MD_STRATEGY` | first | Selection strategy: first, last, all, n |
| `--index` | `MD_INDEX` | - | Index for 'n' strategy (0-indexed) |
| `--paginated` | `MD_PAGINATED` | false | Enable pagination explicitly |
| `--offset` | `MD_OFFSET` | 0 | Character offset to start from |
| `--limit` | `MD_LIMIT` | 1000 | Characters per page |
| `--page` | `MD_PAGE` | 1 | Page number (1-indexed) |

**Note:** Pagination is auto-enabled when `--offset`, `--limit`, or `--page` are set.

### Extract Table of Contents

```bash
# Get TOC (default indented format)
mcptools md toc https://docs.example.com

# Get TOC as markdown list
mcptools md toc https://example.com --output markdown

# Get TOC as JSON with offsets
mcptools md toc https://example.com --json

# Filter by selector
mcptools md toc https://docs.example.com --selector "main"
```

**Output Formats:** `indented`, `markdown`, `json`

## Best Practice Workflow

1. **Get page structure first:**
   ```bash
   mcptools md toc https://docs.example.com --json
   ```

2. **Fetch specific section using offset:**
   ```bash
   mcptools md fetch https://docs.example.com --offset 1234 --limit 580
   ```

3. **Or use CSS selectors:**
   ```bash
   mcptools md fetch https://docs.example.com --selector "main" --limit 1000
   ```

## MCP Tools

### md_fetch

```json
{
  "method": "tools/call",
  "params": {
    "name": "md_fetch",
    "arguments": {
      "url": "https://docs.example.com",
      "selector": "main",
      "limit": 1000,
      "page": 1
    }
  }
}
```

**Arguments:**
- `url` (required): URL to fetch
- `timeout` (optional): Timeout in seconds (default: 30)
- `raw_html` (optional): Return raw HTML instead of Markdown
- `selector` (optional): CSS selector to filter content
- `strategy` (optional): Selection strategy (first, last, all, n)
- `index` (optional): Index for 'n' strategy (0-indexed)
- `offset` (optional): Character offset to start from
- `limit` (optional): Characters per page (default: 1000)
- `page` (optional): Page number, 1-indexed

### md_toc

```json
{
  "method": "tools/call",
  "params": {
    "name": "md_toc",
    "arguments": {
      "url": "https://docs.example.com",
      "output": "json"
    }
  }
}
```

**Arguments:**
- `url` (required): URL to fetch
- `timeout` (optional): Timeout in seconds (default: 30)
- `selector` (optional): CSS selector to filter content
- `strategy` (optional): Selection strategy
- `index` (optional): Index for 'n' strategy
- `output` (optional): Output format (indented, markdown, json)

## Site-Specific Tips

- **LocalStack Documentation** (`https://docs.localstack.cloud/*`): Use `selector: "main"`
- Most documentation sites: Try `selector: "main"` or `selector: "article"`

## Environment Variables

All CLI flags can be set via environment variables for scripting/testing:

| Variable | Description |
|----------|-------------|
| `MD_URL` | URL to fetch |
| `MD_TIMEOUT` | Timeout in seconds |
| `MD_SELECTOR` | CSS selector |
| `MD_STRATEGY` | Selection strategy |
| `MD_INDEX` | Index for 'n' strategy |
| `MD_PAGINATED` | Enable pagination |
| `MD_OFFSET` | Character offset |
| `MD_LIMIT` | Characters per page |
| `MD_PAGE` | Page number |

Requires Chrome/Chromium installed on the system.
