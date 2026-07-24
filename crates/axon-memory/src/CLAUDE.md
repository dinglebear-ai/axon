# axon-memory — Agent Guide

`axon-memory` owns **durable user/agent memory as a first-class source-like
domain**: memory records and their full lifecycle (remember, search, show, link,
supersede, review, decay, reinforce, archive, context), memory graph links, and
context assembly. A narrow `memory://` adapter projects authoritative records
through the canonical source pipeline; this crate does **not** own source
orchestration or vector publication. Full
contract (owns / API / deps / tests):
[../../../docs/pipeline-unification/crates/axon-memory/README.md](../../../docs/pipeline-unification/crates/axon-memory/README.md)
· behavior spec:
[../../../docs/pipeline-unification/runtime/memory-contract.md](../../../docs/pipeline-unification/runtime/memory-contract.md).

## Status — live crate, Phase 8 landed
The full lifecycle is real and tested: `SqliteMemoryStore` (remember, search,
show, link, supersede, reinforce, decay, review, update, pin, archive, forget,
compact, import, export) and recall-only `VectorBackedMemoryStore` are live.
`axon-services::memory::memory_store()` composes vector recall over SQLite while
successful mutations synchronously run or durably enqueue canonical
`memory://` source publication. `GraphBackedMemoryStore` remains available for
isolated domain tests, but production mutation graph writes come from adapter
graph candidates in the source pipeline. `context.rs`, `link.rs`, `recall.rs`,
and `review.rs` remain marker files — their real logic already lives inside
`store.rs`/`sqlite.rs`/`sqlite/*.rs` rather than as separate modules; do not
duplicate it there. SQLite remains authoritative; vector and graph state are
derived publications.

## Module map
| File | Owns |
|---|---|
| `store.rs` | `MemoryStore` trait + `FakeMemoryStore` — the durable boundary all callers use |
| `sqlite.rs` + `sqlite/{error,lifecycle,compact,rows,recall}.rs` | `SqliteMemoryStore` — full lifecycle implementation (remember/search/show/link/supersede/reinforce/decay/review/update/pin/archive/forget/compact/import/export) |
| `vector.rs` + `vector/search.rs` | `VectorBackedMemoryStore` recall decorator over canonical `source_kind=memory` vectors; mutation methods delegate only |
| `graph.rs` | reusable graph decorator/mirror for isolated domain composition; production graph candidates are emitted by `axon-adapters::memory` |
| `graph_refs.rs` / `observe.rs` | memory-search graph-ref enrichment + memory-lifecycle observability (emits `axon-observe` events) |
| `migration.rs` | forward-only SQLite memory schema |
| `record.rs` | time/age scoring — `Clock` trait + `parse_epoch_secs`/`age_days` (`MemoryRecord` itself lives in `axon-api::source`) |
| `decay.rs` | `MemoryDecayPolicy` — decay + reinforcement rules |
| `testing.rs` | `FixedClock` + fixtures |
| `link.rs` / `recall.rs` / `review.rs` / `context.rs` | marker files — superseded by the real logic in `store.rs`/`sqlite.rs`/`sqlite/*.rs`; do not duplicate |

## Boundary — keep OUT of this crate
- General source acquisition, source routing, parser registry, general SourceGraph storage.
- Vector publication, deletion, or direct Qdrant client ownership. Memory
  mutations hand stable identities to the canonical source pipeline.
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
- Public memory export reads authoritative SQLite metadata records. Qdrant
  points are derived recall indexes, so `qdrant_page_size` remains reserved
  for vector maintenance rather than defining export correctness.
- No memory lifecycle mutation writes a generation-0 vector or directly mirrors
  a production graph node. A queued source job is the durable publication
  recovery marker; an enqueue failure appends `memory.source_sync_pending` to
  memory history.

## DTO ownership
Wire DTOs (`MemoryRecord`, `MemoryLink`, `MemoryDecayPolicy`,
`MemoryReviewPolicy`, `MemoryRecallRequest`, `MemoryContext`, …) are defined in
**`axon-api`**; this crate stores and returns them — it does not redefine
transport-facing shapes.

## Keep in sync when shapes change
`README.md` (crate contract) · `runtime/memory-contract.md` ·
`sources/source-graph.md` · `schemas/database-schema.md` (memory tables) ·
`sources/metadata-payload.md` · the memory DTO components in `axon-api`.
