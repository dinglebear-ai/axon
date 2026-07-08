# axon-retrieval — Agent Guide

`axon-retrieval` owns **query understanding and context assembly**: the
`RetrievalEngine`, retrieval planning, dense/sparse/hybrid ranking and fusion,
filters, citations, and context budgets shared by `query`/`search`/`retrieve`
and the retrieval part of `ask`. Final LLM synthesis stays outside. Full
contract (owns / API / deps / tests):
[../../../docs/pipeline-unification/crates/axon-retrieval/README.md](../../../docs/pipeline-unification/crates/axon-retrieval/README.md)
· boundary spec:
[../../../docs/pipeline-unification/foundation/boundary-map.md](../../../docs/pipeline-unification/foundation/boundary-map.md).

## Status — live crate, Phase 7 landed (plain `query` cutover for #298)
`RetrievalEngine`/`run_query` is real and wired: `axon-services::query::
query_via_retrieval` routes plain `query` (no LLM) through it, embedding +
dense/sparse hybrid-searching via injected `VectorStore`/`EmbeddingProvider`
trait objects; `ask`/`evaluate`/`retrieve` remain on the legacy
`axon-vector`-owned path (a separate, not-yet-migrated slice). Namespace
isolation between plain `query`/`ask` and `memory search` is enforced here via
`RetrievalPlan.excluded_namespaces` (only applied when no positive
`namespace_filters` is set). `filter.rs`, `rank.rs`, and `graph.rs` remain
marker files — filtering lives in `engine.rs`'s `search_filters`/
`excluded_by_namespace`, ranking is delegated to the vector store's hybrid
RRF fusion, not reimplemented here.

## Module map
| File | Owns |
|---|---|
| `engine.rs` | `RetrievalEngine` — the boundary all retrieval callers use; namespace/visibility/generation filter construction |
| `service.rs` | `run_query`/`QueryServiceRequest`/`QueryServiceHit` — the public entrypoint `axon-services` calls |
| `plan.rs` | `RetrievalPlan` — dense/sparse/hybrid planning DTOs |
| `query.rs` | `RetrievalRequest`/`RetrievalMatch`/`RetrievalResult` shaping |
| `context.rs` | `ContextBundle` — context budgets, source grouping, result explanation |
| `citation.rs` | `Citation` assembly mapped to stored source metadata/chunk spans |
| `memory.rs` | `MEMORY_VECTOR_NAMESPACE`/`memory_retrieval_filter()` — the memory-namespace opt-in boundary |
| `testing.rs` | shared test fixtures |
| `filter.rs` / `rank.rs` / `graph.rs` | marker files — logic lives in `engine.rs` and the injected vector store; do not duplicate |

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
