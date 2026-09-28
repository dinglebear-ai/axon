# axon-observe design-contract maintenance

This directory documents the `axon-observe` boundary: Own correlated events, progress, heartbeat, reservation/cooling signals, metrics, and redaction-aware sinks.
Rust implementations belong in the crate, not in this documentation directory.

## Read together

[Design contract](README.md) · [Current implementation guide](../../../../crates/axon-observe/src/AGENTS.md) · [Crate exports](../../../../crates/axon-observe/src/lib.rs)

[observability](../../../../docs/reference/runtime/observability.md) · [events](../../../../docs/reference/runtime/events.md) · [redaction](../../../../docs/reference/runtime/redaction.md)

## Review the actual boundary

- Keep job/source/provider/item correlation, phase/status, counts, timing, and event sequence across CLI, HTTP, MCP, and durable job output. Transports render this model rather than inventing a second status lifecycle.

- Reject high-cardinality metric labels and do not smuggle private paths, prompts, credentials, or arbitrary user data into labels. This crate does not own job storage or transport routing.

For implementation evidence, inspect [event.rs](../../../../crates/axon-observe/src/event.rs), [progress.rs](../../../../crates/axon-observe/src/progress.rs), [heartbeat.rs](../../../../crates/axon-observe/src/heartbeat.rs).
Check the manifest and actual callers before describing a dependency or API as
shipped. Distinguish current behavior, intended constraints, and remaining work;
historical phase/cutover prose is not authority to restore removed runtime paths
or to assume that existing databases are empty. Preserve dated outcomes.

## Verification and paired edits

collector_tests, event_tests, heartbeat_tests, provider_failure_tests, security_audit_tests, reservation_tests and sink/sequence sidecars.

When shapes or behavior change, update this contract and its linked live guide.
Regenerate schema projections from owning inputs rather than hand-editing them;
see [documentation validation](../../../development/documentation.md).
Documentation-only edits need link/structural checks, not provider deployment.
