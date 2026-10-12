# Confluence Integration

## CLI Commands

```bash
# Search for pages containing text
mcptools atlassian confluence search "text ~ 'deployment'"

# Search in a specific space
mcptools atlassian confluence search "space = WIKI AND text ~ 'api'"

# Limit results
mcptools atlassian confluence search "text ~ 'guide'" --limit 5

# Output as JSON
mcptools atlassian confluence search "text ~ 'documentation'" --json

# Read one page by ID
mcptools atlassian confluence get 123456

# Read one page by URL; read is an alias for get
mcptools atlassian confluence read 'https://example.atlassian.net/wiki/spaces/WIKI/pages/123456/Runbook' --json
```

## MCP Tool

### confluence_search

```json
{
  "method": "tools/call",
  "params": {
    "name": "confluence_search",
    "arguments": {
      "query": "space = WIKI AND text ~ 'deployment'",
      "limit": 10
    }
  }
}
```

### confluence_page_get

```json
{
  "method": "tools/call",
  "params": {
    "name": "confluence_page_get",
    "arguments": {
      "idOrUrl": "https://example.atlassian.net/wiki/spaces/WIKI/pages/123456/Runbook"
    }
  }
}
```

The page reader accepts a numeric page ID, a URL with a `pageId` query parameter, or a URL with a numeric ID after a `/pages/` path segment. It returns the title, space ID, current version number, Markdown body, and web URL.

## Environment Variables

| Variable | Description | Fallback |
|----------|-------------|----------|
| `CONFLUENCE_BASE_URL` | Confluence instance URL | `ATLASSIAN_BASE_URL` |
| `CONFLUENCE_EMAIL` | Email for Confluence auth | `ATLASSIAN_EMAIL` |
| `CONFLUENCE_API_TOKEN` | API token for Confluence | `ATLASSIAN_API_TOKEN` |

## CQL Query Tips

- `text ~ 'keyword'` - Full-text search
- `space = KEY` - Filter by space key
- `type = page` - Filter by content type
- `lastModified >= -30d` - Recently modified pages
- `creator = currentUser()` - Pages you created
