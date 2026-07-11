# REST Contract
Last Modified: 2026-06-30

## Contract

This is the target clean-break REST contract. It intentionally describes the
desired end-state surface, not the route inventory currently implemented.

REST is a projection over the shared `SourceRequest` model.

```text
HTTP request
  -> axon-api request DTO
  -> axon-services
  -> transport-neutral result
  -> HTTP response
```

REST must not reimplement source resolution, adapter selection, graph writes,
watch behavior, metadata rules, provider selection, pruning, or error taxonomy.

This file describes the complete desired end-state REST shape. It is not a
description of the current route inventory.

Canonical OpenAPI/client generation must expose only the routes below. Removed
verb routes are not part of the desired client surface.

This is a clean-break contract. There are no compatibility aliases, hidden
legacy routes, backwards-compatible request shims, or public tombstone windows.
Removed routes are deleted from the router, OpenAPI, generated clients, and
docs.

Clean break means old route names can disappear; it does not mean current REST
capabilities disappear. Any capability currently exposed over REST must either
have a canonical route in this document or be listed as intentionally CLI-only /
not exposed.

## Current Implementation Snapshot

Implemented today:

- REST still exposes direct operation routes such as `/v1/scrape`, `/v1/crawl`,
  `/v1/embed`, `/v1/extract`, `/v1/ingest`, `/v1/purge`, `/v1/dedupe`,
  `/v1/search`, `/v1/research`, `/v1/query`, `/v1/retrieve`, `/v1/map`,
  `/v1/ask`, `/v1/watch/{id}/run`, and `/api-docs/openapi.json`.
- Job REST routes are family-scoped today: `/v1/crawl`, `/v1/embed`,
  `/v1/extract`, and `/v1/ingest` expose list/status/cancel/errors/cleanup/
  clear/recover/worker-style operations instead of a generic `/v1/jobs`
  collection.
- `GET /v1/sources` is a discovery/listing route today; `POST /v1/sources` is
  not the canonical acquisition path yet.
- Async job starts return `202` with a narrow job response such as `job_id`,
  `status`, and `status_url`.
- Success responses are raw typed results or route-specific response objects,
  not one universal success envelope.
- REST errors currently use `kind`, `message`, and optional `diagnostics`
  fields, not the full target `ApiError` shape.
- Read/write route groups are enforced with broad Axon read/write scopes.

Planned by this contract:

- Direct verb/family routes are removed, and the canonical source
  lifecycle route is `/v1/sources`.
- Job events, streams, retries, watches, artifacts, uploads, graph, providers,
  and prune routes use the end-state route inventory below.
- OpenAPI generation reflects only the clean-break route surface.

## Shared Response Envelope

Every non-stream, non-byte REST response uses the same envelope as CLI JSON and
MCP responses. Route examples may focus on `data`, but generated OpenAPI and
fixtures must include the complete envelope shape.

Success:

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
  "trace": {
    "job_id": null,
    "trace_id": "trace_..."
  }
}
```

Failure:

```json
{
  "ok": false,
  "request_id": "req_...",
  "contract_version": "2026-06-30",
  "error": {
    "code": "route.validation.invalid_field",
    "message": "Invalid request.",
    "stage": "validation",
    "retryable": false,
    "severity": "failed",
    "visibility": "public",
    "details": {}
  },
  "warnings": [],
  "trace": {
    "job_id": null,
    "trace_id": "trace_..."
  }
}
```

Streaming routes use `StreamEvent`/`SourceProgressEvent` envelopes from the
event schema. Artifact-content routes may return bytes after the metadata route
authorizes the read.

## End-State Route Inventory

| Method | Route | Auth | Purpose |
|---|---|---|
| `GET` | `/healthz` | public | Liveness. |
| `GET` | `/readyz` | public | Dependency readiness. |
| `GET` | `/metrics` | public/internal | Prometheus metrics. |
| `GET` | `/openapi.json` | read | Canonical OpenAPI document for this contract. |
| `GET` | `/docs` | public | Interactive OpenAPI docs. |
| `GET` | `/v1/server` | read | Server version, contract version, build info, auth mode. |
| `GET` | `/v1/capabilities` | read | Complete capability document. |
| `GET` | `/v1/adapters` | read | List source adapters and scopes. |
| `GET` | `/v1/adapters/{adapter}` | read | Adapter option schema, scopes, credentials, limits. |
| `GET` | `/v1/providers` | read | LLM/embedding/vector/ledger/graph/memory/artifact provider status and capabilities. |
| `GET` | `/v1/providers/{provider}` | read | One provider capability/health report. |
| `POST` | `/v1/preflight` | read | Validate config, providers, credentials, paths, and limits before starting work. |
| `POST` | `/v1/smoke` | write/admin | Run explicit live smoke checks against configured providers. |
| `POST` | `/v1/resolve` | read | Resolve a source without mutation. |
| `POST` | `/v1/sources` | write | Create/acquire/refresh a source lifecycle. |
| `GET` | `/v1/sources` | read | List ledger-known sources. |
| `GET` | `/v1/sources/{source_id}` | read | Source detail. |
| `PATCH` | `/v1/sources/{source_id}` | write | Update user label, tags, authority pins, stored options. |
| `POST` | `/v1/sources/{source_id}/refresh` | write | Refresh an existing source. |
| `POST` | `/v1/sources/{source_id}/resolve` | read | Re-resolve stored source aliases/authority without mutation. |
| `DELETE` | `/v1/sources/{source_id}` | write/admin | Create prune debt or destructive source removal job. |
| `GET` | `/v1/sources/{source_id}/items` | read | Ledger source items/files/pages/entries. |
| `GET` | `/v1/sources/{source_id}/items/{item_key}` | read | One source item. |
| `GET` | `/v1/sources/{source_id}/documents` | read | Prepared/indexed document status. |
| `GET` | `/v1/sources/{source_id}/generations` | read | Source generation history. |
| `GET` | `/v1/sources/{source_id}/generations/{generation}` | read | One source generation. |
| `GET` | `/v1/sources/{source_id}/graph` | read | Graph nodes/edges tied to source. |
| `GET` | `/v1/sources/{source_id}/artifacts` | read | Artifacts produced by source jobs. |
| `GET` | `/v1/domains` | read | Indexed domain/source-host summaries. |
| `GET` | `/v1/documents/{document_id}` | read | Prepared document metadata and chunk summary. |
| `GET` | `/v1/documents/{document_id}/chunks` | read | Document chunks and vector payload metadata. |
| `GET` | `/v1/documents/{document_id}/chunks/{chunk_id}` | read | One prepared chunk. |
| `GET` | `/v1/jobs` | read | List source jobs. |
| `GET` | `/v1/jobs/{job_id}` | read | Latest job status. |
| `GET` | `/v1/jobs/{job_id}/events` | read | Durable progress events. |
| `GET` | `/v1/jobs/{job_id}/stream` | read | SSE progress stream. |
| `GET` | `/v1/jobs/{job_id}/artifacts` | read | Artifacts produced by job. |
| `POST` | `/v1/jobs/recover` | write/admin | Recover stale/interrupted jobs. |
| `POST` | `/v1/jobs/cleanup` | write/admin | Cleanup old terminal job rows/events. |
| `DELETE` | `/v1/jobs` | write/admin | Clear terminal jobs/events by filter. |
| `POST` | `/v1/jobs/{job_id}/cancel` | write | Request cancellation. |
| `POST` | `/v1/jobs/{job_id}/retry` | write | Retry from submitted config snapshot. |
| `POST` | `/v1/watches` | write | Create/ensure a source watch. |
| `GET` | `/v1/watches` | read | List watches. |
| `GET` | `/v1/watches/{watch_id}` | read | Watch detail. |
| `PATCH` | `/v1/watches/{watch_id}` | write | Update schedule/options/enabled state. |
| `POST` | `/v1/watches/{watch_id}/exec` | write | Run watch now. |
| `POST` | `/v1/watches/{watch_id}/pause` | write | Pause watch. |
| `POST` | `/v1/watches/{watch_id}/resume` | write | Resume watch. |
| `DELETE` | `/v1/watches/{watch_id}` | write | Delete watch. |
| `GET` | `/v1/watches/{watch_id}/history` | read | Watch job history. |
| `GET` | `/v1/graph/kinds` | read | Supported node/edge/evidence kinds. |
| `POST` | `/v1/graph/resolve` | read | Resolve identifiers to graph nodes. |
| `POST` | `/v1/graph/query` | read | Typed graph query. |
| `GET` | `/v1/graph/nodes/{node_id}` | read | Node detail. |
| `GET` | `/v1/graph/nodes/{node_id}/edges` | read | Node edges/neighbors. |
| `GET` | `/v1/graph/edges/{edge_id}` | read | Edge detail and evidence. |
| `GET` | `/v1/graph/sources/{source_id}` | read | Source-linked graph subgraph. |
| `POST` | `/v1/query` | read | Vector/graph query. |
| `POST` | `/v1/retrieve` | read | Retrieve stored chunks/documents. |
| `POST` | `/v1/ask` | read | Retrieval plus synthesis; write scope only if request persists trace/artifacts. |
| `POST` | `/v1/ask/stream` | read | Streaming retrieval plus synthesis; write scope only if request persists trace/artifacts. |
| `POST` | `/v1/chat` | read | Direct LLM chat without retrieval; write scope only if request persists trace/artifacts. |
| `POST` | `/v1/chat/stream` | read | Streaming direct LLM chat; write scope only if request persists trace/artifacts. |
| `POST` | `/v1/evaluate` | read | RAG evaluation/judging; write scope only if persisted as artifact/job. |
| `POST` | `/v1/suggest` | read | Suggest sources to acquire. |
| `POST` | `/v1/search` | read | Web search; write scope only when `auto_source=true`. |
| `POST` | `/v1/research` | read | Web research plus synthesis; write scope only when `auto_source=true` or artifacts are persisted. |
| `POST` | `/v1/research/stream` | read | Streaming web research plus synthesis; write scope only when mutating options are set. |
| `POST` | `/v1/summarize` | read | Scrape/fetch and summarize one or more sources; write scope only when artifacts are persisted. |
| `POST` | `/v1/summarize/stream` | read | Streaming summarization; write scope only when artifacts are persisted. |
| `POST` | `/v1/map` | read/write | Discover source items/URLs without embedding. |
| `POST` | `/v1/endpoints` | write | Discover API/network endpoints from a web source. |
| `POST` | `/v1/brand` | write | Extract brand assets/identity from a source. |
| `POST` | `/v1/diff` | write | Compare two sources/URLs. |
| `POST` | `/v1/screenshot` | write | Capture a screenshot artifact. |
| `POST` | `/v1/extract` | write | Explicit structured LLM extraction. |
| `POST` | `/v1/memories` | write | Remember one durable memory. |
| `GET` | `/v1/memories` | read | List memory metadata. |
| `POST` | `/v1/memories/search` | read | Semantic memory search. |
| `POST` | `/v1/memories/context` | read | Build bounded memory context. |
| `GET` | `/v1/memories/review` | read | List memories needing confirmation, conflict handling, or decay review. |
| `POST` | `/v1/memories/compact` | write | Distill related memories into a compact durable memory. |
| `POST` | `/v1/memories/import` | write | Import portable memory bundle. |
| `GET` | `/v1/memories/export` | read | Export portable memory bundle. |
| `GET` | `/v1/memories/{memory_id}` | read | Show one memory. |
| `PATCH` | `/v1/memories/{memory_id}` | write | Update memory metadata/status. |
| `POST` | `/v1/memories/{memory_id}/links` | write | Link memory to another memory/source/graph node. |
| `POST` | `/v1/memories/{memory_id}/supersede` | write | Supersede another memory with this memory. |
| `POST` | `/v1/memories/{memory_id}/reinforce` | write | Reinforce, decay-adjust, or mark memory use. |
| `POST` | `/v1/memories/{memory_id}/contradict` | write | Mark another memory as conflicting with this memory. |
| `POST` | `/v1/memories/{memory_id}/pin` | write | Pin memory or set minimum recall score. |
| `POST` | `/v1/memories/{memory_id}/archive` | write | Archive memory without deleting history. |
| `DELETE` | `/v1/memories/{memory_id}` | write/admin | Forget memory with tombstone/audit record. |
| `GET` | `/v1/artifacts` | read | List artifacts. |
| `GET` | `/v1/artifacts/{artifact_id}` | read | Artifact metadata. |
| `GET` | `/v1/artifacts/{artifact_id}/content` | read | Artifact bytes/content. |
| `POST` | `/v1/uploads` | write | Create a prepared upload for remote file/session/source ingestion. |
| `GET` | `/v1/uploads/{upload_id}` | read | Upload metadata/status. |
| `PUT` | `/v1/uploads/{upload_id}/content` | write | Upload bytes/content. |
| `POST` | `/v1/uploads/{upload_id}/complete` | write | Finalize upload and return an artifact/source reference. |
| `DELETE` | `/v1/uploads/{upload_id}` | write | Abort upload and delete staged bytes. |
| `POST` | `/v1/prune/plan` | write/admin | Dry-run prune plan. |
| `POST` | `/v1/prune/exec` | write/admin | Execute prune plan. |
| `POST` | `/v1/prune/dedupe` | write/admin | Deduplicate near-identical vector chunks. |
| `POST` | `/v1/prune/purge` | write/admin | Purge indexed content by source/url/filter. |
| `GET` | `/v1/prune/jobs/{job_id}` | read | Prune job status projection. |
| `POST` | `/v1/reset/plan` | write/admin | Plan destructive clean-slate reset. |
| `POST` | `/v1/reset/exec` | write/admin | Execute confirmed clean-slate reset. |
| `GET` | `/v1/mobile/sessions` | read | List mobile chat/session state. |
| `GET` | `/v1/mobile/sessions/{session_id}` | read | Mobile session detail. |
| `PUT` | `/v1/mobile/sessions/{session_id}` | write | Upsert mobile session state. |
| `DELETE` | `/v1/mobile/sessions/{session_id}` | write | Delete mobile session state. |
| `GET` | `/api/panel/state` | panel | Panel session/bootstrap state. |
| `POST` | `/api/panel/login` | panel | Panel password login. |
| `GET` | `/api/panel/config` | panel | Read editable config TOML. |
| `PUT` | `/api/panel/config` | panel | Save editable config TOML. |
| `GET` | `/api/panel/env` | panel | Read editable `.env`. |
| `PUT` | `/api/panel/env` | panel | Save editable `.env`. |
| `GET` | `/api/panel/status` | panel | Panel status projection. |
| `GET` | `/api/panel/doctor` | panel | Panel doctor projection. |
| `POST` | `/api/panel/command` | panel | Execute panel-approved command. |
| `GET` | `/api/panel/ops` | panel | Panel operations metadata. |
| `GET` | `/api/panel/collections` | panel | Panel collection list. |
| `GET` | `/api/panel/stack` | panel | Stack/service status. |
| `POST` | `/api/panel/first-run/crawl` | panel | First-run crawl helper. |
| `POST` | `/api/panel/first-run/ask` | panel | First-run ask helper. |
| `GET` | `/api/panel/setup/targets` | panel | Setup target inventory. |
| `GET` | `/api/panel/artifact/{path}` | panel | Panel-auth artifact content. |
| `GET` | `/v1/status` | read | Aggregate source/job/provider status. |
| `GET` | `/v1/doctor` | read | Diagnostic report. |
| `GET` | `/v1/collections` | read | Vector collection inventory. |
| `GET` | `/v1/collections/{collection}` | read | Vector collection detail and payload indexes. |
| `GET` | `/v1/stats` | read | Store/index stats. |

## Resource Model

| Resource | ID | Owner |
|---|---|---|
| Source | `source_id` | `axon-ledger` |
| Source item | `source_item_key` scoped by `source_id` | `axon-ledger` |
| Generation | numeric generation scoped by `source_id` | `axon-ledger` |
| Chunk | `chunk_id` scoped by `document_id` | `axon-document` + `axon-vectors` |
| Document | `document_id` | `axon-document` + `axon-ledger` |
| Job | `job_id` | `axon-jobs` |
| Watch | `watch_id` | `axon-jobs`/watch scheduler |
| Upload | `upload_id` | `ArtifactStore` staging + `axon-web` request handling |
| Mobile session | `session_id` | mobile session store |
| Graph node | `node_id` | `axon-graph` |
| Graph edge | `edge_id` | `axon-graph` |
| Memory | `memory_id` | `axon-memory` over `MemoryStore` + `axon-graph` + `axon-vectors` |
| Artifact | `artifact_id` | `ArtifactStore` |
| Prune plan | `prune_plan_id` | `axon-prune` |

## Common Types

### Job Descriptor

Any response that starts or references background work includes this shape:

```json
{
  "kind": "source",
  "id": "uuid",
  "status_url": "/v1/jobs/uuid",
  "events_url": "/v1/jobs/uuid/events",
  "stream_url": "/v1/jobs/uuid/stream",
  "poll_after_ms": 5000
}
```

`kind` values:

- `source`
- `watch_exec`
- `prune`
- `extract`
- `retrieval`

### Source Summary

```json
{
  "source_id": "src_...",
  "canonical_uri": "https://github.com/jmagar/axon",
  "display_name": "jmagar/axon",
  "source_kind": "git",
  "adapter": "github",
  "default_scope": "repo",
  "authority": "official",
  "current_generation": 42,
  "committed_generation": 42,
  "status": "complete",
  "watch_id": "watch_...",
  "graph_node_ids": ["node_..."],
  "counts": {
    "items": 1200,
    "documents": 1200,
    "chunks": 5200,
    "graph_nodes": 88,
    "graph_edges": 230,
    "artifacts": 3
  },
  "last_job_id": "uuid",
  "last_refreshed_at": "2026-06-30T16:20:00Z",
  "created_at": "2026-06-30T16:00:00Z",
  "updated_at": "2026-06-30T16:20:00Z",
  "tags": [],
  "user_label": null
}
```

### Graph Reference

```json
{
  "node_id": "node_...",
  "kind": "repo",
  "canonical_uri": "https://github.com/jmagar/axon",
  "display_name": "jmagar/axon",
  "authority": "official",
  "confidence": 0.98
}
```

## Canonical Source Request

`POST /v1/sources` creates or refreshes a source lifecycle. It may enqueue a job
or run synchronously depending on `execution`.

Request:

```json
{
  "source": "https://ui.shadcn.com/docs",
  "adapter": null,
  "scope": "site",
  "embed": true,
  "refresh": "if_stale",
  "watch": {
    "enabled": false,
    "mode": "detached",
    "schedule": null
  },
  "execution": {
    "mode": "background",
    "wait": false,
    "timeout_ms": null
  },
  "collection": "axon",
  "options": {
    "render_mode": "auto_switch",
    "max_pages": 2000,
    "max_depth": 10,
    "headers": [],
    "respect_robots": false
  },
  "metadata": {
    "user_label": null,
    "tags": []
  }
}
```

Response:

```json
{
  "ok": true,
  "data": {
  "job_id": "job_...",
    "source_id": "src_...",
    "canonical_uri": "https://ui.shadcn.com/docs",
    "source_kind": "web",
    "adapter": "web",
    "scope": "site",
    "status": "running",
    "generation": 42,
    "authority": "inferred",
    "job": {
      "kind": "source",
      "id": "uuid",
      "status_url": "/v1/jobs/uuid",
      "events_url": "/v1/jobs/uuid/events",
      "stream_url": "/v1/jobs/uuid/stream",
      "poll_after_ms": 5000
    },
    "watch": null,
    "warnings": []
  }
}
```

## Resolve Route

`POST /v1/resolve` accepts target-shaping fields but must not fetch, embed,
mutate LedgerStore, write GraphStore, write ArtifactStore, or create a job.

Request:

```json
{
  "source": "shadcn.com",
  "adapter": null,
  "scope": null,
  "hints": {
    "prefer_official": true,
    "allow_network_probe": false
  }
}
```

Response:

```json
{
  "ok": true,
  "data": {
    "source": "shadcn.com",
    "canonical_uri": "https://ui.shadcn.com/docs",
    "source_kind": "web",
    "adapter": "web",
    "default_scope": "site",
    "available_scopes": ["page", "site", "docs", "map"],
    "authority": "inferred",
    "confidence": 0.86,
    "reason": "host alias mapped to known docs root",
    "graph": {
      "node_id": "node_...",
      "edge_ids": ["edge_..."]
    },
    "warnings": []
  },
  "warnings": [],
  "request_id": "req_...",
  "job": null
}
```

## Source Routes

`GET /v1/sources` lists ledger-known sources.

Query parameters:

| Parameter | Meaning |
|---|---|
| `kind` | Filter by source kind. |
| `adapter` | Filter by adapter. |
| `q` | Search label, canonical URI, project key, package name, repo slug. |
| `status` | Filter by current lifecycle status. |
| `limit` / `cursor` | Pagination. |

`GET /v1/sources/{source_id}` returns source identity, current committed
generation, latest job, watch status, graph node ids, counts, last errors, and
durable metadata.

Detail response:

```json
{
  "ok": true,
  "data": {
    "source": {
      "source_id": "src_...",
      "canonical_uri": "https://github.com/jmagar/axon",
      "display_name": "jmagar/axon",
      "source_kind": "git",
      "adapter": "github",
      "authority": "official",
      "current_generation": 42,
      "committed_generation": 42,
      "status": "complete",
      "metadata": {}
    },
    "watch": {
      "watch_id": "watch_...",
      "enabled": true,
      "schedule": {"kind": "filesystem", "debounce_ms": 1000},
      "next_run_at": null,
      "last_job_id": "uuid"
    },
    "graph": {
      "nodes": [],
      "edges": []
    },
    "latest_job": null,
    "warnings": []
  },
  "warnings": [],
  "request_id": "req_...",
  "job": null
}
```

`POST /v1/sources/{source_id}/refresh` forces or conditionally refreshes an
existing source using its stored config snapshot unless override fields are
provided.

`DELETE /v1/sources/{source_id}` is destructive and must create prune debt or a
prune job. It must not silently delete VectorStore/ArtifactStore state without a
durable pruning record.

`GET /v1/sources/{source_id}/items` returns source item rows with path/URL/key,
hash, status, generation, graph refs, document ids, and last error.

`GET /v1/sources/{source_id}/documents` returns document status rows with chunk
counts, vector point counts, committed generation, payload keys, and graph refs.

`GET /v1/documents/{document_id}` returns the prepared document metadata,
source linkage, committed generation, chunk summary, vector payload keys,
graph refs, and current `DocumentStatus`.

`GET /v1/documents/{document_id}/chunks` returns chunk metadata and content
according to caller authorization and redaction policy. It must not require a
VectorStore scroll; document/chunk identity comes from ledger/document state.

`GET /v1/sources/{source_id}/generations` returns generation history, publish
state, counts, cleanup debt, and prune status.

## Job Status and Events

`GET /v1/jobs/{job_id}` returns the latest durable `SourceJobStatus`.

```json
{
  "job_id": "job_...",
  "source_id": "src_...",
  "source_kind": "web",
  "canonical_uri": "https://ui.shadcn.com/docs",
  "adapter": "web",
  "scope": "site",
  "generation": 42,
  "phase": "embedding",
  "status": "running",
  "severity": "info",
  "message": "embedding changed documents",
  "started_at": "2026-06-30T16:20:00Z",
  "heartbeat_at": "2026-06-30T16:21:00Z",
  "finished_at": null,
  "counts": {
    "items_total": 1200,
    "items_done": 431,
    "items_failed": 0,
    "items_skipped": 12,
    "chunks_total": 5200,
    "chunks_done": 1800,
    "bytes_total": 1234567,
    "bytes_done": 456789
  },
  "current": {
    "source_item_key": "docs/components/button",
    "document_id": "doc_...",
    "path": null
  },
  "last_error": null,
  "warnings": [],
  "degraded": false
}
```

`GET /v1/jobs/{job_id}/events` returns persisted `SourceProgressEvent` values in
durable sequence order. It supports `after_sequence`, `limit`, and `cursor`.

`GET /v1/jobs/{job_id}/stream` is SSE. SSE is a transport projection over the
same persisted event schema; it must not invent separate event names or fields.

`POST /v1/jobs/{job_id}/cancel` requests cancellation. Cancellation is
best-effort and must end in `canceled`, `completed`, `failed`, or
`completed_degraded`.

`POST /v1/jobs/{job_id}/retry` retries a failed/degraded job from its submitted
config snapshot unless overrides are provided.

## Progress Event Shape

`GET /v1/jobs/{job_id}/events` and SSE both use this event shape:

```json
{
  "event_id": "evt_...",
  "sequence": 123,
  "job_id": "job_...",
  "source_id": "src_...",
  "source_kind": "git",
  "canonical_uri": "https://github.com/jmagar/axon",
  "adapter": "github",
  "scope": "repo",
  "generation": 42,
  "phase": "embedding",
  "status": "running",
  "severity": "info",
  "visibility": "public",
  "message": "embedding changed files",
  "timestamp": "2026-06-30T16:20:00Z",
  "counts": {
    "items_total": 1200,
    "items_done": 431,
    "items_failed": 0,
    "items_skipped": 12,
    "chunks_total": 5200,
    "chunks_done": 1800,
    "chunks_failed": 0,
    "chunks_skipped": 0,
    "bytes_total": 1234567,
    "bytes_done": 456789
  },
  "current": {
    "source_item_key": "src/lib.rs",
    "document_id": "doc_...",
    "content_kind": "code",
    "path": "src/lib.rs"
  },
  "retry": null,
  "warning": null,
  "error": null
}
```

## Watch Routes

`POST /v1/watches` creates or ensures a recurring source freshness lifecycle.

Request:

```json
{
  "source": "file:///home/jmagar/workspace/axon",
  "scope": "repo",
  "schedule": {
    "kind": "filesystem",
    "debounce_ms": 1000
  },
  "embed": true,
  "collection": "axon",
  "options": {}
}
```

Response:

```json
{
  "ok": true,
  "data": {
    "watch_id": "watch_...",
    "source_id": "src_...",
    "canonical_uri": "file:///home/jmagar/workspace/axon",
    "adapter": "local",
    "scope": "repo",
    "enabled": true,
    "schedule": {"kind": "filesystem", "debounce_ms": 1000},
    "job": {
      "kind": "watch_exec",
      "id": "uuid",
      "status_url": "/v1/jobs/uuid",
      "events_url": "/v1/jobs/uuid/events",
      "stream_url": "/v1/jobs/uuid/stream",
      "poll_after_ms": 1000
    }
  },
  "warnings": [],
  "request_id": "req_...",
  "job": null
}
```

`POST /v1/watches/{watch_id}/exec` runs the watch immediately and returns a
`job_id`.

Watch routes must share source resolution, ledger, graph, document preparation,
embedding, and pruning behavior with plain source runs.

## Graph Routes

Graph routes expose `SourceGraph`; they do not write arbitrary caller-provided
edges by default. Normal graph writes come from trusted source jobs and parser
outputs.

`POST /v1/graph/query` supports typed relationship queries:

```json
{
  "start": {
    "kind": "repo",
    "canonical_uri": "https://github.com/jmagar/axon"
  },
  "edges": ["repo_declares_dependency", "repo_declares_service"],
  "direction": "out",
  "depth": 1,
  "limit": 100
}
```

`POST /v1/graph/resolve` maps URI/package/repo/path/session identifiers to graph
nodes and includes authority/confidence/evidence.

`GET /v1/graph/sources/{source_id}` returns graph nodes and edges directly tied
to a ledger source.

Graph query response:

```json
{
  "ok": true,
  "data": {
    "nodes": [
      {
        "node_id": "node_...",
        "kind": "package",
        "canonical_uri": "cargo:tokio",
        "display_name": "tokio",
        "authority": "inferred",
        "confidence": 0.99,
        "metadata": {}
      }
    ],
    "edges": [
      {
        "edge_id": "edge_...",
        "kind": "repo_declares_dependency",
        "from_node_id": "node_repo",
        "to_node_id": "node_pkg",
        "authority": "inferred",
        "confidence": 0.99,
        "evidence": []
      }
    ],
    "next_cursor": null
  },
  "warnings": [],
  "request_id": "req_...",
  "job": null
}
```

## Retrieval Routes

`POST /v1/query` returns ranked chunks/documents from VectorStore, optionally
enriched by graph filters.

`POST /v1/retrieve` fetches stored documents/chunks for a source, URL, graph
node, or document id.

`POST /v1/ask` and `POST /v1/ask/stream` execute retrieval plus synthesis. They
use `LlmProvider`, not transport-specific LLM code.

Common retrieval filters:

```json
{
  "query": "what packages does this repo use?",
  "source_id": "src_...",
  "graph_node_id": "node_...",
  "source_kind": "git",
  "generation": "committed",
  "limit": 20,
  "include_graph": true
}
```

Query response:

```json
{
  "ok": true,
  "data": {
    "results": [
      {
        "document_id": "doc_...",
        "chunk_id": "chk_...",
        "source_id": "src_...",
        "canonical_uri": "https://github.com/jmagar/axon",
        "score": 0.87,
        "content": "matched chunk text",
        "metadata": {},
        "graph": {
          "node_ids": ["node_..."],
          "edge_ids": []
        }
      }
    ],
    "graph": {
      "nodes": [],
      "edges": []
    }
  },
  "warnings": [],
  "request_id": "req_...",
  "job": null
}
```

## Extraction Routes

`POST /v1/extract` is explicit structured LLM extraction.

It is not an indexing category and does not replace source acquisition. It may
optionally persist artifacts or create graph evidence when called from a trusted
source job, but ad hoc REST extraction should default to returning structured
results without mutating SourceGraph.

Request:

```json
{
  "source": "https://example.com/pricing",
  "schema": {
    "type": "object",
    "properties": {
      "plans": {"type": "array"}
    }
  },
  "persist_artifact": true
}
```

## Memory Routes

Memory is a first-class durable knowledge surface, not a generic action
envelope. Memory content is embedded for semantic recall and mirrored into
SourceGraph for relationships.

Memory node types:

- `decision`
- `fact`
- `preference`
- `task`
- `bug`
- `procedure`
- `incident`
- `entity`
- `episode`
- `working`

Memory statuses:

- `active`
- `review`
- `superseded`
- `archived`
- `forgotten`

Memory decay modes:

- `none`
- `time`
- `access`
- `confidence`
- `supersession`
- `custom`

Memory link types:

- `relates_to`
- `supersedes`
- `contradicts`
- `about_source`
- `about_graph_node`
- `about_file`
- `about_issue`
- `about_pr`

Memory scoring fields:

- `confidence`: how likely the memory is true.
- `salience`: how important/useful the memory is.
- `decay_score`: current decay-adjusted score.
- `recency_score`: recent-use boost separate from truth.
- `pinned`: bypasses decay or enforces a minimum score.
- `review_required`: excludes memory from high-confidence recall unless
  explicitly requested.

Memory scope fields:

- `global`
- `project`
- `repo`
- `file`
- `source_id`
- `graph_node_id`
- `agent`
- `user`
- `environment`

`POST /v1/memories` stores one memory.

Request:

```json
{
  "type": "decision",
  "title": "Use SourceGraph for source relationships",
  "body": "SourceGraph owns typed edges and evidence. SourceLedger owns lifecycle.",
  "project": "axon",
  "repo": "jmagar/axon",
  "file": "docs/pipeline-unification/source-graph.md",
  "confidence": 0.95,
  "salience": 0.8,
  "scope": {
    "kind": "repo",
    "repo": "jmagar/axon"
  },
  "decay": {
    "mode": "time",
    "half_life_days": 180,
    "min_score": 0.2,
    "reinforce_on_context": true
  },
  "tags": ["pipeline-unification"],
  "links": [
    {
      "type": "about_source",
      "source_id": "src_..."
    }
  ]
}
```

Response:

```json
{
  "ok": true,
  "data": {
    "memory_id": "mem_...",
    "graph_node_id": "node_...",
    "document_id": "doc_...",
    "vector_point_ids": ["uuid"],
    "status": "active",
    "memory_score": 0.95,
    "confidence": 0.95,
    "salience": 0.8,
    "pinned": false,
    "review_required": false,
    "decay": {
      "mode": "time",
      "half_life_days": 180,
      "last_reinforced_at": null,
      "next_decay_at": "2026-07-01T00:00:00Z"
    },
    "created_at": "2026-06-30T16:20:00Z"
  },
  "warnings": [],
  "request_id": "req_...",
  "job": null
}
```

`GET /v1/memories` lists metadata only. It supports `project`, `repo`, `file`,
`type`, `status`, `tag`, `limit`, and `cursor`.

`GET /v1/memories/{memory_id}` hydrates the memory body plus graph links,
supersession status, and source/vector references.

`POST /v1/memories/search` performs semantic search over active memories by
default.

Request:

```json
{
  "query": "where did we decide graph storage should live?",
  "project": "axon",
  "repo": "jmagar/axon",
  "file": null,
  "type": null,
  "status": "active",
  "limit": 10,
  "include_graph": true
}
```

`POST /v1/memories/context` builds a bounded, defanged context block for agent
startup or task recall. It accepts the same filters as search plus `depth` and
`token_budget`.

Context packing must:

- combine vector score, graph-neighborhood relevance, confidence, salience,
  recency, decay, and pinning
- cluster related memories
- dedupe superseded or near-duplicate memories
- surface contradictions instead of silently choosing a winner
- defang prompt-injection-like content
- respect `token_budget`
- exclude `review`, `superseded`, `archived`, and `forgotten` memories unless
  explicitly requested

`POST /v1/memories/{memory_id}/links` creates or refreshes an idempotent memory
edge. The target may be another memory, source, graph node, issue, PR, artifact,
or file reference.

Request:

```json
{
  "type": "relates_to",
  "target": {
    "kind": "memory",
    "memory_id": "mem_other"
  },
  "confidence": 0.9
}
```

`POST /v1/memories/{memory_id}/supersede` marks the target memory superseded and
creates a `supersedes` edge from the replacement memory to the old memory.

Request:

```json
{
  "old_memory_id": "mem_old",
  "reason": "Replaced by the pipeline-unification contract."
}
```

`POST /v1/memories/{memory_id}/contradict` creates a `contradicts` edge and
marks both memories for review unless the caller explicitly resolves the
conflict.

Request:

```json
{
  "other_memory_id": "mem_other",
  "reason": "Newer implementation contract disagrees with old note.",
  "resolution": "needs_review"
}
```

`POST /v1/memories/{memory_id}/pin` sets `pinned=true` or a minimum recall score.
Pinned memories still require status/confidence checks; pinning is not a bypass
for redaction or review-required safety.

Request:

```json
{
  "pinned": true,
  "min_score": 0.9,
  "reason": "Repo convention should always be recalled."
}
```

`GET /v1/memories/review` lists memories that need attention because they are
low-confidence, contradicted, inferred-but-unconfirmed, decayed below threshold,
or repeatedly retrieved but ignored.

`POST /v1/memories/compact` distills related memories into a new durable memory.
The source memories are linked to the compacted memory and may remain active,
be archived, or be superseded depending on request policy.

Request:

```json
{
  "memory_ids": ["mem_a", "mem_b", "mem_c"],
  "strategy": "semantic_summary",
  "result_type": "procedure",
  "archive_sources": false
}
```

`DELETE /v1/memories/{memory_id}` forgets a memory. Forgetting is distinct from
archive and supersede: it removes the memory from search/context and writes a
tombstone/audit record so future imports or graph sync do not resurrect it
silently.

`POST /v1/memories/{memory_id}/reinforce` records a use signal or explicit
reinforcement. Reinforcement can raise the memory score, update recency, record
context use, or lower confidence when a memory is contradicted.

Request:

```json
{
  "signal": "context_used",
  "weight": 0.2,
  "reason": "Included in repo startup context.",
  "job_id": "job_..."
}
```

Memory writes update:

- memory metadata store
- memory vector collection
- SourceGraph memory node/edge mirror
- decay/reinforcement state
- access/provenance log
- review queue state

Memory search must exclude `superseded` and `archived` memories unless the
caller explicitly requests those statuses. Ranking and context selection must
combine vector score, graph relevance, confidence, status, recency, and decay
score instead of treating vector similarity as the only signal.

## Prune Routes

`POST /v1/prune/plan` returns a dry-run pruning plan for source ids,
generations, vector points, artifacts, graph edges, or orphaned records.

`POST /v1/prune/exec` executes a previously returned plan id or an inline plan
request. It must write a job, emit progress, and verify deletes before clearing
cleanup debt.

Prune routes require write/admin-level authorization. They must be explicit
about whether they touch LedgerStore, GraphStore, VectorStore, and ArtifactStore.

Plan request:

```json
{
  "targets": [
    {"kind": "source", "source_id": "src_..."}
  ],
  "include": {
    "ledger": true,
    "graph": true,
    "vectors": true,
    "artifacts": true
  },
  "mode": "dry_run"
}
```

Plan response:

```json
{
  "ok": true,
  "data": {
    "prune_plan_id": "prune_...",
    "summary": {
      "ledger_rows": 1200,
      "graph_nodes": 20,
      "graph_edges": 80,
      "vector_points": 5200,
      "artifacts": 3
    },
    "requires_confirmation": true,
    "expires_at": "2026-06-30T17:20:00Z"
  },
  "warnings": [],
  "request_id": "req_...",
  "job": null
}
```

## Upload Routes

Uploads are the REST-safe way to ingest caller-provided local files, session
exports, source archives, Repomix outputs, WARC files, or other prepared bundles.
Remote REST must not scan arbitrary server-local paths on behalf of the caller.

`POST /v1/uploads` creates a staged upload.

Request:

```json
{
  "filename": "codex-session.jsonl",
  "content_type": "application/jsonl",
  "size_bytes": 123456,
  "purpose": "source",
  "source_hint": {
    "kind": "session",
    "adapter": "sessions"
  }
}
```

Response:

```json
{
  "ok": true,
  "data": {
    "upload_id": "upl_...",
    "put_url": "/v1/uploads/upl_.../content",
    "expires_at": "2026-06-30T17:20:00Z"
  },
  "warnings": [],
  "request_id": "req_...",
  "job": null
}
```

`PUT /v1/uploads/{upload_id}/content` writes the bytes. Implementations may also
support multipart uploads later, but the canonical contract is a staged upload
resource, not ad hoc source-body fields.

`POST /v1/uploads/{upload_id}/complete` verifies size/hash/content type, stores
the upload as an artifact, and returns a reference usable in `POST /v1/sources`.

Completion response:

```json
{
  "ok": true,
  "data": {
    "upload_id": "upl_...",
    "artifact_id": "art_...",
    "source_ref": "upload:upl_..."
  },
  "warnings": [],
  "request_id": "req_...",
  "job": null
}
```

`POST /v1/sources` accepts `source = "upload:<upload_id>"` or an explicit
`upload_id` option when the adapter supports prepared uploads.

## Capabilities

`GET /v1/capabilities` returns:

- server version and contract version
- adapter capabilities and scopes
- provider capabilities: LLM, embedding, vector, ledger, graph, memory,
  artifact, search, fetch, render, network capture, job store, watch store,
  mobile session store, config store, credential, cache, rate limiter, security
  policy, health
- enabled auth mode and execution-affinity constraints
- limits: body size, source size, page/file caps, batch caps
- supported graph node/edge/evidence kinds
- supported parser families and manifest patterns
- supported memory types, statuses, decay modes, and link types
- supported upload purposes, max upload size, accepted content types, and
  staging retention

`GET /v1/adapters` and `GET /v1/adapters/{adapter}` provide adapter-specific
scope names, option schemas, default scope, whether watch is supported, whether
remote execution is allowed, and required credentials.

Capability response must be sufficient for CLI help, MCP help, and generated
REST clients to render supported source shapes without hardcoded adapter lists.

## Envelopes

Success:

```json
{
  "ok": true,
  "data": {},
  "warnings": [],
  "request_id": "req_...",
  "job": null
}
```

For job-starting operations, the top-level `job` field duplicates the poll
descriptor inside `data` so generic clients can find it without understanding
the operation-specific payload:

```json
{
  "ok": true,
  "data": {},
  "warnings": [],
  "request_id": "req_...",
  "job": {
    "kind": "source",
    "id": "uuid",
    "status_url": "/v1/jobs/uuid",
    "events_url": "/v1/jobs/uuid/events",
    "stream_url": "/v1/jobs/uuid/stream",
    "poll_after_ms": 5000
  }
}
```

Error:

```json
{
  "ok": false,
  "error": {
    "code": "source.resolve.ambiguous",
    "message": "Could not choose a source adapter for target",
    "stage": "resolving",
    "retryable": false,
    "severity": "error",
    "details": {}
  },
  "request_id": "req_..."
}
```

HTTP status codes map to broad categories, but clients should rely on
`error.code` for programmatic behavior.

## Complete Route Schemas

Every route below uses the success/error envelopes above unless explicitly
called out as raw streaming/content. `Request` lists path, query, header, or
body inputs. `Response data` describes the `data` field inside the envelope.
`Side effects` must be exact; read routes do not mutate state except for normal
access logs/metrics.

### System and Capability Routes

| Route | Request | Response data | Side effects |
|---|---|---|---|
| `GET /healthz` | none | plain text or `{ "status": "ok" }` | none |
| `GET /readyz` | none | `{ "status", "checks": [{ "name", "status", "message", "latency_ms" }] }` | dependency probes only |
| `GET /metrics` | none | Prometheus text exposition | metrics scrape only |
| `GET /openapi.json` | none | OpenAPI 3.x JSON for canonical routes only | none |
| `GET /docs` | none | Interactive OpenAPI UI HTML/assets | none |
| `GET /v1/server` | none | `{ "name", "version", "contract_version", "build", "auth_mode", "data_dir", "features" }` | none |
| `GET /v1/capabilities` | optional query `include=providers,adapters,limits,schemas` | `{ "server", "adapters", "providers", "limits", "graph", "memory", "uploads", "auth" }` | provider capability probes only |
| `GET /v1/adapters` | optional query `kind`, `watch_supported`, `credential_required` | `{ "items": [SourceAdapterCapability], "next_cursor": null }` | none |
| `GET /v1/adapters/{adapter}` | path `adapter` | `SourceAdapterCapability` plus option schema and scope schema | none |
| `GET /v1/providers` | optional query `kind`, `status` | `{ "items": [ProviderCapability], "summary": ProviderSummary }` | provider health/capability probes only |
| `GET /v1/providers/{provider}` | path `provider` | `ProviderCapability` plus latest health report, limits, cooling state | provider health/capability probe only |
| `POST /v1/preflight` | body `{ "config?", "providers?", "sources?", "strict?": bool }` | `PreflightReport` | validates config, provider reachability, credentials, paths, quotas, and safety gates without indexing |
| `POST /v1/smoke` | body `{ "live": bool, "providers"?, "include_write_checks?": bool, "source?" }` | `SmokeReport` or `JobDescriptor` | may create test provider calls, test artifacts, or a smoke job; no source is published unless explicitly requested |
| `GET /v1/status` | optional query `include=jobs,watches,providers,cleanup` | `{ "jobs", "watches", "providers", "cleanup", "degraded", "warnings" }` | none |
| `GET /v1/doctor` | optional query `deep=true` | `{ "status", "checks", "remediation", "warnings" }` | dependency probes only |
| `GET /v1/stats` | optional query `collection`, `source_id` | `{ "sources", "documents", "chunks", "vectors", "graph", "memory", "jobs", "storage" }` | none |

### Source Routes

| Route | Request | Response data | Side effects |
|---|---|---|---|
| `POST /v1/resolve` | body `{ "source", "adapter?", "scope?", "hints": { "prefer_official?", "allow_network_probe?" } }` | `{ "source", "canonical_uri", "source_kind", "adapter", "default_scope", "available_scopes", "authority", "confidence", "reason", "graph", "warnings" }` | optional bounded network probe only when requested |
| `POST /v1/sources` | body `SourceRequest`; optional `Idempotency-Key`; use `scope="map"` and `embed=false` for map-only discovery | `SourceResult` with `job`/`watch` descriptors when async | creates source/job/watch rows, may acquire, map/discover, parse, graph, embed, publish, and prune according to scope/options |
| `GET /v1/sources` | query `kind`, `adapter`, `q`, `status`, `tag`, `limit`, `cursor` | paged `{ "items": [SourceSummary], "next_cursor", "limit" }` | none |
| `GET /v1/sources/{source_id}` | path `source_id`; query `include=watch,graph,latest_job,counts` | `{ "source": SourceSummary, "watch", "graph", "latest_job", "warnings" }` | none |
| `PATCH /v1/sources/{source_id}` | body `{ "user_label?", "tags?", "authority_pin?", "stored_options?" }` | updated `SourceSummary` | updates ledger source metadata only |
| `POST /v1/sources/{source_id}/refresh` | body `{ "refresh": "force"|"if_stale", "execution"?, "options"?, "collection"? }` | `SourceResult` | creates refresh job from stored config snapshot plus explicit overrides |
| `POST /v1/sources/{source_id}/resolve` | body `{ "hints"?: ResolveHints }` | `ResolvedSource` | may update authority/cache only when explicitly requested by options |
| `DELETE /v1/sources/{source_id}` | body `{ "mode": "prune"|"forget", "dry_run": bool, "include": PruneInclude }` | `PrunePlan` or `JobDescriptor` | creates prune debt/job; destructive deletes require prune execution |
| `GET /v1/sources/{source_id}/items` | query `status`, `generation`, `q`, `limit`, `cursor` | paged source item rows with hash/status/document/graph refs | none |
| `GET /v1/sources/{source_id}/items/{item_key}` | path `source_id`, encoded `item_key` | source item detail with manifest fields, statuses, errors, document ids, graph refs | none |
| `GET /v1/sources/{source_id}/documents` | query `status`, `generation`, `content_kind`, `limit`, `cursor` | paged document summaries with chunk/vector counts and graph refs | none |
| `GET /v1/sources/{source_id}/generations` | query `status`, `limit`, `cursor` | paged generation summaries with publish/cleanup status | none |
| `GET /v1/sources/{source_id}/generations/{generation}` | path `source_id`, `generation` | generation detail with item/document/chunk counts, publish state, cleanup debt | none |
| `GET /v1/sources/{source_id}/graph` | query `depth`, `edge_kind`, `limit`, `cursor` | `{ "nodes", "edges", "evidence", "next_cursor" }` | none |
| `GET /v1/sources/{source_id}/artifacts` | query `kind`, `job_id`, `limit`, `cursor` | paged artifact metadata | none |
| `GET /v1/domains` | query `domain`, `source_kind`, `limit`, `cursor` | paged domain/source-host summaries with counts and latest refresh state | none |

### Document Routes

| Route | Request | Response data | Side effects |
|---|---|---|---|
| `GET /v1/documents/{document_id}` | path `document_id`; query `include=chunks,graph,source` | document detail with source item, generation, metadata, chunk summary, vector keys | none |
| `GET /v1/documents/{document_id}/chunks` | query `include_content`, `limit`, `cursor` | paged chunk metadata and optionally redacted content | none |
| `GET /v1/documents/{document_id}/chunks/{chunk_id}` | path `document_id`, `chunk_id`; query `include_content=true` | one chunk with locator, content hash, payload metadata, vector refs, graph refs | none |

### Job Routes

| Route | Request | Response data | Side effects |
|---|---|---|---|
| `GET /v1/jobs` | query `status`, `kind`, `source_id`, `watch_id`, `limit`, `cursor` | paged job summaries | none |
| `GET /v1/jobs/{job_id}` | path `job_id` | latest `SourceJobStatus`, heartbeat, counts, current item, warnings, last error, poll hint | none |
| `GET /v1/jobs/{job_id}/events` | query `after_sequence`, `limit`, `severity`, `visibility` | paged `SourceProgressEvent` values | none |
| `GET /v1/jobs/{job_id}/stream` | query `after_sequence`, `heartbeat_ms` | SSE stream of `SourceProgressEvent` | opens stream only |
| `GET /v1/jobs/{job_id}/artifacts` | query `kind`, `limit`, `cursor` | paged artifacts produced by the job | none |
| `POST /v1/jobs/recover` | body `{ "kind"?, "stale_before"?, "limit"? }` | recovery summary and recovered job ids | updates stale/interrupted jobs to recoverable state |
| `POST /v1/jobs/cleanup` | body `{ "kind"?, "older_than"?, "status"?, "dry_run": bool }` | cleanup summary | deletes terminal job/event rows only when `dry_run=false` |
| `DELETE /v1/jobs` | body `{ "kind"?, "status": ["completed","failed","canceled"], "older_than"?, "confirm": true }` | clear summary | deletes matching terminal job/event rows |
| `POST /v1/jobs/{job_id}/cancel` | body `{ "reason"?, "force_after_ms"? }` | updated job status | records cancellation request; runner observes it |
| `POST /v1/jobs/{job_id}/retry` | body `{ "mode": "same_config"|"with_overrides", "overrides"? }` | new `JobDescriptor` and linked retry metadata | creates retry job from submitted config snapshot |

### Watch Routes

| Route | Request | Response data | Side effects |
|---|---|---|---|
| `POST /v1/watches` | body `{ "source", "scope?", "schedule", "embed", "collection?", "options?" }` | watch detail plus optional initial `JobDescriptor` | creates or updates a watch and may enqueue initial run |
| `GET /v1/watches` | query `enabled`, `source_id`, `adapter`, `limit`, `cursor` | paged watch summaries | none |
| `GET /v1/watches/{watch_id}` | path `watch_id`; query `include=latest_job,history` | watch detail, schedule, heartbeat, latest job, warnings | none |
| `PATCH /v1/watches/{watch_id}` | body `{ "enabled?", "schedule?", "options?", "embed?", "collection?" }` | updated watch detail | updates watch config and next-run calculation |
| `POST /v1/watches/{watch_id}/exec` | body `{ "reason"?, "refresh"?, "wait"? }` | `JobDescriptor` / `SourceResult` when waited | creates immediate watch execution job |
| `POST /v1/watches/{watch_id}/pause` | body `{ "reason"? }` | updated watch detail | disables scheduler execution |
| `POST /v1/watches/{watch_id}/resume` | body `{ "run_now": bool }` | updated watch detail plus optional `JobDescriptor` | enables scheduler execution and maybe enqueues run |
| `DELETE /v1/watches/{watch_id}` | body `{ "delete_history": bool }` | `{ "watch_id", "deleted": true }` | deletes watch config; history retained unless requested |
| `GET /v1/watches/{watch_id}/history` | query `limit`, `cursor`, `status` | paged linked job summaries | none |

### Graph Routes

| Route | Request | Response data | Side effects |
|---|---|---|---|
| `GET /v1/graph/kinds` | none | `{ "node_kinds", "edge_kinds", "evidence_kinds", "authority_levels" }` | none |
| `POST /v1/graph/resolve` | body `{ "identifiers": [GraphIdentifier], "include_edges": bool }` | resolved graph refs plus misses/confidence/evidence | none |
| `POST /v1/graph/query` | body `{ "start", "edges"?, "direction", "depth", "filters"?, "limit", "cursor"? }` | `{ "nodes", "edges", "evidence", "next_cursor" }` | none |
| `GET /v1/graph/nodes/{node_id}` | path `node_id`; query `include=evidence,edges` | graph node detail | none |
| `GET /v1/graph/nodes/{node_id}/edges` | query `direction`, `edge_kind`, `limit`, `cursor` | paged graph edges plus neighbor node refs | none |
| `GET /v1/graph/edges/{edge_id}` | path `edge_id`; query `include=evidence` | graph edge detail and evidence | none |
| `GET /v1/graph/sources/{source_id}` | query `depth`, `limit`, `cursor` | source-linked subgraph | none |

### Retrieval and Extraction Routes

| Route | Request | Response data | Side effects |
|---|---|---|---|
| `POST /v1/query` | body `{ "query", "source_id?", "graph_node_id?", "filters"?, "committed_generation"?: string, "limit", "include_graph" }` | ranked chunk/document results plus optional graph refs; generation filters target committed published generations, never staged numeric source generations | may read VectorStore/GraphStore/DocumentCache only |
| `POST /v1/retrieve` | body `{ "source?", "source_id?", "document_id?", "url?", "chunk_id?", "include_content": bool, "limit" }` | documents/chunks with metadata, content subject to auth/redaction | may read ArtifactStore/DocumentCache only |
| `POST /v1/ask` | body `{ "question", "filters"?, "retrieval"?, "synthesis"?, "include_trace"?: bool }` | `{ "answer", "citations", "retrieval", "graph", "model", "warnings" }` | reads retrieval stores and calls `LlmProvider`; may write job/event rows if async |
| `POST /v1/ask/stream` | same as `/v1/ask` plus streaming options | SSE stream of retrieval, token, citation, and final events | reads retrieval stores and calls streaming `LlmProvider` |
| `POST /v1/chat` | body `{ "message", "system"?, "model"?, "temperature"?, "history"? }` | `{ "message", "model", "usage"?, "warnings" }` | calls `LlmProvider`; no retrieval |
| `POST /v1/chat/stream` | same as `/v1/chat` plus streaming options | SSE token/final/error events | calls streaming `LlmProvider`; no retrieval |
| `POST /v1/evaluate` | body `{ "question", "expected"?, "filters"?, "judge"?, "limit"? }` | evaluation result with answer, baseline, judge scores, citations | calls retrieval and `LlmProvider` judge |
| `POST /v1/suggest` | body `{ "focus"?, "source_id"?, "limit"?, "constraints"? }` | suggested sources with reasons/confidence | may call search/retrieval/LLM; no source mutation unless requested elsewhere |
| `POST /v1/search` | body `{ "query", "limit"?, "offset"?, "time_range"?, "auto_source"?: bool }` | web search results and optional queued source jobs | may call search provider; may enqueue source jobs when `auto_source=true` |
| `POST /v1/research` | body `{ "query", "limit"?, "depth"?, "full_content"?, "auto_source"?: bool }` | synthesized research answer, sources, citations, optional jobs | calls search/fetch/LLM; may enqueue source jobs when requested |
| `POST /v1/research/stream` | same as `/v1/research` plus stream options | SSE search/source/token/final/error events | calls search/fetch/streaming LLM; may enqueue source jobs when requested |
| `POST /v1/summarize` | body `{ "source"|"url"|"urls", "instructions"?, "format"?, "headers"? }` | summary, source metadata, citations/artifact refs | fetches/scrapes source and calls `LlmProvider`; no indexing unless requested through source pipeline |
| `POST /v1/summarize/stream` | same as `/v1/summarize` plus stream options | SSE fetch/token/final/error events | fetches/scrapes source and calls streaming `LlmProvider` |
| `POST /v1/map` | body `{ "source", "scope": "map", "limit"?, "offset"?, "options"? }` | discovered source items/URLs with pagination and source hints | calls `SourceRequest` with `scope=map` and `embed=false`; may fetch maps/sitemaps/indexes but must not embed |
| `POST /v1/endpoints` | body `{ "source"|"url", "render_mode"?, "capture"?, "limit"? }` | discovered endpoint bundle/report | may fetch/render with Chrome and capture network |
| `POST /v1/brand` | body `{ "source"|"url", "render_mode"?, "include_screenshot"?: bool }` | brand colors/fonts/assets/favicon/logo refs | may fetch/render and write artifacts |
| `POST /v1/diff` | body `{ "source_a"|"url_a", "source_b"|"url_b", "mode"?, "headers"? }` | diff result with changed fields/chunks/artifacts | fetches/scrapes both sources; no indexing |
| `POST /v1/screenshot` | body `{ "source"|"url", "viewport"?, "full_page"?, "render_mode"?, "wait_for"? }` | screenshot artifact metadata | renders page and writes screenshot artifact |
| `POST /v1/extract` | body `{ "source", "schema", "instructions"?, "persist_artifact": bool, "trusted_graph_write": false }` | `{ "result", "schema", "artifact_id?", "graph_candidates?", "warnings" }` | calls `LlmProvider`; writes artifact only when requested; graph writes only from trusted source jobs |

### Memory Routes

| Route | Request | Response data | Side effects |
|---|---|---|---|
| `POST /v1/memories` | body `MemoryRequest` | `MemoryResult` with memory id, graph node, document/vector refs, score | writes MemoryStore, VectorStore, GraphStore, provenance log |
| `GET /v1/memories` | query `project`, `repo`, `file`, `type`, `status`, `tag`, `limit`, `cursor` | paged memory metadata only | none |
| `POST /v1/memories/search` | body `{ "query", "filters"?, "limit", "include_graph": bool, "include_archived": bool }` | ranked memories with scores and graph refs | records access/retrieval signal only if caller requests reinforcement |
| `POST /v1/memories/context` | body `{ "query"?, "source_id?", "graph_node_id?", "filters"?, "token_budget", "depth" }` | defanged context block plus selected memories and exclusions | may record context-use signals when requested |
| `GET /v1/memories/review` | query `reason`, `project`, `repo`, `limit`, `cursor` | paged memories needing review | none |
| `POST /v1/memories/compact` | body `{ "memory_ids", "strategy", "result_type", "archive_sources": bool }` | new compacted `MemoryResult` plus source memory updates | writes new memory and requested source status changes |
| `POST /v1/memories/import` | body `{ "format", "bundle"|"artifact_id"|"upload_id", "mode": "merge"|"replace_scope", "dry_run": bool }` | import plan/result with created/updated/skipped counts | writes only when `dry_run=false` |
| `GET /v1/memories/export` | query `project`, `repo`, `format`, `include_archived` | artifact descriptor or streamed export | may write export artifact |
| `GET /v1/memories/{memory_id}` | path `memory_id`; query `include=body,links,graph,history` | full memory detail | none |
| `PATCH /v1/memories/{memory_id}` | body `{ "title?", "body?", "type?", "status?", "confidence?", "salience?", "scope?", "tags?", "decay?" }` | updated `MemoryResult` | updates memory metadata, vectors when body changes, graph mirror when scope/links change |
| `POST /v1/memories/{memory_id}/links` | body `{ "type", "target", "confidence", "evidence?" }` | updated memory links and graph edge refs | writes MemoryStore link and GraphStore mirror |
| `POST /v1/memories/{memory_id}/supersede` | body `{ "old_memory_id", "reason" }` | updated replacement and old memory statuses | marks old memory superseded and writes edge |
| `POST /v1/memories/{memory_id}/reinforce` | body `{ "signal", "weight", "reason?", "job_id?" }` | updated memory score/recency/decay state | writes reinforcement/access event |
| `POST /v1/memories/{memory_id}/contradict` | body `{ "other_memory_id", "reason", "resolution" }` | updated contradiction/review state | writes contradiction edge and review markers |
| `POST /v1/memories/{memory_id}/pin` | body `{ "pinned": bool, "min_score"?, "reason" }` | updated pin/score policy | updates memory ranking policy |
| `POST /v1/memories/{memory_id}/archive` | body `{ "reason"?, "archive_links"?: bool }` | updated memory status | marks archived; keeps audit/history |
| `DELETE /v1/memories/{memory_id}` | body `{ "reason" }` | `{ "memory_id", "status": "forgotten" }` | removes from recall/search and writes an audit record |

### Artifact, Upload, Prune, and Collection Routes

| Route | Request | Response data | Side effects |
|---|---|---|---|
| `GET /v1/artifacts` | query `kind`, `source_id`, `job_id`, `limit`, `cursor` | paged artifact metadata | none |
| `GET /v1/artifacts/{artifact_id}` | path `artifact_id` | artifact metadata, retention, content URL, producer refs | none |
| `GET /v1/artifacts/{artifact_id}/content` | path `artifact_id`; optional `download=true` | raw artifact bytes/content with content type | none |
| `POST /v1/uploads` | body `{ "filename", "content_type", "size_bytes", "sha256"?, "purpose", "source_hint"? }` | `{ "upload_id", "put_url", "expires_at" }` | creates staged upload row |
| `GET /v1/uploads/{upload_id}` | path `upload_id` | upload status, bytes received, expiry, artifact/source refs | none |
| `PUT /v1/uploads/{upload_id}/content` | raw body bytes; headers `Content-Type`, optional `Digest` | `{ "upload_id", "bytes_received", "sha256", "status" }` | writes staged bytes |
| `POST /v1/uploads/{upload_id}/complete` | body `{ "sha256"?, "source_options"? }` | `{ "upload_id", "artifact_id", "source_ref" }` | verifies and promotes upload to artifact/source ref |
| `DELETE /v1/uploads/{upload_id}` | body `{ "reason"? }` | `{ "upload_id", "deleted": true }` | deletes staged bytes and marks upload aborted |
| `POST /v1/prune/plan` | body `{ "targets", "include", "mode": "dry_run", "retention"?, "filters"? }` | `PrunePlan` with counts, risk flags, confirmation requirements | writes reusable prune plan artifact/row |
| `POST /v1/prune/exec` | body `{ "prune_plan_id" }` or inline plan plus `confirm=true` | `JobDescriptor` | creates prune execution job |
| `POST /v1/prune/dedupe` | body `{ "collection"?, "threshold"?, "source_id"?, "dry_run": bool }` | dedupe summary or `JobDescriptor` | scans VectorStore and deletes/marks duplicates only when not dry-run |
| `POST /v1/prune/purge` | body `{ "source_id"?, "url"?, "prefix"?, "filters"?, "dry_run": bool, "confirm"?: bool }` | purge summary, prune plan, or `JobDescriptor` | creates prune debt/job and deletes only through prune execution |
| `GET /v1/prune/jobs/{job_id}` | path `job_id` | prune job status, delete counts, verification state | none |
| `POST /v1/reset/plan` | body `{ "stores", "dry_run": true, "collection"?, "include_artifacts"?, "include_config"?, "reason"? }` | `ResetPlan` with counts, risk flags, confirmation requirements | computes destructive reset plan only |
| `POST /v1/reset/exec` | body `{ "reset_plan_id", "confirm": true, "reason"? }` | `JobDescriptor` or `ResetResult` when waited | executes selected destructive reset and writes receipt artifact |
| `GET /v1/collections` | query `include_stats=true` | collection summaries with vector modes, payload indexes, counts | may probe VectorStore |
| `GET /v1/collections/{collection}` | path `collection`; query `include=payload_indexes,segments` | collection detail, health, schema/index state, source counts | may probe VectorStore |

### Mobile Session Routes

| Route | Request | Response data | Side effects |
|---|---|---|---|
| `GET /v1/mobile/sessions` | none or query `limit`, `cursor` | mobile session summaries for authenticated owner | none |
| `GET /v1/mobile/sessions/{session_id}` | path `session_id` | mobile session detail | none |
| `PUT /v1/mobile/sessions/{session_id}` | body mobile session state with version/revision | upsert response with stored revision | writes mobile session store |
| `DELETE /v1/mobile/sessions/{session_id}` | path `session_id` | delete response | deletes mobile session state |

### Panel Routes

Panel routes are intentionally REST because the embedded web UI uses them, but
they are panel-password scoped, not general `/v1` API routes. They are excluded
from public SDK-first source clients unless the client explicitly targets the
panel control plane.

| Route | Request | Response data | Side effects |
|---|---|---|---|
| `GET /api/panel/state` | panel cookie | setup/auth/bootstrap state | none |
| `POST /api/panel/login` | body `{ "password" }` | login/session result | sets panel session cookie |
| `GET /api/panel/config` | panel cookie | config path, raw TOML, restart flag | none |
| `PUT /api/panel/config` | body `{ "raw_toml" }` | save result and restart flag | writes config TOML |
| `GET /api/panel/env` | panel cookie | env path, raw env text, restart flag | none |
| `PUT /api/panel/env` | body `{ "raw_env" }` | save result and restart flag | writes `.env` |
| `GET /api/panel/status` | panel cookie | panel status projection | none |
| `GET /api/panel/doctor` | panel cookie | panel doctor projection | dependency probes only |
| `POST /api/panel/command` | body approved command request | command result | executes allowlisted panel command |
| `GET /api/panel/ops` | panel cookie | qdrant/tei/collection/server metadata | none |
| `GET /api/panel/collections` | panel cookie | collection list | may probe VectorStore |
| `GET /api/panel/stack` | panel cookie | compose/service/runtime stack status | dependency probes only |
| `POST /api/panel/first-run/crawl` | body first-run crawl request | job/result | creates first-run source job |
| `POST /api/panel/first-run/ask` | body first-run ask request | answer/result | calls retrieval and `LlmProvider` |
| `GET /api/panel/setup/targets` | panel cookie | setup target inventory | none |
| `GET /api/panel/artifact/{path}` | path artifact path | raw artifact bytes/content | none |

## Pagination

List routes use cursor pagination:

```json
{
  "items": [],
  "next_cursor": "opaque-or-null",
  "limit": 100
}
```

Cursors are opaque. Clients must not parse them.

## Idempotency

Mutating routes accept `Idempotency-Key`. Repeated requests with the same key and
same normalized body return the same submitted job/result. Reusing a key with a
different normalized body returns an idempotency conflict.

`POST /v1/sources` may also dedupe by canonical source plus refresh/watch mode
when the caller omits `Idempotency-Key`, but that optimization is not the
contract.

## Auth

Read routes require read scope. Mutating source/job/watch/prune routes require
write scope. Admin-level destructive routes may require an additional admin
policy depending on deployment.

Remote REST must not trigger server-local session scans or arbitrary local-path
reads unless execution affinity explicitly allows it. Remote session ingestion
uses prepared uploads, not server-local transcript discovery.

Local-path source requests over REST require one of:

- loopback/local trusted execution affinity
- a configured allowed root
- a prepared upload reference

Absent one of those, local path requests fail before resolution fetches or
filesystem probes.

## Removed Route Behavior

These routes are not part of the desired OpenAPI/client surface. There are no
aliases, no compatibility execution paths, and no public tombstone handlers.
The final HTTP router must not register these routes.

| Removed route | Canonical route |
|---|---|
| `POST /v1/actions` | no replacement; action-envelope REST is removed in favor of direct routes |
| `POST /v1/migrate` | no replacement; old collection migration is removed for the clean-slate cutover |
| `POST /v1/scrape` | `POST /v1/sources` with `scope=page` |
| `POST /v1/crawl` | `POST /v1/sources` with `scope=site` |
| `POST /v1/embed` | `POST /v1/sources` |
| `POST /v1/ingest` | `POST /v1/sources` |
| `GET /v1/{crawl,embed,extract,ingest}` | `GET /v1/jobs` with `kind` filter |
| `GET /v1/{crawl,embed,extract,ingest}/{id}` | `GET /v1/jobs/{job_id}` |
| `POST /v1/{crawl,embed,extract,ingest}/{id}/cancel` | `POST /v1/jobs/{job_id}/cancel` |
| `POST /v1/{crawl,embed,extract,ingest}/cleanup` | `POST /v1/jobs/cleanup` |
| `POST /v1/{crawl,embed,extract,ingest}/recover` | `POST /v1/jobs/recover` |
| `DELETE /v1/{crawl,embed,extract,ingest}` | `DELETE /v1/jobs` with `kind` filter |
| `POST /v1/ingest/sessions/prepared` | `POST /v1/uploads` then `POST /v1/sources` with `upload:<upload_id>` |
| `POST /v1/watch` | `POST /v1/watches` |
| `POST /v1/watch/{id}/run` | `POST /v1/watches/{watch_id}/exec` |
| `POST /v1/memory` | `/v1/memories/*` routes by memory operation. **Note (C6-25, 2026-07-09 audit):** listed here as "removed" in the target route mapping, but the deprecated `POST /v1/memory` passthrough is intentionally still live in code pending client migration — see the follow-up plan tracked as P1-04. This is not a contradiction once that context is read; do not remove the passthrough without checking P1-04's status first. |
| `POST /v1/purge` | `POST /v1/prune/purge` |
| `POST /v1/dedupe` | `POST /v1/prune/dedupe` |
| `GET /v1/artifacts?path=...` | `GET /v1/artifacts/{artifact_id}/content` after artifact lookup |
| `GET /v1/artifacts/{path}` | `GET /v1/artifacts/{artifact_id}/content` after artifact lookup |

Unknown old routes return normal `404 Not Found`.

## OpenAPI

The generated OpenAPI document must describe the canonical routes, shared
envelopes, request ids, pagination, job descriptors, progress events, error
codes, and canonical schemas.

Client generation should target the canonical source-pipeline routes. Removed
routes must be excluded from generated clients.

Generated artifacts:

```text
docs/reference/rest/openapi.json
docs/reference/rest/openapi.md
apps/web/openapi/axon.json
apps/android/app/src/main/assets/openapi/axon.json
```

The `docs/reference/rest/*` and committed Android asset paths above are target
artifacts. The current implementation generates and checks
`apps/web/openapi/axon.json`, TypeScript clients for `apps/web` and
`apps/palette-tauri`, and Android route-contract generated files derived from
the web OpenAPI spec. The clean-break generator must converge those current
outputs into the canonical artifact set above.

The `apps/web` and Android artifacts are generated copies of the canonical
OpenAPI output, not separate hand-maintained route contracts. Drift checks fail
when any generated artifact includes a removed route or omits an end-state route
from this contract.
