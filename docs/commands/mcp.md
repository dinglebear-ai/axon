# axon mcp
Last Modified: 2026-03-10

Start Axon's MCP server exposing a single unified tool: `axon`.

## Synopsis

```bash
axon mcp [--transport stdio|http|both]
```

## Transport Modes

`axon mcp` supports three transport modes:

- `stdio` (default): starts stdio transport only
- `http`: starts the HTTP MCP server on `/mcp`
- `both`: starts stdio and HTTP concurrently

Transport selection:

| Selector | Default | Description |
|----------|---------|-------------|
| `axon mcp` | `stdio` | Local MCP client entrypoint |
| `axon serve mcp` | `http` | HTTP MCP server entrypoint |
| `--transport` | command default | Explicit CLI transport selector |

## HTTP Runtime Binding

When HTTP transport is enabled (`http` or `both`), these environment variables control bind address:

| Variable | Default | Description |
|----------|---------|-------------|
| `AXON_MCP_HTTP_HOST` | `127.0.0.1` | MCP server bind host; non-loopback requires `AXON_MCP_HTTP_TOKEN` |
| `AXON_MCP_HTTP_PORT` | `8001` | MCP server bind port |
| `AXON_MCP_HTTP_TOKEN` | unset | Bearer or `x-api-key` token for MCP HTTP requests; required for non-loopback binds |

The primary HTTP MCP endpoint is mounted at `/mcp`.

## Tool Contract

- Tool count: 1
- Tool name: `axon`
- Routing: `action` + `subaction` (for lifecycle families)

Supported top-level action families include: `status`, `help`, `crawl`, `extract`, `embed`, `ingest`, `query`, `retrieve`, `search`, `map`, `doctor`, `domains`, `sources`, `stats`, `artifacts`, `scrape`, `research`, `ask`, `screenshot`, `elicit_demo`.

## Examples

```bash
# Stdio only
axon mcp

# HTTP bind 0.0.0.0:8001
axon serve mcp

# HTTP + stdio together
axon mcp --transport both

# Custom HTTP bind
AXON_MCP_HTTP_HOST=127.0.0.1 AXON_MCP_HTTP_PORT=8900 axon serve mcp
```

## Notes

- If `AXON_MCP_HTTP_PORT` is not a valid `u16`, startup fails immediately.
- OAuth-related endpoints apply to HTTP mode only.
- `stdio` mode is intended for local MCP clients such as Claude Desktop.
- See `docs/MCP.md` and `docs/MCP-TOOL-SCHEMA.md` for full request/response contract details.
