# mcptools

Useful MCP Tools to use with LLM Coding Agents

## Overview

`mcptools` is a Model Context Protocol (MCP) server that exposes various tools for LLM agents to interact with external services. Currently, it provides access to HackerNews data through the `hn_read_item` tool.

## Installation

```bash
# Build and install to ~/.local/bin
cargo xtask install

# Or specify a custom installation path
cargo xtask install --path /usr/local/bin
```

## Usage

### MCP Server Modes

The MCP server can run in two transport modes:

#### 1. stdio Transport (for local agents)

```bash
mcptools mcp stdio
```

This mode communicates via standard input/output, making it suitable for local LLM agents like Claude Desktop or other MCP clients that spawn server processes.

#### 2. SSE Transport (for web-based agents)

```bash
mcptools mcp sse --port 3000 --host 127.0.0.1
```

This mode runs an HTTP server with Server-Sent Events (SSE) support, suitable for web-based clients.

Endpoints:

- `GET /sse` - SSE endpoint for real-time updates
- `POST /message` - JSON-RPC endpoint for tool calls

### Configuring MCP Clients

#### Claude Desktop Configuration

Add to your Claude Desktop config file (`~/Library/Application Support/Claude/claude_desktop_config.json` on macOS):

```json
{
  "mcpServers": {
    "mcptools": {
      "command": "mcptools",
      "args": ["mcp", "stdio"]
    }
  }
}
```

#### Claude Code Configuration

**Option 1: Using the `claude mcp add` command (recommended)**

```bash
claude mcp add mcptools -- mcptools mcp stdio
```

This automatically adds the server to your Claude Code configuration.

**Option 2: Manual configuration**

Add to your Claude Code config file (`~/Library/Application Support/Claude/claude_code_config.json` on macOS, `%APPDATA%\Claude\claude_code_config.json` on Windows):

```json
{
  "mcpServers": {
    "mcptools": {
      "command": "mcptools",
      "args": ["mcp", "stdio"]
    }
  }
}
```

After adding the configuration, restart Claude Code for the changes to take effect. The `hn_read_item` tool will be available for use in your coding sessions.

#### Generic MCP Client (stdio)

Any MCP client can connect by spawning the process and communicating via JSON-RPC 2.0 over stdio:

```bash
mcptools mcp stdio
```

Send JSON-RPC requests via stdin, receive responses via stdout.

## Available Tools

### hn_read_item

Read HackerNews posts and comments with pagination support.

**Parameters:**

- `item` (string, required) - HackerNews item ID (e.g., "8863") or full URL (e.g., "https://news.ycombinator.com/item?id=8863")
- `limit` (number, optional) - Number of comments per page (default: 10)
- `page` (number, optional) - Page number, 1-indexed (default: 1)
- `thread` (string, optional) - Comment thread ID to read a specific comment thread

**Example Usage:**

```json
{
  "jsonrpc": "2.0",
  "id": 1,
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

**Response:**

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "content": [
      {
        "type": "text",
        "text": "{\"id\":8863,\"title\":\"My YC app: Dropbox...\",\"comments\":[...],\"pagination\":{...}}"
      }
    ]
  }
}
```

## MCP Protocol Implementation

This server implements the Model Context Protocol specification with the following methods:

### initialize

Establishes connection and returns server capabilities.

**Request:**

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "initialize",
  "params": {}
}
```

**Response:**

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "protocolVersion": "2024-11-05",
    "capabilities": {
      "tools": {}
    },
    "serverInfo": {
      "name": "mcptools",
      "version": "0.0.0"
    }
  }
}
```

### tools/list

Returns list of available tools.

**Request:**

```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "tools/list",
  "params": {}
}
```

**Response:**

```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "result": {
    "tools": [
      {
        "name": "hn_read_item",
        "description": "Read a HackerNews post and its comments...",
        "inputSchema": {
          "type": "object",
          "properties": {
            "item": {
              "type": "string",
              "description": "HackerNews item ID or URL"
            },
            "limit": {
              "type": "number",
              "description": "Number of comments per page (default: 10)"
            },
            "page": {
              "type": "number",
              "description": "Page number, 1-indexed (default: 1)"
            },
            "thread": {
              "type": "string",
              "description": "Comment thread ID to read (optional)"
            }
          },
          "required": ["item"]
        }
      }
    ]
  }
}
```

### tools/call

Executes a specific tool.

**Request:**

```json
{
  "jsonrpc": "2.0",
  "id": 3,
  "method": "tools/call",
  "params": {
    "name": "hn_read_item",
    "arguments": {
      "item": "8863",
      "limit": 2
    }
  }
}
```

**Response:**

```json
{
  "jsonrpc": "2.0",
  "id": 3,
  "result": {
    "content": [
      {
        "type": "text",
        "text": "{\"id\":8863,\"title\":\"My YC app: Dropbox - Throw away your USB drive\",\"url\":\"http://www.getdropbox.com/u/2/screencast.html\",\"author\":\"dhouston\",\"score\":104,\"time\":\"2007-04-04 19:16:40 UTC\",\"text\":null,\"total_comments\":71,\"comments\":[{\"id\":9224,\"author\":\"BrandonM\",\"time\":\"2007-04-05 15:16:54 UTC\",\"text\":\"I have a few qualms with this app...\",\"replies_count\":1},{\"id\":8917,\"author\":\"brett\",\"time\":\"2007-04-04 21:48:13 UTC\",\"text\":\"This is genius...\",\"replies_count\":0}],\"pagination\":{\"current_page\":1,\"total_pages\":17,\"total_comments\":33,\"limit\":2,\"next_page_command\":\"mcptools hn read 8863 --page 2\",\"prev_page_command\":null}}"
      }
    ]
  }
}
```

## Testing with curl (SSE mode)

Start the server:

```bash
mcptools mcp sse --port 3000
```

Test the endpoints:

```bash
# Initialize
curl -X POST http://127.0.0.1:3000/message \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}'

# List tools
curl -X POST http://127.0.0.1:3000/message \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}'

# Call hn_read_item tool
curl -X POST http://127.0.0.1:3000/message \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"hn_read_item","arguments":{"item":"8863","limit":2}}}'
```

## CLI Usage (Non-MCP)

You can also use the HackerNews functionality directly via CLI:

```bash
# Read a post with default settings (10 comments)
mcptools hn read 8863

# Read with custom limit
mcptools hn read 8863 --limit 5

# Navigate to specific page
mcptools hn read 8863 --page 2

# Read specific comment thread
mcptools hn read 8863 --thread 9224

# Output as JSON
mcptools hn read 8863 --json

# Use full URL instead of ID
mcptools hn read "https://news.ycombinator.com/item?id=8863"
```

## Development

```bash
# Build
cargo build

# Run tests
cargo test

# Run clippy
cargo clippy

# Watch mode (requires bacon)
bacon
```

## License

MIT
