# Axon HTTP API

Last Modified: 2026-07-16

Axon exposes direct REST routes under `/v1`. Direct REST is the canonical client/server API; the legacy `POST /v1/actions` action-envelope endpoint has been removed (it now returns `404`, as does `POST /v1/migrate`).

Process health is served unauthenticated at `GET /healthz` and `GET /readyz`. The admin/setup panel is served under `/api/panel/*` (panel-password session auth) — see `docs/operations/security.md` §6 and `src/web/CLAUDE.md` for its route tree.

## Routes

Read routes:

- `GET /v1/capabilities`
- `GET /v1/sources?limit=100&offset=0`
- `GET /v1/sources?domain=docs.rs&limit=100`
- `GET /v1/sources?domain=docs.rs&limit=100&cursor=<next_cursor>`
- `GET /v1/domains?limit=100&offset=0`
- `GET /v1/domains?domain=docs.rs`
- `GET /v1/stats`
- `GET /v1/status`
- `GET /v1/doctor`

RAG routes:

- `POST /v1/query` with `{ "query": "...", "limit": 10, "offset": 0 }`
- `POST /v1/retrieve` with `{ "url": "...", "max_points": 20, "cursor": null, "token_budget": 10000 }`
- `POST /v1/chat` with `{ "message": "..." }`
- `POST /v1/chat/stream` streams direct LLM chat (SSE).
- `POST /v1/evaluate` with `{ "question": "..." }`
- `POST /v1/suggest` with `{ "focus": "..." }`
- `POST /v1/ask` remains supported for existing ask clients.
- `POST /v1/ask/stream` streams the ask synthesis (SSE) for clients that want incremental tokens.

Exploration routes:

- `POST /v1/sources` with a `SourceRequest`; use `"scope":"page"` for a
  single-page scrape projection, `"scope":"site"` for site acquisition, and a
  `session:<provider>:<path>` source for session transcript ingestion.
- `POST /v1/summarize` with `{ "url": "..." }` or `{ "urls": ["..."] }`
- `POST /v1/summarize/stream` streams summarization synthesis (SSE).
- `POST /v1/map` with `{ "url": "...", "limit": 100, "offset": 0 }`
- `POST /v1/endpoints` with `{ "url": "...", ... }` — API-endpoint discovery (`axon:write`); see `docs/reference/endpoints.md`.
- `POST /v1/brand` with `{ "url": "..." }`
- `POST /v1/diff` with `{ "url_a": "...", "url_b": "..." }`
- `POST /v1/screenshot` with `{ "url": "...", "viewport": "1280x720", "full_page": true }`
- `POST /v1/search` with `{ "query": "...", "limit": 10, "offset": 0, "time_range": "week" }`
- `POST /v1/research` with the same body as search; HTTP requests time out after 35 seconds.
- `POST /v1/research/stream` streams research synthesis (SSE) and emits a terminal `error` event if the 35-second stream budget is exceeded.

Search and research responses report result indexing through
`source_index_status`, `source_jobs`, and `source_jobs_rejected`. The returned
job IDs use the same `/v1/jobs` lifecycle as every other durable operation.

Header forwarding: source-backed web acquisition, `summarize`, and `extract`
accept `headers` arrays for origin fetches. Treat these as credential
forwarding: values may include bearer tokens or cookies for the target origin.
Axon rejects hop-by-hop and internal forwarding headers such as `Connection`,
`Host`, `Content-Length`, `Forwarded`, and `X-Forwarded-*`.

Domain filters are exact host matches against indexed `payload.domain` values. `example.com` does not include `docs.example.com` unless that exact host is requested.

Artifacts:

- `GET /v1/artifacts` lists artifact metadata and accepts `kind`, `source_id`, `job_id`, `limit`, and `cursor` filters.
- `GET /v1/artifacts/{artifact_id}` returns metadata for one opaque artifact ID.
- `GET /v1/artifacts/{artifact_id}/content` returns bytes; `download=true` forces attachment disposition.
- Filesystem paths are never accepted or exposed by the public REST API.
- Browser apps fetch authenticated bytes and render object URLs; image tags must not point directly at authenticated artifact routes.
- Only raster image artifacts are inline preview content. HTML, SVG, unknown types, JSON, markdown, text, and logs are served as attachments with `nosniff`.

Async job routes:

- `POST /v1/extract`
- `GET /v1/jobs`, `GET /v1/jobs/{id}`, `GET /v1/jobs/{id}/events`

Extract job status, cancellation, cleanup, clear, and recovery use canonical
`/v1/jobs` routes. The legacy `/v1/extract/*` lifecycle routes were removed.

Start responses use `202 Accepted`, a `Location` header, and:

```json
{
  "job_id": "...",
  "status": "pending",
  "status_url": "/v1/jobs/..."
}
```

The removed indexing routes `POST /v1/embed`, `POST /v1/ingest`,
`POST /v1/scrape`, `POST /v1/crawl`, the removed admin routes
`POST /v1/purge`, `POST /v1/dedupe`, and the old `/v1/extract/*` lifecycle
routes now return `404`. Use `POST /v1/sources` for source
acquisition/indexing, `/v1/prune/*` for cleanup, and `/v1/jobs` for job
lifecycle.

Admin routes:

- `POST /v1/prune/plan`
- `POST /v1/prune/exec`
- `GET /v1/watches?limit=100`
- `POST /v1/watches`
- `POST /v1/watches/{watch_id}/exec`

Cleanup selectors, including duplicate and targeted-removal policies, are
inputs to prune planning/execution. There are no public `dedupe` or `purge`
commands, actions, or REST subroutes. Memory uses the explicit `/v1/memories/*`
resource routes; the deprecated singular `POST /v1/memory` route is absent.

`POST /v1/migrate` is intentionally not exposed. Collection migration is a long-running CLI-only operation until it has a dedicated async job family.

## Auth

When MCP HTTP auth is mounted, OAuth email allowlisting is the access boundary. Axon-issued OAuth tokens default to both `axon:read` and `axon:write`, and either Axon scope is accepted for all Axon read/write routes for compatibility with existing tokens. Loopback development mode keeps the existing local trust boundary.

OpenAPI declares both static/JWT bearer auth (`bearerAuth`) and OAuth authorization-code auth (`oauth2`). Protected operations include operation-level security with the required Axon scope; unauthenticated `healthz`, `readyz`, Swagger UI, and the OpenAPI JSON route remain public.

All REST auth and handler failures use the JSON error envelope:

```json
{ "kind": "unauthorized", "message": "unauthorized" }
```
