# axon-observe — Agent Guide

`axon-observe` owns the **unified observability contract** for pipeline jobs and
interactive operations: the observable event schema + stable event names,
progress/heartbeat emission, tracing span-field conventions, metric
names/units/labels, and redaction-aware structured log context. Transports
render observed state; they do not invent alternate status models. Full contract
(owns / API / deps / tests):
[../../../docs/pipeline-unification/crates/axon-observe/README.md](../../../docs/pipeline-unification/crates/axon-observe/README.md)
· behavior spec:
[../../../docs/pipeline-unification/runtime/observability-contract.md](../../../docs/pipeline-unification/runtime/observability-contract.md).

## Status — live crate, Phase 8+9 landed
`SourceProgressEvent`/event registry, provider reservation tracking
(`reservation.rs`), the structured log/span field registries (`log.rs`,
`span.rs`), typed progress-update helpers (`progress.rs`), and the
`PipelinePhase` descriptor registry (`phase.rs`) are all real and tested, not
markers. Do not add durable job storage, worker scheduling, or
transport-specific status rendering here.

## Module map
| File | Owns |
|---|---|
| `event.rs` | Pure `SourceProgressEvent` builders (`stage_started`/`stage_completed`/`stage_degraded`/`stage_failed`/`provider_waiting`) — terminal + start lifecycle events. `sequence` is left at the `0` sentinel; the emitting sink stamps the real value (see `sequence.rs`). |
| `progress.rs` | `ProgressUpdate` — typed builder for in-flight `status=running` progress ticks, materialized via `into_event()` onto the same base envelope `event.rs` uses |
| `phase.rs` | `PhaseDescriptor`/`PHASE_REGISTRY` — applies-to scope + human meaning for every canonical `PipelinePhase` (the enum itself is owned by `axon-api`); `label`/`meaning`/`applies_to`/`is_terminal` helpers. Does not redefine the enum. |
| `heartbeat.rs` | `heartbeat()` builder + `JobHeartbeatExt` — heartbeat construction, foreground/background interval constants |
| `metric.rs` | `MetricSample` — metric sample shape (name/value/unit/labels/timestamp) |
| `span.rs` | `SpanFieldSet` — bounded tracing span/log field set (`job_id`, `source_id`, `adapter`, `scope`, `phase`, `provider_id`, counts, error code/severity), built from an event or heartbeat via `from_event`/`from_heartbeat`. Consumed by `sink/tracing_sink.rs` instead of ad hoc hardcoded fields. |
| `log.rs` | `LogFieldSet`/`LogLevel` — structured log field set (timestamp/level/target/message + correlation ids); `message` is redacted through `axon_core::redact::redact_secrets` at construction (the redaction hook point) |
| `sequence.rs` | `SequenceRegistry` — monotonic per-`job_id` sequence assignment, applied by sinks at emit time |
| `reservation.rs` | provider reservation/cooling tracking |
| `collector.rs` | `ObservabilitySink` trait (`emit`/`heartbeat`/`metric`/`flush`) — the shared emit boundary |
| `sink.rs` + `sink/sqlite.rs` + `sink/tracing_sink.rs` | production `ObservabilitySink` impls: `SqliteObservabilitySink` (durable event/heartbeat/provider-health rows, owns an in-crate migration) and `TracingObservabilitySink` (forwards to the `tracing` subscriber via `SpanFieldSet`) |
| `migration.rs` | `MIGRATIONS`/`migration_set()` — this crate's SQL migration set, composed into the shared cross-crate SQLite runner (see `axon-jobs`) rather than run standalone in production |
| `schema_registry.rs` | `EventSpec`/`event_registry()` — runtime event name/phase/status registry consumed by schema-contract generation |
| `testing.rs` | `InMemoryObservabilitySink` + `InMemoryObservabilitySnapshot` — in-memory `ObservabilitySink` fixture for tests, plus `test_error()` |

## Boundary — keep OUT of this crate
- durable job rows, job scheduling, worker orchestration.
- CLI progress rendering, REST SSE routing, MCP response formatting.
- provider clients or retry-policy **decisions** (those are `axon-error`).
- business logic for pipeline stages.

## Dependencies
- **Allowed:** `axon-api`, `axon-error`, `axon-core`; tracing / metrics / serde crates.
- **Forbidden:** job store implementations, transport frameworks, source adapters, vector stores, embedding providers, LLM providers. Enforced by `cargo xtask check-layering`.

## Invariants (review checklist)
- Every pipeline phase emits **start / progress / complete / fail / degrade** events.
- Every long-running operation emits the **same `job_id` / `source_id` phase events** across surfaces (also preserve status, counts, timings, degradation, provider, current-item fields).
- Event JSON is **stable and schema-backed**.
- **Redacted fields stay redacted** in logs and traces.
- Metric helpers **reject high-cardinality labels**.
- Provider saturation and graceful degradation are **visible without log scraping**.

## DTO ownership
Wire-facing progress/status/heartbeat DTOs (`JobProgress`, `JobHeartbeat`,
`JobEvent`, `SourceProgressEvent`) are defined in **`axon-api`**; this crate emits
and populates them — it does not redefine transport-facing shapes.

## Keep in sync when shapes change
`README.md` (crate contract) · `runtime/observability-contract.md` ·
`schemas/event-schema.md` (event schema + fixture catalog + metric inventory) ·
the progress/event DTO components in `axon-api`.
