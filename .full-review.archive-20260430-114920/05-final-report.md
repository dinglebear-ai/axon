# Comprehensive Code Review Report

## Review Target

All RAG-related code in `axon_rust`: embedding, vector retrieval, context assembly, ask/evaluate/query/suggest/research service paths, CLI/MCP entry points, and directly relevant docs.

## Executive Summary

The RAG stack has solid separation in several places, but the evaluate path currently mixes diagnostic scoring with crawl job mutation and bypasses the services layer. Prompt construction also treats retrieved web/search content as plain user-message text, which leaves ask/evaluate/research exposed to source-level prompt injection. The remaining issues are mostly parity, latency, and docs drift around the current retrieval implementation.

## Findings by Priority

### Critical Issues

- None found in this pass.

### High Priority

- Phase 2 — `crates/mcp/server/handlers_embed_ingest.rs:67`
  MCP embed can index arbitrary local files without root, symlink, size, or secret-file safeguards.
  Fix by making MCP embed URL/text-only by default and adding explicit safe local-root controls.

- Phase 2 — `crates/services/acp_llm/ws_runner.rs:59`
  ACP WebSocket token-bearing URLs can be logged.
  Fix by redacting logged URLs and moving away from query-string tokens where possible.

- Phase 1/2 — `crates/vector/ops/commands/evaluate.rs:144`
  Evaluation auto-discovers and enqueues crawl jobs when RAG underperforms.
  Fix by returning suggestions as data and requiring explicit opt-in before enqueue.

- Phase 1/4 — `crates/vector/ops/commands/evaluate.rs:8`
  Vector command code imports `start_crawl_job` directly, bypassing the services-first contract.
  Fix by removing job mutation from `crates/vector` and routing any explicit enqueue through services.

- Phase 2 — `crates/vector/ops/commands/streaming.rs:194`
  Ask/evaluate prompt construction does not strongly delimit retrieved source text as untrusted.
  Fix by wrapping source text and hardening the system prompt against source instructions.

- Phase 2 — `crates/services/search.rs:209`
  Research synthesis injects web snippets without source-data delimiting or anti-instruction prompt language.
  Fix by applying the same untrusted-source prompt pattern.

- Phase 1 — `crates/services/types/service.rs:254`
  RAG services mostly return JSON pass-throughs instead of typed contracts.
  Fix by introducing typed result structs and serializing only at entrypoint boundaries.

- Phase 1 — `crates/vector/ops/commands/ask.rs:47`
  Evaluate judges raw RAG output while ask returns normalized/citation-gated output.
  Fix by sharing grounded answer generation and evaluating the production-normalized answer.

- Phase 1 — `crates/vector/ops/commands/query.rs:13`
  Query, ask, and judge-reference retrieval use separate pipelines with drifting filters.
  Fix by centralizing candidate retrieval with explicit mode options.

### Medium Priority

- Phase 1/2 — `crates/vector/ops/commands/evaluate.rs:106`
  Structured evaluate runs RAG and baseline answers sequentially.
  Fix by running both answer futures concurrently after context retrieval.

- Phase 1/2 — `crates/vector/ops/commands/evaluate/scoring.rs:7`
  Judge-reference retrieval does not share ask's low-signal, allowlist, and topical-overlap guards.
  Fix by factoring or duplicating the same candidate filter semantics.

- Phase 2 — `crates/mcp/server/handlers_query.rs:29`
  MCP query/ask collection overrides are not validated before Qdrant URL interpolation.
  Fix with central collection-name validation or encoded path segments plus allowed-collection policy.

- Phase 2 — `crates/services/acp_llm/pool.rs:153`
  ACP prewarm/completion concurrency is not globally bounded.
  Fix by adding a global completion semaphore and bounded warm-pool refill.

- Phase 1 — `crates/vector/ops/qdrant/commands.rs:44`
  `since`/`before` filters apply to initial search but not full-document expansion.
  Fix by combining scraped-at filters with URL filters for full-doc retrieval.

- Phase 1/2 — `crates/vector/ops/commands/ask/context/build.rs:47`
  Context assembly can include the same URL as both a top chunk and a full document.
  Fix by excluding top-chunk URLs from full-doc insertion or replacing the top chunk with the full doc intentionally.

- Phase 1 — `crates/vector/ops/commands/suggest.rs:360`
  Suggestion reasons are dropped at the service/CLI boundary.
  Fix by preserving structured suggestion reasons.

- Phase 1/4 — `crates/vector/ops/tei/qdrant_store.rs:30`
  Vector-mode cache is keyed by collection name only, ignoring Qdrant endpoint identity.
  Fix by keying on Qdrant base URL plus collection name.

- Phase 2 — `crates/vector/ops/qdrant/filter.rs:46`
  Invalid date filters are ignored after warning.
  Fix by returning validation errors through services and MCP invalid-params responses.

- Phase 3 — `crates/core/config/cli.rs:224`
  `evaluate --responses-mode` is parsed but not used by the current service path.
  Fix by wiring the renderer or removing the dead flag/scaffold.

- Phase 3 — `docs/CONTEXT-INJECTION.md:36`
  Docs say raw query embedding is used, but code prepends `QUERY_INSTRUCTION` and may also embed a keyword form.
  Fix by documenting asymmetric query encoding and dual embedding.

- Phase 3 — `docs/CONTEXT-INJECTION.md:110`
  RAG config defaults in docs are stale compared with `build_config.rs`.
  Fix by updating the table and adding a maintenance note/check.

- Phase 3 — `docs/CONTEXT-INJECTION.md:209`
  Docs describe concurrent evaluate behavior that the current structured service path does not perform.
  Fix by implementing concurrency or correcting docs.

### Low Priority

- Phase 1 — `crates/services/search.rs:184`
  Research synthesis asks for JSON but accepts arbitrary text as summary.
  Fix by making the prompt plain text or recording parse failures explicitly.

- Phase 1 — `crates/vector/ops/commands/query.rs:80`
  Paginated query ranks restart at 1 and `chunk_index` is always null.
  Fix by using absolute rank and returning payload chunk index.

- Phase 2 — `crates/vector/ops/tei/tei_client.rs:92`
  TEI retry logs include the full TEI URL.
  Fix by redacting URL credentials or logging host-only endpoints.

- Phase 2 — `crates/services/search.rs:337`
  Search/research logs can include full user queries.
  Fix by logging query length/hash by default and redacting token-like substrings.

- Phase 2 — `crates/ingest/sessions/claude.rs:111`
  Session ingest reads full JSONL files and lacks visible redaction.
  Fix by streaming parsing, adding size caps, and redacting common secret patterns before embedding.

## Findings by Category

### Architecture and Code Quality

- Evaluate has read/write responsibility leakage and direct job backend coupling.
- Judge reference retrieval duplicates ask retrieval logic and can drift.
- Context assembly does not coordinate source selection across tiers.
- Vector-mode cache identity is too narrow for mixed endpoint processes.

### Security

- Retrieved context and web snippets need explicit untrusted-source treatment.
- Evaluate's model-driven auto-enqueue behavior should not trigger crawling implicitly.
- Judge reference retrieval should honor the same source restrictions as ask.

### Performance

- Structured evaluate serializes two independent LLM answer calls.
- Full-doc retrieval can fetch substantially more content than the remaining context budget can use.

### Testing

- Missing regressions for side-effect-free evaluate, prompt source isolation, judge-reference filter parity, source deduplication, and endpoint-aware vector-mode caching.

### Documentation

- Context injection docs are stale for query instruction, dual embedding, defaults, and evaluate concurrency.

### Standards and Operations

- The services-first contract needs enforcement for RAG command code.
- Config-default docs need an ownership/checking mechanism.

## Recommended Fix Order

1. Lock down MCP embed local-file access and redact ACP WebSocket token logging.
2. Remove implicit crawl enqueue side effects from evaluate and remove direct job imports from vector command code.
3. Harden ask/evaluate/research prompt construction for untrusted retrieved source text.
4. Centralize retrieval/filtering and align judge-reference filtering with ask retrieval.
5. Make evaluate judge the normalized production answer and run RAG/baseline answers concurrently.
6. Deduplicate context source selection, preserve date filters during full-doc expansion, and key vector-mode cache by endpoint plus collection.
7. Tighten typed service contracts, suggestion metadata, date-filter validation, query result schema, docs, and focused regression tests.

## Residual Risks

- This review did not run the full test suite before findings were written.
- The worktree had pre-existing unrelated changes before review artifacts were created.
- Two read-only review agents were still running when this report was written; their later findings should be reconciled if they produce additional unique issues.
