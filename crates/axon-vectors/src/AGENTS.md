# axon-vectors

Implement vector stores, Qdrant collections, validated point batches, payloads, filtering, search primitives, and scoped deletion.

## Read before changing

[adding vector store](../../../docs/development/adding-vector-store.md) · [vector payload](../../../docs/reference/sources/vector-payload.md) · [qdrant payload schema](../../../docs/reference/qdrant-payload-schema.md) · [storage](../../../docs/reference/runtime/storage.md)

## Implementation map

[Crate exports](lib.rs) and [manifest](../Cargo.toml); focused entry points:
[store.rs](store.rs) · [qdrant.rs](qdrant.rs) · [point.rs](point.rs) · [collection.rs](collection.rs) · [payload.rs](payload.rs) · [filter.rs](filter.rs) · [health.rs](health.rs) · [payload_generation.rs](payload_generation.rs) · [payload_redaction.rs](payload_redaction.rs)

## Change requirements

- Validate dimensions, vector names, model identity, and required shared payload metadata before writes. Collection creation and point upserts must be safely repeatable.

- Keep source/generation filters on queries and deletes. Cleanup cannot broaden a selector or remove current-generation points while reclaiming stale generations.

- Preserve payload redaction and visibility; do not publish server absolute paths or credentials. Distinguish provider reachability from valid collection/payload state in health diagnostics.

- Use VectorStore/QdrantVectorStore boundaries rather than adding storage-specific calls to transports or source adapters. Embedding generation and final LLM synthesis are not vector-store responsibilities.

## Verification for code changes

collection_tests, payload_tests, point_tests, store_tests, store_mode_tests, qdrant_tests and explicitly isolated live-Qdrant tests for wire changes.

Use focused `cargo test -p axon-vectors` targets. For contract changes, follow
[generated-contract validation](../../../docs/development/documentation.md);
update the linked references and affected transport consumers together.
