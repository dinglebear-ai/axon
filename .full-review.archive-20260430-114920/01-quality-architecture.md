# Phase 1: Code Quality and Architecture

## Findings

- High — `crates/vector/ops/commands/evaluate.rs:144`
  The evaluation path has write-side effects: when the judge says RAG underperformed, `evaluate_payload()` calls `discover_crawl_suggestions()` and then `enqueue_suggested_crawls()`, which directly starts crawl jobs.
  Impact: a diagnostic command can unexpectedly mutate the job queue and trigger network crawling. This makes evaluate hard to reason about, violates command separation, and surprises MCP/CLI callers that expect evaluation to be read-only.
  Fix: make evaluate return suggestions only by default. Move crawl enqueue behind an explicit opt-in flag/service option, and route any enqueue through the services runtime rather than calling `start_crawl_job()` from vector command code.

- High — `crates/vector/ops/commands/evaluate.rs:8`
  `evaluate.rs` imports `crate::crates::jobs::crawl::start_crawl_job` directly from the vector command layer.
  Impact: this pierces the services-first architecture documented in `crates/services/CLAUDE.md`; RAG command code now owns job orchestration and backend details that should stay in `crates/services`.
  Fix: remove direct job calls from `crates/vector`. If evaluate ever needs enqueue behavior, expose it as a typed service-level operation with an explicit option.

- High — `crates/services/types/service.rs:254`
  Query, ask, evaluate, retrieve, and suggest results are mostly `serde_json::Value` pass-throughs instead of stable typed service contracts.
  Impact: CLI and MCP handlers manually index JSON fields, so schema drift is runtime-only and cross-entrypoint compatibility is fragile.
  Fix: introduce typed `QueryHit`, `AskPayload`, `EvaluatePayload`, `Suggestion`, and retrieve result structs; serialize only at CLI/MCP edges.

- High — `crates/vector/ops/commands/ask.rs:47`
  `ask_payload()` normalizes and citation-gates the final answer, but `evaluate_payload()` judges the raw RAG answer returned from `run_rag_answer()`.
  Impact: evaluate can score an answer shape users would not receive from the production ask path, making evaluation results misleading.
  Fix: factor shared grounded-answer generation that returns both raw and normalized text, and evaluate the normalized production answer.

- High — `crates/vector/ops/commands/query.rs:13`
  Query, ask, and judge-reference retrieval each embed, search, build candidates, and rerank through separate implementations with different filters.
  Impact: low-signal filtering, allowlists, topical overlap, relevance thresholds, and candidate shaping can drift across RAG surfaces.
  Fix: create one candidate retrieval pipeline with explicit options for query, ask, and evaluate behavior.

- Medium — `crates/vector/ops/commands/evaluate.rs:106`
  The current structured `evaluate_payload()` path runs the RAG answer and baseline answer sequentially even though the module contains a parallel streaming implementation and the docs describe concurrent answer generation.
  Impact: every evaluate call pays nearly the sum of both answer latencies. The code also leaves a large, partly dead parallel implementation in `evaluate/streaming.rs`, increasing maintenance cost.
  Fix: run the RAG and baseline futures concurrently in the JSON/service path, or remove the unused parallel path and update docs. Prefer a single shared answer orchestration path.

- Medium — `crates/vector/ops/commands/evaluate/scoring.rs:7`
  Judge reference retrieval duplicates candidate construction and reranking instead of reusing the ask retrieval pipeline. It misses ask-specific guards such as low-signal filtering, authoritative allowlist filtering, and topical-overlap filtering.
  Impact: the judge can be grounded in noisier or out-of-policy material than the answer it is evaluating, which makes scores inconsistent and makes changes to ask retrieval easy to forget in evaluate.
  Fix: factor candidate building/filtering into a reusable helper, or apply the same low-signal, allowlist, rerank, and topical-overlap gates in `build_judge_reference()`.

- Medium — `crates/vector/ops/commands/ask/context/build.rs:47`
  Context assembly tracks inserted full-document URLs but not URLs already added as top chunks. Tier 2 can fetch and insert a full document for a URL that already appears in Tier 1.
  Impact: duplicate source content consumes scarce context budget and lowers diversity, especially with the default `ask_full_docs = 4` and large `ask_doc_chunk_limit`.
  Fix: seed the full-document exclusion set with selected top-chunk URLs, or intentionally replace the top chunk with the full document for the same URL.

- Medium — `crates/vector/ops/qdrant/commands.rs:44`
  Initial vector search applies `since`/`before`, but full-document expansion retrieves by URL only.
  Impact: `ask --since/--before` can include out-of-window chunks in final answer context.
  Fix: pass the same scraped-at filter into `qdrant_retrieve_by_url()` and combine it with the URL filter.

- Medium — `crates/vector/ops/commands/suggest.rs:360`
  Suggestion reasons are produced by vector ops but dropped by the service and CLI result.
  Impact: callers cannot explain or audit why a URL was suggested, while evaluate keeps a richer suggestion model.
  Fix: change `SuggestResult` to preserve `Vec<Suggestion { url, reason }>` through services, CLI, and MCP.

- Medium — `crates/vector/ops/tei/qdrant_store.rs:30`
  The process-wide vector mode cache is keyed only by collection name. It ignores `qdrant_url`, so two configs using the same collection name against different Qdrant servers can share a stale `VectorMode`.
  Impact: tests, MCP requests, or long-lived processes that switch Qdrant endpoints can route named collections to legacy search or vice versa.
  Fix: key the cache by `(qdrant_base, collection)` or an explicit collection identity struct.

- Low — `crates/vector/ops/commands/query.rs:80`
  Paginated query ranks restart at 1 after `offset`, and `chunk_index` is always `null` despite payload support.
  Impact: clients cannot tell absolute rank or retrieve the exact matched chunk from query results.
  Fix: set rank to `offset + i + 1` and include `h.payload.chunk_index`.

- Low — `crates/services/search.rs:184`
  Research synthesis builds a JSON-returning prompt but treats non-JSON LLM output as a valid summary string.
  Impact: callers receive inconsistent payload semantics: `summary` sometimes contains parsed JSON content and sometimes raw model text that may include wrappers or extra fields.
  Fix: either make the prompt plain text, or enforce JSON parsing with a fallback object that records parse failure.

## Critical Issues for Phase 2 Context

- `evaluate_payload()` currently has side effects that can enqueue crawl jobs; security review must treat evaluate as a write-capable network-triggering path.
- Ask, evaluate, and research inject retrieved or searched source text directly into LLM prompts; security review must assess prompt-injection isolation.
- Vector-mode caching is global and endpoint-insensitive; performance/correctness review must assess long-lived mixed-config processes.
