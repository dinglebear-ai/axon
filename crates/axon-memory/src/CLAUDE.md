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

## Status — PR0 skeleton
Modules below are **markers only**. Real implementation lands in **Phase 8**
(move memory into `axon-memory`). Do not turn memory into a source adapter, a
vector-store owner, or an LLM synthesis path here.

## Module map
| File | Owns |
|---|---|
| `store.rs` | `MemoryStore` trait — the durable boundary all callers use |
| `sqlite.rs` | SQLite `MemoryStore` implementation |
| `migration.rs` | forward-only SQLite memory schema |
| `record.rs` | `MemoryRecord` — memory record shape + retention rules |
| `link.rs` | `MemoryLink` — graph links to sources/sessions/repos/issues/artifacts/tools |
| `decay.rs` | `MemoryDecayPolicy` — decay + reinforcement rules |
| `review.rs` | `MemoryReviewPolicy` — review/archive policy |
| `recall.rs` | `MemoryRecallRequest` — lexical + vector + graph recall |
| `context.rs` | `MemoryContext` — bounded, source-cited context assembly |
| `graph.rs` | SourceGraph link integration points + vector indexing request builder |
| `testing.rs` | `FakeMemoryStore` + fixtures (stable, superseded chain, decay, context) |

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
