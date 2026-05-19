# Session Log: MCP HTTP + Google OAuth + Codex Login Compatibility
Date: 2026-03-03
Project: axon_rust

## Goal
Implement Google OAuth on the MCP HTTP server and make `codex mcp login axon-http` work end-to-end.

## What Was Implemented

### 1) MCP HTTP transport + runtime auth gate
- Added MCP transport selection support (`stdio|http|both`) in CLI/config plumbing.
- Implemented HTTP transport server at `/mcp`.
- Added Google OAuth routes:
  - `/oauth/google/status`
  - `/oauth/google/login`
  - `/oauth/google/callback`
  - `/oauth/google/token`
  - `/oauth/google/logout`
- Added middleware protection so `/mcp` returns `401` when not authorized.

### 2) MCP auth discovery compliance
- Added auth discovery behavior required by MCP clients:
  - `WWW-Authenticate: Bearer resource_metadata="..."` on `/mcp` unauthorized responses
  - `/.well-known/oauth-protected-resource`
- Added authorization server metadata endpoint:
  - `/.well-known/oauth-authorization-server`

### 3) OAuth broker for Codex dynamic registration flow
Built broker endpoints so `codex mcp login` can complete without Google dynamic registration support:
- `POST /oauth/register` (dynamic client registration)
- `GET /oauth/authorize` (auth code issuance; redirects to Google login when needed)
- `POST /oauth/token` (authorization_code + refresh_token grants)
- PKCE support (`S256`/`plain`)
- Bearer token validation on `/mcp` (access token store + expiry)

## Key Files Changed
- `Cargo.toml`
  - RMCP feature set updated to include:
    - `transport-streamable-http-server`
    - `transport-streamable-http-server-session`
    - `transport-worker`
- `crates/mcp/server.rs`
  - Routed all OAuth discovery/broker endpoints
- `crates/mcp/server/oauth_google.rs`
  - Implemented Google OAuth flow + OAuth broker + auth middleware
- `crates/core/config/cli/mod.rs`
- `crates/core/config/parse/build_config.rs`
- `crates/core/config/types/config.rs`
- `crates/core/config/types/config_impls.rs`
- `crates/cli/commands/mcp.rs`
- `crates/mcp/mod.rs`

## Verification Performed

### Server behavior
- `/mcp` returns 401 with discovery header when unauthorized:
  - `WWW-Authenticate: Bearer resource_metadata="http://127.0.0.1:8001/.well-known/oauth-protected-resource"`
- `/.well-known/oauth-protected-resource` returns valid JSON.
- `/.well-known/oauth-authorization-server` returns valid JSON including:
  - issuer
  - authorization_endpoint
  - token_endpoint
  - registration_endpoint

### Login flow
- `codex mcp login axon-http` completed successfully during verification run.
- Failure mode identified when server is down:
  - `Error: No authorization support detected`
  - Root cause: nothing listening on `127.0.0.1:8001`

## Operational Notes
- `codex mcp login axon-http` requires the HTTP MCP server running on port `8001`.
- If server is down, login command cannot discover auth support.
- Start command:
  - `./target/debug/axon mcp --transport http --host 127.0.0.1 --port 8001`

## Final State
- Google OAuth for MCP HTTP is implemented.
- MCP auth discovery endpoints are in place.
- Codex dynamic registration/login path is supported via local OAuth broker.
- Tool calls through `axon-http` are functional after auth.
