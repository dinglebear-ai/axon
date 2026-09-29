# axon-api design-contract maintenance

This directory documents the `axon-api` boundary: Own transport-neutral operation DTOs, identities, enums, envelopes, and deterministic schema inputs.
Rust implementations belong in the crate, not in this documentation directory.

## Read together

[Design contract](README.md) · [Current implementation guide](../../../../crates/axon-api/src/AGENTS.md) · [Crate exports](../../../../crates/axon-api/src/lib.rs)

[api parity](../../../../docs/reference/api-parity.md) · [vector payload](../../../../docs/reference/sources/vector-payload.md) · [artifact candidate pipeline](../../../../docs/architecture/specs/artifact-candidate-pipeline.md)

## Review the actual boundary

- axon-error is already a direct dependency, not a future migration. Keep the dependency direction below domain/services/transports; do not introduce provider clients, persistence, HTTP frameworks, or filesystem/process side effects.

- Preserve exact neutral dinglebear.artifact-candidate/v1 parity. Delivery and batch metadata belong in Axon-specific wrappers, not the shared candidate payload.

For implementation evidence, inspect [action.rs](../../../../crates/axon-api/src/action.rs), [source.rs](../../../../crates/axon-api/src/source.rs), [source/artifact_candidate.rs](../../../../crates/axon-api/src/source/artifact_candidate.rs).
Check the manifest and actual callers before describing a dependency or API as
shipped. Distinguish current behavior, intended constraints, and remaining work;
historical phase/cutover prose is not authority to restore removed runtime paths
or to assume that existing databases are empty. Preserve dated outcomes.

## Verification and paired edits

source_* and service_job/job_status/result sidecars; regenerate affected API, CLI, MCP, and OpenAPI contracts and test parity.

When shapes or behavior change, update this contract and its linked live guide.
Regenerate schema projections from owning inputs rather than hand-editing them;
see [documentation validation](../../../development/documentation.md).
Documentation-only edits need link/structural checks, not provider deployment.
