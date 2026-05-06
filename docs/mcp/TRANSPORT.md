# Transport Methods -- Axon MCP

## Overview

Axon MCP supports three transport configurations:

| Transport | Auth | Use case | Command |
|-----------|------|----------|---------|
| stdio | None (process isolation) | Claude Desktop, Codex CLI, local tools | `axon mcp` (default) |
| HTTP | `AXON_MCP_HTTP_TOKEN` bearer | Docker, remote servers, shared access | `axon serve mcp` or `axon mcp --transport http` |
| Both | Mixed | Serve HTTP while also accepting stdio | `axon mcp --transport both` |

`axon mcp` defaults to **stdio**. The HTTP transport is the default for the
`axon serve mcp` subcommand. The unified `axon serve` command runs **both**
transports concurrently. There is no OAuth flow — see
[`../auth/MCP-AUTH.md`](../auth/MCP-AUTH.md) for the actual auth model.

## stdio

JSON-RPC messages over stdin/stdout. No network listener, no auth required.

```bash
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
        "TEI_URL": "http://127.0.0.1:52000"
      }
    }
  }
}
```

Lite mode is the only runtime mode — Postgres / Redis / AMQP env vars are not
read by the MCP server.

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
AXON_MCP_HTTP_HOST=127.0.0.1
AXON_MCP_HTTP_PORT=8001
axon serve mcp
```

Non-loopback binds such as `0.0.0.0` require `AXON_MCP_HTTP_TOKEN`; tokenless
HTTP startup is limited to loopback hosts.

### Endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/mcp` | POST | MCP JSON-RPC streamable-HTTP transport |

When running under `axon serve` (unified web + MCP), additional web endpoints
are mounted alongside `/mcp`. The MCP HTTP server itself does not expose a
dedicated health endpoint.

### Claude Code configuration

`.claude/settings.local.json`:

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

Loopback binds (`127.0.0.1`) may run without `AXON_MCP_HTTP_TOKEN`; non-loopback
binds (e.g. `0.0.0.0`) require it or the server refuses to start.

### When to use

- Docker deployments (axon-workers exposes port 8001)
- Remote/shared MCP server
- Multiple clients connecting to one server
- `axon serve` (automatically starts MCP HTTP on port 8001)

## Both

Run HTTP transport while also accepting stdio connections.

```bash
axon mcp --transport both
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
