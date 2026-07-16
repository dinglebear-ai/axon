# axon-retrieval — Agent Guide

`axon-retrieval` owns **query understanding and context assembly**: the
`RetrievalEngine`, retrieval planning, dense/sparse/hybrid ranking and fusion,
filters, citations, and context budgets shared by `query`/`search`/`retrieve`
and the retrieval part of `ask`. Final LLM synthesis stays outside. Full
contract (owns / API / deps / tests):
[../../../docs/pipeline-unification/crates/axon-retrieval/README.md](../../../docs/pipeline-unification/crates/axon-retrieval/README.md)
· boundary spec:
[../../../docs/pipeline-unification/foundation/boundary-map.md](../../../docs/pipeline-unification/foundation/boundary-map.md).

## Status — live crate, ask/evaluate/query/retrieve read-plane cutover for #298
`RetrievalEngine`/`run_query` is real and wired: `axon-services::query::
query_via_retrieval` routes plain `query` (no LLM) through it, and
`axon-services::query::ask_retrieval::retrieval_ask_context` routes the
SEARCH + CONTEXT half of `ask`/`evaluate` through it too, embedding +
dense/sparse hybrid-searching via injected `VectorStore`/`EmbeddingProvider`
trait objects. `retrieve.rs` (`retrieve_document`) ports legacy
`axon-vector`'s `retrieve_result` as a thin composition over
`axon-vectors::QdrantVectorStore::retrieve_by_url` +
`render_full_doc_from_points`. LLM synthesis for `ask`/`evaluate` stays OUT of
this crate per its charter below — it lives in
`axon-services::query::synthesis`/`query::evaluate`. The legacy
`build_ask_context` reranker (`ask --explain`, used by `train`) remains on the
`axon-vector`-owned path — it depends on qdrant/tei/ranking dispatch
internals shared with `code_search`/legacy `query_hits`, which stay in
`axon-vector` until a separate slice migrates them; porting it here would mean
either duplicating that shared dispatch layer or changing its ranking
algorithm, so it was deliberately left out of this cutover. Memory isolation
between plain `query`/`ask` and `memory search` is enforced here via
`RetrievalPlan.excluded_source_kinds`. `filter.rs`, `rank.rs`, and `graph.rs`
remain marker files — filtering lives in `engine.rs`'s `search_filters`/
`excluded_by_source_kind`, ranking is delegated to the vector store's hybrid
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
| `retrieve.rs` | `retrieve_document`/`RetrievedDocument` — full-document fetch by URL, composed over `axon-vectors::QdrantVectorStore::retrieve_by_url` |
| `memory.rs` | `MEMORY_SOURCE_KIND`/`memory_retrieval_filter()` — the memory-source opt-in boundary |
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
