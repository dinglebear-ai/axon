# Axon HTTP API

Last Modified: 2026-05-18

Axon exposes traditional REST routes under `/v1`. `POST /v1/actions` remains for compatibility, but it is deprecated and responses include:

- `Deprecation: true`
- `Sunset: Tue, 01 Sep 2026 00:00:00 GMT`

## Routes

Read routes:

- `GET /v1/capabilities`
- `GET /v1/sources?limit=100&offset=0`
- `GET /v1/domains?limit=100&offset=0`
- `GET /v1/stats`
- `GET /v1/status`
- `GET /v1/doctor`

RAG routes:

- `POST /v1/query` with `{ "query": "...", "limit": 10, "offset": 0 }`
- `POST /v1/retrieve` with `{ "url": "...", "max_points": 20, "cursor": null, "token_budget": 10000 }`
- `POST /v1/evaluate` with `{ "question": "..." }`
- `POST /v1/suggest` with `{ "focus": "..." }`
- `POST /v1/ask` remains supported for existing ask clients.

Exploration routes:

- `POST /v1/scrape` with `{ "url": "..." }` or `{ "urls": ["..."] }`
- `POST /v1/map` with `{ "url": "...", "limit": 100, "offset": 0 }`
- `POST /v1/search` with `{ "query": "...", "limit": 10, "offset": 0, "time_range": "week" }`
- `POST /v1/research` with the same body as search; HTTP requests time out after 35 seconds.

Async job routes:

- `POST /v1/crawl`, `GET /v1/crawl/{id}`
- `POST /v1/embed`, `GET /v1/embed/{id}`
- `POST /v1/extract`, `GET /v1/extract/{id}`
- `POST /v1/ingest`, `GET /v1/ingest/{id}`

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

When MCP HTTP auth is mounted, read routes require `axon:read` and write routes require `axon:write`. A write-scoped token is accepted for read routes. Loopback development mode keeps the existing local trust boundary.
