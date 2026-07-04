# axon-retrieval — Agent Guide

`axon-retrieval` owns **query understanding and context assembly**: the
`RetrievalEngine`, retrieval planning, dense/sparse/hybrid ranking and fusion,
filters, citations, and context budgets shared by `query`/`search`/`retrieve`
and the retrieval part of `ask`. Final LLM synthesis stays outside. Full
contract (owns / API / deps / tests):
[../../../docs/pipeline-unification/crates/axon-retrieval/README.md](../../../docs/pipeline-unification/crates/axon-retrieval/README.md)
· boundary spec:
[../../../docs/pipeline-unification/foundation/boundary-map.md](../../../docs/pipeline-unification/foundation/boundary-map.md).

## Status — PR0 skeleton
Modules below are **markers only**. Real implementation lands in **Phase 7**,
decomposed out of `axon-vector`'s retrieval/RAG logic. Do not implement vector
store internals, embedding/LLM providers, or transport formatting here.

## Module map
| File | Owns |
|---|---|
| `engine.rs` | `RetrievalEngine` — the boundary all retrieval callers use |
| `plan.rs` | `RetrievalPlan` — dense/sparse/hybrid planning DTOs |
| `query.rs` | query normalization + `RetrievalRequest`/`RetrievalResult`/`SearchResult` shaping |
| `filter.rs` | source-visibility + generation-constraint filters |
| `rank.rs` | ranking/fusion → `RankedChunk` |
| `context.rs` | `ContextBundle` — context budgets, source grouping, result explanation |
| `citation.rs` | `Citation` assembly mapped to stored source metadata/chunk spans |
| `graph.rs` / `memory.rs` | graph- and memory-augmented retrieval joins through store/provider traits |
| `testing.rs` | `FakeRetrievalEngine` + multi-source/graph/memory citation fixtures |

## Boundary — keep OUT of this crate
- Source ingestion, vector-store implementation, embedding provider implementation, LLM provider implementation, final answer generation.
- CLI/MCP/REST formatting.

## Dependencies
- **Allowed:** `axon-api`, `axon-error`, `axon-core`, `axon-observe`, `axon-embedding`, `axon-vectors`, `axon-graph`, `axon-memory`, and `axon-llm` **types**.
- **Forbidden:** concrete Qdrant/TEI clients (reach them via provider/store traits), `axon-llm` final-synthesis implementation, transport crates. Enforced by `cargo xtask check-layering`.

## Invariants (review checklist)
- Filters **preserve source visibility and generation constraints**.
- Ranking is **deterministic** given fixed store/provider fakes.
- Context assembly **respects token/byte budgets**.
- Citations **always map** to stored source metadata and chunk spans.
- `query`, `search`, `retrieve`, and the retrieval part of `ask` **share this engine**; final synthesis stays out.

## DTO ownership
Serializable wire shapes (`RetrievalResult`, `SearchResult`, `ContextBundle`,
`Citation`, ranking/fusion DTOs) are defined in **`axon-api`**; this crate
computes and returns them — it does not redefine transport-facing shapes.

## Keep in sync when shapes change
`README.md` (crate contract) · `foundation/boundary-map.md` · the retrieval /
citation / context DTO components in `axon-api` (REST/MCP/CLI parity fixtures).
