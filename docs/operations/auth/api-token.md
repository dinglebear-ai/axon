---
title: "Static API Token Authentication"
created: 2026-03-10
updated: 2026-08-02
---

# Static API Token Authentication

Axon can protect REST and MCP-over-HTTP with a static bearer token configured by
`AXON_HTTP_TOKEN`.

## Configure

Add a strong random value to `~/.axon/.env`:

```dotenv
AXON_AUTH_MODE=bearer
AXON_HTTP_TOKEN=replace-with-a-long-random-secret
```

Restart the owning `axon serve` process after changing authentication
configuration.

## Send the token

Preferred header:

```http
Authorization: Bearer replace-with-a-long-random-secret
```

Axon also accepts `x-api-key` for existing clients; the request is normalized
to a bearer header before the shared auth layer evaluates it.

```bash
curl -H "Authorization: Bearer $AXON_HTTP_TOKEN"   http://127.0.0.1:8001/v1/jobs

curl -H "x-api-key: $AXON_HTTP_TOKEN"   http://127.0.0.1:8001/v1/jobs
```

## Runtime ownership

- Policy and `lab-auth` layer construction:
  `crates/axon-authz/src/http.rs`
- MCP-over-HTTP router integration and API-key normalization:
  `crates/axon-mcp/src/server/http.rs`
- REST/server middleware and error conversion:
  `crates/axon-web/src/server/routing.rs`
- MCP action scope authorization:
  `crates/axon-mcp/src/server/authz.rs`

Static token validation uses constant-time comparison through the shared auth
layer. REST and MCP share the same transport authentication policy.

## Bind policy

The default bind is loopback. Before publishing beyond loopback:

1. Configure a non-empty token or OAuth mode.
2. Restrict ingress with a firewall, reverse proxy, VPN, or Tailscale ACL.
3. Keep Qdrant, TEI, Chrome/CDP, and SQLite private.
4. Test both missing-token and wrong-token requests.

A whitespace-only `AXON_HTTP_TOKEN` is ignored and produces a startup warning.

## Verification

```bash
curl -i http://127.0.0.1:8001/v1/jobs
curl -i -H 'Authorization: Bearer wrong' http://127.0.0.1:8001/v1/jobs
curl -i -H "Authorization: Bearer $AXON_HTTP_TOKEN"   http://127.0.0.1:8001/v1/jobs
```

The first two requests should be rejected when bearer auth is active; the final
request should reach the route and then be subject to its authorization scope.

## Rotation

1. Generate a replacement secret.
2. Update `~/.axon/.env`.
3. Restart Axon.
4. Update every client and plugin connection.
5. Verify the old token fails and the new token succeeds.

Never place the token in `config.toml`, command history, screenshots, logs, or
committed examples.
