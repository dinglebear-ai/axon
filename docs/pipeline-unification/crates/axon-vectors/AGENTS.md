# axon-vectors design-contract maintenance

This directory documents the `axon-vectors` boundary: Implement vector stores, Qdrant collections, validated point batches, payloads, filtering, search primitives, and scoped deletion.
Rust implementations belong in the crate, not in this documentation directory.

## Read together

[Design contract](README.md) · [Current implementation guide](../../../../crates/axon-vectors/src/AGENTS.md) · [Crate exports](../../../../crates/axon-vectors/src/lib.rs)

[adding vector store](../../../../docs/development/adding-vector-store.md) · [vector payload](../../../../docs/reference/sources/vector-payload.md) · [qdrant payload schema](../../../../docs/reference/qdrant-payload-schema.md) · [storage](../../../../docs/reference/runtime/storage.md)

## Review the actual boundary

- Validate dimensions, vector names, model identity, and required shared payload metadata before writes. Collection creation and point upserts must be safely repeatable.

- Use VectorStore/QdrantVectorStore boundaries rather than adding storage-specific calls to transports or source adapters. Embedding generation and final LLM synthesis are not vector-store responsibilities.

For implementation evidence, inspect [store.rs](../../../../crates/axon-vectors/src/store.rs), [qdrant.rs](../../../../crates/axon-vectors/src/qdrant.rs), [point.rs](../../../../crates/axon-vectors/src/point.rs).
Check the manifest and actual callers before describing a dependency or API as
shipped. Distinguish current behavior, intended constraints, and remaining work;
historical phase/cutover prose is not authority to restore removed runtime paths
or to assume that existing databases are empty. Preserve dated outcomes.

## Verification and paired edits

collection_tests, payload_tests, point_tests, store_tests, store_mode_tests, qdrant_tests and explicitly isolated live-Qdrant tests for wire changes.

When shapes or behavior change, update this contract and its linked live guide.
Regenerate schema projections from owning inputs rather than hand-editing them;
see [documentation validation](../../../development/documentation.md).
Documentation-only edits need link/structural checks, not provider deployment.
