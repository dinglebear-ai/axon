# HTTP API Parity Inventory
Last Modified: 2026-06-11

This inventory tracks parity across the three Axon control surfaces:

- CLI commands from `src/core/config/types/enums.rs` and dispatch in `src/lib.rs`
- MCP tool routing from `src/mcp/schema.rs` and `src/mcp/server.rs`
- Direct REST routes from `src/web/server/routing.rs`

Direct REST under `/v1` is the canonical client/server API. The legacy
`POST /v1/actions` action-envelope endpoint is removed and returns 404.

Status meanings:

- `Implemented`: HTTP has a first-party route that reaches the same service path for this surface.
- `Partial`: HTTP supports only part of the CLI/MCP surface.
- `Missing`: MCP/CLI has a route, but HTTP has no first-party route yet.
- `Deferred`: No remote HTTP endpoint is currently expected; the reason is listed.

## Current HTTP Surfaces

| Route | Purpose | Auth | Notes |
|---|---|---|---|
| `GET /healthz` | process health | none | Panel/server health, not CLI/MCP parity. |
| `GET /readyz` | readiness | none | Panel/server readiness, not CLI/MCP parity. |
| `GET /api-docs/openapi.json` | OpenAPI contract | none | Source for generated TypeScript client types. |
| `GET /v1/capabilities` | client/server capability metadata | axon:read or axon:write | Advertises `supported_routes` for direct REST. |
| `POST /v1/actions` | removed legacy action envelope | none | Always returns 404 with direct REST migration text. |
| `/api/panel/*` | web panel operations | panel token / local policy | Panel-only, excluded from parity accounting unless promoted to `/v1`. |

## Route Parity Matrix

| CLI command | Service entry point(s) | MCP action/subaction | HTTP endpoint/status | Notes |
|---|---|---|---|---|
| `ask` | `services::query::ask` | `ask` | `POST /v1/ask`, `POST /v1/ask/stream` = Implemented | `/v1/ask/stream` is advertised in `supported_routes()` and serves SSE; non-streaming `/v1/ask` remains for existing clients. |
| `chat` | `services::llm_backend` direct completion path | no dedicated action | `POST /v1/chat`, `POST /v1/chat/stream` = Implemented | Direct LLM chat without RAG retrieval/synthesis prompt. Used by clients that need plain model chat. |
| `crawl` | `services::crawl::{crawl_start_with_context,crawl_status,crawl_list,crawl_cancel,crawl_cleanup,crawl_clear,crawl_recover}` | `crawl.start`, `crawl.status`, `crawl.cancel`, `crawl.list`, `crawl.cleanup`, `crawl.clear`, `crawl.recover` | `POST /v1/crawl`, `GET /v1/crawl`, `GET /v1/crawl/{id}`, `POST /v1/crawl/{id}/cancel`, `POST /v1/crawl/cleanup`, `DELETE /v1/crawl`, `POST /v1/crawl/recover` = Implemented | CLI-only `crawl worker`, `crawl errors`, `crawl audit`, and `crawl diff` are local process/reporting operations. |
| `debug` | `services::debug::debug_report` | no dedicated action | Missing | Needs API shape decision for diagnostics payload and LLM troubleshooting artifacts. |
| `dedupe` | `services::system::dedupe` | no dedicated action | `POST /v1/dedupe` = Implemented | Mutating vector maintenance command; migrate remains CLI-only. |
| `doctor` | `services::system::doctor` | `doctor` | `GET /v1/doctor` = Implemented | Returns diagnostics to authenticated callers; boolean probes use healthz/readyz. |
| `domains` | `services::system::{domains,detailed_domains}` | `domains` | `GET /v1/domains` = Implemented | HTTP exposes the domain facets service path. |
| `brand` | `services::brand::brand` | `brand` | `POST /v1/brand` = Implemented | Brand-identity extraction is available through CLI, MCP, and direct REST. |
| `diff` | `services::diff::diff` | `diff` | `POST /v1/diff` = Implemented | Two-URL compare is available through CLI, MCP, and direct REST. |
| `endpoints` | `services::endpoints::discover` | `endpoints` | `POST /v1/endpoints` = Implemented | API-endpoint discovery. `--probe-rpc`/`--probe-rpc-subdomains` remain CLI-only (no MCP/REST toggle); see `docs/reference/endpoints.md`. |
| `embed` | `services::embed::{embed_start_with_context,embed_status,embed_list,embed_cancel,embed_cleanup,embed_clear,embed_recover}` | `embed.start`, `embed.status`, `embed.cancel`, `embed.list`, `embed.cleanup`, `embed.clear`, `embed.recover` | `POST /v1/embed`, `GET /v1/embed`, `GET /v1/embed/{id}`, `POST /v1/embed/{id}/cancel`, `POST /v1/embed/cleanup`, `DELETE /v1/embed`, `POST /v1/embed/recover` = Implemented | REST validates local file inputs with the shared server-side embed guard. CLI-only `embed worker` is local process control. |
| `evaluate` | `services::query::evaluate` | `evaluate` | `POST /v1/evaluate` = Implemented | Uses typed result and shared HttpError envelope. |
| `extract` | `services::extract::{extract_start_with_context,extract_status,extract_list,extract_cancel,extract_cleanup,extract_clear,extract_recover}` | `extract.start`, `extract.status`, `extract.cancel`, `extract.list`, `extract.cleanup`, `extract.clear`, `extract.recover` | `POST /v1/extract`, `GET /v1/extract`, `GET /v1/extract/{id}`, `POST /v1/extract/{id}/cancel`, `POST /v1/extract/cleanup`, `DELETE /v1/extract`, `POST /v1/extract/recover` = Implemented | REST accepts canonical DTOs; the public schema only advertises `auto` until the async service has mode parity. |
| `ingest` | `services::ingest::{ingest_start_with_context,ingest_status,ingest_list,ingest_cancel,ingest_cleanup,ingest_clear,ingest_recover}` | `ingest.start`, `ingest.status`, `ingest.cancel`, `ingest.list`, `ingest.cleanup`, `ingest.clear`, `ingest.recover` | `POST /v1/ingest`, `GET /v1/ingest`, `GET /v1/ingest/{id}`, `POST /v1/ingest/{id}/cancel`, `POST /v1/ingest/cleanup`, `DELETE /v1/ingest`, `POST /v1/ingest/recover` = Implemented | Uses canonical `target` field for Git, Reddit, YouTube, and sessions. CLI-only `ingest worker` is local process control. |
| `map` | `services::map::discover` | `map` | `POST /v1/map` = Implemented | Uses typed body with url, limit, and offset. |
| `memory` | `services::memory::{dispatch,remember,list,search,show,link,supersede,context}` | `memory.remember`, `memory.list`, `memory.search`, `memory.show`, `memory.link`, `memory.supersede`, `memory.context` | `POST /v1/memory` = Implemented | Single direct REST endpoint accepts the memory subaction envelope and uses write scope because some subactions mutate persistent memory. |
| `migrate` | `services::migrate::migrate` | no dedicated action | Deferred | One-time collection migration is intentionally not exposed remotely; `POST /v1/migrate` returns 404. |
| `query` | `services::query::query` | `query` | `POST /v1/query` = Implemented | Uses canonical request DTO and typed query service result. |
| `refresh` | `services::refresh::{plan_refresh,execute_refresh}` | no dedicated action | Missing | Re-enqueues crawl/ingest jobs for indexed origins; CLI-only, gated behind an interactive confirmation. |
| `research` | `services::search::synthesis::research` | `research` | `POST /v1/research` = Implemented | HTTP applies a 35-second server-side timeout. Streaming remains deferred. |
| `retrieve` | `services::query::retrieve` | `retrieve` | `POST /v1/retrieve` = Implemented | Supports collection, max_points, cursor, and token_budget. |
| `scrape` | `services::scrape::{scrape_batch,scrape_batch_with_optional_embed}` | `scrape` | `POST /v1/scrape` = Implemented | Supports render mode, format, selectors, headers, collection, and optional embedding. |
| `summarize` | `services::summarize::summarize` | `summarize` | `POST /v1/summarize` = Implemented | Supports render mode, selectors, and headers for the underlying scrape step. |
| `screenshot` | `services::screenshot::screenshot_capture` | `screenshot` | `POST /v1/screenshot` = Implemented | Captures screenshots through Chrome with the shared service path. |
| `search` | `services::search_crawl::search_and_crawl` for CLI/MCP handler path; `services::search::search` for side-effect-free helpers | `search` | `POST /v1/search` = Implemented | HTTP intentionally follows CLI/MCP auto-crawl behavior. |
| `sessions` | `services::ingest::ingest_sessions*` via `services::ingest::ingest_start_with_context`; `services::ingest::ingest_sessions_prepared_start_with_context` for remote prepared payloads | `ingest.start` with `source_type: "sessions"` | `POST /v1/ingest`, `POST /v1/ingest/sessions/prepared` = Implemented | CLI command maps to ingest with `source_type: "sessions"` and typed session source options. Remote callers use the prepared sessions endpoint because server-local session scanning is disabled. `sessions watch` is host-local CLI/service automation and intentionally adds no new REST route; prepared uploads use `POST /v1/ingest/sessions/prepared`. |
| `sources` | `services::system::sources` | `sources` | `GET /v1/sources` = Implemented | HTTP exposes the same service path. |
| `stats` | `services::system::stats` | `stats` | `GET /v1/stats` = Implemented | HTTP exposes the same service path. |
| `status` | `services::system::full_status` | `status` | `GET /v1/status` = Implemented | Also backs client/server `axon status --server-url ...`. |
| `suggest` | `services::query::suggest` | `suggest` | `POST /v1/suggest` = Implemented | Supports focus and collection. |
| `watch` | `services::watch::{create_watch_def,list_watch_defs,run_watch_now,list_watch_runs}` | no dedicated action | `GET /v1/watch`, `POST /v1/watch`, `POST /v1/watch/{id}/run` = Partial | HTTP exposes create, list, and run-now. Other parsed CLI subcommands remain unimplemented. |
| `completions` | CLI generator only | no action | Deferred | Shell completion generation is local developer tooling. |
| `mcp` | MCP server startup | no action | Deferred | Starts the MCP transport itself; not a remote API operation. |
| `serve` | HTTP server startup | no action | Deferred | Starts this server; not a route. |
| `setup` | `services::setup::*` | no action | Deferred | First-run/local/SSH setup mutates host config and should remain panel/local until an admin API is designed. |
| `train` | `cli::commands::train` | no action | Deferred | Interactive/local feedback command; no services-first API exists. |
| `config` | `cli::commands::config` | no action | Deferred | Reads/writes `~/.axon/.env` and `~/.axon/config.toml` on the host; local-only by design. |
| `monitor` | `cli::commands::monitor` | no action | Deferred | Local operational monitoring command; no remote API. |
| `preflight` | `cli::commands::preflight` | no action | Deferred | Local pre-run environment checks; no remote API. |
| `smoke` | `cli::commands::smoke` | no action | Deferred | Local smoke-test runner; no remote API. |
| `compose` | `cli::commands::compose` | no action | Deferred | Local Docker Compose helper; no remote API. |
| `sync` | `cli::commands::sync` | no action | Deferred | Local container/binary sync helper; no remote API. |

## Advertised Direct REST Routes

`ServerInfo::rest_capabilities().supported_routes` currently advertises:

```text
GET /healthz
GET /readyz
GET /api-docs/openapi.json
GET /docs
GET /v1/capabilities
GET /v1/sources
GET /v1/domains
GET /v1/stats
GET /v1/status
GET /v1/doctor
POST /v1/query
POST /v1/retrieve
POST /v1/map
GET /v1/artifacts
POST /v1/endpoints
POST /v1/brand
POST /v1/diff
POST /v1/screenshot
POST /v1/ask
POST /v1/ask/stream
POST /v1/chat
POST /v1/chat/stream
POST /v1/evaluate
POST /v1/suggest
POST /v1/scrape
POST /v1/summarize
POST /v1/summarize/stream
POST /v1/search
POST /v1/research
POST /v1/research/stream
POST /v1/memory
POST /v1/crawl
GET /v1/crawl
GET /v1/crawl/{id}
POST /v1/crawl/{id}/cancel
POST /v1/crawl/cleanup
DELETE /v1/crawl
POST /v1/crawl/recover
POST /v1/embed
GET /v1/embed
GET /v1/embed/{id}
POST /v1/embed/{id}/cancel
POST /v1/embed/cleanup
DELETE /v1/embed
POST /v1/embed/recover
POST /v1/extract
GET /v1/extract
GET /v1/extract/{id}
POST /v1/extract/{id}/cancel
POST /v1/extract/cleanup
DELETE /v1/extract
POST /v1/extract/recover
POST /v1/ingest
POST /v1/ingest/sessions/prepared
GET /v1/ingest
GET /v1/ingest/{id}
POST /v1/ingest/{id}/cancel
POST /v1/ingest/cleanup
DELETE /v1/ingest
POST /v1/ingest/recover
POST /v1/dedupe
GET /v1/watch
POST /v1/watch
POST /v1/watch/{id}/run
```

`supported_actions` and action-envelope `required_request_fields` remain in
`ServerInfo` only for internal legacy response structs. The public
`GET /v1/capabilities` response omits them and advertises direct REST routes.

## OpenAPI Contract

The server serves the REST contract at `GET /api-docs/openapi.json`.
The same document can be exported without starting a server:

```bash
cargo run --bin axon-openapi > apps/web/openapi/axon.json
```

Fast API-only local check:

```bash
cargo test --test http_api_parity_inventory -- --nocapture
npm --prefix apps/web run openapi:check
```

The web client generates TypeScript declarations from that contract:

```bash
cd apps/web
npm run openapi:generate
```

Generated files:

- `apps/web/openapi/axon.json`
- `apps/web/lib/generated/axon-api.ts`

`apps/web/lib/axon-client.ts` imports generated component schemas plus `paths`
and `operations` maps for request, response, parameter, and auth metadata. The
wrapper still owns fetch/auth/error ergonomics, but request and route-shape
drift now comes from OpenAPI generation instead of manual hand-carved DTOs.

## MCP-Only or MCP-First Surfaces

| MCP action | Service or handler path | HTTP endpoint/status | Notes |
|---|---|---|---|
| `elicit_demo` | `src/mcp/server/handlers_elicit.rs` | Deferred | MCP UX/demo action; not a CLI or HTTP product requirement. |
| `help` | `src/mcp/server.rs` help handler | Partial | `/v1/capabilities` and OpenAPI expose route metadata but do not mirror MCP help text. |

## Remaining Direct REST Gaps

1. Artifact inspection and cleanup routes with explicit path validation and dry-run defaults.
2. `debug` diagnostics route and artifact model.
3. Optional watch subcommands beyond create/list/run-now once the service layer implements them.
