# HTTP API Parity Inventory
Last Modified: 2026-05-16

This inventory tracks parity across the three supported Axon control surfaces:

- CLI commands from `src/core/config/types/enums.rs` and dispatch in `src/lib.rs`
- MCP tool routing from `src/mcp/schema.rs` and `src/mcp/server.rs`
- HTTP routes from `src/web/server/routing.rs` and `src/web/actions.rs`

Status meanings:

- `Implemented`: HTTP has a first-party route that reaches the same service path for this surface.
- `Partial`: HTTP supports only part of the CLI/MCP surface or uses a legacy/special route.
- `Missing`: MCP/CLI has a route, but HTTP has no first-party route yet.
- `Deferred`: No remote HTTP endpoint is currently expected; the reason is listed.

## Current HTTP Surfaces

| Route | Purpose | Auth | Notes |
|---|---|---|---|
| `GET /healthz` | process health | none | Panel/server health, not CLI/MCP parity. |
| `GET /readyz` | readiness | none | Panel/server readiness, not CLI/MCP parity. |
| `POST /v1/ask` | RAG ask | MCP HTTP auth policy | Legacy supported API. Uses `services::query::ask` through `src/web/server/handlers/ask.rs`. |
| `GET /v1/capabilities` | client/server capability metadata | none | Advertises `/v1/actions` schema version and supported action list. |
| `POST /v1/actions` | generic action envelope | MCP HTTP auth policy | Body is `{ "request_id": "...", "action": <AxonRequest> }`. Dispatch is intentionally narrower than MCP today. |
| `/api/panel/*` | web panel operations | panel token / local policy | Panel-only, excluded from parity accounting unless promoted to `/v1`. |

## Route Parity Matrix

| CLI command | Service entry point(s) | MCP action/subaction | HTTP endpoint/status | Notes |
|---|---|---|---|---|
| `ask` | `services::query::ask` | `ask` | `POST /v1/ask` = Implemented; `POST /v1/actions` `ask` = Missing | Dedicated legacy API exists. Generic action API returns `unsupported_action`. Streaming/SSE is not exposed as a stable `/v1` route. |
| `crawl` | `services::crawl::{crawl_start_with_context,crawl_status,crawl_list,crawl_cancel,crawl_cleanup,crawl_clear,crawl_recover}` | `crawl.start`, `crawl.status`, `crawl.cancel`, `crawl.list`, `crawl.cleanup`, `crawl.clear`, `crawl.recover` | `POST /v1/actions` = Implemented for listed subactions | CLI-only `crawl worker`, `crawl errors`, `crawl audit`, and `crawl diff` are not in MCP or HTTP. Worker is local process control; errors are folded into status output. |
| `debug` | `services::debug::debug_report` | no dedicated action | Missing | Requires API shape decision for diagnostics payload and whether LLM troubleshooting output is artifact-first. |
| `dedupe` | `services::system::dedupe` | no dedicated action | Missing | Mutating vector maintenance command. Needs request/response schema and auth scope decision. |
| `doctor` | `services::system::doctor` | `doctor` | Missing | MCP supports it; `/v1/actions` does not dispatch it yet. |
| `domains` | `services::system::{domains,detailed_domains}` | `domains` | Missing | MCP supports domain facets; `/v1/actions` does not dispatch it yet. |
| `embed` | `services::embed::{embed_start_with_context,embed_status,embed_list,embed_cancel,embed_cleanup,embed_clear,embed_recover}` | `embed.start`, `embed.status`, `embed.cancel`, `embed.list`, `embed.cleanup`, `embed.clear`, `embed.recover` | `POST /v1/actions` = Implemented for listed subactions | CLI-only `embed worker` is local process control and excluded from remote parity. |
| `evaluate` | `services::query::evaluate` | `evaluate` | Missing | MCP supports evaluation, including diagnostics/retrieval A/B flags. HTTP needs typed action dispatch and error contract. |
| `extract` | `services::extract::{extract_start_with_context,extract_status,extract_list,extract_cancel,extract_cleanup,extract_clear,extract_recover}` | `extract.start`, `extract.status`, `extract.cancel`, `extract.list`, `extract.cleanup`, `extract.clear`, `extract.recover` | `POST /v1/actions` = Implemented for listed subactions | CLI-only `extract worker` is local process control and excluded from remote parity. |
| `ingest` | `services::ingest::{ingest_start_with_context,ingest_status,ingest_list,ingest_cancel,ingest_cleanup,ingest_clear,ingest_recover}` | `ingest.start`, `ingest.status`, `ingest.cancel`, `ingest.list`, `ingest.cleanup`, `ingest.clear`, `ingest.recover` | `POST /v1/actions` = Implemented for listed subactions | Covers GitHub, Reddit, YouTube, and sessions through `source_type`. CLI-only `ingest worker` is local process control. |
| `map` | `services::map::discover` | `map` | Missing | MCP supports map discovery. HTTP needs direct action dispatch and pagination schema. |
| `migrate` | `services::migrate::migrate` | no dedicated action | Missing | One-time collection migration is not exposed remotely. Needs safety checks and likely write/admin auth. |
| `query` | `services::query::query` | `query` | Missing | MCP supports query. HTTP has no generic action dispatcher for it. |
| `research` | `services::search::synthesis::research` | `research` | Missing | MCP supports research. HTTP needs response-mode and streaming/artifact policy. |
| `retrieve` | `services::query::retrieve` | `retrieve` | Missing | MCP supports paged document reads. HTTP has no `/v1/retrieve` or action dispatcher yet. |
| `scrape` | `services::scrape::scrape` | `scrape` | `POST /v1/actions` = Implemented | Server-mode writes under server-owned output paths and returns artifact metadata. |
| `screenshot` | `services::screenshot::screenshot_capture` | `screenshot` | `POST /v1/actions` = Implemented | Returns screenshot metadata/artifact path. |
| `search` | `services::search_crawl::search_and_crawl` for CLI/MCP handler path; `services::search::search` for side-effect-free search helpers | `search` | Missing | MCP handler currently uses `search_and_crawl`, matching CLI auto-crawl behavior. Docs should keep this explicit because older MCP docs described search as side-effect-free. |
| `sessions` | `services::ingest::ingest_sessions*` via `services::ingest::ingest_start_with_context` | `ingest.start` with `source_type: "sessions"` | `POST /v1/actions` = Implemented through `ingest.start` | CLI command maps to ingest action rather than a separate MCP action. |
| `sources` | `services::system::sources` | `sources` | Missing | MCP supports it; `/v1/actions` does not dispatch it yet. |
| `stats` | `services::system::stats` | `stats` | Missing | MCP supports it; `/v1/actions` does not dispatch it yet. |
| `status` | `services::system::full_status` | `status` | `POST /v1/actions` = Implemented | Also backs client/server `axon status --server-url ...`. |
| `suggest` | `services::query::suggest` | `suggest` | Missing | MCP supports it; `/v1/actions` does not dispatch it yet. |
| `watch` | `services::watch::{create_watch_def,list_watch_defs,run_watch_now,list_watch_runs}` | no dedicated action | Missing | CLI implements `create`, `list`, `run-now`, and `history`; other watch subcommands parse but are not implemented. Needs full schema before HTTP. |
| `completions` | CLI generator only | no action | Deferred | Shell completion generation is local developer tooling. |
| `mcp` | MCP server startup | no action | Deferred | Starts the MCP transport itself; not a remote API operation. |
| `serve` | HTTP server startup | no action | Deferred | Starts this server; not a route. |
| `setup` | `services::setup::*` | no action | Deferred | First-run/local/SSH setup mutates host config and should remain panel/local until an admin API is designed. |
| `train` | `cli::commands::train` | no action | Deferred | Interactive/local feedback command; no services-first API exists. |

## MCP-Only or MCP-First Surfaces

| MCP action | Service or handler path | HTTP endpoint/status | Notes |
|---|---|---|---|
| `artifacts.head` | `src/mcp/server/artifacts*` | Missing | Artifact inspection exists for MCP result files but is not exposed through `/v1/actions`. |
| `artifacts.grep` | `src/mcp/server/artifacts*` | Missing | Needs HTTP-safe artifact path validation and auth scope reuse. |
| `artifacts.wc` | `src/mcp/server/artifacts*` | Missing | Same artifact API gap. |
| `artifacts.read` | `src/mcp/server/artifacts*` | Missing | Should preserve pagination/filtered-read protections. |
| `artifacts.list` | `src/mcp/server/artifacts*` | Missing | Candidate for read-only HTTP parity. |
| `artifacts.search` | `src/mcp/server/artifacts*` | Missing | Candidate for read-only HTTP parity. |
| `artifacts.delete` | `src/mcp/server/artifacts*` | Missing | Needs write/admin scope. |
| `artifacts.clean` | `src/mcp/server/artifacts*` | Missing | Needs explicit dry-run/default safety in HTTP docs. |
| `elicit_demo` | `src/mcp/server/handlers_elicit.rs` | Deferred | MCP UX/demo action; not a CLI or HTTP product requirement. |
| `help` | `src/mcp/server.rs` help handler | Missing | Could map to capabilities/schema endpoint; currently only `/v1/capabilities` exists and is not equivalent. |

## Current `/v1/actions` Capability Set

`ServerInfo::current().supported_actions` currently advertises:

```text
status
scrape
screenshot
crawl.start
crawl.status
crawl.list
crawl.cancel
crawl.cleanup
crawl.clear
crawl.recover
extract.start
extract.status
extract.list
extract.cancel
extract.cleanup
extract.clear
extract.recover
embed.start
embed.status
embed.list
embed.cancel
embed.cleanup
embed.clear
embed.recover
ingest.start
ingest.status
ingest.list
ingest.cancel
ingest.cleanup
ingest.clear
ingest.recover
```

Notable gaps versus MCP parser/handler support:

- Direct read/query actions: `query`, `retrieve`, `search`, `map`, `evaluate`, `suggest`, `doctor`, `domains`, `sources`, `stats`, `research`
- Legacy/direct ask parity: `ask` is implemented only as `POST /v1/ask`, not as `/v1/actions`
- Artifact operations: all `artifacts.*`
- Maintenance and host operations: `dedupe`, `migrate`, `watch`, `debug`, `setup`

## Representative API Shapes

### `/v1/actions`

```json
{
  "request_id": "req-123",
  "action": {
    "action": "crawl",
    "subaction": "list",
    "limit": 20,
    "offset": 0
  }
}
```

Success response:

```json
{
  "request_id": "req-123",
  "ok": true,
  "result": {
    "jobs": [],
    "limit": 20,
    "offset": 0
  },
  "server": {
    "version": "...",
    "schema_version": "client-server.v1",
    "minimum_client_schema_version": "client-server.v1",
    "required_request_fields": ["request_id", "action"],
    "supported_actions": ["status"]
  }
}
```

Error response:

```json
{
  "request_id": "req-123",
  "ok": false,
  "error": {
    "kind": "unsupported_action",
    "message": "query is not supported by the first-party action API yet",
    "retryable": false,
    "hint": "call /v1/capabilities to discover supported actions"
  },
  "server": { "...": "..." }
}
```

### `/v1/ask`

`POST /v1/ask` is the existing backcompat route for ask callers. It should remain supported while `/v1/actions` parity is added. Migration guidance for future clients should prefer one canonical action envelope only after `ask` is implemented in `dispatch_action`.

## Next Implementation Slices

1. Add read-only direct actions to `services::action_api::dispatch_action`: `query`, `retrieve`, `doctor`, `domains`, `sources`, `stats`.
2. Add tests that compare MCP request parsing and `/v1/actions` dispatch output for the read-only actions above.
3. Decide whether `search` HTTP should preserve CLI/MCP auto-crawl behavior or expose side-effect-free Tavily search separately.
4. Promote artifact inspection to `/v1/actions` only after preserving MCP artifact path validation and dry-run deletion defaults.
5. Design admin/write scopes for `dedupe`, `migrate`, `watch`, `debug`, and setup operations before routing them remotely.
