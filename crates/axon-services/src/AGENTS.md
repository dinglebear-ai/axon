# axon-services

Compose typed use cases, ServiceContext, providers/stores, source execution, and control runtimes across domains.

## Read before changing

[crate ownership](../../../docs/architecture/crate-ownership.md) · [source pipeline](../../../docs/architecture/source-pipeline.md) · [adding source](../../../docs/development/adding-source.md) · [pipeline performance boundaries](../../../docs/guides/pipeline-performance-boundaries.md)

## Implementation map

[Crate exports](lib.rs) and [manifest](../Cargo.toml); focused entry points:
[context.rs](context.rs) · [runtime.rs](runtime.rs) · [source.rs](source.rs) · [source_jobs.rs](source_jobs.rs) · [service_traits.rs](service_traits.rs) · [projections.rs](projections.rs) · [reserved_call.rs](reserved_call.rs) · [codex_control.rs](codex_control.rs) · [artifact_candidate_outbox.rs](artifact_candidate_outbox.rs)

## Change requirements

- Keep single-domain implementation in its owning crate; services coordinate, inject boundaries, and manage cross-domain lifecycle. Do not duplicate handlers separately for CLI/MCP/HTTP.

- Source execution preserves one job ID and explicit stage outputs, generation fencing, document status, cleanup debt, and provider reservation cleanup. Cancellation and failed acquisition must not publish partial generations.

- Keep artifact candidate delivery/outbox independent from source publication authority. Record retriable sink failures without silently losing evidence or rerunning acquisition unnecessarily.

- Trusted Codex control is separate from synthesis. Memory publication, watches, and source projections reuse shared pipeline/job semantics. Errors and warnings identify stage, partial effects, and a concrete recovery path.

## Verification for code changes

Affected use-case and source integration sidecars, including source_pipeline_differential, source_security, source observability, cancellation/reuse, memory sync, and codex_control tests.

Use focused `cargo test -p axon-services` targets. For contract changes, follow
[generated-contract validation](../../../docs/development/documentation.md);
update the linked references and affected transport consumers together.
