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

# Create a published page from Confluence storage-format XHTML
mcptools atlassian confluence create --space-id 9988 --title 'Runbook' --body '<p>Deploy</p>' --parent 123456 --json

# Update the title, storage body, or both; the current version is read and incremented
mcptools atlassian confluence update 123456 --title 'Deployment Runbook' --body '<p>Deploy and verify</p>' --json
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

### confluence_page_create

```json
{
  "method": "tools/call",
  "params": {
    "name": "confluence_page_create",
    "arguments": {
      "spaceId": "9988",
      "title": "Runbook",
      "body": "<p>Deploy</p>",
      "parentIdOrUrl": "123456",
      "allowWrites": true
    }
  }
}
```

### confluence_page_update

```json
{
  "method": "tools/call",
  "params": {
    "name": "confluence_page_update",
    "arguments": {
      "idOrUrl": "https://example.atlassian.net/wiki/spaces/WIKI/pages/123456/Runbook",
      "title": "Deployment Runbook",
      "body": "<p>Deploy and verify</p>",
      "allowWrites": true
    }
  }
}
```

Create and update bodies use Confluence storage-format XHTML. Update requires `title`, `body`, or both. It reads the current storage body and version, preserves omitted content, then sends the next version number. Both mutation tools require `allowWrites`.

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
