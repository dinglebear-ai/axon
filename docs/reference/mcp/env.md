# MCP Environment Variables -- Axon

Last reviewed: 2026-09-29

MCP shares typed Axon configuration with CLI and HTTP services. The
[generated environment inventory](../config/env.md) owns the complete names
and parsing contract; this page describes MCP-specific behavior and migration.
Precedence is **CLI > environment > TOML > defaults**.

## Listener, projection, and authentication

| Variable | Purpose |
|---|---|
| `AXON_MCP_TRANSPORT` | `stdio`, `http`, or `both`; command defaults and listener behavior are described in [transport](transport.md) |
| `AXON_MCP_TOOL_PROJECTION` | Startup-static legacy aggregate, atomic leaves, or both; reconnect clients after a change |
| `AXON_HTTP_HOST`, `AXON_HTTP_PORT` | Unified HTTP listener, default loopback port 8001 |
| `AXON_HTTP_TOKEN` | Static bearer/API-key authorization when using bearer mode |
| `AXON_AUTH_MODE` | Select configured bearer or OAuth behavior |
| `AXON_PUBLIC_URL` | Public origin for OAuth discovery and protected-resource metadata |
| `AXON_ALLOWED_ORIGINS` | Explicit browser-origin policy; not a substitute for authorization |

Non-loopback HTTP requires a bearer token or correctly configured OAuth.
Panel unlock passwords and sessions do not authorize API/MCP calls. OAuth
requires its configured client/secret, allowed identity, redirect, and public
origin settings; follow [the auth guide](../../operations/auth/mcp-auth.md)
instead of copying a token from an unrelated service. Never commit secrets.

The deprecated projection alias has narrowly defined compatibility behavior;
see [projection selection](overview.md#projection-selection-and-compatibility).
It does not mean removed configuration keys remain valid.

## Artifacts and local source selection

`AXON_MCP_ARTIFACT_DIR` selects response artifact storage;
`AXON_INLINE_BYTES_THRESHOLD` controls auto-inline selection. Returned
artifacts expose opaque IDs, not public server paths. Size/visibility rules
still apply to inline responses.

`AXON_SOURCE_LOCAL_ALLOWED_ROOTS` is the canonical local acquisition root
setting. `AXON_MCP_EMBED_ALLOWED_ROOTS` is rejected during configuration
validation; rename it before restart. A client filesystem path is evaluated
on the executing server with its authorization and containment policy.

The legacy `AXON_MCP_EMBED_MAX_LOCAL_BYTES`,
`AXON_MCP_EMBED_MAX_LOCAL_DEPTH`, and `AXON_MCP_EMBED_MAX_LOCAL_ENTRIES`
values are parsed into configuration, but the current shared source pipeline
does not consume those fields as its acquisition limits. Do not rely on the
old per-MCP 10 MiB/16-level/10,000-entry table as an enforced safety boundary.
Use the [local source guide](../../guides/local-sources.md), adapter policy,
and operation diagnostics for actual limits. A configuration declaration is
not evidence that every adapter enforces it.

## Tasks and worker lifetime

Only extraction start supports negotiated MCP protocol tasks. The current
server returns terminal task data from `tasks/get`; it does not implement
the old blocking `tasks/result` flow. The retained tuning helper for
`AXON_TASK_RESULT_WAIT_TIMEOUT_SECS` has no current MCP task-handler caller.
Do not describe it as an active result-wait deadline.

Durable jobs need active workers. The long-lived hosting context controls
workers and watch scheduling; inspect job and worker liveness rather than
assuming a transport selection alone completes queued work. See
[jobs](../runtime/jobs.md) and [task support](tool-contract.md#task-support).

## Provider settings and removed keys

MCP operations use the configured vector, embedding, synthesis, browser, and
source providers. `QDRANT_URL`, `TEI_URL`, selected LLM settings, and
adapter credentials are operation prerequisites, not alternative job stores.
The complete definitions belong to [configuration](../../guides/configuration.md).

`AXON_OPENAI_MODEL` is rejected: use `AXON_SYNTHESIS_OPENAI_MODEL`.
Generic CLI forwarding keys `AXON_SERVER_URL`, `AXON_LOCAL_MODE`, and
`AXON_SERVER_INSECURE` do not turn CLI commands into remote API calls.
Configure remote HTTP/MCP clients through [the connection guide](connect.md).

After changing a setting, verify which file/environment the actual service
loaded, restart/reconnect where required, repeat discovery, and invoke one
authorized read-only operation. A workstation edit does not reconfigure a
remote server.
