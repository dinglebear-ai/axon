# axon-embedding design-contract maintenance

This directory documents the `axon-embedding` boundary: Own embedding provider calls, batching, capabilities, reservations, cooldown, and cache behavior.
Rust implementations belong in the crate, not in this documentation directory.

## Read together

[Design contract](README.md) · [Current implementation guide](../../../../crates/axon-embedding/src/AGENTS.md) · [Crate exports](../../../../crates/axon-embedding/src/lib.rs)

[adding provider](../../../../docs/development/adding-provider.md) · [providers](../../../../docs/reference/runtime/providers.md) · [provider capabilities](../../../../docs/reference/runtime/provider-capabilities.md) · [pipeline performance boundaries](../../../../docs/guides/pipeline-performance-boundaries.md)

## Review the actual boundary

- Preserve input IDs/order and validate model identity, dimensions, response cardinality, and vector validity. Do not let source adapters implement TEI/OpenAI retry policy.

- Embedding output is not a Qdrant publication receipt. Keep vector point construction, collection management, and vector persistence outside this crate.

For implementation evidence, inspect [provider.rs](../../../../crates/axon-embedding/src/provider.rs), [batch.rs](../../../../crates/axon-embedding/src/batch.rs), [tei.rs](../../../../crates/axon-embedding/src/tei.rs).
Check the manifest and actual callers before describing a dependency or API as
shipped. Distinguish current behavior, intended constraints, and remaining work;
historical phase/cutover prose is not authority to restore removed runtime paths
or to assume that existing databases are empty. Preserve dated outcomes.

## Verification and paired edits

provider_tests, tei_client_tests, reservation_compat_tests, capability_tests, and cache sidecars; include mixed hits/misses, timeout, dimension mismatch, and retry exhaustion.

When shapes or behavior change, update this contract and its linked live guide.
Regenerate schema projections from owning inputs rather than hand-editing them;
see [documentation validation](../../../development/documentation.md).
Documentation-only edits need link/structural checks, not provider deployment.
