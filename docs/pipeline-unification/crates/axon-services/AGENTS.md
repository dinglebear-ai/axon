# axon-services design-contract maintenance

This directory documents the `axon-services` boundary: Compose typed use cases, ServiceContext, providers/stores, source execution, and control runtimes across domains.
Rust implementations belong in the crate, not in this documentation directory.

## Read together

[Design contract](README.md) · [Current implementation guide](../../../../crates/axon-services/src/AGENTS.md) · [Crate exports](../../../../crates/axon-services/src/lib.rs)

[crate ownership](../../../../docs/architecture/crate-ownership.md) · [source pipeline](../../../../docs/architecture/source-pipeline.md) · [adding source](../../../../docs/development/adding-source.md) · [pipeline performance boundaries](../../../../docs/guides/pipeline-performance-boundaries.md)

## Review the actual boundary

- Keep single-domain implementation in its owning crate; services coordinate, inject boundaries, and manage cross-domain lifecycle. Do not duplicate handlers separately for CLI/MCP/HTTP.

- Trusted Codex control is separate from synthesis. Memory publication, watches, and source projections reuse shared pipeline/job semantics. Errors and warnings identify stage, partial effects, and a concrete recovery path.

For implementation evidence, inspect [context.rs](../../../../crates/axon-services/src/context.rs), [runtime.rs](../../../../crates/axon-services/src/runtime.rs), [source.rs](../../../../crates/axon-services/src/source.rs).
Check the manifest and actual callers before describing a dependency or API as
shipped. Distinguish current behavior, intended constraints, and remaining work;
historical phase/cutover prose is not authority to restore removed runtime paths
or to assume that existing databases are empty. Preserve dated outcomes.

## Verification and paired edits

Affected use-case and source integration sidecars, including source_pipeline_differential, source_security, source observability, cancellation/reuse, memory sync, and codex_control tests.

When shapes or behavior change, update this contract and its linked live guide.
Regenerate schema projections from owning inputs rather than hand-editing them;
see [documentation validation](../../../development/documentation.md).
Documentation-only edits need link/structural checks, not provider deployment.
