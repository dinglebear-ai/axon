# axon serve
Last Modified: 2026-06-01

<!-- BEGIN GENERATED ACTION SURFACES -->
## Surfaces

| Surface | Entry point |
|---|---|
| CLI | `axon serve ...` |
| REST | Deferred |
| MCP | Not exposed as a dedicated MCP action. |
| Service | `HTTP server startup` |

Parity notes: Starts this server; not a route.
<!-- END GENERATED ACTION SURFACES -->


Start Axon's unified HTTP server.

## Synopsis

```bash
axon serve
axon serve mcp [--transport http|both]
```

## What It Runs

`axon serve` starts one Axum HTTP server on
`AXON_MCP_HTTP_HOST:AXON_MCP_HTTP_PORT` (default `127.0.0.1:8001`). HTTP MCP
transport uses this same server and port; there is no separate MCP-only HTTP
listener in the normal command path.

Mounted surfaces:

- `POST /mcp` - MCP streamable HTTP transport.
- `GET /v1/capabilities` - first-party HTTP API capability metadata.
- Direct `/v1` REST routes - HTTP API surface (e.g. `POST /v1/ask`) for external clients and the web panel.
- `/api/panel/*` - local setup/config panel APIs.
- Static web panel assets.
- OAuth metadata and auth routes when `AXON_MCP_AUTH_MODE=oauth`.

`serve` is the only way to expose Axon over HTTP — the `axon` CLI and MCP server
otherwise run every action in-process. `serve` does not supervise Next.js, a shell
WebSocket server, or separate worker processes. In-process workers are initialized
lazily by the service context when API requests need them.

## Environment

| Variable | Default | Description |
|---|---|---|
| `AXON_MCP_HTTP_HOST` | `127.0.0.1` | HTTP bind host. Non-loopback binds require auth. |
| `AXON_MCP_HTTP_PORT` | `8001` | HTTP listen port. |
| `AXON_MCP_HTTP_TOKEN` | unset | Static bearer / `x-api-key` token. Required for non-loopback bearer mode. |
| `AXON_MCP_AUTH_MODE` | `bearer` | `bearer` for static token mode, `oauth` for Google OAuth + DCR through lab-auth. |
| `AXON_MCP_PUBLIC_URL` | unset | Public origin used by OAuth metadata, for example `https://axon.example.com`. |
| `AXON_MCP_GOOGLE_CLIENT_ID` | unset | Required for OAuth mode. |
| `AXON_MCP_GOOGLE_CLIENT_SECRET` | unset | Required for OAuth mode. |
| `AXON_MCP_AUTH_ADMIN_EMAIL` | unset | Required for OAuth mode. |

## Examples

```bash
# Local loopback server
axon serve

# Equivalent unified web + MCP HTTP entrypoint
axon serve mcp

# Bind for LAN/reverse-proxy use with static bearer auth
AXON_MCP_HTTP_HOST=0.0.0.0 \
AXON_MCP_HTTP_TOKEN="$(openssl rand -hex 32)" \
axon serve

# Call the HTTP API from an external client
curl -s -H "Authorization: Bearer $AXON_MCP_HTTP_TOKEN" \
  -H 'content-type: application/json' \
  -d '{"query":"what changed?"}' http://127.0.0.1:8001/v1/ask
```

## Notes

- Docker Compose publishes the server with `AXON_MCP_HTTP_PUBLISH` while the container binds `AXON_MCP_HTTP_HOST=0.0.0.0` internally.
- `/mcp`, direct `/v1` REST routes, and the web panel are mounted on the same listener.
- Server-owned jobs, output, screenshots, and artifacts live under the server process `AXON_DATA_DIR`.
- The old port `49000` websocket bridge, `49010` Next.js dev server, `49011` shell server, `/download/*`, and `/ws*` routes are not part of the current `axon serve` runtime.
