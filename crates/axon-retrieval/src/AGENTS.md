# axon-retrieval

Own committed-state retrieval planning, dense/sparse fusion, filters, ranking, context budgets, and citations.

## Read before changing

[ask query retrieve search](../../../docs/guides/ask-query-retrieve-search.md) · [ask rag](../../../docs/guides/ask-rag.md) · [vector payload](../../../docs/reference/sources/vector-payload.md) · [ledger](../../../docs/reference/runtime/ledger.md)

## Implementation map

[Crate exports](lib.rs) and [manifest](../Cargo.toml); focused entry points:
[engine.rs](engine.rs) · [service.rs](service.rs) · [filter.rs](filter.rs) · [rank.rs](rank.rs) · [ask_context.rs](ask_context.rs) · [context.rs](context.rs) · [citation.rs](citation.rs) · [publish.rs](publish.rs) · [retrieve.rs](retrieve.rs)

## Change requirements

- Inspect run_query and the concrete engine as well as boundary traits. Historical scaffolding comments do not prove the live service is absent.

- Preserve visibility, source/path/content-kind, and committed-generation constraints. Test removed/updated content and never expose an unpublished generation.

- Ranking/fusion must be deterministic under fixed providers/stores; context respects byte/token budgets and citation spans still map to stored source metadata after reranking/deduplication.

- Keep final synthesis in axon-llm/service composition and provider storage mechanics behind their boundaries. Distinguish query/retrieve/indexed RAG from external web search orchestration.

## Verification for code changes

engine_tests, generation_tests, memory_tests and rank/context/citation sidecars plus service ask/query regressions; exercise empty results, budgets, and generation filtering.

Use focused `cargo test -p axon-retrieval` targets. For contract changes, follow
[generated-contract validation](../../../docs/development/documentation.md);
update the linked references and affected transport consumers together.
