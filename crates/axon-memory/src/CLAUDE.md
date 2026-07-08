# axon-memory — Agent Guide

`axon-memory` owns **durable user/agent memory as a first-class source-like
domain**: memory records and their full lifecycle (remember, search, show, link,
supersede, review, decay, reinforce, archive, context), memory graph links, and
context assembly. Memory is **not** a generic source adapter and does **not** own
the vector store — it is an observable, SourceGraph-linked durable domain. Full
contract (owns / API / deps / tests):
[../../../docs/pipeline-unification/crates/axon-memory/README.md](../../../docs/pipeline-unification/crates/axon-memory/README.md)
· behavior spec:
[../../../docs/pipeline-unification/runtime/memory-contract.md](../../../docs/pipeline-unification/runtime/memory-contract.md).

## Status — live crate, Phase 8 landed
The full lifecycle is real and tested: `SqliteMemoryStore` (remember, search,
show, link, supersede, reinforce, decay, review, update, pin, archive, forget,
compact, import, export), `VectorBackedMemoryStore` (Qdrant indexing via
`MemoryVectorConfig`/`MemoryBatchLimits`, batched embedding with partial-failure
recovery), and `GraphBackedMemoryStore` (mirrors lifecycle transitions into
`axon-graph` via `GraphBackedMemoryMirror`) all compose in `axon-services::memory::
memory_store()` as `Vector(Graph(Sqlite))`. `context.rs`, `link.rs`, `recall.rs`,
and `review.rs` remain marker files — their real logic already lives inside
`store.rs`/`sqlite.rs`/`sqlite/*.rs` rather than as separate modules; do not
duplicate it there. Memory is still **not** a generic source adapter and does
**not** own the vector store directly — it composes over injected
`VectorStore`/`GraphStore` boundaries.

## Module map
| File | Owns |
|---|---|
| `store.rs` | `MemoryStore` trait + `FakeMemoryStore` — the durable boundary all callers use |
| `sqlite.rs` + `sqlite/{error,lifecycle,compact,rows}.rs` | `SqliteMemoryStore` — full lifecycle implementation (remember/search/show/link/supersede/reinforce/decay/review/update/pin/archive/forget/compact/import/export) |
| `vector.rs` + `vector/{batch,payload}.rs` | `VectorBackedMemoryStore` decorator — Qdrant indexing, batched embed with partial-failure recovery |
| `graph.rs` | `GraphBackedMemoryStore`/`GraphBackedMemoryMirror` decorator — mirrors lifecycle into `axon-graph`; also `memory_graph_candidates()` |
| `migration.rs` | forward-only SQLite memory schema |
| `record.rs` | `MemoryRecord` — memory record shape + retention rules |
| `decay.rs` | `MemoryDecayPolicy` — decay + reinforcement rules |
| `testing.rs` | `FixedClock` + fixtures |
| `link.rs` / `recall.rs` / `review.rs` / `context.rs` | marker files — superseded by the real logic in `store.rs`/`sqlite.rs`/`sqlite/*.rs`; do not duplicate |

## Boundary — keep OUT of this crate
- General source acquisition, source routing, parser registry, general SourceGraph storage.
- Vector store **implementation** and direct Qdrant client ownership — build indexing requests, do not own the provider.
- RAG answer synthesis outside memory context retrieval; transport command rendering.

## Dependencies
- **Allowed:** `axon-api`, `axon-error`, `axon-core`, `axon-ledger`, `axon-graph`, `axon-observe`, SQLite + migration crates.
- **Forbidden:** direct Qdrant client ownership, LLM provider implementations, transport crates. Enforced by `cargo xtask check-layering`.

## Invariants (review checklist)
- **Decay never deletes without policy approval** — decay/review policies are explicit and testable.
- **Supersession preserves old records and their graph links** — history is never lost.
- Recall can **combine lexical, vector, and graph filters**.
- Memory context output is **bounded and source-cited**.
- Memory lifecycle is **observable and linked into SourceGraph**.

## DTO ownership
Wire DTOs (`MemoryRecord`, `MemoryLink`, `MemoryDecayPolicy`,
`MemoryReviewPolicy`, `MemoryRecallRequest`, `MemoryContext`, …) are defined in
**`axon-api`**; this crate stores and returns them — it does not redefine
transport-facing shapes.

## Keep in sync when shapes change
`README.md` (crate contract) · `runtime/memory-contract.md` ·
`sources/source-graph.md` · `schemas/database-schema.md` (memory tables) ·
`sources/metadata-payload.md` · the memory DTO components in `axon-api`.
