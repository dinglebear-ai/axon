# Observability Contract
Last Modified: 2026-06-30

## Contract

`axon-observe` owns the shared observability model: progress events, heartbeat
helpers, tracing span construction, metric instruments, structured log fields,
and event sinks. `axon-jobs` persists job events; transports render
observability state; domain crates emit through `axon-observe` rather than
inventing local progress shapes.

This is the target observability contract. Current observability is job-row and
progress-JSON based, with narrower event semantics.

Every source job, watch run, retrieval job, extraction job, prune job, and
provider operation emits one coherent observable shape across CLI, MCP, REST,
logs, traces, metrics, SourceLedger, SourceGraph, VectorStore payloads, and job
rows.

One `job_id` ties together:

- logs
- progress JSON
- tracing spans
- job rows
- SourceLedger rows
- SourceGraph updates
- ArtifactStore outputs
- VectorStore payloads
- watch/status output

`run_id` is not the shared correlation field. Use `job_id`.

## Design Rules

- Every long-running operation has a `job_id`.
- Every crate emits through `axon-observe` helpers or traits, not local ad hoc
  JSON progress structs.
- Every progress event has monotonic sequence.
- Every event has phase, status, severity, visibility, timestamp, and message.
- Every stage emits start and finish events.
- Heartbeats are required for active jobs.
- Human output is rendered from the same event model as JSON/status APIs.
- Logs are structured and include `job_id`.
- Metrics use bounded labels.
- Sensitive fields are redacted before public logs/events.
- Provider cooling/degradation is visible.

## Crate Ownership

| Crate | Owns |
|---|---|
| `axon-observe` | event model, span builders, metric definitions, heartbeat helpers, redacted log fields, event sink trait |
| `axon-jobs` | durable event rows, heartbeat rows, event pagination, job stream ordering |
| `axon-api` | transport-neutral observable DTO projections |
| `axon-cli` | human/JSON rendering of events |
| `axon-web` | REST/SSE exposure of events |
| `axon-mcp` | MCP status/event response conversion |

## Event Sink Trait

`axon-observe` exposes the event sink boundary used by jobs, domain crates, and
transports. This is the required trait shape; implementations may add batching
internals but must preserve these semantics.

```rust
#[async_trait]
pub trait ObservabilitySink: Send + Sync {
    async fn emit(&self, event: SourceProgressEvent) -> Result<()>;
    async fn heartbeat(&self, heartbeat: JobHeartbeat) -> Result<()>;
    async fn metric(&self, metric: MetricSample) -> Result<()>;
    async fn flush(&self) -> Result<()>;
}
```

Rules:

- `emit` persists durable job events when a `job_id` is present.
- `heartbeat` updates the active heartbeat row and may also emit an internal
  heartbeat event when configured.
- `metric` never stores unbounded labels.
- `flush` is required before process shutdown and after terminal job events.
- test fakes must record every call for ordering assertions.

## Current Implementation Snapshot

Refreshed 2026-07-10 against HEAD `5a4558cc7`:

Implemented today:

- Jobs have SQLite rows with lifecycle status and `progress_json`.
- Current statuses include `pending`, `running`, `completed`, `failed`, and
  `canceled`.
- Generic progress exists for some family jobs such as embed, extract, and
  ingest; crawl remains more specialized.
- Code-search reindexing has its own progress events such as started, batch
  finished, cleanup started, commit started, and finished.
- Heartbeat/worker freshness is implemented today by refreshing job row
  `updated_at`, lifecycle updates, worker heartbeat guards, and a starvation
  watchdog. It is not a durable heartbeat event stream/table yet.
- `SourceProgressEvent` (`axon-api::source`, emitted via `axon-observe`'s
  `EventCollector::emit`) is implemented and real, not target-only.
- `GET /v1/jobs/{id}/events` and `GET /v1/jobs/{id}/stream` both exist
  (`crates/axon-web/src/server/handlers/jobs.rs`), backed by
  `services::jobs::unified_job_events`.

Planned by this contract:

- Wire `SourceProgressEvent` emission into every long-running operation
  uniformly (today's coverage matches the family-specific progress state
  above, not yet a single event model across crawl/embed/ingest/code-search).
- `job_id` is propagated into ledger rows, graph updates, artifacts, vector
  payloads, traces, logs, and watch/status output.

## Observable Identifiers

| Field | Required | Meaning |
|---|---:|---|
| `job_id` | yes | Durable operation id. |
| `request_id` | yes at transport boundary | One CLI/MCP/REST invocation. |
| `trace_id` | yes when tracing enabled | Distributed trace id. |
| `span_id` | yes when tracing enabled | Current span id. |
| `source_id` | source jobs | Stable source id. |
| `watch_id` | watch jobs | Durable watch id. |
| `generation` | mutable sources | Source generation. |
| `document_id` | document events | Document id. |
| `source_item_key` | item events | Source item key. |
| `provider_id` | provider events | Provider instance/model/store id. |

## Phase Registry

| Phase | Applies To | Meaning |
|---|---|---|
| `queued` | all async jobs | job accepted, not running |
| `requested` | transport boundaries | caller request accepted before planning |
| `resolving` | source/watch/map | source identity and adapter resolution |
| `routing` | source/watch/map | adapter/scope/provider selection |
| `authorizing` | all protected ops | auth, credentials, execution policy |
| `planning` | source/prune/research | execution plan built |
| `leasing` | source/watch/jobs | lease acquisition |
| `discovering` | source/map/watch | manifest/item discovery |
| `diffing` | mutable sources | manifest diff |
| `fetching` | source/research/summarize | network/local/package fetch |
| `rendering` | web/screenshot/brand/endpoints | browser/render path |
| `enriching` | source/research/memory | optional LLM/metadata/source enrichment |
| `normalizing` | source | SourceDocument construction |
| `parsing` | source/extract | parser facts and graph candidates |
| `graphing` | source/memory/sessions | graph writes |
| `preparing` | source | chunking/preparation |
| `batching` | source/memory/query | batching provider inputs |
| `embedding` | source/memory | embedding batches |
| `vectorizing` | source/memory | vector point construction before write |
| `upserting` | source/memory | vector writes |
| `retrieving` | query/retrieve/ask | vector/document retrieval |
| `synthesizing` | ask/research/summarize/chat | LLM generation |
| `evaluating` | evaluate | judge/baseline evaluation |
| `publishing` | source | generation publish |
| `cleaning` | source/prune | cleanup debt execution |
| `complete` | all | terminal success |
| `canceled` | all | terminal cancellation |

This table is a projection of `PipelinePhase` in
`foundation/types/enum-contract.md`. `degraded` and `failed` are lifecycle
statuses/severities, not phases.

## Status Values

| Status | Terminal | Meaning |
|---|---:|---|
| `queued` | no | waiting to start |
| `pending` | no | accepted by a current/legacy projection but not yet queued |
| `running` | no | active work |
| `waiting` | no | waiting on provider/rate limit/cooldown |
| `blocked` | no | waiting on dependency, capacity, approval, or policy |
| `canceling` | no | cancellation requested and stages unwinding |
| `completed` | yes | completed required work |
| `completed_degraded` | yes | completed with missing optional capability |
| `failed` | yes | did not complete required work |
| `canceled` | yes | canceled by caller/system |
| `expired` | yes | exceeded retention/deadline without safe recovery |
| `skipped` | yes | skipped by policy or unchanged input |

Status values are projections of the canonical `LifecycleStatus` enum. Event
`phase` values are projections of the canonical `PipelinePhase` enum. Do not
invent local spellings such as `cancelled`, `complete`, or `acquiring`.

## Severity and Visibility

Severity:

| Severity | Meaning |
|---|---|
| `debug` | diagnostic only |
| `info` | normal progress |
| `warning` | non-fatal issue |
| `degraded` | functionality reduced |
| `error` | operation item failed |
| `fatal` | operation failed |

Visibility:

| Visibility | Surfaces |
|---|---|
| `public` | CLI/MCP/REST status |
| `internal` | logs/traces/admin APIs |
| `sensitive` | never emitted directly; redacted/hashed |

## SourceProgressEvent

Every progress surface uses this shape.

```json
{
  "event_id": "evt_...",
  "sequence": 42,
  "job_id": "job_...",
  "source_id": "src_...",
  "canonical_uri": "github://jmagar/axon",
  "adapter": "github",
  "scope": "repo",
  "generation": 12,
  "phase": "embedding",
  "status": "running",
  "severity": "info",
  "visibility": "public",
  "message": "embedding changed files",
  "timestamp": "2026-06-30T20:20:00Z",
  "counts": {
    "items_total": 1200,
    "items_done": 431,
    "items_failed": 0,
    "documents_total": 431,
    "documents_done": 431,
    "chunks_total": 5200,
    "chunks_done": 1800,
    "bytes_total": 1234567,
    "bytes_done": 456789
  },
  "timing": {
    "phase_elapsed_ms": 12500,
    "job_elapsed_ms": 45000,
    "eta_ms": 90000
  },
  "throughput": {
    "items_per_sec": 4.8,
    "chunks_per_sec": 31.2,
    "bytes_per_sec": 150000
  },
  "current": {
    "source_item_key": "src/lib.rs",
    "document_id": "doc_...",
    "chunk_id": "chk_..."
  },
  "retry": null,
  "warning": null,
  "error": null
}
```

Required fields:

| Field | Required | Meaning |
|---|---:|---|
| `event_id` | yes | Stable event id. |
| `sequence` | yes | Monotonic sequence per job. |
| `job_id` | yes | Correlation id. |
| `phase` | yes | Phase registry value. |
| `status` | yes | Status value. |
| `severity` | yes | Severity. |
| `visibility` | yes | Public/internal/sensitive classification. |
| `message` | yes | Redacted human message. |
| `timestamp` | yes | Event time. |

## Heartbeats

Active jobs emit heartbeat events.

Rules:

- heartbeat interval defaults to 5 seconds for foreground jobs
- heartbeat interval defaults to 15 seconds for background jobs
- heartbeat includes current phase, status, counts, and last progress time
- missing heartbeat after configured lease timeout makes the job recoverable
- provider waits/cooling still emit heartbeats with `status=waiting`

Heartbeat surfaces:

- `axon jobs get <job_id>`
- `axon jobs events <job_id>`
- REST `GET /v1/jobs/{job_id}`
- REST `GET /v1/jobs/{job_id}/events`
- MCP `action=jobs subaction=get/events`
- watch status

## Logs

Logs are structured.

Required fields:

| Field | Meaning |
|---|---|
| `timestamp` | log time |
| `level` | trace/debug/info/warn/error |
| `target` | crate/module/component |
| `message` | redacted message |
| `job_id` | job id when available |
| `request_id` | request id when available |
| `source_id` | source id when available |
| `phase` | pipeline phase when available |
| `provider_id` | provider id when available |
| `error_code` | structured error code when logging errors |

Forbidden in logs:

- raw auth headers
- tokens/API keys/cookies
- raw env values
- private prompts/responses unless retained as redacted artifacts
- unredacted local absolute paths in public logs

## Tracing

Trace spans should mirror pipeline stages.

Span naming:

```text
source.resolve
source.route
source.discover
source.diff
source.fetch
source.normalize
source.parse
source.graph
source.prepare
source.embed
source.vector.upsert
source.publish
source.cleanup
```

Span attributes:

- `job_id`
- `source_id`
- `adapter`
- `scope`
- `phase`
- `provider_id`
- bounded counts
- error code/severity

Never attach full content, secrets, raw prompts, or raw tool output as span
attributes.

## Metrics

Metric labels must be bounded. Do not use raw URL, path, query, document id, or
chunk id as metric labels.

Counters:

| Metric | Labels | Meaning |
|---|---|---|
| `axon_jobs_started_total` | `kind`, `adapter`, `scope` | jobs started |
| `axon_jobs_completed_total` | `kind`, `status` | terminal jobs |
| `axon_source_items_total` | `adapter`, `scope`, `status` | items processed |
| `axon_documents_prepared_total` | `content_kind`, `profile`, `status` | documents prepared |
| `axon_chunks_prepared_total` | `content_kind`, `profile` | chunks emitted |
| `axon_embeddings_total` | `provider`, `model`, `status` | embedding requests/items |
| `axon_vector_points_written_total` | `collection`, `namespace` | vector upserts |
| `axon_graph_candidates_total` | `kind`, `status` | graph candidates |
| `axon_errors_total` | `stage`, `code`, `severity` | errors |

Histograms:

| Metric | Labels | Meaning |
|---|---|---|
| `axon_stage_duration_seconds` | `phase`, `adapter`, `scope` | stage duration |
| `axon_provider_latency_seconds` | `provider`, `operation` | provider latency |
| `axon_embedding_batch_size` | `provider`, `model` | embedding batch size |
| `axon_chunk_tokens` | `content_kind`, `profile` | chunk token estimate |
| `axon_job_duration_seconds` | `kind`, `status` | full job duration |

Gauges:

| Metric | Labels | Meaning |
|---|---|---|
| `axon_jobs_active` | `kind`, `phase` | active jobs |
| `axon_provider_cooling` | `provider` | provider cooling state |
| `axon_cleanup_debt_open` | `kind` | open cleanup debt |
| `axon_watch_due` | `status` | due watches |

## Status Surfaces

All status surfaces render from the same job/event/status store.

| Surface | Required Data |
|---|---|
| CLI foreground | phase, counts, current item, warnings, final result |
| CLI `jobs get` | job descriptor, status, phase, heartbeat, counts |
| CLI `jobs events` | paged event stream |
| MCP job response | job descriptor and poll request |
| REST status | same `SourceStatus` DTO |
| watch status | watch config, current run, heartbeat, next run |
| Qdrant/vector payload | `job_id`, `source_id`, generation for correlation |

## Degraded Provider Reporting

Provider degradation must be explicit.

Fields:

| Field | Meaning |
|---|---|
| `provider_id` | provider instance/model/store |
| `provider_kind` | LLM, embedding, vector, fetch, render, ledger, graph, artifact |
| `status` | ready, degraded, cooling, unavailable, disabled |
| `required` | whether job can continue without it |
| `cooldown_until` | retry cooling timestamp |
| `last_error_code` | stable error code |
| `fallback_provider` | provider used instead |

## Artifact and Redaction Observability

When output is moved to ArtifactStore:

- progress event includes artifact id and kind
- log includes artifact id, size, hash, redaction profile
- public status includes artifact id but not secret path/content
- retrieval can reference artifact metadata

Redaction events include:

- redaction profile
- fields/classes redacted
- count of redactions
- warning if content was omitted

## Validation Checklist

Implementation is incomplete until:

- every async operation has a `job_id`
- every active job heartbeats
- every stage emits start/finish or equivalent progress
- CLI/MCP/REST status all use the same event/status data
- `--json --wait` streams valid JSON events
- logs include `job_id` when available
- metrics use bounded labels
- provider cooling is visible
- cleanup debt is visible
- redaction is observable without leaking content
- fatal errors include stable error codes
- public outputs contain no sensitive fields
