# HackerNews Integration

## CLI Commands

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
mcptools hn list --story-type new
mcptools hn list --story-type best
mcptools hn list --story-type ask
mcptools hn list --story-type show
mcptools hn list --story-type job

# Pagination
mcptools hn list --limit 10 --page 2

# Output as JSON
mcptools hn list --json
```

**Story Types:** `top`, `new`, `best`, `ask`, `show`, `job`

## MCP Tools

### hn_read_item

```json
{
  "method": "tools/call",
  "params": {
    "name": "hn_read_item",
    "arguments": {
      "item": "8863",
      "limit": 5,
      "page": 1,
      "thread": "9224"
    }
  }
}
```

**Arguments:**
- `item` (required): HackerNews item ID or full URL
- `limit` (optional): Number of comments per page (default: 10)
- `page` (optional): Page number, 1-indexed (default: 1)
- `thread` (optional): Comment thread ID to read

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

No environment variables required. HackerNews API is public.
