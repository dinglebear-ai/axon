# REST API Design
**Date:** 2026-03-21
**Status:** Approved

## Overview

Add a full-parity REST API to axon mounted at `/api/v1/` on the existing axum server (port `49000`). The API serves two consumers: the Next.js web frontend (`apps/web/`) as a simpler alternative to the WebSocket execution bridge for non-streaming operations, and external tooling/scripts (e.g. `scripts/reingest.py`) that currently drive axon via CLI subprocess.

---

## Architecture

```
Next.js / scripts
      │
      ▼
  axum (port 49000)
  ├── /ws              ← existing WS execution bridge (unchanged)
  ├── /ws/shell        ← existing shell WS (unchanged)
  ├── /download/...    ← existing file download routes (unchanged)
  └── /api/v1/...      ← NEW REST API
            │
            ▼
     crates/services/  ← same typed functions MCP and CLI use
            │
            ▼
  Qdrant / Postgres / Redis / AMQP
```

**Key constraints:**
- Same port (`49000`) — no new listeners, no new infrastructure
- Same `AppState` — `pub(crate)` visibility is sufficient since all handler files live in the same crate. REST handlers only use `state.cfg` and `state.api_token`; the WS-only fields (`stats_tx`, `job_dirs`, `session_ownership`, `rate_limiter`) are present in the struct but unused by REST handlers — this is acceptable.
- Same CORS middleware already applied at the router level — no duplication needed
- Same `crates::services::*` functions — no reimplementation of business logic
- Reference pattern: MCP handlers in `crates/mcp/` (direct service dispatch, typed result → wire format)

---

## Module Layout

```
crates/web.rs                      ← adds .merge(api::router(Arc::clone(&state)))
crates/web/api.rs                  ← router() fn + auth middleware layer + shared response helpers
crates/web/api/
  handlers_crawl.rs                ← /crawl endpoints
  handlers_embed.rs                ← /embed endpoints
  handlers_extract.rs              ← /extract endpoints
  handlers_ingest.rs               ← /ingest endpoints
  handlers_refresh.rs              ← /refresh endpoints
  handlers_graph.rs                ← /graph endpoints
  handlers_query.rs                ← /query, /ask, /retrieve, /evaluate, /suggest
  handlers_content.rs              ← /scrape, /map, /search, /research
  handlers_system.rs               ← /sources, /domains, /stats, /status, /doctor
```

Each handler file mirrors the MCP handler split (`crates/mcp/handlers_*.rs`). Every file calls `crates::services::*` directly. All files must stay within the 500-line monolith policy.

---

## Authentication

**Approach: tower middleware layer on the `api::router()` subtree.**

The `api::router()` function applies a single `middleware::from_fn_with_state` that runs `check_auth()` before any handler. With 40+ endpoints this is cleaner than per-handler extractors and ensures no handler is accidentally left unprotected. The middleware short-circuits with a `401` before extractors run.

**Token:** The Rust server's `AppState.api_token` is populated solely from `AXON_WEB_API_TOKEN`. The two-tier `AXON_WEB_BROWSER_API_TOKEN` system is a Next.js (`proxy.ts`) concern — it does not exist in the Rust server. The REST API uses `AXON_WEB_API_TOKEN` for all requests, consistent with how the existing `/ws` and `/download/*` routes work.

The auth middleware reuses the existing `WsQuery` struct (defined in `crates/web.rs` as `struct WsQuery { token: Option<String> }` — private, no visibility modifier). Since `crates/web/api.rs` is a child module of `crates/web`, it can access private items from the parent module without any visibility change needed. Do not define a duplicate `TokenQuery` struct.

```rust
pub(crate) fn router(state: Arc<AppState>) -> Router {
    Router::new()
        // ... all /api/v1/ routes ...
        .with_state(Arc::clone(&state))
        .layer(middleware::from_fn_with_state(
            Arc::clone(&state),
            api_auth_middleware,
        ))
}

async fn api_auth_middleware(
    State(state): State<Arc<AppState>>,
    Query(params): Query<WsQuery>,
    request: Request<Body>,
    next: Next,
) -> Response {
    match check_auth(request.headers(), params.token.as_deref(), state.api_token.as_deref()) {
        AuthOutcome::Token => next.run(request).await,
        AuthOutcome::Denied(_) => unauthorized(),
    }
}
```

Missing or invalid token → `401 Unauthorized` with `{ "error": "unauthorized" }`.

---

## Rate Limiting

REST endpoints are **not rate-limited in v1**. The existing `AppState.rate_limiter` is an IP-keyed sliding window applied specifically by the WS handler — it is not middleware and does not cover HTTP routes. REST callers are expected to be internal (Next.js proxy, trusted scripts). Add in v2 if needed.

---

## Input Validation & SSRF Guard

All URL inputs (`url`, `urls`, `target` where it resolves to a URL) must pass through `crates::core::http::validate_url()` before being passed to a service. This enforces the existing SSRF guard (blocks RFC-1918 addresses, loopback, link-local, metadata endpoints). Invalid URLs → `400 Bad Request`.

Handler pattern for URL fields:
```rust
use crate::crates::core::http::validate_url;

if let Err(e) = validate_url(&body.url) {
    return bad_request(&e.to_string());
}
```

---

## Service Variant Rule

**Use `_raw` service variants for all REST handlers that need typed structs.**

The services layer provides two variants for job lookups and lists:
- `crawl_status(cfg, id)` → `Result<CrawlJobResult>` (opaque `payload: Value`)
- `crawl_status_raw(cfg, id)` → `Result<Option<CrawlJob>>` (typed, `None` if not found)
- `crawl_list(cfg, limit, offset)` → `Result<CrawlJobResult>` (opaque payload)
- `crawl_list_raw(cfg, limit, offset)` → `Result<Vec<CrawlJob>>` (typed slice)

REST handlers use the `_raw` variants exclusively: typed structs serialize cleanly to JSON, and `Option` at the top level allows precise `None → 404` mapping. The non-`_raw` variants exist for CLI output formatting and are not appropriate for REST serialization.

The same rule applies for embed and ingest where `_raw` variants exist.

---

## Response Conventions

### Async job submissions (fire-and-forget, default)

POST to any job endpoint enqueues and returns immediately with a `201 Created`:
```json
{ "job_id": "550e8400-e29b-41d4-a716-446655440000" }
```
Crawl is the exception — it submits multiple URLs in one call and returns all job IDs (see Crawl section).

### Async job submissions (`wait=true`)

When `wait=true` is in the query string or request body, the handler polls the job DB using the `_raw` service variant until the job reaches a terminal state. Terminal state detection:

| Job type | Status field | Terminal values |
|----------|-------------|-----------------|
| Crawl | `CrawlJob.status: String` | `"completed"`, `"failed"`, `"canceled"` |
| Embed | `EmbedJob.status: String` | `"completed"`, `"failed"`, `"canceled"` |
| Ingest | `IngestJob.status: String` | `"completed"`, `"failed"`, `"canceled"` |
| Refresh | `RefreshJob.status: String` | `"completed"`, `"failed"`, `"canceled"` |

Polling interval: 500ms. Timeout: 300 seconds. On timeout → `504 Gateway Timeout` with `{ "error": "job timed out" }`. On completion → same `{ "job": { ... } }` shape as `GET /api/v1/<type>/{id}`.

### Job status endpoints (`GET /{type}/{id}`)

Service functions differ by type:

**Outer `Option` (crawl, embed, ingest):** `*_raw` variants return `Result<Option<T>>`.
```rust
match services::crawl::crawl_status_raw(&state.cfg, id).await {
    Ok(Some(job)) => Json(json!({ "job": job })).into_response(),
    Ok(None)      => not_found("job not found"),
    Err(e)        => service_err(e),
}
```

**Inner `Option` (refresh):** `refresh_status` returns `Result<RefreshJobResult>` where `RefreshJobResult { job: Option<RefreshJob> }`. The `None` check is on the inner field:
```rust
match services::refresh::refresh_status(&state.cfg, id).await {
    Ok(result) if result.job.is_none() => not_found("job not found"),
    Ok(result) => Json(json!({ "job": result.job })).into_response(),
    Err(e)     => service_err(e),
}
```

Never serialize `None` as `{ "job": null }` with a 200 — always map to `not_found()`.

### Synchronous operations
Return the service result struct serialized directly — same shape as `--json` CLI output. No envelope wrapper.

### Unknown request fields
All request structs carry `#[serde(deny_unknown_fields)]`, matching the MCP schema convention. Unknown fields → `400 Bad Request` with serde's error message. Do not remove this.

### Errors
Uniform across all endpoints:
```json
{ "error": "crawl job not found" }
```

HTTP status codes:
| Code | When |
|------|------|
| `201` | Async job successfully enqueued |
| `400` | Bad request body / invalid input / SSRF-blocked URL / unknown fields |
| `401` | Missing or invalid auth token |
| `404` | Job or resource not found |
| `504` | `wait=true` job timed out (300s) |
| `500` | Internal service error |

---

## Route Disambiguation

For every job group, fixed-segment paths (`/recover`, `/cleanup`) are registered **before** the parameterized path (`/{id}`). Axum resolves by specificity (literal before wildcard) regardless of registration order, but preserving this order in code makes intent explicit.

---

## Endpoints

### Crawl (`handlers_crawl.rs`)

`POST /api/v1/crawl` serializes the full `CrawlStartResult` struct, including all four fields:
- `job_ids: Vec<String>` — one UUID per submitted URL
- `output_dir: Option<String>` — base output directory
- `predicted_paths: Vec<String>` — expected output file paths
- `jobs: Vec<CrawlStartJob>` — per-URL detail (url + job_id + predicted_paths)

| Method | Path | Body / Params | Response |
|--------|------|---------------|----------|
| `POST` | `/api/v1/crawl` | `{ urls, max_pages?, max_depth?, wait? }` | Full `CrawlStartResult` JSON |
| `GET` | `/api/v1/crawl` | `?limit=&offset=` | `{ jobs: [...CrawlJob] }` |
| `GET` | `/api/v1/crawl/{id}` | — | `{ job: CrawlJob }` or `404` |
| `DELETE` | `/api/v1/crawl/{id}` | — | `{ canceled: true }` |
| `POST` | `/api/v1/crawl/recover` | — | `{ recovered: N }` |
| `POST` | `/api/v1/crawl/cleanup` | — | `{ deleted: N }` |

`crawl_clear` (delete all jobs) is **excluded from v1** — too destructive without confirmation UX. Add in v2.
List handler uses `crawl_list_raw` → `Vec<CrawlJob>`. Status handler uses `crawl_status_raw` → `Option<CrawlJob>`.

### Embed (`handlers_embed.rs`)

| Method | Path | Body / Params | Response |
|--------|------|---------------|----------|
| `POST` | `/api/v1/embed` | `{ input, source_type?, wait? }` | `{ job_id }` |
| `GET` | `/api/v1/embed` | `?limit=&offset=` | `{ jobs: [...] }` |
| `GET` | `/api/v1/embed/{id}` | — | `{ job: { ... } }` or `404` |
| `DELETE` | `/api/v1/embed/{id}` | — | `{ canceled: true }` |
| `POST` | `/api/v1/embed/recover` | — | `{ recovered: N }` |
| `POST` | `/api/v1/embed/cleanup` | — | `{ deleted: N }` |

`embed_clear` **excluded from v1**. Status uses `embed_status_raw` → `Option<EmbedJob>` (outer Option, typed struct).

### Extract (`handlers_extract.rs`)

| Method | Path | Body / Params | Response |
|--------|------|---------------|----------|
| `POST` | `/api/v1/extract` | `{ urls, prompt?, wait? }` | `{ job_id }` |
| `GET` | `/api/v1/extract` | `?limit=&offset=` | `{ jobs: [...] }` |
| `GET` | `/api/v1/extract/{id}` | — | `{ job: { ... } }` or `404` |
| `DELETE` | `/api/v1/extract/{id}` | — | `{ canceled: true }` |
| `POST` | `/api/v1/extract/recover` | — | `{ recovered: N }` |
| `POST` | `/api/v1/extract/cleanup` | — | `{ deleted: N }` |

`extract_clear` **excluded from v1**. Status uses `extract_status_raw` → `Option<ExtractJob>` (outer Option, typed struct).

### Ingest (`handlers_ingest.rs`)

The request `target` string is converted to `IngestSource` via `services::ingest::classify_target(target, cfg.github_include_source)` before calling `ingest_start`. The `include_source` argument controls whether GitHub repos index source code — it is resolved from `cfg.github_include_source` (default `true`, set by `GITHUB_INCLUDE_SOURCE` env or `--no-source` CLI flag). REST callers cannot override this per-request in v1; it uses the server-startup default. Unknown/unclassifiable target → `400 Bad Request`.

| Method | Path | Body / Params | Response |
|--------|------|---------------|----------|
| `POST` | `/api/v1/ingest` | `{ target, wait? }` | `{ job_id }` |
| `GET` | `/api/v1/ingest` | `?limit=&offset=` | `{ jobs: [...IngestJob] }` |
| `GET` | `/api/v1/ingest/{id}` | — | `{ job: IngestJob }` or `404` |
| `DELETE` | `/api/v1/ingest/{id}` | — | `{ canceled: true }` |
| `POST` | `/api/v1/ingest/recover` | — | `{ recovered: N }` |
| `POST` | `/api/v1/ingest/cleanup` | — | `{ deleted: N }` |

Status uses `ingest_status_raw` → `Option<IngestJob>` (outer Option, typed struct).

### Refresh (`handlers_refresh.rs`)

`refresh_status` returns `Result<RefreshJobResult>` where `RefreshJobResult.job: Option<RefreshJob>`. Use inner-`None` pattern for `404` (see Service Variant Rule above). `RefreshJob.status: String` — terminal values same as other job types.

`PATCH /api/v1/refresh/schedules/{name}` is **intentionally limited to toggling `enabled`** in v1. Dispatches on `body.enabled: bool` → `refresh_schedule_enable` or `refresh_schedule_disable`. Any other field in the body returns `400` via `deny_unknown_fields`.

| Method | Path | Body / Params | Response |
|--------|------|---------------|----------|
| `POST` | `/api/v1/refresh` | `{ urls, wait? }` | `{ job_id, urls }` |
| `GET` | `/api/v1/refresh` | `?limit=&offset=` | `{ jobs: [...] }` |
| `GET` | `/api/v1/refresh/{id}` | — | `{ job: RefreshJob }` or `404` |
| `DELETE` | `/api/v1/refresh/{id}` | — | `{ canceled: true }` |
| `POST` | `/api/v1/refresh/recover` | — | `{ recovered: N }` |
| `POST` | `/api/v1/refresh/cleanup` | — | `{ deleted: N }` |
| `GET` | `/api/v1/refresh/schedules` | `?limit=` | `{ schedules: [...] }` |
| `POST` | `/api/v1/refresh/schedules` | `{ ...RefreshScheduleCreate }` | `{ schedule: { ... } }` |
| `PATCH` | `/api/v1/refresh/schedules/{name}` | `{ "enabled": bool }` | `{ updated: true }` |
| `DELETE` | `/api/v1/refresh/schedules/{name}` | — | `{ deleted: true }` |

Note: `RefreshStartResult` has `job_id: String` and `urls: Vec<String>` — no `status` field.

### Graph (`handlers_graph.rs`)

| Method | Path | Body / Params | Response |
|--------|------|---------------|----------|
| `POST` | `/api/v1/graph/build` | `{ url?, domain?, all? }` | GraphBuildResult JSON |
| `GET` | `/api/v1/graph/status` | — | GraphStatusResult JSON |
| `POST` | `/api/v1/graph/explore` | `{ entity }` | GraphExploreResult JSON |
| `GET` | `/api/v1/graph/stats` | — | GraphStatsResult JSON |

### Query & RAG (`handlers_query.rs`)

The `graph?` field in the ask body sets `cfg.use_graph = body.graph.unwrap_or(cfg.use_graph)` before calling `services::query::ask`. The service reads `cfg.use_graph` internally — there is no separate function parameter for graph toggle.

| Method | Path | Body / Params | Response |
|--------|------|---------------|----------|
| `POST` | `/api/v1/query` | `{ query, limit?, collection? }` | QueryResult JSON |
| `POST` | `/api/v1/ask` | `{ question, graph? }` | AskResult JSON |
| `POST` | `/api/v1/retrieve` | `{ url }` | RetrieveResult JSON |
| `POST` | `/api/v1/evaluate` | `{ question }` | EvaluateResult JSON |
| `POST` | `/api/v1/suggest` | `{ focus? }` | SuggestResult JSON |

### Content (`handlers_content.rs`)

| Method | Path | Body / Params | Response |
|--------|------|---------------|----------|
| `POST` | `/api/v1/scrape` | `{ url, embed? }` | ScrapeResult JSON |
| `POST` | `/api/v1/map` | `{ url, limit?, offset? }` | MapResult JSON |
| `POST` | `/api/v1/search` | `{ query, limit?, offset?, time_range? }` | SearchResult JSON |
| `POST` | `/api/v1/research` | `{ query, limit?, depth? }` | ResearchResult JSON |

### System (`handlers_system.rs`)

| Method | Path | Body / Params | Response |
|--------|------|---------------|----------|
| `GET` | `/api/v1/sources` | `?limit=&offset=` | SourcesResult JSON |
| `GET` | `/api/v1/domains` | `?limit=&offset=&detailed=` | DomainsResult JSON |
| `GET` | `/api/v1/stats` | — | StatsResult JSON |
| `GET` | `/api/v1/status` | — | StatusResult JSON |
| `GET` | `/api/v1/doctor` | — | DoctorResult JSON |

### Out of Scope (v1)

| Service / Endpoint | Reason |
|-------------------|--------|
| `crawl_clear`, `embed_clear`, `extract_clear` | Too destructive without confirmation UX — add in v2 |
| `watch.rs` (`list_watch_defs`, `create_watch_def`, `list_watch_runs`) | Requires additional schema investigation — add in v2 |
| `screenshot.rs` (`screenshot_capture`) | Binary response (image bytes) needs separate content-type handling — add in v2 |
| `export.rs` (`export_manifest`) | Large JSON blob better served as a download endpoint — add in v2 |
| `system.rs::dedupe` | Destructive full-collection scan; requires explicit confirmation UX — add in v2 |
| `debug.rs` (`debug_report`) | Internal diagnostic; WS streaming is more appropriate than REST for this |

---

## Error Handling

Shared helpers in `crates/web/api.rs`. All return `Response` (not a tuple) to avoid type-composition issues:

```rust
fn service_err(e: Box<dyn Error>) -> Response {
    (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))).into_response()
}
fn not_found(msg: &str) -> Response {
    (StatusCode::NOT_FOUND, Json(json!({ "error": msg }))).into_response()
}
fn bad_request(msg: &str) -> Response {
    (StatusCode::BAD_REQUEST, Json(json!({ "error": msg }))).into_response()
}
fn unauthorized() -> Response {
    (StatusCode::UNAUTHORIZED, Json(json!({ "error": "unauthorized" }))).into_response()
}
```

---

## Config Propagation

The `Arc<Config>` in `AppState` was built at server startup from env vars. Handlers clone it (`let mut cfg = (*state.cfg).clone()`) and override only the fields relevant to the request body. This preserves all server-level defaults (Qdrant URL, TEI URL, queue names, credentials) while allowing per-request tuning.

Common overrides:
```rust
cfg.max_pages  = body.max_pages.unwrap_or(cfg.max_pages);
cfg.max_depth  = body.max_depth.unwrap_or(cfg.max_depth);
cfg.embed      = body.embed.unwrap_or(cfg.embed);
cfg.use_graph  = body.graph.unwrap_or(cfg.use_graph);  // ask endpoint
```

---

## Testing

Each handler file has a `#[cfg(test)]` module using **`tower::ServiceExt::oneshot()`** with `http::Request` builders — no new crate dependency required.

```rust
#[cfg(test)]
mod tests {
    use tower::ServiceExt;
    use http::{Request, StatusCode};

    #[tokio::test]
    async fn get_status_requires_auth() {
        let app = super::super::router(test_app_state());
        let resp = app
            .oneshot(Request::get("/api/v1/status").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }
}
```

Test coverage per endpoint:
- Happy path: valid auth + valid body → correct HTTP status + response shape
- Auth missing → `401`
- Auth invalid → `401`
- Unknown field in body → `400` (verifying `deny_unknown_fields`)
- `Ok(None)` / inner `.is_none()` job lookup → `404` (not `200` with null)
- Service error → `500`
- SSRF-blocked URL → `400`

---

## Monolith Policy Compliance

All new files must stay within:
- File size: ≤ 500 lines
- Function size: ≤ 120 lines (warn at 80)

The handler split across 9 files (3-4 handlers each at ~30-50 lines per handler) fits comfortably.
