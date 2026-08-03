---
title: "Security"
updated: 2026-08-02
---

# Security

Axon is self-hosted, but its source pipeline accepts untrusted URLs, files,
tool output, and remote content. Security boundaries apply at transport,
authorization, acquisition, storage, logging, and presentation layers.

## Authentication and authorization

REST and MCP authenticate at the transport boundary. Authorization is owned by
`axon-authz` and uses caller context, scopes, execution affinity, and
visibility policy.

Primary scopes:

- `axon:read`
- `axon:write`
- `axon:admin`
- `axon:execute`
- `axon:local`

Non-loopback HTTP exposure requires configured token or OAuth authentication.
Loopback location alone does not grant remote callers local-file or tool
execution authority. See [Runtime Auth](../reference/runtime/auth.md) and the
[API token](auth/api-token.md) / [MCP auth](auth/mcp-auth.md) runbooks.

## URL and SSRF protection

URL validation and HTTP-client policy live in `crates/axon-core/src/http/`:

- `ssrf.rs` rejects loopback, link-local, private, and otherwise prohibited
  destinations.
- `SsrfBlockingResolver` revalidates resolved addresses at connection time to
  close DNS-rebinding windows.
- `client.rs` applies redirect-time validation and shared HTTP policy.
- conditional and impersonated clients retain the same connect-time boundary.

The web adapter also applies discovery-time filtering under
`crates/axon-adapters/src/web_engine/`. Every newly introduced network client
must either use the shared boundary or be explicitly covered by the fetch-
divergence gate.

## Local sources

Local paths require local authority and containment checks. Never infer that a
path is safe merely because the caller supplied it. Secret-like paths,
traversal, and execution-affinity violations fail closed before acquisition.

## Tool sources

CLI-tool and MCP-tool sources require `axon:execute` and declared allowlists.
The runtime must not execute arbitrary caller-provided commands by default.
Tool output remains untrusted source material and passes through redaction and
normalization before public exposure or indexing.

## Secrets and configuration

- Store secrets and endpoint credentials in `~/.axon/.env`.
- Keep `~/.axon/config.toml` non-secret.
- Do not place tokens in command examples, logs, job metadata, artifacts, or
  generated docs.
- Treat unknown TOML fields and removed environment keys as configuration
  errors rather than silently accepting them.

The complete environment ownership model is in
[Configuration](../guides/configuration.md).

## Redaction

Redaction is owned by `crates/axon-core/src/redact/` and is applied at public
boundaries including vector payloads, job events, graph evidence, memory,
artifacts, logs/traces, CLI JSON, and REST/MCP errors. Public output must fail
closed when sensitive content cannot be safely represented.

See [Runtime Redaction](../reference/runtime/redaction.md).

## Durable authorization

Jobs store an immutable authorization snapshot. Workers enforce the permissions
captured at enqueue time; a later configuration change must not silently grant
a queued job broader access.

Provider reservations, retries, watches, recovery, and detached execution do
not bypass that snapshot.

## Network exposure

The default server bind is `127.0.0.1:8001`. When publishing Axon beyond
loopback:

1. Configure token or OAuth authentication.
2. Restrict ingress with the host firewall, reverse proxy, VPN, or Tailscale
   policy.
3. Keep Qdrant, TEI, Chrome/CDP, and SQLite inaccessible to untrusted clients.
4. Validate `/readyz` and auth behavior from both trusted and untrusted
   network positions.
5. Do not expose Chrome debugging endpoints publicly.

## Destructive operations

Administrative cleanup, reset, prune execution, provider mutation, and other
destructive operations require explicit admin authority and confirmation where
specified. Use plan/dry-run forms before execution.

## Verification

Repository gates covering these boundaries include:

- layering and reserved-call checks
- fetch-divergence checks
- schema and removed-surface checks
- cross-surface scope tests
- redaction and authorization tests
- CodeQL and dependency/security CI

Run `cargo xtask check-fetch-divergence`, the relevant focused tests, and
`just precommit` before shipping security-sensitive changes.

## Source map

- `crates/axon-authz/src/`
- `crates/axon-core/src/http/`
- `crates/axon-core/src/redact/`
- `crates/axon-adapters/src/acquisition_security.rs`
- `crates/axon-services/src/source/authorize.rs`
- `crates/axon-mcp/src/server/authz.rs`
- `crates/axon-web/src/server/`
