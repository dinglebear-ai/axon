# axon serve
Last Modified: 2026-05-09

Start Axon's unified HTTP server.

## Synopsis

```bash
axon serve
axon serve mcp [--transport http|both]
```

## What It Runs

`axon serve` starts one Axum HTTP server on `AXON_MCP_HTTP_HOST:AXON_MCP_HTTP_PORT` (default `127.0.0.1:8001`).

Mounted surfaces:

- `POST /mcp` - MCP streamable HTTP transport.
- `GET /v1/capabilities` - first-party CLI client/server capability metadata.
- `POST /v1/actions` - first-party CLI action dispatch for supported stateful commands.
- `POST /v1/ask` - ask endpoint used by `axon ask` when `AXON_SERVER_URL` is set.
- `/api/panel/*` - local setup/config panel APIs.
- Static web panel assets.
- OAuth metadata and auth routes when `AXON_MCP_AUTH_MODE=oauth`.

`serve` does not supervise Next.js, a shell WebSocket server, or separate worker
processes. In-process workers are initialized lazily by the service context when
server-side commands need them.

## Environment

| Variable | Default | Description |
|---|---|---|
| `AXON_MCP_HTTP_HOST` | `127.0.0.1` | HTTP bind host. Non-loopback binds require auth. |
| `AXON_MCP_HTTP_PORT` | `8001` | HTTP listen port. |
| `AXON_MCP_HTTP_TOKEN` | unset | Static bearer / `x-api-key` token. Required for non-loopback bearer mode. |
| `AXON_MCP_AUTH_MODE` | `bearer` | `bearer` for static token mode, `oauth` for Google OAuth + DCR through lab-auth. |
| `AXON_MCP_PUBLIC_URL` | unset | Public origin used by OAuth metadata, for example `https://axon.example.com`. |
| `AXON_SERVER_URL` | unset | CLI client/server endpoint. Set on client shells, not required by the server. |

## Examples

```bash
# Local loopback server
axon serve

# HTTP MCP-only entrypoint
axon serve mcp

# Bind for LAN/reverse-proxy use with static bearer auth
AXON_MCP_HTTP_HOST=0.0.0.0 \
AXON_MCP_HTTP_TOKEN="$(openssl rand -hex 32)" \
axon serve

# Host CLI talking to the running server
AXON_SERVER_URL=http://127.0.0.1:8001 axon status --json
AXON_SERVER_URL=http://127.0.0.1:8001 axon scrape https://example.com --json
```

## Notes

- Docker Compose publishes the server with `AXON_MCP_HTTP_PUBLISH` while the container binds `AXON_MCP_HTTP_HOST=0.0.0.0` internally.
- `/mcp` and `/v1/actions` share the same auth boundary.
- Server-owned jobs, output, screenshots, and artifacts live under the server process `AXON_DATA_DIR`.
- The old port `49000` websocket bridge, `49010` Next.js dev server, `49011` shell server, `/download/*`, and `/ws*` routes are not part of the current `axon serve` runtime.
