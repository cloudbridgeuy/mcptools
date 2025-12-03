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
