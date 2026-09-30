# Axon MCP Server Guide
Last reviewed: 2026-09-27

`axon mcp` supports legacy and atomic MCP tool projections. The server tool
catalog defines discovery, including the auxiliary `axon_status_dashboard`
MCP App tool and transport task handlers.

- Transport: stdio, streamable HTTP (`/mcp`), or both.
- Projection: `AXON_MCP_TOOL_PROJECTION=legacy|atomic|both` (default `legacy`).
- Legacy surface: one tool named `axon`, routed by `action` plus optional `subaction`.
- Atomic surface: unprefixed canonical leaf tools such as `query`, `jobs_get` and `jobs_cancel`. Fixed `action`/`subaction` fields are omitted and rejected if supplied.
- `both`: publishes legacy and atomic surfaces simultaneously for compatibility rollout.
- Auxiliary tool: `axon_status_dashboard`.
- Task handling: transport task handlers map protocol operations to durable jobs.
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
the crawl-like site acquisition shape. Focused `scrape`, `crawl`, `embed`, and
`ingest` actions reuse this pipeline, while `code_search` reads committed code
vectors. Removed actions such as `vertical_scrape`, `purge`, `dedupe`, and
`code_search_watch` are rejected before dispatch.

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
| `AXON_MCP_TOOL_PROJECTION` | `legacy` | `legacy`, `atomic`, or `both` MCP tool projection. |
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

## Projection selection and compatibility

`McpToolProjection` is startup-static. Precedence is typed
`--mcp-tool-projection` (supported on `mcp`, `serve mcp` and `serve`),
then `AXON_MCP_TOOL_PROJECTION`, then the deprecated
`AXON_MCP_PROJECTION` alias, then `legacy`. Invalid CLI values are rejected;
invalid environment values warn and fall back to legacy. The settings are not
independent switches. Restart the server to change its published surface.

An atomic call to `jobs_get` supplies only the request fields, for example
`{"job_id":"<owned-job-id>"}`. It must not supply `action` or `subaction`,
even with the same value or null. The legacy equivalent is the `axon` tool
with `{"action":"jobs","subaction":"get","job_id":"<owned-job-id>"}`.
Atomic tools reject fields belonging only to sibling subactions, such as
`retry_mode` on `jobs_get`.
Both forms enter one canonical dispatcher with the presented name retained in
audit events. Auxiliary `axon_status_dashboard` identity, UI metadata and
callability are unchanged in all modes.

`axon://schema/mcp-tool` retains its legacy aggregate schema/Markdown contract.
`axon://schema/mcp-operations` returns canonical leaf schemas, operation policy
hints, the selected projection and the exact active tool names. Its canonical
catalog includes all operations; `active_tools` distinguishes callable tools in
legacy mode. `help` (or `axon` with `action=help`) publishes the same operation
identities. Generated JSON and reference tables come from the runtime registry,
not an independently maintained generator action list.

Safety annotations are hints, not grants. Plans may persist records even when
execution needs separate confirmation. Read-shaped `search` and `research`
retain explicit write elevation. Admin scopes, caller visibility and destructive
confirmation are checked by the existing policy/services. Only `extract_start`
currently implements optional MCP task augmentation; in legacy form this is
`axon` with `action=extract, subaction=start`. Progress metadata is preserved and
a client without task capability is refused before enqueue.

### Real-client validation

The existing `scripts/test-mcp-tools-mcporter.sh` harness accepts
`MCP_PROJECTION=legacy|atomic|both`. Both mode runs separate aggregate and leaf
sweeps. It compares the complete actual inventory, explicitly separates the
dashboard, and fails on extra, missing or duplicate tools. Evidence distinguishes
successful operations, expected denials, handled service errors, skips and failures.
Tests create `axon_e2e_*` collections and isolated local state. HTTP runs require
an owned disposable instance and `MCP_E2E_ISOLATED_HTTP=1`; setting a local mode
variable is not proof that a remote server uses that mode.

`scripts/test-mcp-tasks-wire.py` reuses the same canonical call projection for
stdio and HTTP, with `--projection` and `--call-form`. It exercises actual task
creation, progress, terminal results, cancellation and reconnection. Use owned
input and storage; no production reset or cleanup is a test prerequisite.
