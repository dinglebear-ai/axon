# axon-api

Own transport-neutral operation DTOs, identities, enums, envelopes, and deterministic schema inputs.

## Read before changing

[api parity](../../../docs/reference/api-parity.md) · [vector payload](../../../docs/reference/sources/vector-payload.md) · [artifact candidate pipeline](../../../docs/architecture/specs/artifact-candidate-pipeline.md)

## Implementation map

[Crate exports](lib.rs) and [manifest](../Cargo.toml); focused entry points:
[action.rs](action.rs) · [source.rs](source.rs) · [source/artifact_candidate.rs](source/artifact_candidate.rs) · [job_dto.rs](job_dto.rs) · [reset.rs](reset.rs) · [migration.rs](migration.rs) · [loadout.rs](loadout.rs)

## Change requirements

- axon-error is already a direct dependency, not a future migration. Keep the dependency direction below domain/services/transports; do not introduce provider clients, persistence, HTTP frameworks, or filesystem/process side effects.

- Preserve serialized enum names and request/result semantics. Update schema fixtures and transport consumers when fields or variants change; Rust compilation alone does not establish wire compatibility.

- The MCP-only tagged router and server envelopes live in axon-mcp; do not infer the entire MCP catalog from shared action DTOs.

- Preserve exact neutral dinglebear.artifact-candidate/v1 parity. Delivery and batch metadata belong in Axon-specific wrappers, not the shared candidate payload.

## Verification for code changes

source_* and service_job/job_status/result sidecars; regenerate affected API, CLI, MCP, and OpenAPI contracts and test parity.

Use focused `cargo test -p axon-api` targets. For contract changes, follow
[generated-contract validation](../../../docs/development/documentation.md);
update the linked references and affected transport consumers together.
