# REST Overview

Last Modified: 2026-07-19

The REST surface (`/v1/*`) is a transport over `axon-api` DTOs and
`axon-services`. It is a clean-break projection over the shared `SourceRequest`
model: `HTTP request → axon-api request DTO → axon-services → transport-neutral
result → HTTP response`. REST does not reimplement source resolution, adapter
selection, graph writes, watch behavior, metadata rules, pruning, or error
taxonomy.

> The machine-readable source of truth is the live OpenAPI 3.1.0 spec at
> [`apps/web/openapi/axon.json`](../../../apps/web/openapi/axon.json)
> (`info.title: "Axon REST API"`, `version: 7.1.5`, 83 paths). The route-contract
> schema at [`openapi.json`](openapi.json) (`AxonOpenApiSchema`) is the
> generator-checked shape contract.

## Serving

`axon serve` mounts the REST surface, MCP-over-HTTP (`/mcp`), the web control
panel, and OpenAPI/Swagger UI on one Axum listener at
`AXON_HTTP_HOST:AXON_HTTP_PORT` (default `127.0.0.1:8001`):

- `/v1/*` — REST API (see [routes.md](routes.md)).
- `/mcp` — MCP streamable HTTP (same DTOs, same auth).
- `/docs` — Swagger UI; `/openapi.json` — OpenAPI document.
- `/healthz`, `/readyz`, `/metrics` — operational endpoints.
- `/api/panel/*` — local setup/config panel APIs (panel-password session auth,
  excluded from the public SDK).

## Shared envelope

Every `/v1/*` response uses the shared envelope:

```json
{
  "ok": true,
  "request_id": "req_...",
  "contract_version": "2026-06-30",
  "data": {},
  "job": null,
  "watch": null,
  "artifacts": [],
  "warnings": [],
  "pagination": null,
  "trace": { "job_id": null, "trace_id": "trace_..." }
}
```

Failures use `error: { code, message, stage, retryable, severity, visibility, details }`.
HTTP status codes map to broad categories; clients should branch on `error.code`
for programmatic behavior.

## Async job starts

Operations that enqueue a durable job return `202` with a narrow job descriptor:

```json
{ "kind": "source", "id": "<job_uuid>", "status": "queued",
  "status_url": "/v1/jobs/<id>", "events_url": "/v1/jobs/<id>/events",
  "stream_url": "/v1/jobs/<id>/stream", "poll_after_ms": 1000 }
```

`kind` ∈ `{source, watch_exec, prune, extract, retrieval}`. For job-starting
ops the top-level `job` field duplicates the descriptor inside `data` so
generic clients can find it.

## Auth

| Bind | Allowed auth |
|---|---|
| loopback (`127.0.0.1` / `::1`) | tokenless **or** `AXON_HTTP_TOKEN` **or** OAuth |
| non-loopback | `AXON_HTTP_TOKEN` (bearer / `x-api-key`) **or** OAuth (`AXON_AUTH_MODE=oauth`) |

`AXON_HTTP_TOKEN` enables static bearer auth. OAuth mode mounts Google
OAuth/JWT at `/.well-known/*`, `/authorize`, `/token`, `/register`. Read routes
require read scope; mutating source/job/watch/prune routes require write scope;
destructive routes (e.g. `prune exec`, `reset exec`) require admin policy.
Newly issued OAuth tokens default to both `axon:read` and `axon:write`.

Local-path source requests over REST are fail-closed unless under loopback
affinity, a configured allowed root (`AXON_SOURCE_LOCAL_ALLOWED_ROOTS`), or a
prepared upload.

## Rules

- REST routes do not own alternate pipeline behavior — they project over
  `axon-services`.
- OpenAPI is generated from the live route and DTO surface.
- Removed pre-unification routes return normal 404 — no aliases, no tombstones.
- Source acquisition uses the canonical `/v1/sources` route.

## Generated files

- [`openapi.json`](openapi.json) — route-contract JSON schema (`AxonOpenApiSchema`).
- [`openapi.md`](openapi.md), [`schemas.md`](schemas.md) — generator-produced markdown.
- [`routes.md`](routes.md) — route-family summary (this tree).

If the REST surface changes, regenerate via `cargo xtask schemas openapi` and
update `apps/web/openapi/axon.json` in the same PR.
