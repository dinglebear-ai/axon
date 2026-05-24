# Transport Methods -- Axon MCP

## Overview

Axon MCP supports three transport configurations:

| Transport | Auth | Use case | Command |
|-----------|------|----------|---------|
| stdio | None (process isolation) | Claude Desktop, Codex CLI, local tools | `axon mcp` (default) |
| HTTP | static bearer or OAuth | Docker, remote servers, shared access | `axon serve mcp` or `axon mcp --transport http` |
| Both | Mixed | Serve HTTP while also accepting stdio | `axon mcp --transport both` |

`axon mcp` defaults to **stdio** and does not open an HTTP listener unless
`--transport http` or `--transport both` is set. The HTTP transport is the
default for `axon serve mcp`. Any HTTP MCP transport selector starts the
unified HTTP server, so MCP, the web panel, and first-party client routes all
share the same listener.
See [`../auth/MCP-AUTH.md`](../auth/MCP-AUTH.md) for bearer and OAuth auth modes.

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

MCP uses the same SQLite/in-process job runtime as the CLI and HTTP server.

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
- Local process isolation when each client should own its own Axon process

## HTTP

Streamable-HTTP transport with MCP protocol support. Uses the `rmcp` crate with `transport-streamable-http-server` feature.

```bash
AXON_MCP_HTTP_HOST=127.0.0.1
AXON_MCP_HTTP_PORT=8001
axon serve mcp
```

Non-loopback binds such as `0.0.0.0` require either `AXON_MCP_HTTP_TOKEN` or
`AXON_MCP_AUTH_MODE=oauth` with the OAuth env vars configured. Tokenless HTTP
startup is limited to loopback hosts.

### Endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/mcp` | POST | MCP JSON-RPC streamable-HTTP transport |
| `/v1/capabilities` | GET | First-party CLI server capability document, when served by `axon serve` |
| `/v1/*` | GET/POST/DELETE | First-party direct REST routes, when served by `axon serve` |

The same listener also mounts the web panel and first-party HTTP routes. Use
`/healthz` for server health and `/mcp` for MCP protocol checks.

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

- Docker deployments (`axon` service exposes port 8001)
- Remote/shared MCP server
- Multiple clients connecting to one server
- `axon serve` (automatically starts MCP HTTP on port 8001)

## Both

Run HTTP transport while also accepting stdio connections.

```bash
axon mcp --transport both
```

This is useful for development: serve web and MCP HTTP on port 8001 while also
allowing local stdio connections.

## Transport in `axon serve`

When running `axon serve`, no separate `axon mcp` invocation is needed. The same
listener mounts `/mcp`, the setup panel, `/v1/ask`, and first-party
client/server routes.

## Port assignments

| Service | Default port | Env var |
|---------|-------------|---------|
| Web + MCP HTTP | 8001 | `AXON_MCP_HTTP_PORT` |

## See also

- [CONNECT.md](CONNECT.md) -- client connection instructions
- [ENV.md](ENV.md) -- MCP environment variables
- [DEPLOY.md](DEPLOY.md) -- deployment patterns
