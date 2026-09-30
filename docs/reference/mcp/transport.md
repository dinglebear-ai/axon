# Transport Methods -- Axon MCP

Last reviewed: 2026-09-29

Axon serves the same shared services through stdio or streamable HTTP.
Transport selection does not create another source pipeline or job store.

| Selection | Entry point | Boundary |
|---|---|---|
| Stdio | `axon mcp` | Child process stdin/stdout; no HTTP listener by default |
| HTTP | `axon serve mcp` or `axon mcp --transport http` | Unified listener, `/mcp`, REST routes, and web panel |
| Both | `axon mcp --transport both` | Stdio and HTTP in the same hosting runtime |

## Stdio

Configure the client with an absolute Axon executable path and `mcp` argument.
Use the executing account's intended configuration and provider endpoints.
Stdout is protocol output; diagnostics belong on stderr. No HTTP bearer
handshake is required for a local child process, but local execution
authority and source/tool access policies still matter.

Keep workers alive when accepting detached jobs. A child process closing
after enqueue does not guarantee that another worker will complete the job.
See [jobs](../runtime/jobs.md) for runtime ownership and liveness.

Client formats differ. [Connect to Axon MCP](connect.md) contains current
Claude Code, Codex TOML, and Gemini settings examples. Do not reuse a
`mcpServers` JSON block as Codex configuration or assume every client
selects HTTP with the same field.

## HTTP

For an explicitly loopback-only development listener:

```bash
AXON_HTTP_HOST=127.0.0.1 AXON_HTTP_PORT=8001 axon serve mcp
```

A non-loopback bind requires `AXON_HTTP_TOKEN` or configured OAuth. The
listener uses the same HTTP auth/allowed-origin boundary as the REST API.
Panel password/session unlock is separate and cannot substitute for API/MCP
credentials. Use [MCP authentication](../../operations/auth/mcp-auth.md).

| Route | Purpose |
|---|---|
| `/mcp` | MCP initialization, negotiated session, tool/resource/task protocol |
| `/healthz` | Process health, not proof of healthy providers or completed jobs |
| `/v1/capabilities` | HTTP API capability document |
| `/v1/*` | Direct REST routes and their own request/response contracts |

Use a real MCP client for protocol verification. Initialization, negotiated
capabilities, content negotiation, and session headers are part of the
transport; a raw tool-call POST is not a complete smoke test. Respect the
server's actual response rather than treating any successful HTTP status as
business-operation completion.

## Shared server and deployment

`axon serve` already mounts MCP HTTP with the API and panel; it does not
need a second stdio `axon mcp` process merely to expose `/mcp`. The configured
port defaults to 8001, not a fixed address every installation must use.

Production is native Axon under systemd in Incus or bare-metal Linux, with
external providers. Compose remains development/reference material. Inspect
verified deployment notes for actual service names, mounts, proxy routes,
and restart commands. See [deployment](deploy.md).

## Projection, tasks, and reconnects

`AXON_MCP_TOOL_PROJECTION` selects legacy/atomic/both at startup. Reconnect
clients and repeat discovery after changing it. All projections preserve the
auxiliary dashboard and canonical policy checks. Schema resources include
`axon://schema/mcp-tool` and `axon://schema/mcp-operations`.

Only extraction start currently supports negotiated task augmentation.
Other source jobs use the durable job lifecycle, not assumed task methods.
See [tool contract](tool-contract.md) and [environment](env.md).
