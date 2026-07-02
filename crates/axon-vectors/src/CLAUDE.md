# axon-vectors — Agent Guide

`axon-vectors` owns **vector storage**: the `VectorStore` trait, the Qdrant
implementation, point-batch construction, collection/index management, and
payload writes/search/delete. It stores vectors; it never generates them. Full
contract (owns / API / deps / tests):
[../../../docs/pipeline-unification/crates/axon-vectors/README.md](../../../docs/pipeline-unification/crates/axon-vectors/README.md)
· behavior spec:
[../../../docs/pipeline-unification/runtime/storage-contract.md](../../../docs/pipeline-unification/runtime/storage-contract.md).

## Status — Phase 10 (Qdrant store live)
Most modules are still **markers** pending later phases, but `qdrant.rs` is now a
**live** `VectorStore` over the Qdrant REST API (reqwest): GET-then-PUT-on-404
collection ensure, named dense + bm42 sparse RRF hybrid search, generation-aware
publish (`mark_generation_committed` flips visibility in place;
`mark_unchanged_items_committed` carries points into the new generation without
mutating the old one). Wire logic lives in `qdrant/{http,convert,store_impl,
search,commit}.rs`. Credentials from the configured URL are stripped before any
error is surfaced (only the `endpoint = "configured"` marker leaks). Do not add
embedding generation, chunking, ledger commits, or RAG synthesis here.

## Module map
| File | Owns |
|---|---|
| `store.rs` | `VectorStore` trait — the durable boundary retrieval depends on |
| `qdrant.rs` | `QdrantVectorStore` — the only concrete implementation |
| `collection.rs` | `CollectionSpec` — named/sparse vectors, dimensions, payload indexes |
| `point.rs` | `VectorPointBatch`, `VectorPoint` construction from prepared chunks + embeddings |
| `payload.rs` | `VectorPayload` — validation + schema snapshots |
| `filter.rs` | `VectorFilter` — source/generation-scoped filters |
| `query.rs` | `VectorQuery`, `VectorSearchResult` — search primitives (upsert/delete/scroll/retrieve-by-source) |
| `health.rs` | store health + backpressure errors |
| `testing.rs` | `FakeVectorStore` — deterministic search ordering + outage/partial/slow fixtures |

## Boundary — keep OUT of this crate
- Embedding generation, source acquisition, chunking, ledger generation commits, RAG synthesis, transport rendering.
- Provider throughput decisions beyond store-side backpressure errors.

## Dependencies
- **Allowed:** `axon-api`, `axon-error`, `axon-core`, `axon-observe`, `axon-embedding` **types**, Qdrant client + serde/schema crates.
- **Forbidden:** embedding provider implementations, source adapters, parser impls, job runtime, transport crates, LLM providers, and `axon-ledger` (the `axon-vectors -> axon-ledger` edge is forbidden — cleanup is driven from cleanup debt). Enforced by `cargo xtask check-layering`.

## Invariants (review checklist)
- Vector payload contains the **required shared metadata** fields.
- Collection creation is **idempotent** and validates dimensions/vector names.
- **All vector writes go through validated point batches.**
- Delete filters match source id, generation, and cleanup debt **safely** (no over-broad deletes).
- Qdrant is **replaceable behind `VectorStore`** — retrieval depends on the trait, not Qdrant internals.

## DTO ownership
Serializable wire shapes (`VectorPayload`, `VectorSearchResult`, `CollectionSpec`
components) are defined in **`axon-api`**; this crate validates, stores, and
returns them — it does not redefine transport-facing shapes.

## Keep in sync when shapes change
`README.md` (crate contract) · `runtime/storage-contract.md` ·
`schemas/vector-payload-schema.md` · the vector payload/collection DTO
components in `axon-api`.
