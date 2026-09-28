# axon-observe

Own correlated events, progress, heartbeat, reservation/cooling signals, metrics, and redaction-aware sinks.

## Read before changing

[observability](../../../docs/reference/runtime/observability.md) · [events](../../../docs/reference/runtime/events.md) · [redaction](../../../docs/reference/runtime/redaction.md)

## Implementation map

[Crate exports](lib.rs) and [manifest](../Cargo.toml); focused entry points:
[event.rs](event.rs) · [progress.rs](progress.rs) · [heartbeat.rs](heartbeat.rs) · [provider_failure.rs](provider_failure.rs) · [reservation.rs](reservation.rs) · [sequence.rs](sequence.rs) · [security_audit.rs](security_audit.rs) · [sink.rs](sink.rs) · [metric.rs](metric.rs)

## Change requirements

- Keep job/source/provider/item correlation, phase/status, counts, timing, and event sequence across CLI, HTTP, MCP, and durable job output. Transports render this model rather than inventing a second status lifecycle.

- Warnings and degraded events must state impact, actual fallback, safe cause, and corrective action. Saturation/cooling must be diagnosable without scraping unstructured logs.

- Bound event buffers and sinks; preserve ordering and heartbeat coverage for long-running work. Keep redaction before persistence or transmission.

- Reject high-cardinality metric labels and do not smuggle private paths, prompts, credentials, or arbitrary user data into labels. This crate does not own job storage or transport routing.

## Verification for code changes

collector_tests, event_tests, heartbeat_tests, provider_failure_tests, security_audit_tests, reservation_tests and sink/sequence sidecars.

Use focused `cargo test -p axon-observe` targets. For contract changes, follow
[generated-contract validation](../../../docs/development/documentation.md);
update the linked references and affected transport consumers together.
