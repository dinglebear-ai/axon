# Axon HTTP API

Last Modified: 2026-06-01

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
  single-page scrape projection and `"scope":"site"` for site acquisition.
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

Header forwarding: source-backed web acquisition, `summarize`, and `extract`
accept `headers` arrays for origin fetches. Treat these as credential
forwarding: values may include bearer tokens or cookies for the target origin.
Axon rejects hop-by-hop and internal forwarding headers such as `Connection`,
`Host`, `Content-Length`, `Forwarded`, and `X-Forwarded-*`.

Domain filters are exact host matches against indexed `payload.domain` values. `example.com` does not include `docs.example.com` unless that exact host is requested.

Artifact download:

- `GET /v1/artifacts?path=<relative_path>` serves files under `output_dir` and requires read auth.
- Clients must pass the `relative_path` from an `ArtifactHandle`; absolute server paths are not accepted.
- `GET /v1/artifacts/{relative_path}` is kept as a legacy compatibility route; new clients should use the query form so slash-preserving paths are explicit.
- Browser apps fetch authenticated bytes and render object URLs; image tags must not point directly at authenticated artifact routes.
- Only raster image artifacts are inline preview content. HTML, SVG, unknown types, JSON, markdown, text, and logs are served as attachments with `nosniff`.

Async job routes:

- `POST /v1/extract`, `GET /v1/extract`, `GET /v1/extract/{id}`
- `POST /v1/ingest/sessions/prepared`
- `GET /v1/jobs`, `GET /v1/jobs/{id}`, `GET /v1/jobs/{id}/events`

The extract family also supports:

- `GET /v1/{family}`
- `POST /v1/{family}/{id}/cancel`
- `POST /v1/{family}/cleanup`
- `DELETE /v1/{family}`
- `POST /v1/{family}/recover`

Start responses use `202 Accepted`, a `Location` header, and:

```json
{
  "job_id": "...",
  "status": "pending",
  "status_url": "/v1/jobs/..."
}
```

The removed indexing routes `POST /v1/embed`, `POST /v1/ingest`,
`POST /v1/scrape`, and `POST /v1/crawl` now return `404`. Use
`POST /v1/sources` for all source acquisition/indexing.

Admin routes:

- `POST /v1/prune/plan`
- `POST /v1/prune/exec`
- `GET /v1/watches?limit=100`
- `POST /v1/watches`
- `POST /v1/watches/{watch_id}/exec`

`POST /v1/migrate` is intentionally not exposed. Collection migration is a long-running CLI-only operation until it has a dedicated async job family.

## Auth

When MCP HTTP auth is mounted, OAuth email allowlisting is the access boundary. Axon-issued OAuth tokens default to both `axon:read` and `axon:write`, and either Axon scope is accepted for all Axon read/write routes for compatibility with existing tokens. Loopback development mode keeps the existing local trust boundary.

OpenAPI declares both static/JWT bearer auth (`bearerAuth`) and OAuth authorization-code auth (`oauth2`). Protected operations include operation-level security with the required Axon scope; unauthenticated `healthz`, `readyz`, Swagger UI, and the OpenAPI JSON route remain public.

All REST auth and handler failures use the JSON error envelope:

```json
{ "kind": "unauthorized", "message": "unauthorized" }
```
