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

## Status — live crate, Phase 8 landed
`SourceProgressEvent`/event registry and provider reservation tracking
(`reservation.rs`) are real and tested, not markers. Do not add durable job
storage, worker scheduling, or transport-specific status rendering here.

## Module map
| File | Owns |
|---|---|
| `event.rs` | `ObserveEvent` — observable event schema + stable event names |
| `phase.rs` | `ObservePhase` — pipeline phase enum (start/progress/complete/fail/degrade) |
| `heartbeat.rs` | `Heartbeat` — heartbeat emission helpers + stream builders |
| `progress.rs` | `ProgressUpdate` — progress emission helpers |
| `metric.rs` | `MetricSample` — metric names, units, labels, bounded-cardinality rules |
| `span.rs` | `SpanFields` — tracing span-field conventions |
| `log.rs` | structured log context + redaction hooks |
| `collector.rs` | `EventEmitter`, `ObserveCollector`, `NoopEmitter`, `TestEmitter` — emit boundary + test collectors |
| `testing.rs` | in-memory collector, heartbeat/degraded/cooling event fixtures |

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
