# Transport Methods -- Axon MCP

## Overview

Axon MCP supports three transport configurations:

| Transport | Auth | Use case | Config value |
|-----------|------|----------|--------------|
| stdio | None (process isolation) | Claude Desktop, Codex CLI, local tools | `stdio` |
| HTTP | OAuth / bearer token | Docker, remote servers, shared access | `http` |
| Both | Mixed | Serve HTTP while also accepting stdio | `both` |

Set the transport via environment variable:

```bash
AXON_MCP_TRANSPORT=http  # default
```

Or via `axon.json`:

```json
{
  "mcp": {
    "transport": "http",
    "http_host": "0.0.0.0",
    "http_port": 8001
  }
}
```

## stdio

JSON-RPC messages over stdin/stdout. No network listener, no auth required.

```bash
AXON_MCP_TRANSPORT=stdio
axon mcp
```

### Claude Desktop configuration

`~/.claude/claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "axon": {
      "command": "/path/to/axon",
      "args": ["mcp"],
      "env": {
        "QDRANT_URL": "http://127.0.0.1:53333",
        "TEI_URL": "http://127.0.0.1:52000",
        "AXON_PG_URL": "postgresql://axon:pass@127.0.0.1:53432/axon",
        "AXON_REDIS_URL": "redis://:pass@127.0.0.1:53379",
        "AXON_AMQP_URL": "amqp://axon:pass@127.0.0.1:45535"
      }
    }
  }
}
```

### Codex CLI configuration

`.codex/mcp.json`:

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

### When to use

- Local development with Claude Desktop or Codex CLI
- Single-user setups where the binary runs as a child process
- Lite mode (`AXON_LITE=1`) for zero-infrastructure operation

## HTTP

Streamable-HTTP transport with MCP protocol support. Uses the `rmcp` crate with `transport-streamable-http-server` feature.

```bash
AXON_MCP_TRANSPORT=http
AXON_MCP_HTTP_HOST=0.0.0.0
AXON_MCP_HTTP_PORT=8001
axon mcp
```

### Endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/mcp` | POST | MCP JSON-RPC with streamable-http |
| `/health` | GET | Health check |

### Claude Code configuration

`.claude/settings.local.json`:

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

### When to use

- Docker deployments (axon-workers exposes port 8001)
- Remote/shared MCP server
- Multiple clients connecting to one server
- `axon serve` (automatically starts MCP HTTP on port 8001)

## Both

Run HTTP transport while also accepting stdio connections.

```bash
AXON_MCP_TRANSPORT=both
axon mcp
```

This is useful for development: serve HTTP for web clients while also allowing local stdio connections.

## Transport in `axon serve`

When running `axon serve`, the MCP HTTP server starts automatically as part of the supervised process tree. No separate `axon mcp` invocation is needed.

| Supervisor child | Port | Transport |
|-----------------|------|-----------|
| MCP HTTP server | 8001 | streamable-http |
| Backend bridge | 49000 | HTTP/WebSocket |
| Next.js | 49010 | HTTP |

## Port assignments

| Service | Default port | Env var |
|---------|-------------|---------|
| MCP HTTP | 8001 | `AXON_MCP_HTTP_PORT` |
| Backend bridge | 49000 | `AXON_SERVE_PORT` |
| Next.js web UI | 49010 | `AXON_WEB_DEV_PORT` |
| Shell WebSocket | 49011 | `SHELL_SERVER_PORT` |

## See also

- [CONNECT.md](CONNECT.md) -- client connection instructions
- [ENV.md](ENV.md) -- MCP environment variables
- [DEPLOY.md](DEPLOY.md) -- deployment patterns
