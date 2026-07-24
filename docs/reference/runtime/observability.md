# Observability

Last Modified: 2026-07-19

`axon-observe` owns progress events, heartbeat helpers, tracing span
construction, metric instruments, structured log fields, and event sinks. It
does **not** own durable job rows, scheduling, transport rendering, provider
clients, or stage business logic. DTOs (`JobProgress`/`JobHeartbeat`/`JobEvent`/
`SourceProgressEvent`) live in `axon-api`; this crate emits and populates them.

> Contract source:
> [`docs/pipeline-unification/runtime/observability-contract.md`](../../pipeline-unification/runtime/observability-contract.md).
> Implementation: [`crates/axon-observe/src/`](../../../crates/axon-observe/src/).
> Phase 8+9 landed — source progress events are real (not target-only).

## `SourceProgressEvent`

Required: `event_id`, `sequence` (monotonic per job), `job_id`, `phase`,
`status`, `severity`, `visibility`, `message` (redacted), `timestamp`.

Optional: `source_id`, `canonical_uri`, `adapter`, `scope`, `generation`,
`counts{items_total/done/failed, documents_total/done, chunks_total/done,
bytes_total/done}`, `timing{phase_elapsed_ms, job_elapsed_ms, eta_ms}`,
`throughput{items_per_sec, chunks_per_sec, bytes_per_sec}`,
`current{source_item_key, document_id, chunk_id}`, `retry`, `warning`, `error`.

## `PipelinePhase` (closed registry)

`queued`, `requested`, `resolving`, `routing`, `authorizing`, `planning`,
`leasing`, `discovering`, `diffing`, `fetching`, `rendering`, `enriching`,
`normalizing`, `parsing`, `graphing`, `preparing`, `batching`, `embedding`,
`vectorizing`, `upserting`, `retrieving`, `synthesizing`, `evaluating`,
`publishing`, `cleaning`, `complete`, `canceled`. The descriptor registry
(`phase.rs`/`PHASE_REGISTRY`) carries `label`/`meaning`/`applies_to`/
`is_terminal` per phase. Do not invent local spellings (`cancelled`,
`complete` as a non-terminal, `acquiring`).

## Severity / visibility

- **Severity:** `debug`, `info`, `warning`, `degraded`, `error`, `fatal`.
- **Visibility:** `public` (CLI/MCP/REST status), `internal` (logs/traces/admin),
  `sensitive` (never emitted directly; redacted/hashed).

## Event sink trait

```rust
trait ObservabilitySink {
    fn emit(&self, event: SourceProgressEvent);
    fn heartbeat(&self, hb: JobHeartbeat);
    fn metric(&self, sample: MetricSample);
    fn flush(&self);
}
```

Implementations: `SqliteObservabilitySink` (durable rows + own migration),
`TracingObservabilitySink` (forwards via `SpanFieldSet`),
`InMemoryObservabilitySink` (tests). Rules: `emit` persists durable job events
when `job_id` present; `metric` never stores unbounded labels; `flush` required
before shutdown and after terminal events.

Builders (`event.rs`): `stage_started`, `stage_completed`, `stage_degraded`,
`stage_failed`, `provider_waiting`. Sequence is stamped by the sink's
`SequenceRegistry` (`sequence.rs`).

## Heartbeats

Foreground 5s, background 15s. Missing heartbeat past lease timeout ⇒
recoverable. Provider waits still emit with `status=waiting`.

## Event flow (one store, many surfaces)

`axon-jobs` persists durable event rows; CLI renders (`axon jobs get` /
`axon jobs events` / `axon monitor jobs`); REST exposes
`GET /v1/jobs/{id}/events` and `GET /v1/jobs/{id}/stream`; MCP `action=jobs
subaction=events`. All render from the same store.

## Tracing + logs

`log_info`/`log_done`/`log_warn` from `axon_core::logging`. `LogFieldSet`
redacts `message` through `axon_core::redact::redact_secrets` at construction.
`SpanFieldSet` provides bounded fields (`job_id`, `source_id`, `adapter`,
`scope`, `phase`, `provider_id`, counts, error). Span names mirror stages
(`source.resolve`, `source.fetch`, `source.embed`, `source.vector.upsert`,
`source.publish`).

## Metrics (bounded labels only)

- **Counters:** `axon_jobs_started_total{kind,adapter,scope}`,
  `axon_jobs_completed_total{kind,status}`,
  `axon_source_items_total{adapter,scope,status}`,
  `axon_embeddings_total{provider,model,status}`,
  `axon_vector_points_written_total{collection,namespace}`,
  `axon_errors_total{stage,code,severity}`.
- **Histograms:** `axon_stage_duration_seconds`,
  `axon_provider_latency_seconds`, `axon_embedding_batch_size`,
  `axon_chunk_tokens`, `axon_job_duration_seconds`.
- **Gauges:** `axon_jobs_active{kind,phase}`,
  `axon_provider_cooling{provider}`, `axon_cleanup_debt_open{kind}`,
  `axon_watch_due{status}`.

## Vector redaction skips

A chunk whose payload trips
`axon_vectors::payload::VectorPayloadValidationError::ForbiddenValue` is
**skipped, not indexed**, surfaced via per-chunk `tracing::warn!` and per-batch
`SourceWarning` codes `web.vectorize.redaction_skipped_chunks` /
`source.vectorize.redaction_skipped_chunks`. Publish-write invariants must
compare publish-stage counts (`points_attempted` vs `points_written`), never
`chunks_prepared` against `points_written`.

If the event/phase surface changes, update this file and
[`events.schema.json`](events.schema.json) in the same PR.
