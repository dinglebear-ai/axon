# axon-observe Crate Contract
Last Modified: 2026-06-30

## Purpose

`axon-observe` owns the unified event, heartbeat, metric, trace, and structured
logging contract for source pipeline jobs and interactive operations.

## Owns

- observable event schema and stable event names
- progress and heartbeat emission helpers
- tracing span field conventions
- metrics names, units, labels, and cardinality rules
- log redaction hooks and structured log context
- test collectors for event/metric assertions

## Must Not Own

- durable job rows or job scheduling
- CLI progress rendering, REST SSE routing, or MCP response formatting
- provider clients or retry policy decisions
- business logic for pipeline stages

## Public Modules

```text
lib.rs
event.rs
phase.rs
heartbeat.rs
progress.rs
metric.rs
span.rs
log.rs
collector.rs
testing.rs
```

## Public API

- `ObserveEvent`
- `ObservePhase`
- `Heartbeat`
- `ProgressUpdate`
- `MetricSample`
- `SpanFields`
- `EventEmitter`
- `ObserveCollector`
- `NoopEmitter`
- `TestEmitter`

## Dependencies Allowed

- `axon-api`, `axon-error`, `axon-core`
- tracing/metrics/serde crates

## Dependencies Forbidden

- job store implementations
- transport frameworks
- source adapters, vector stores, embedding providers, LLM providers

## Generated Artifacts

- [../../schemas/event-schema.md](../../schemas/event-schema.md)
- event fixture catalog for CLI/MCP/REST parity
- metric inventory for docs generator

## Fixtures And Fakes

- in-memory event collector
- heartbeat stream fixture
- degraded-stage event fixture
- provider cooling event fixture

## Tests

- every pipeline phase has start/progress/complete/fail/degrade events
- event JSON is stable and schema-backed
- redacted fields remain redacted in logs and traces
- high-cardinality labels are rejected in metrics helpers

## Acceptance Criteria

- every long-running operation emits the same job_id/source_id phase events
- transports render observed state; they do not invent alternate status models
- provider saturation and graceful degradation are visible without log scraping

See [../README.md](../README.md) and
[../../runtime/observability-contract.md](../../runtime/observability-contract.md).
