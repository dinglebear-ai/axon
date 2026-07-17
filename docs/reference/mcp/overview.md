# Axon MCP Server Guide
Last Modified: 2026-07-16

`axon mcp` exposes Axon through one MCP tool named `axon`.

- Transport: stdio, streamable HTTP (`/mcp`), or both.
- Tool count: 1.
- Tool name: `axon`.
- Routing fields: `action` plus optional `subaction`.
- Schema resource: `axon://schema/mcp-tool`.
- MCP Apps resource: `ui://axon/status-dashboard`.

The live machine-readable schema is generated at
`docs/reference/mcp/tool-schema.json`; the markdown reference is
`docs/reference/mcp/pipeline-tool-schema.md`.

## Source Indexing

All MCP indexing goes through `action=source`.

```json
{ "action": "source", "source": "https://example.com", "scope": "page", "embed": true }
{ "action": "source", "source": "https://example.com", "scope": "site", "embed": true }
```

`scope=page` is the single-page scrape shape. `scope=site` or `scope=docs` is
the crawl-like site acquisition shape. The removed MCP actions `scrape`,
`crawl`, `embed`, `ingest`, `code_search`, `vertical_scrape`, `purge`, and
`dedupe` are not valid action enum values and are rejected before dispatch.

## Transport

```bash
axon mcp                 # stdio
axon serve mcp           # unified HTTP server with /mcp mounted
axon mcp --transport both
```

HTTP transport shares the same listener as `axon serve`.

| Variable | Default | Description |
|---|---|---|
| `AXON_HTTP_HOST` | `127.0.0.1` | Unified HTTP bind host; non-loopback requires auth. |
| `AXON_HTTP_PORT` | `8001` | Unified HTTP bind port. |
| `AXON_HTTP_TOKEN` | unset | Static bearer or `x-api-key` token. |
| `AXON_AUTH_MODE` | bearer/static mode | Set `oauth` for lab-auth Google OAuth/JWT. |

Tokenless HTTP is allowed only on loopback binds. Non-loopback binds require
OAuth mode or `AXON_HTTP_TOKEN`.

## Request Pattern

```json
{
  "action": "query",
  "query": "embedding pipeline architecture",
  "limit": 10,
  "response_mode": "inline"
}
```

Grouped actions use `subaction`, for example:

```json
{ "action": "jobs", "subaction": "events", "job_id": "..." }
{ "action": "extract", "subaction": "start", "urls": ["https://example.com"] }
{ "action": "watch", "subaction": "list" }
{ "action": "prune", "subaction": "plan", "source": "https://example.com" }
```

## Response Modes

| Mode | Behavior |
|---|---|
| `artifact` | Persist the result and return metadata containing an opaque `artifact_id`. |
| `inline` | Return content inline when allowed by size and visibility policy. |
| `both` | Return an opaque artifact reference and include inline content. |
| `auto_inline` | Inline small payloads; use artifact metadata for larger payloads. |

`retrieve` is inline-first for document reading. Source indexing and heavier
operations default to artifact-backed responses. MCP responses never expose a
server filesystem path; follow the returned `artifact_id` through the artifact
resource surface.

## Smoke Examples

```bash
mcporter --config config/mcporter.json call axon.axon action:doctor --output json
mcporter --config config/mcporter.json call axon.axon action:source source:https://example.com scope:page embed:true --output json
mcporter --config config/mcporter.json call axon.axon action:jobs subaction:list limit:5 --output json
```

## Auth

MCP HTTP auth uses the same Axon OAuth/static bearer policy as the unified HTTP
server. Valid OAuth users receive Axon read/write scopes; admin-scoped actions
such as destructive prune execution still require the admin scope.
