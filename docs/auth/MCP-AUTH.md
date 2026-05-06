# MCP Auth — Axon
Last Modified: 2026-05-06

## Table of Contents

1. [Overview](#overview)
2. [How It Works](#how-it-works)
3. [Setup](#setup)
4. [Environment Variables](#environment-variables)
5. [MCP Client Configuration](#mcp-client-configuration)
6. [Verification](#verification)
7. [Security Model](#security-model)
8. [Troubleshooting](#troubleshooting)

---

## Overview

`axon mcp` supports `stdio`, `http`, and `both` transport modes. This document covers
authentication on the HTTP transport. The `stdio` transport runs as a child process
of the MCP client and has no network listener, so no auth is applied there.

The HTTP transport exposes a single MCP tool (`axon`) over streamable HTTP at `/mcp`.
Access is gated by a simple bearer token (`AXON_MCP_HTTP_TOKEN`). There is **no
OAuth broker, no Google sign-in, no dynamic client registration, and no `atk_`
token issuance** in the current code base — those features are not implemented.

**Key facts:**
- Auth is enforced by `crates/mcp/auth.rs` via `mcp_auth_middleware`.
- A single shared static token is configured via `AXON_MCP_HTTP_TOKEN`.
- Clients authenticate using either header:
  - `Authorization: Bearer <AXON_MCP_HTTP_TOKEN>`
  - `x-api-key: <AXON_MCP_HTTP_TOKEN>`
- Token comparison uses constant-time equality (`subtle::ConstantTimeEq`).
- Loopback binds (`127.0.0.1`, `::1`, `localhost`) may run **without** a token —
  startup logs a warning. Non-loopback binds (`0.0.0.0`, public hostnames) **require**
  a token; otherwise the server refuses to start (`enforce_mcp_http_startup_policy`).

---

## How It Works

```
[MCP Client]
    │  POST /mcp
    │  Authorization: Bearer <AXON_MCP_HTTP_TOKEN>
    ▼
[axum router (crates/mcp/server/http.rs)]
    │  └─ host_validation_middleware  (HostAllowlist)
    │  └─ mcp_http_cors_middleware    (AXON_MCP_ALLOWED_ORIGINS)
    │  └─ mcp_auth_middleware         (AXON_MCP_HTTP_TOKEN check)
    ▼
[StreamableHttpService → AxonMcpServer]
    │  Tool dispatch by action / subaction
```

The middleware extracts the token from the `Authorization: Bearer …` or
`x-api-key` header. If `AXON_MCP_HTTP_TOKEN` is unset, the middleware allows the
request through and emits a one-time warning (allowed only because the startup
policy already verified the bind is loopback).

---

## Setup

### 1. Pick a strong token

```bash
# Generate a token (any cryptographically random secret works)
openssl rand -hex 32
```

### 2. Set environment variables in `.env`

```bash
# Required for non-loopback binds
AXON_MCP_HTTP_TOKEN=your-strong-random-token

# MCP server bind (defaults shown)
AXON_MCP_HTTP_HOST=127.0.0.1
AXON_MCP_HTTP_PORT=8001

# Optional CORS allowlist (comma-separated origins)
# Defaults to strict same-origin/loopback when unset.
AXON_MCP_ALLOWED_ORIGINS=
```

### 3. Start the MCP server

```bash
# stdio transport (default for `axon mcp`)
axon mcp

# HTTP transport
axon serve mcp                         # default for the `serve mcp` subcommand
axon mcp --transport http              # explicit
axon mcp --transport both              # stdio + HTTP concurrently
```

The unified `axon serve` command (no subcommand) also starts MCP HTTP at
`/mcp` on the same port as the web UI.

---

## Environment Variables

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `AXON_MCP_HTTP_HOST` | no | `127.0.0.1` | MCP HTTP bind address. Non-loopback values require `AXON_MCP_HTTP_TOKEN`. |
| `AXON_MCP_HTTP_PORT` | no | `8001` | MCP HTTP listen port. |
| `AXON_MCP_HTTP_TOKEN` | conditional | unset | Bearer / `x-api-key` token. Required when bind is non-loopback. |
| `AXON_MCP_ALLOWED_ORIGINS` | no | unset | Comma-separated CORS allowlist. Unset = strict default (same-origin/loopback browsers only; non-browser tools unaffected). |
| `AXON_MCP_TRANSPORT` | no | per-command | Override transport for `axon mcp` / `axon serve mcp` (`stdio`, `http`, `both`). |

**There are no `GOOGLE_OAUTH_*` variables, no `AXON_MCP_API_KEY`, and no Redis
prefix used by the MCP HTTP server.** Earlier revisions of this document
referenced an OAuth broker — that feature was never implemented. Search
`crates/mcp/auth.rs` for the full set of token-related code.

---

## MCP Client Configuration

### Claude Code (HTTP transport)

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

Or with `x-api-key`:

```json
{
  "mcpServers": {
    "axon": {
      "type": "http",
      "url": "http://localhost:8001/mcp",
      "headers": {
        "x-api-key": "YOUR_AXON_MCP_HTTP_TOKEN"
      }
    }
  }
}
```

### Claude Code (stdio transport)

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

stdio transport does not perform any auth check — the MCP client owns the
process lifecycle and the binary inherits its environment.

### mcporter

```bash
# Repo-local stdio config
mcporter --config config/mcporter.json list axon --schema
mcporter --config config/mcporter.json call axon.axon action:doctor

# HTTP transport
mcporter add axon-http http://localhost:8001/mcp \
  --header "Authorization: Bearer $AXON_MCP_HTTP_TOKEN"
mcporter call axon-http.axon action:help
```

### Smoke test

```bash
# Primary MCP smoke test (requires mcporter, jq, and a built debug binary)
bash ./scripts/test-mcp-tools-mcporter.sh
```

The smoke harness uses `config/mcporter.json` and writes logs under
`.cache/mcporter-test/`.

---

## Verification

```bash
# 1. Verify /mcp accepts a valid bearer token
curl -s -o /dev/null -w "%{http_code}\n" \
  -H "Authorization: Bearer $AXON_MCP_HTTP_TOKEN" \
  http://localhost:8001/mcp
# → 200 (or 405 / 406 depending on missing JSON-RPC body — auth has passed)

# 2. Verify /mcp rejects an invalid bearer token when the env var is set
curl -s -o /dev/null -w "%{http_code}\n" \
  -H "Authorization: Bearer wrong" \
  http://localhost:8001/mcp
# → 401

# 3. Discover tools via MCP JSON-RPC
curl -s -X POST http://localhost:8001/mcp \
  -H "Authorization: Bearer $AXON_MCP_HTTP_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list"}'
```

---

## Security Model

- **Static bearer token** — single shared secret, comparable in constant time.
- **Loopback default** — startup refuses non-loopback binds without a token, so a
  forgotten env var on a public host fails closed instead of running unauthenticated.
- **Host allowlist** — `host_validation_middleware` rejects requests whose `Host`
  header is not in the loopback set or `AXON_MCP_ALLOWED_ORIGINS`.
- **CORS allowlist** — `mcp_http_cors_middleware` restricts browser-origin requests.
- **No user accounts** — there is no per-user auth, no OAuth, no DCR. Any caller
  with the token has full access to the `axon` tool.
- **stdio is unauthenticated** — relies on OS process boundaries: the MCP client
  spawns the server with its own environment.

---

## Troubleshooting

### `/mcp` returns `401 Unauthorized`

`AXON_MCP_HTTP_TOKEN` is set on the server but the client either omitted the
header or sent the wrong value.

1. Confirm the server token: `grep AXON_MCP_HTTP_TOKEN .env`
2. Verify the request header matches exactly. Both schemes are accepted:
   - `Authorization: Bearer <token>`
   - `x-api-key: <token>`
3. Bearer token comparison is case-sensitive; only the `Bearer` scheme name is
   case-insensitive.

### Server refuses to start with `refusing to start unauthenticated MCP HTTP server`

You bound to a non-loopback address (e.g. `0.0.0.0`) without setting
`AXON_MCP_HTTP_TOKEN`. Either bind to `127.0.0.1` / `localhost` or set the token.

### MCP client cannot connect

1. Verify the server is running: `pgrep -fa 'axon.*mcp'`
2. Verify the port: `ss -ltn 'sport = :8001'`
3. For remote clients, confirm the host firewall allows inbound traffic.
4. Run `axon doctor` to verify infrastructure connectivity (Qdrant, TEI).

### Where is the OAuth flow?

It does not exist. Earlier docs referenced a Google OAuth broker / `atk_`
tokens / `/.well-known/oauth-*` endpoints — none of that is implemented in
`crates/mcp/`. The only auth path is `AXON_MCP_HTTP_TOKEN` enforced by
`mcp_auth_middleware` in `crates/mcp/auth.rs`. If you need OAuth, run the MCP
server behind a reverse proxy that performs the OAuth handshake and forwards
the bearer token (or none, on loopback) to `/mcp`.
