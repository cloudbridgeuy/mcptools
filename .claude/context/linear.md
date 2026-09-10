# Linear Integration

## Auth

```bash
export LINEAR_API_KEY="lin_api_..."
mcptools linear auth status
mcptools linear auth status --json
```

Missing or invalid credentials return actionable errors. The key never appears in output.

## CLI Commands

### Issues

```bash
# Get one issue by UUID or identifier
mcptools linear issue get GUZ-79

# List issues, newest first
mcptools linear issue list --limit 5

# Filter by team, state, label, cycle, assignee, title text
mcptools linear issue list --team GUZ --state "In Progress"
mcptools linear issue list --team GUZ --project "MCPTools"
mcptools linear issue list --assignee me
mcptools linear issue list --label Bug --query "auth"
mcptools linear issue list --cycle 3
mcptools linear issue list --updated-after 2026-01-01T00:00:00Z

# Pagination
mcptools linear issue list --cursor <cursor>
mcptools linear issue list --all

# Output as JSON
mcptools linear issue list --json
```

Selector rules:

- `--team` accepts id, key, or name.
- `--project` accepts id or name. Names need `--team`.
- `--assignee` accepts a user UUID or `me` (current viewer). Find UUIDs with `linear users list --query NAME`.
- `--state` and `--label` match by name. `--cycle` accepts a cycle number or id.
- `--query` matches a title substring. `--updated-after` needs RFC3339 and is rejected before any request when malformed.

### Discovery

```bash
mcptools linear teams list
mcptools linear teams get GUZ
mcptools linear projects list --team GUZ
mcptools linear projects get "MCPTools" --team GUZ
mcptools linear users list --query "Ada"
mcptools linear states list --team GUZ
mcptools linear labels list --team GUZ
mcptools linear cycles list --team GUZ
```

`users list` needs `--query`. `projects`, `states`, `labels`, `cycles` need `--team`. Ambiguous names fail with candidate IDs instead of picking a match.

## Output

- Tables show compact rows plus a `hasMore/endCursor` line. Truncation is never hidden.
- `--json` returns `{"nodes": [...], "pageInfo": {...}}`.
- `--all` follows cursors and caps results at 50 items.
