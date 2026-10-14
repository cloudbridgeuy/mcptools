# HackerNews Integration

## CLI Commands

For `hn list` and `hn read`, `page` and `limit` must be greater than zero. Invalid values return an error before any API request.

### Read Posts and Comments

```bash
# Read a post by ID
mcptools hn read 8863

# Read using full URL
mcptools hn read "https://news.ycombinator.com/item?id=8863"

# Limit comments and paginate
mcptools hn read 8863 --limit 5 --page 2

# Read a specific comment thread
mcptools hn read 8863 --thread 9224

# Output as JSON
mcptools hn read 8863 --json
```

### List Stories

```bash
# List top stories (default)
mcptools hn list

# List different story types
mcptools hn list new
mcptools hn list best
mcptools hn list ask
mcptools hn list show
mcptools hn list job

# Pagination
mcptools hn list --limit 10 --page 2

# Output as JSON
mcptools hn list --json
```

**Story Types:** `top`, `new`, `best`, `ask`, `show`, `job`

## MCP Tools

Both tools reject `page: 0` and `limit: 0` with invalid params (`-32602`) before any API request.

### hn_read_item

```json
{
  "method": "tools/call",
  "params": {
    "name": "hn_read_item",
    "arguments": {
      "item": "8863",
      "limit": 5,
      "page": 1
    }
  }
}
```

**Arguments:**
- `item` (required): HackerNews item ID or full URL
- `limit` (optional): Number of comments per page (default: 10)
- `page` (optional): Page number, 1-indexed (default: 1)
- `thread` (optional): Not supported in MCP data mode; use CLI `--thread` instead

### hn_list_items

```json
{
  "method": "tools/call",
  "params": {
    "name": "hn_list_items",
    "arguments": {
      "story_type": "top",
      "limit": 10,
      "page": 1
    }
  }
}
```

**Arguments:**
- `story_type` (optional): Type of stories (default: "top")
- `limit` (optional): Number of stories per page (default: 30)
- `page` (optional): Page number, 1-indexed (default: 1)

## Environment Variables

No environment variables required. HackerNews API is public. CLI defaults can use `HN_ITEM` (item ID or URL) and `HN_LIMIT` (items per page).
