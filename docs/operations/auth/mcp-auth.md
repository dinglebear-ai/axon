# MCP Auth — Axon
Last Modified: 2026-06-01

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
HTTP auth has two modes:

- **Bearer mode** (default): static `AXON_HTTP_TOKEN` accepted as
  `Authorization: Bearer ...` or `x-api-key`.
- **OAuth mode**: `AXON_AUTH_MODE=oauth` initializes lab-auth with Google
  OAuth, dynamic client registration, JWT bearer validation, and OAuth metadata
  endpoints. The static bearer token continues to work in dual mode when set.

**Key facts:**
- Auth is enforced by `src/mcp/auth.rs` via lab-auth `AuthLayer`.
- A shared static token can be configured via `AXON_HTTP_TOKEN`.
- OAuth mode is configured through `AXON_AUTH_MODE=oauth` and the
  `AXON_MCP_*` Google/public URL variables below.
- Clients authenticate using either header:
  - `Authorization: Bearer <AXON_HTTP_TOKEN>`
  - `x-api-key: <AXON_HTTP_TOKEN>`
- Token comparison uses constant-time equality (`subtle::ConstantTimeEq`).
- Loopback binds (`127.0.0.1`, `::1`, `localhost`) may run **without** auth —
  startup logs a warning. Non-loopback binds (`0.0.0.0`, public hostnames) **require**
  `AXON_HTTP_TOKEN` or `AXON_AUTH_MODE=oauth`; otherwise the server refuses
  to start in `build_auth_policy`.

---

## How It Works

```
[MCP Client]
    │  POST /mcp
    │  Authorization: Bearer <AXON_HTTP_TOKEN>
    ▼
[axum router (src/mcp/server/http.rs)]
    │  └─ host_validation_middleware  (HostAllowlist)
    │  └─ mcp_http_cors_middleware    (AXON_ALLOWED_ORIGINS)
    │  └─ AuthLayer                   (static bearer and/or OAuth JWT)
    ▼
[StreamableHttpService → AxonMcpServer]
    │  Tool dispatch by action / subaction
```

Bearer mode normalizes `x-api-key` to `Authorization: Bearer ...` before
lab-auth checks the request. If `AXON_HTTP_TOKEN` is unset and OAuth mode is
not active, the server runs unauthenticated only for loopback binds.

OAuth mode mounts the lab-auth router beside `/mcp`, including OAuth metadata,
JWKS, authorization, token, Google callback, and dynamic registration routes.

---

## Setup

### 1. Choose auth mode

For a private loopback-only development server, no auth is required. For
network-accessible servers, use either a static bearer token or OAuth.

### 2. Static bearer setup

```bash
# Generate a token (any cryptographically random secret works)
openssl rand -hex 32
```

```bash
# Required for non-loopback binds
AXON_HTTP_TOKEN=your-strong-random-token

# MCP server bind (defaults shown)
AXON_HTTP_HOST=127.0.0.1
AXON_HTTP_PORT=8001

# Optional CORS allowlist (comma-separated origins)
# Defaults to strict same-origin/loopback when unset.
AXON_ALLOWED_ORIGINS=
```

### 3. OAuth setup

```bash
AXON_AUTH_MODE=oauth
AXON_PUBLIC_URL=https://axon.example.com
AXON_GOOGLE_CLIENT_ID=your-google-client-id
AXON_GOOGLE_CLIENT_SECRET=your-google-client-secret
AXON_AUTH_ADMIN_EMAIL=you@example.com

# Optional. Claude's MCP callback is always included by default.
AXON_ALLOWED_REDIRECT_URIS=https://callback.example.com/callback/*
```

OAuth mode also accepts `AXON_HTTP_TOKEN` when set, so existing bearer
clients can continue working while OAuth clients use dynamic registration and
JWT bearer tokens. OAuth/JWT callers are scope-checked by MCP action: write
actions require an Axon write scope and read actions require an Axon read
scope, but `scope_satisfies` in `crates/axon-authz/src/lib.rs` treats either Axon scope
(`axon:read` or `axon:write`) as satisfying any Axon-scoped action, so a token
holding either scope reaches Axon-scoped routes. Admin/destructive operations
such as prune execution still require their configured admin/write checks;
unknown actions fail closed. The Google account matching
`AXON_AUTH_ADMIN_EMAIL` always receives the full configured Axon OAuth
scope set (`axon:read axon:write`) even if a client asks for a narrower scope.
Other allowlisted users keep the scope they requested.

### 4. Start the MCP server

```bash
# stdio transport (default for `axon mcp`)
axon mcp

# HTTP transport
axon serve mcp                         # default for the `serve mcp` subcommand
axon mcp --transport http              # explicit
axon mcp --transport both              # stdio + HTTP concurrently
```

The unified `axon serve` command (no subcommand) also starts MCP HTTP at
`/mcp` on the same port as the web panel.

---

## Environment Variables

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `AXON_HTTP_HOST` | no | `127.0.0.1` | MCP HTTP bind address. Non-loopback values require `AXON_HTTP_TOKEN`. |
| `AXON_HTTP_PORT` | no | `8001` | MCP HTTP listen port. |
| `AXON_HTTP_TOKEN` | conditional | unset | Bearer / `x-api-key` token. Required when bind is non-loopback. |
| `AXON_ALLOWED_ORIGINS` | no | unset | Comma-separated CORS allowlist. Unset = strict default (same-origin/loopback browsers only; non-browser tools unaffected). |
| `AXON_MCP_TRANSPORT` | no | per-command | Override transport for `axon mcp` / `axon serve mcp` (`stdio`, `http`, `both`). |
| `AXON_AUTH_MODE` | no | `bearer` | Set to `oauth` to enable Google OAuth + DCR. |
| `AXON_PUBLIC_URL` | oauth | -- | Public origin used in OAuth metadata, e.g. `https://axon.example.com`. |
| `AXON_GOOGLE_CLIENT_ID` | oauth | -- | Google OAuth client ID. |
| `AXON_GOOGLE_CLIENT_SECRET` | oauth | -- | Google OAuth client secret. |
| `AXON_AUTH_ADMIN_EMAIL` | oauth | -- | Admin email accepted by the auth layer; this account receives full Axon OAuth scopes. |
| `AXON_ALLOWED_REDIRECT_URIS` | no | Claude callback included | Additional comma-separated OAuth redirect URIs. |

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
        "Authorization": "Bearer YOUR_AXON_HTTP_TOKEN"
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
        "x-api-key": "YOUR_AXON_HTTP_TOKEN"
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
  --header "Authorization: Bearer $AXON_HTTP_TOKEN"
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
  -H "Authorization: Bearer $AXON_HTTP_TOKEN" \
  http://localhost:8001/mcp
# → 200 (or 405 / 406 depending on missing JSON-RPC body — auth has passed)

# 2. Verify /mcp rejects an invalid bearer token when the env var is set
curl -s -o /dev/null -w "%{http_code}\n" \
  -H "Authorization: Bearer wrong" \
  http://localhost:8001/mcp
# → 401

# 3. Discover tools via MCP JSON-RPC
curl -s -X POST http://localhost:8001/mcp \
  -H "Authorization: Bearer $AXON_HTTP_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list"}'
```

---

## Security Model

- **Static bearer token** — single shared secret, comparable in constant time.
- **OAuth mode** — Google OAuth + lab-auth JWT validation and dynamic client
  registration. OAuth mode can run alongside the static bearer token.
- **Loopback default** — startup refuses non-loopback binds without bearer or OAuth
  auth, so a forgotten env var on a public host fails closed instead of running
  unauthenticated.
- **Host allowlist** — `host_validation_middleware` rejects requests whose `Host`
  header is not in the loopback set or `AXON_ALLOWED_ORIGINS`.
- **CORS allowlist** — `mcp_http_cors_middleware` restricts browser-origin requests.
- **stdio is unauthenticated** — relies on OS process boundaries: the MCP client
  spawns the server with its own environment.

---

## Troubleshooting

### `/mcp` returns `401 Unauthorized`

`AXON_HTTP_TOKEN` is set on the server but the client either omitted the
header or sent the wrong value.

1. Confirm the server token is present without printing it:
   `awk -F= '$1=="AXON_HTTP_TOKEN" && length($2)>0 { print "AXON_HTTP_TOKEN is set" }' ~/.axon/.env`
2. Verify the request header matches exactly. Both schemes are accepted:
   - `Authorization: Bearer <token>`
   - `x-api-key: <token>`
3. Bearer token comparison is case-sensitive; only the `Bearer` scheme name is
   case-insensitive.

### Server refuses to start with `refusing to start unauthenticated MCP HTTP server`

You bound to a non-loopback address (e.g. `0.0.0.0`) without configuring bearer
or OAuth auth. Either bind to `127.0.0.1` / `localhost`, set
`AXON_HTTP_TOKEN`, or set `AXON_AUTH_MODE=oauth` with the required OAuth
variables.

### MCP client cannot connect

1. Verify the server is running: `pgrep -fa 'axon.*mcp'`
2. Verify the port: `ss -ltn 'sport = :8001'`
3. For remote clients, confirm the host firewall allows inbound traffic.
4. Run `axon doctor` to verify infrastructure connectivity (Qdrant, TEI).

### OAuth metadata is missing

OAuth routes are only mounted when `AXON_AUTH_MODE=oauth`. Confirm
`AXON_PUBLIC_URL`, Google client credentials, and admin email are present in
the server environment, then restart `axon serve`.

In OAuth mode, Axon advertises protected-resource metadata at
`$AXON_PUBLIC_URL/.well-known/oauth-protected-resource`. The metadata
document's `resource` value remains `$AXON_PUBLIC_URL/mcp`, matching the
canonical MCP endpoint and token audience.
