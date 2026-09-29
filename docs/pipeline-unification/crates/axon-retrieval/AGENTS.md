# axon-retrieval design-contract maintenance

This directory documents the `axon-retrieval` boundary: Own committed-state retrieval planning, dense/sparse fusion, filters, ranking, context budgets, and citations.
Rust implementations belong in the crate, not in this documentation directory.

## Read together

[Design contract](README.md) · [Current implementation guide](../../../../crates/axon-retrieval/src/AGENTS.md) · [Crate exports](../../../../crates/axon-retrieval/src/lib.rs)

[ask query retrieve search](../../../../docs/guides/ask-query-retrieve-search.md) · [ask rag](../../../../docs/guides/ask-rag.md) · [vector payload](../../../../docs/reference/sources/vector-payload.md) · [ledger](../../../../docs/reference/runtime/ledger.md)

## Review the actual boundary

- Inspect run_query and the concrete engine as well as boundary traits. Historical scaffolding comments do not prove the live service is absent.

- Keep final synthesis in axon-llm/service composition and provider storage mechanics behind their boundaries. Distinguish query/retrieve/indexed RAG from external web search orchestration.

For implementation evidence, inspect [engine.rs](../../../../crates/axon-retrieval/src/engine.rs), [service.rs](../../../../crates/axon-retrieval/src/service.rs), [filter.rs](../../../../crates/axon-retrieval/src/filter.rs).
Check the manifest and actual callers before describing a dependency or API as
shipped. Distinguish current behavior, intended constraints, and remaining work;
historical phase/cutover prose is not authority to restore removed runtime paths
or to assume that existing databases are empty. Preserve dated outcomes.

## Verification and paired edits

engine_tests, generation_tests, memory_tests and rank/context/citation sidecars plus service ask/query regressions; exercise empty results, budgets, and generation filtering.

When shapes or behavior change, update this contract and its linked live guide.
Regenerate schema projections from owning inputs rather than hand-editing them;
see [documentation validation](../../../development/documentation.md).
Documentation-only edits need link/structural checks, not provider deployment.
