# Connect to Axon MCP

How to connect to the Axon MCP server from supported clients.

## Claude Code CLI

### stdio

```bash
claude mcp add axon -- /path/to/axon mcp
```

Or with environment variables:

```bash
claude mcp add axon -- env QDRANT_URL=http://127.0.0.1:53333 TEI_URL=http://127.0.0.1:52000 /path/to/axon mcp
```

### HTTP

`axon mcp` defaults to **stdio** transport. To run the HTTP transport, use one
of:

```bash
axon mcp --transport http       # HTTP only
axon mcp --transport both       # stdio + HTTP concurrently
axon serve mcp                  # HTTP transport (defaults to HTTP for the serve subcommand)
axon serve                      # unified web + MCP HTTP on the same port
```

Then register the HTTP transport with Claude Code:

```bash
claude mcp add --transport http axon http://localhost:8001/mcp \
  --header "Authorization: Bearer $AXON_MCP_HTTP_TOKEN"
```

### Scopes

| Flag | Scope | Config file |
|------|-------|-------------|
| `--scope project` | Current project only | `.claude/settings.local.json` |
| `--scope user` | All projects | `~/.claude/settings.json` |
| (none) | Project default | `.claude/settings.local.json` |

## Codex CLI

### stdio

`.codex/mcp.json` (project) or `~/.codex/mcp.json` (global):

```json
{
  "mcpServers": {
    "axon": {
      "command": "/path/to/axon",
      "args": ["mcp"],
      "env": {
        "QDRANT_URL": "http://127.0.0.1:53333",
        "TEI_URL": "http://127.0.0.1:52000"
      }
    }
  }
}
```

MCP uses the same SQLite/in-process job runtime as the CLI and HTTP server.

### HTTP

```json
{
  "mcpServers": {
    "axon": {
      "type": "http",
      "url": "http://localhost:8001/mcp",
      "headers": {
        "Authorization": "Bearer YOUR_AXON_MCP_HTTP_TOKEN"
      }
    }
  }
}
```

## Gemini CLI

### stdio

`gemini-extension.json` (project root or `~/.gemini/`):

```json
{
  "mcpServers": {
    "axon": {
      "command": "/path/to/axon",
      "args": ["mcp"],
      "env": {
        "QDRANT_URL": "http://127.0.0.1:53333",
        "TEI_URL": "http://127.0.0.1:52000"
      }
    }
  }
}
```

### HTTP

```json
{
  "mcpServers": {
    "axon": {
      "type": "http",
      "url": "http://localhost:8001/mcp"
    }
  }
}
```

## Manual configuration reference

### Config file locations

| Client | Scope | File |
|--------|-------|------|
| Claude Code | Project | `.claude/settings.local.json` |
| Claude Code | User | `~/.claude/settings.json` |
| Codex CLI | Project | `.codex/mcp.json` |
| Codex CLI | User | `~/.codex/mcp.json` |
| Gemini CLI | Project | `gemini-extension.json` |
| Gemini CLI | Global | `~/.gemini/gemini-extension.json` |

## Local stdio connection

For local stdio MCP with SQLite-backed jobs and no external queue broker:

```json
{
  "mcpServers": {
    "axon": {
      "command": "/path/to/axon",
      "args": ["mcp"],
      "env": {
        "QDRANT_URL": "http://127.0.0.1:53333",
        "TEI_URL": "http://127.0.0.1:52000"
      }
    }
  }
}
```

## Verifying connection

```bash
# HTTP probe (requires bearer token; returns 401 without it when token is set)
curl -s -o /dev/null -w "%{http_code}\n" \
  -H "Authorization: Bearer $AXON_MCP_HTTP_TOKEN" \
  http://localhost:8001/mcp

# Test via doctor
axon doctor

# Test a tool call via Claude Code
claude "call axon with action=doctor"
```

If connection fails:

1. Verify the server is running (`just dev` or `axon serve`)
2. Check port 8001 is not blocked
3. For stdio: confirm the `axon` binary path is correct and all env vars are set
4. For HTTP: confirm `AXON_MCP_HTTP_TOKEN` is set on both server and client
5. Run `axon doctor` to check infrastructure connectivity

The unified HTTP server exposes `/healthz` for process health. Auth-pass on
`/mcp` verifies the MCP/auth path specifically.

## HTTP API access

The `axon` CLI and MCP server always run in-process (local execution against
Qdrant and TEI) — they do not forward to a remote `axon serve`. To expose Axon
over HTTP for external API clients, run `axon serve`, which serves the first-party
`/v1` REST routes and MCP-over-HTTP on `/mcp` behind the same bearer token policy
(`AXON_MCP_HTTP_TOKEN`). Point your own HTTP/MCP clients at it; the bundled CLI
does not consume those routes.

## See also

- [TRANSPORT.md](transport.md) -- transport configuration details
- [ENV.md](env.md) -- environment variables
- [DEPLOY.md](deploy.md) -- deployment patterns
