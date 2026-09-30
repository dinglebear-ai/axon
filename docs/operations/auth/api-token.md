---
title: "API Tokens and Panel Credentials"
created: 2026-03-10
updated: 2026-09-30
---

# API Tokens and Panel Credentials

Last reviewed: 2026-09-29

Axon separates API/MCP authentication, panel unlock, and third-party provider
credentials. This is a navigation map, not a complete inventory of every
secret used by an installation. See [security](../security.md) and the
[generated environment registry](../../reference/config/env.md).

## Quick Map

| Credential | Purpose | Reference |
|---|---|---|
| `AXON_HTTP_TOKEN` | Static bearer/API-key authorization for protected HTTP API and MCP | [HTTP token](#mcp-http-token) |
| Configured OAuth credentials and issued tokens | Public-origin discovery, login, caller identity and scope | [OAuth](#mcp-oauth) |
| Panel password | Administrative panel unlock | [Panel password](#web-panel-password) |
| Provider credentials | Access to a selected acquisition, embedding, search or synthesis provider | [Third-party credentials](#third-party-credentials) |

A panel password is not an API token. A provider key is not a credential for
the Axon HTTP listener. Metadata discovery does not authorize tool execution.

## MCP HTTP token

`AXON_HTTP_TOKEN` enables static authorization with `Authorization: Bearer
<token>` or the supported `x-api-key` header. It protects the shared HTTP
boundary, not only one legacy MCP action. The stdio transport has no HTTP
auth handshake but still runs under local process and operation authority.

Loopback-only development can omit network auth. A non-loopback listener
requires the configured token or OAuth; a blank/whitespace value is not
protection. Verify the effective service configuration and listener.

Generate a strong secret through the approved credential process and place
its literal value in the actual service environment. A dotenv file does not
execute shell command substitution: do not put a literal
`$(openssl rand -hex 32)` expression into it and assume a random token was
created. Do not expose token values in shell history, logs, or examples.

See [MCP authentication](mcp-auth.md) for accepted headers, OAuth coexistence,
scope semantics, and invalid-credential behavior. Implementation is shared
in [axon-authz](../../../crates/axon-authz/src/) and re-exported by
[MCP auth](../../../crates/axon-mcp/src/auth.rs).

## MCP OAuth

`AXON_AUTH_MODE=oauth` uses the configured public origin, Google client ID
and secret, allowed identity, and redirect settings. The source of truth is
[MCP authentication](mcp-auth.md), not a stale `AXON_MCP_*` alias table.
A static token can remain configured for supported dual-mode access.

Scope compatibility and admin identity behavior must be checked in the
current authorization implementation. Do not assume a descriptive tool hint
or requested narrow scope overrides the server policy. Keep caller identity
and authorization intact through system/watch requests, tasks, resources,
and legacy/atomic projections.

## Web panel password

The panel uses a file-backed shared password under the configured Axon home.
[Panel auth](../../../crates/axon-web/src/auth.rs) creates a missing password
with random bytes, exclusive creation, and restrictive Unix file mode.
Existing-file reads are a separate path; verify ownership, permissions, and
symlink state instead of assuming creation checks apply to every read.

The login handler currently returns the same credential as the panel token.
Treat it as a bearer secret, not a separate short-lived or independently
revocable session. Everyone possessing it has the associated panel access.
Use the current router/auth code for protected routes rather than a static
list claiming no other endpoints exist.

For a planned rotation, identify the effective credential path, service,
backup/access implications, and all clients before making changes. Follow
an approved replacement/restart procedure; this page does not authorize
deleting a live password file or rotating API/provider credentials.

## Third-party credentials

Provider credentials can include GitHub/Reddit API access, embedding/search
keys, synthesis credentials, and CLI-provider identity. They authorize the
provider interaction, not the Axon client. Configure only the selected
provider and preserve unrelated secrets.

Secrets may live in service environments, explicit environment files,
provider stores, OAuth state, and client configuration; `~/.axon/.env` is
not the only secret store. TOML unknown-key rejection is not a generic
secret detector. Never commit credentials in accepted string/header fields.

## Verification and recovery

After an authorized credential/configuration change, verify the actual
service loaded it, reconnect the intended client, discover the catalog, and
make a safe read-only call. Test missing/wrong credentials separately from
origin/host restrictions and operation-scope denial. A healthy process or
new configuration entry alone does not prove authenticated access.

Report the failing hop and safe recovery action without dumping tokens.
Panel unlock, API auth, source authorization, and provider credentials are
different prerequisites. See [client setup](../../reference/mcp/connect.md).
