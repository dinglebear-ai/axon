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

When `axon serve` or `axon mcp` is running with HTTP transport (default):

```bash
claude mcp add --transport http axon http://localhost:8001/mcp
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
        "TEI_URL": "http://127.0.0.1:52000",
        "AXON_PG_URL": "postgresql://axon:pass@127.0.0.1:53432/axon",
        "AXON_REDIS_URL": "redis://:pass@127.0.0.1:53379",
        "AXON_AMQP_URL": "amqp://axon:pass@127.0.0.1:45535"
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

## Lite mode connection

For zero-infrastructure MCP (no Postgres/Redis/RabbitMQ):

```json
{
  "mcpServers": {
    "axon": {
      "command": "/path/to/axon",
      "args": ["mcp"],
      "env": {
        "AXON_LITE": "1",
        "QDRANT_URL": "http://127.0.0.1:53333",
        "TEI_URL": "http://127.0.0.1:52000"
      }
    }
  }
}
```

## Verifying connection

```bash
# HTTP health check
curl -s http://localhost:8001/health

# Test via doctor
axon doctor

# Test a tool call via Claude Code
claude "call axon with action=doctor"
```

If connection fails:

1. Verify the server is running (`just dev` or `axon mcp`)
2. Check port 8001 is not blocked
3. For stdio: confirm the `axon` binary path is correct and all env vars are set
4. Run `axon doctor` to check infrastructure connectivity

## See also

- [TRANSPORT.md](TRANSPORT.md) -- transport configuration details
- [ENV.md](ENV.md) -- environment variables
- [DEPLOY.md](DEPLOY.md) -- deployment patterns
