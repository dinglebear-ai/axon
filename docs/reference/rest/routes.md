# REST Routes

Last Modified: 2026-07-19

The `/v1/*` surface grouped by family. The machine-readable source of truth is
the live OpenAPI 3.1.0 spec at
[`apps/web/openapi/axon.json`](../../../apps/web/openapi/axon.json)
(83 paths); this page is a navigable summary.

> Path-param naming: the live OpenAPI uses `{id}` for jobs/uploads/mobile; the
> pipeline-unification contract uses `{job_id}`/`{upload_id}`/`{session_id}`.
> Both refer to the same path segment.

## System / capability

| Method | Path |
|---|---|
| GET | `/healthz`, `/readyz` |
| GET | `/v1/capabilities`, `/v1/status`, `/v1/doctor`, `/v1/stats` |
| GET | `/v1/providers`, `/v1/providers/{provider}` |
| GET | `/v1/domains` |
| GET | `/v1/collections`, `/v1/collections/{collection}` |

## Sources (acquisition + listing)

| Method | Path |
|---|---|
| POST | `/v1/sources` — canonical acquisition (`SourceRequest`) |
| GET | `/v1/sources` — list indexed sources |
| GET | `/v1/sources/{source_id}` |
| POST | `/v1/resolve` — resolve a source string without acquiring |
| POST | `/v1/map` — discover items without embedding |

## Documents / retrieval

| Method | Path |
|---|---|
| POST | `/v1/retrieve` — fetch a source's chunks/documents |

## Jobs (durable lifecycle)

| Method | Path |
|---|---|
| GET | `/v1/jobs` — list |
| DELETE | `/v1/jobs` — clear |
| POST | `/v1/jobs/cleanup`, `/v1/jobs/recover` |
| GET | `/v1/jobs/{id}`, `/v1/jobs/{id}/artifacts`, `/v1/jobs/{id}/events`, `/v1/jobs/{id}/stream` |
| POST | `/v1/jobs/{id}/cancel`, `/v1/jobs/{id}/retry` |

## Watches

| Method | Path |
|---|---|
| GET / POST | `/v1/watches` |
| GET / PATCH / DELETE | `/v1/watches/{watch_id}` |
| POST | `/v1/watches/{watch_id}/exec`, `/pause`, `/resume` |
| GET | `/v1/watches/{watch_id}/history`, `/status` |

## Graph

| Method | Path |
|---|---|
| GET | `/v1/graph/kinds`, `/v1/graph/edges/{edge_id}` |
| GET | `/v1/graph/nodes/{node_id}`, `/v1/graph/nodes/{node_id}/edges` |
| GET | `/v1/graph/sources/{source_id}` |
| POST | `/v1/graph/query`, `/v1/graph/resolve` |

## Retrieval / synthesis

| Method | Path |
|---|---|
| POST | `/v1/query`, `/v1/ask`, `/v1/ask/stream` |
| POST | `/v1/chat`, `/v1/chat/stream` |
| POST | `/v1/search`, `/v1/research`, `/v1/research/stream` |
| POST | `/v1/summarize`, `/v1/summarize/stream` |
| POST | `/v1/evaluate`, `/v1/suggest` |

## Inspection / extraction

| Method | Path |
|---|---|
| POST | `/v1/map`, `/v1/endpoints`, `/v1/brand`, `/v1/diff`, `/v1/screenshot`, `/v1/extract` |

## Memories

| Method | Path |
|---|---|
| GET / POST | `/v1/memories` |
| POST | `/v1/memories/search`, `/context`, `/review`, `/compact`, `/import`, `/export` |
| GET / PATCH / DELETE | `/v1/memories/{memory_id}` |
| POST | `/v1/memories/{memory_id}/{link,supersede,reinforce,contradict,pin,archive}` |

## Artifacts / uploads

| Method | Path |
|---|---|
| GET | `/v1/artifacts`, `/v1/artifacts/{artifact_id}`, `/v1/artifacts/{artifact_id}/content` |
| GET / POST | `/v1/uploads` |
| GET / DELETE | `/v1/uploads/{upload_id}` |
| PUT | `/v1/uploads/{upload_id}/content` |
| POST | `/v1/uploads/{upload_id}/complete` |

## Prune / reset

| Method | Path |
|---|---|
| POST | `/v1/prune/plan`, `/v1/prune/exec`, `/v1/prune/jobs/{job_id}` |
| POST | `/v1/reset/plan`, `/v1/reset/exec` |

## Mobile sessions

| Method | Path |
|---|---|
| GET | `/v1/mobile/sessions` |
| GET / PUT / DELETE | `/v1/mobile/sessions/{id}` |

## Removed routes

Removed pre-unification routes (`/v1/actions`, `/v1/migrate`, `/v1/scrape`,
`/v1/crawl`, `/v1/embed`, `/v1/ingest`, family-scoped `/v1/{crawl,embed,...}*`,
`/v1/watch`, `/v1/memory`) are **not** mounted as aliases — they return normal
404. Compatibility belongs in migration data handling, not public HTTP routes.
Acquisition is `/v1/sources`; watches are `/v1/watches`; memories are
`/v1/memories/*`.

If routes change, update `crates/axon-web/src/schema_registry.rs` and
regenerate via `cargo xtask schemas openapi` in the same PR.
