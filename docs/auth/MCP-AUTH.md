# MCP Auth — Axon
Last Modified: 2026-03-10

## Table of Contents

1. [Overview](#overview)
2. [How It Works](#how-it-works)
3. [Prerequisites](#prerequisites)
4. [Setup](#setup)
5. [OAuth Endpoints](#oauth-endpoints)
6. [Token Persistence](#token-persistence)
7. [Environment Variables](#environment-variables)
8. [MCP Client Configuration](#mcp-client-configuration)
9. [Verification](#verification)
10. [Security Model](#security-model)
11. [Troubleshooting](#troubleshooting)

---

## Overview

`axon mcp` supports `http`, `stdio`, and `both` transport modes. This document covers the HTTP transport only: the separate HTTP server (default port `8001`) exposes a single MCP tool (`axon`) over streamable HTTP (`/mcp`). Access to that HTTP surface supports dual auth modes: OAuth (Google broker) or a static MCP API key.

**Key facts:**
- `atk_` tokens issued by this broker are scoped to `/mcp` **only**. They have no effect on the WebSocket gate (`/ws`) or `/api/*` routes.
- `stdio` transport does not use this OAuth broker. It is a separate local-process transport choice.
- `/mcp` accepts either `Authorization: Bearer atk_...` (OAuth) or `Authorization: Bearer <AXON_MCP_API_KEY>` (static API key).
- OAuth state is persisted in Redis when available; in-memory fallback is used otherwise.
- MCP clients that support dynamic client registration (DCR) — Claude Desktop, mcporter, etc. — auto-register via `/oauth/register`.

---

## How It Works

```
[MCP Client — e.g. Claude Desktop, mcporter]
    │
    │  1. Discover metadata
    │     GET /.well-known/oauth-protected-resource
    │     GET /.well-known/oauth-authorization-server
    │
    │  2. Register (DCR — dynamic client registration)
    │     POST /oauth/register
    │
    │  3. Authorize
    │     GET /oauth/authorize  → redirect to Google login
    │     User signs in with Google account
    │     GET /oauth/google/callback (Google redirect)
    │     GET /oauth/authorize  → issues auth code
    │
    │  4. Exchange auth code for token
    │     POST /oauth/token
    │     → { "access_token": "atk_...", "token_type": "Bearer" }
    │
    │  5. Call MCP
    │     POST /mcp  Authorization: Bearer atk_...
    ▼
[axon mcp server — crates/mcp/]
    │  Validates Bearer token → executes MCP tool
```

The broker acts as an OAuth 2.0 authorization server, using Google as the identity provider. Users authenticate with Google, but the token issued to the MCP client (`atk_`) is Axon's own credential.

---

## Prerequisites

1. A Google OAuth 2.0 application with credentials.
2. The redirect URI configured in Google Cloud Console.
3. `axon mcp` running with HTTP transport enabled and reachable from the MCP client.
4. Redis running (optional but recommended — without it, tokens are lost on server restart).

---

## Setup

### 1. Create a Google OAuth application

1. Go to [Google Cloud Console](https://console.cloud.google.com/) → APIs & Services → Credentials.
2. Create an OAuth 2.0 Client ID (type: **Web application**).
3. Add the redirect URI:
   ```
   https://<your-axon-mcp-host>/oauth/google/callback
   ```
   For local dev: `http://localhost:8001/oauth/google/callback`
4. Copy the **Client ID** and **Client Secret**.

### 2. Set environment variables in `.env`

```bash
# Required — Google OAuth credentials
GOOGLE_OAUTH_CLIENT_ID=your-client-id.apps.googleusercontent.com
GOOGLE_OAUTH_CLIENT_SECRET=your-client-secret

# MCP server host/port (defaults shown)
AXON_MCP_HTTP_HOST=0.0.0.0
AXON_MCP_HTTP_PORT=8001
```

### 3. Start the MCP server

```bash
# From project root — binary reads .env automatically
AXON_MCP_HTTP_PORT=8001 cargo run --locked --bin axon -- serve mcp

# Dual mode also exposes the same OAuth-protected HTTP endpoint
AXON_MCP_HTTP_PORT=8001 cargo run --locked --bin axon -- mcp --transport both

# Or via just dev (starts everything including MCP server at port 8001)
just dev
```

### 4. Verify OAuth is active

```bash
curl -s http://localhost:8001/oauth/google/status | python3 -m json.tool
# → {"configured": true, "provider": "google"}

# Check well-known discovery document
curl -s http://localhost:8001/.well-known/oauth-authorization-server | python3 -m json.tool
```

---

## OAuth Endpoints

All endpoints are on the MCP server (default `http://localhost:8001`), not on the main Axon server (port `49000`).

| Endpoint | Method | Purpose |
|----------|--------|---------|
| `/.well-known/oauth-protected-resource` | GET | Resource server metadata (RFC 9470) |
| `/.well-known/oauth-authorization-server` | GET | Authorization server discovery (RFC 8414) |
| `/oauth/register` | POST | Dynamic Client Registration (RFC 7591) |
| `/oauth/authorize` | GET | Authorization endpoint — redirects to Google |
| `/oauth/token` | POST | Token endpoint — auth code → `atk_` bearer token |
| `/oauth/google/status` | GET | Check if Google OAuth is configured |
| `/oauth/google/login` | GET | Initiate Google sign-in (browser redirect) |
| `/oauth/google/callback` | GET | Google redirect callback |
| `/oauth/google/token` | GET | Current session token info |
| `/oauth/google/logout` | GET, POST | Invalidate session |
| `/mcp` | POST | MCP tool endpoint (requires `Authorization: Bearer atk_...` **or** `Authorization: Bearer <AXON_MCP_API_KEY>`) |

---

## Token Persistence

OAuth state is stored in Redis under the `GOOGLE_OAUTH_REDIS_PREFIX` prefix (default: `axon:oauth:`).

Stored record types:
- Pending login state
- Google session data
- `atk_` access tokens
- Refresh tokens
- Auth codes
- Rate-limit buckets

Session cookie: `__Host-axon_oauth_session`

**TTL semantics:**

| Record | TTL |
|--------|-----|
| OAuth session | 7 days |
| Refresh token | 30 days |
| Auth code | 10 minutes |
| Pending login state | 15 minutes |

When Redis is unavailable, the in-memory fallback is used. Tokens are lost when the `axon mcp` process restarts. For production use, configure Redis (`AXON_REDIS_URL`).

---

## Environment Variables

### Required

| Variable | Description |
|----------|-------------|
| `GOOGLE_OAUTH_CLIENT_ID` | Google OAuth 2.0 Client ID |
| `GOOGLE_OAUTH_CLIENT_SECRET` | Google OAuth 2.0 Client Secret |

### Optional overrides

| Variable | Default | Description |
|----------|---------|-------------|
| `AXON_MCP_HTTP_HOST` | `0.0.0.0` | MCP server bind address |
| `AXON_MCP_HTTP_PORT` | `8001` | MCP server port |
| `GOOGLE_OAUTH_AUTH_URL` | Google default | Override Google authorization URL |
| `GOOGLE_OAUTH_TOKEN_URL` | Google default | Override Google token URL |
| `GOOGLE_OAUTH_REDIRECT_PATH` | `/oauth/google/callback` | Override callback path |
| `GOOGLE_OAUTH_REDIRECT_HOST` | Inferred from request | Override redirect host (useful behind reverse proxies) |
| `GOOGLE_OAUTH_REDIRECT_URI` | Inferred | Full override for redirect URI |
| `GOOGLE_OAUTH_BROKER_ISSUER` | Inferred | OAuth issuer identifier |
| `GOOGLE_OAUTH_SCOPES` | `openid email profile` | Google OAuth scopes |
| `GOOGLE_OAUTH_DCR_TOKEN` | None | Static bearer token required for Dynamic Client Registration (empty = open registration) |
| `GOOGLE_OAUTH_REDIRECT_POLICY` | `loopback_or_https` | Callback URI policy (see below) |
| `GOOGLE_OAUTH_REDIS_URL` | `AXON_REDIS_URL` | Redis URL for OAuth state (falls back to `AXON_REDIS_URL`) |
| `GOOGLE_OAUTH_REDIS_PREFIX` | `axon:oauth:` | Key prefix in Redis |
| `AXON_MCP_API_KEY` | None | Optional static bearer token accepted on `/mcp` as an alternative to OAuth tokens |

### `GOOGLE_OAUTH_REDIRECT_POLICY` modes

| Value | Allowed callback URIs |
|-------|-----------------------|
| `loopback_or_https` (default) | `http://localhost/*`, `http://127.0.0.1/*`, `http://::1/*`, and any `https://` URI |
| `loopback_only` | Loopback HTTP only |
| `any` | Any HTTP/HTTPS URI |

---

## MCP Client Configuration

### Claude Desktop

Add to your Claude Desktop MCP config (`~/.claude/mcp.json` or the path given by `--mcp-config`):

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

Claude Desktop supports OAuth out of the box — it will open a browser window for Google sign-in on first connection.

If you want static API key auth instead of OAuth, use:

```json
{
  "mcpServers": {
    "axon": {
      "type": "http",
      "url": "http://localhost:8001/mcp",
      "headers": {
        "Authorization": "Bearer <AXON_MCP_API_KEY>"
      }
    }
  }
}
```

If you instead configure Claude Desktop to use `stdio` mode (`axon mcp --transport stdio`), this OAuth flow does not apply because there is no HTTP `/mcp` endpoint in that mode.

### mcporter

```bash
# Local repo config (stdio)
mcporter --config config/mcporter.json list axon --schema

# Call a tool
mcporter --config config/mcporter.json call axon.axon action:help
mcporter --config config/mcporter.json call axon.axon action:doctor
mcporter --config config/mcporter.json call axon.axon action:query query:"embedding pipeline"
```

To test OAuth over HTTP instead of the repo-local stdio config, register a separate mcporter entry:
```bash
mcporter add axon-http http://localhost:8001/mcp
mcporter list axon-http
mcporter call axon-http.axon action:help
```

### Smoke test

```bash
# Primary MCP smoke test (requires mcporter, jq, and a built debug binary)
bash ./scripts/test-mcp-tools-mcporter.sh
```

The smoke script uses [`config/mcporter.json`](/home/jmagar/workspace/axon_rust/config/mcporter.json), runs both full (`AXON_LITE=0`) and lite (`AXON_LITE=1`) suites, and writes logs under `.cache/mcporter-test/`.

---

## Verification

```bash
# 1. Check OAuth is configured
curl -s http://localhost:8001/oauth/google/status

# 2. Check discovery document
curl -s http://localhost:8001/.well-known/oauth-authorization-server | python3 -m json.tool

# 3. Check protected resource metadata
curl -s http://localhost:8001/.well-known/oauth-protected-resource | python3 -m json.tool

# 4. Verify /mcp rejects unauthenticated requests
curl -s -o /dev/null -w "%{http_code}\n" http://localhost:8001/mcp
# → 401
```

---

## Security Model

**`atk_` tokens are scoped to `/mcp` only.** They cannot be used for `/ws`, `/api/*`, or any other Axon endpoint. The MCP server and the main Axon server (`axon serve`) are separate processes on separate ports — they share no token state.

**Static MCP API key is an alternative auth mode for `/mcp`.** When `AXON_MCP_API_KEY` is set, the MCP server accepts `Authorization: Bearer <AXON_MCP_API_KEY>` for full `/mcp` access.

**Google is the identity provider.** Axon does not manage passwords or user accounts. Authentication is delegated entirely to Google. Only Google-authenticated users receive `atk_` tokens.

**Dynamic Client Registration can be open or gated.** By default, any MCP client can register via `/oauth/register`. Set `GOOGLE_OAUTH_DCR_TOKEN` to require a static bearer token for registration — prevents unknown clients from registering.

**Redis persistence.** In-memory fallback means tokens are lost on restart. For any production or persistent deployment, Redis (`AXON_REDIS_URL`) should be configured.

**Redirect URI validation.** The `GOOGLE_OAUTH_REDIRECT_POLICY` controls which callback URIs are accepted. The default (`loopback_or_https`) allows local dev clients and HTTPS production clients while blocking plain HTTP non-loopback URIs.

---

## Troubleshooting

### `/mcp` returns `401 Unauthorized`

Neither OAuth nor API key auth is configured, or the provided bearer token is invalid.

1. Configure Google OAuth credentials (`GOOGLE_OAUTH_CLIENT_ID`, `GOOGLE_OAUTH_CLIENT_SECRET`) **or** set `AXON_MCP_API_KEY`.
2. Restart `axon mcp`.

### OAuth redirect fails after Google sign-in

The redirect URI in your Google Cloud Console does not match the one Axon is using.

1. Check what Axon expects:
   ```bash
   curl -s http://localhost:8001/.well-known/oauth-authorization-server | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('authorization_endpoint',''))"
   ```
2. Ensure the redirect URI registered in Google Cloud Console matches exactly (including scheme, host, port, and path).
3. If running behind a reverse proxy (e.g. Tailscale Serve or Nginx), set `GOOGLE_OAUTH_REDIRECT_HOST` to the external hostname so the callback URL matches what Google expects.

### Tokens lost on restart

Redis is not configured or not reachable. Set `AXON_REDIS_URL` (or `GOOGLE_OAUTH_REDIS_URL`) to your Redis instance. Verify connectivity:

```bash
./scripts/axon doctor
# or
redis-cli -u "$AXON_REDIS_URL" ping
```

### MCP client says "OAuth configuration not found"

The MCP client cannot reach the `/.well-known/oauth-authorization-server` endpoint. Verify:
1. `axon mcp` is running: `ps aux | grep 'axon.*mcp'`
2. The port is correct (default `8001`).
3. If the client is remote (not localhost), verify network access to the MCP server port.

### `atk_` token rejected on `/ws`

Expected. MCP OAuth tokens are scoped to `/mcp` only. For WebSocket access, use the shared API token — see `docs/auth/README.md`.
