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
- `POST /v1/evaluate` with `{ "question": "..." }`
- `POST /v1/suggest` with `{ "focus": "..." }`
- `POST /v1/ask` remains supported for existing ask clients.
- `POST /v1/ask/stream` streams the ask synthesis (SSE) for clients that want incremental tokens.

Exploration routes:

- `POST /v1/scrape` with `{ "url": "..." }` or `{ "urls": ["..."] }`
- `POST /v1/summarize` with `{ "url": "..." }` or `{ "urls": ["..."] }`
- `POST /v1/map` with `{ "url": "...", "limit": 100, "offset": 0 }`
- `POST /v1/endpoints` with `{ "url": "...", ... }` — API-endpoint discovery (`axon:write`); see `docs/reference/endpoints.md`.
- `POST /v1/search` with `{ "query": "...", "limit": 10, "offset": 0, "time_range": "week" }`
- `POST /v1/research` with the same body as search; HTTP requests time out after 35 seconds.

Domain filters are exact host matches against indexed `payload.domain` values. `example.com` does not include `docs.example.com` unless that exact host is requested.

Async job routes:

- `POST /v1/crawl`, `GET /v1/crawl`, `GET /v1/crawl/{id}`
- `POST /v1/embed`, `GET /v1/embed`, `GET /v1/embed/{id}`
- `POST /v1/extract`, `GET /v1/extract`, `GET /v1/extract/{id}`
- `POST /v1/ingest`, `GET /v1/ingest`, `GET /v1/ingest/{id}`

Each async family also supports:

- `GET /`
- `POST /{id}/cancel`
- `POST /cleanup`
- `DELETE /`
- `POST /recover`

Start responses use `202 Accepted`, a `Location` header, and:

```json
{
  "job_id": "...",
  "status": "pending",
  "status_url": "/v1/crawl/..."
}
```

Admin routes:

- `POST /v1/dedupe`
- `GET /v1/watch?limit=100`
- `POST /v1/watch`
- `POST /v1/watch/{id}/run`

`POST /v1/migrate` is intentionally not exposed. Collection migration is a long-running CLI-only operation until it has a dedicated async job family.

## Auth

When MCP HTTP auth is mounted, OAuth email allowlisting is the access boundary. Axon-issued OAuth tokens default to both `axon:read` and `axon:write`, and either Axon scope is accepted for all Axon read/write routes for compatibility with existing tokens. Loopback development mode keeps the existing local trust boundary.
