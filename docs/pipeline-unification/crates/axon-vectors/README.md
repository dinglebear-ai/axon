# axon-vectors Crate Contract
Last Modified: 2026-06-30

## Purpose

`axon-vectors` owns vector storage, point construction, collection management,
payload indexes, vector search primitives, and Qdrant implementation details.

## Owns

- `VectorStore` trait and Qdrant implementation
- `VectorPointBatch` construction from prepared chunks and embeddings
- collection spec, named vectors, sparse vectors, payload indexes, and health
- upsert, delete, scroll-by-filter, retrieve-by-source, and vector query methods
- vector payload validation and schema snapshots

## Must Not Own

- embedding generation, source acquisition, chunking, ledger generation commits,
  RAG synthesis, or transport rendering
- provider throughput decisions beyond store-side backpressure errors

## Public Modules

```text
lib.rs
store.rs
qdrant.rs
collection.rs
point.rs
payload.rs
filter.rs
query.rs
health.rs
testing.rs
```

## Public API

- `VectorStore`
- `QdrantVectorStore`
- `VectorPointBatch`
- `VectorPoint`
- `VectorPayload`
- `VectorFilter`
- `VectorQuery`
- `VectorSearchResult`
- `CollectionSpec`
- `FakeVectorStore`

## Dependencies Allowed

- `axon-api`, `axon-error`, `axon-core`, `axon-observe`
- Qdrant client and serde/schema crates

## Dependencies Forbidden

- embedding provider implementations
- source adapters, parser implementations, job runtime, transport crates
- LLM providers

## Generated Artifacts

- [../../schemas/vector-payload-schema.md](../../schemas/vector-payload-schema.md)
- vector collection and payload index schemas

## Fixtures And Fakes

- fake vector store with deterministic search ordering
- Qdrant payload fixture
- collection mismatch fixture
- cleanup/delete-by-source fixture

## Tests

- vector payload contains required shared metadata
- collection creation is idempotent and validates dimensions/vector names
- delete filters match source id, generation, and cleanup debt safely
- fake store can simulate outage, partial failure, and slow writes

## Acceptance Criteria

- Qdrant is replaceable behind `VectorStore`
- all vector writes go through validated point batches
- retrieval code depends on the trait, not Qdrant internals

See [../README.md](../README.md) and
[../../runtime/storage-contract.md](../../runtime/storage-contract.md).
