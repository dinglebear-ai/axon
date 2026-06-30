# axon-retrieval Crate Contract
Last Modified: 2026-06-30

## Purpose

`axon-retrieval` owns query understanding, retrieval planning, hybrid ranking,
context assembly, citations, and retrieve/search/query result shaping.

## Owns

- `RetrievalEngine` and retrieval plan DTOs
- query normalization, dense/sparse/hybrid retrieval planning, filters, and
  ranking/fusion
- citation assembly, source grouping, context budgets, and result explanation
- graph/memory/vector retrieval joins through store/provider traits

## Must Not Own

- source ingestion, vector store implementation, embedding provider
  implementation, LLM provider implementation, or final answer generation
- CLI/MCP/REST formatting

## Public Modules

```text
lib.rs
engine.rs
plan.rs
query.rs
filter.rs
rank.rs
context.rs
citation.rs
memory.rs
graph.rs
testing.rs
```

## Public API

- `RetrievalEngine`
- `RetrievalPlan`
- `RetrievalRequest`
- `RetrievalResult`
- `SearchResult`
- `ContextBundle`
- `Citation`
- `RankedChunk`
- `FakeRetrievalEngine`

## Dependencies Allowed

- `axon-api`, `axon-error`, `axon-core`, `axon-observe`, `axon-embedding`,
  `axon-vectors`, `axon-graph`, `axon-memory`

## Dependencies Forbidden

- concrete Qdrant/TEI clients when avoidable through provider/store traits
- `axon-llm` final synthesis implementation
- transport crates

## Generated Artifacts

- retrieval DTO schema components
- citation/context fixtures for REST, MCP, and CLI parity

## Fixtures And Fakes

- deterministic retrieval engine fixture
- multi-source citation fixture
- graph-augmented retrieval fixture
- memory-augmented retrieval fixture

## Tests

- filters preserve source visibility and generation constraints
- ranking is deterministic given fixed store/provider fakes
- context assembly respects token/byte budgets
- citations always map to stored source metadata and chunk spans

## Acceptance Criteria

- `query`, `search`, `retrieve`, and the retrieval part of `ask` share this
  engine
- retrieval can use graph and memory signals without owning their stores
- final LLM synthesis remains outside this crate

See [../README.md](../README.md) and
[../../foundation/boundary-map.md](../../foundation/boundary-map.md).
