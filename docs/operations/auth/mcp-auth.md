---
title: "MCP Authentication"
created: 2026-03-10
updated: 2026-08-02
---

# MCP Authentication

Axon supports MCP over local stdio and over HTTP at `/mcp`.

- Local stdio runs as the local process identity and does not use HTTP bearer
  middleware.
- MCP-over-HTTP uses the same bearer/OAuth policy as the REST server.

## Static bearer mode

```dotenv
AXON_AUTH_MODE=bearer
AXON_HTTP_TOKEN=replace-with-a-long-random-secret
```

Connect with:

```http
Authorization: Bearer replace-with-a-long-random-secret
```

Legacy `x-api-key` is normalized to a bearer header by
`crates/axon-mcp/src/server/http.rs`.

## OAuth mode

Set `AXON_AUTH_MODE=oauth` and the required OAuth issuer/client/allowlist
variables documented in [Configuration](../../guides/configuration.md). OAuth
mode mounts the authorization routes and uses `lab-auth` through the shared
policy in `crates/axon-authz/src/http.rs`.

Bearer-only and loopback-development modes do not advertise OAuth resource
metadata or mount OAuth callback routes.

## Request flow

```text
HTTP request
  -> x-api-key normalization when present
  -> axon-authz AuthLayer
  -> authenticated caller context
  -> MCP action scope classification
  -> handler authorization and execution
```

Primary implementation:

- `crates/axon-authz/src/http.rs`
- `crates/axon-mcp/src/server/http.rs`
- `crates/axon-mcp/src/server/authz.rs`
- `crates/axon-mcp/src/server.rs`

## Client configuration

The Axon Claude plugin connects to:

```text
http://127.0.0.1:8001/mcp
```

and sends the configured bearer token. Other MCP clients should configure an
HTTP MCP endpoint and an `Authorization` header using their native settings.
Do not place secrets in a checked-in MCP configuration file.

## Authorization scopes

Authentication identifies the caller. The action registry then requires the
appropriate scope, such as read, write, admin, execute, or local. A valid token
does not automatically authorize every operation when the caller context is
scope-limited.

See [Runtime Auth](../../reference/runtime/auth.md) for the current scope model.

## Troubleshooting

| Symptom | Check |
|---|---|
| 401 | Missing, malformed, or invalid bearer token |
| 403 | Token is valid but lacks the action's required scope |
| OAuth routes absent | Server is not running in OAuth mode |
| Plugin cannot connect | Verify server URL uses port 8001 and ends with `/mcp` |
| `x-api-key` fails | Confirm the current server includes normalization middleware |

## Verification

```bash
cargo test -p axon-mcp auth --lib -- --nocapture
cargo test -p axon-web server_tests --lib -- --nocapture
cargo test -p axon-authz http --lib -- --nocapture
```
