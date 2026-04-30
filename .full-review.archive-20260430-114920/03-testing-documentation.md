# Phase 3: Testing and Documentation

## Findings

- High — `crates/vector/ops/commands/evaluate.rs:144`
  There is no regression test proving that evaluate is read-only by default.
  Impact: future changes can keep or reintroduce implicit crawl job creation in a diagnostic path without failing tests.
  Fix: add a unit/service test that RAG underperformance produces suggestions but does not enqueue crawl jobs unless an explicit opt-in is present.

- High — `crates/mcp/server/handlers_embed_ingest.rs:67`
  There is no apparent MCP embed security regression test covering local path restrictions, secret-file denial, symlink escape, and large-file caps.
  Impact: a future MCP embed change can expose local files into the RAG index without test coverage.
  Fix: add handler/prep tests for URL/text-only defaults, explicit allowed roots, canonicalization, symlink escape rejection, and size limits.

- High — `crates/vector/ops/commands/streaming.rs:194`
  Prompt-injection handling is not covered for retrieved context or research snippets.
  Impact: changes to prompt templates can weaken source isolation without tests noticing.
  Fix: add tests around prompt construction that assert untrusted delimiters and anti-instruction language are present for ask/evaluate/research.

- High — `crates/vector/ops/commands/ask.rs:47`
  Evaluation correctness is not pinned to the production-normalized ask answer.
  Impact: evaluate can judge a raw answer that users never see, while tests still pass.
  Fix: add a test proving evaluate uses the same normalized/citation-gated answer shape as ask.

- Medium — `crates/vector/ops/commands/evaluate/scoring.rs:7`
  Judge-reference filtering parity with ask retrieval is not tested.
  Impact: evaluate can drift from ask retrieval semantics as tuning changes.
  Fix: add focused tests for low-signal URL rejection, authoritative allowlist rejection, and topical-overlap filtering in judge-reference candidate handling.

- Medium — `crates/vector/ops/commands/ask/context/build.rs:47`
  Context assembly tests do not lock down duplicate URL behavior between top chunks and full documents.
  Impact: context budget regressions are easy to miss because tests can pass while the model receives redundant source material.
  Fix: add a context-building test where the top chunk URL is also selected as a full doc and assert the URL appears only once or follows the intended replacement behavior.

- Medium — `docs/CONTEXT-INJECTION.md:36`
  The docs say the raw query string is sent to TEI, but the code prepends `QUERY_INSTRUCTION` and may run a second keyword embedding in `retrieve_ask_candidates()`.
  Impact: operators tuning models or debugging retrieval quality receive inaccurate guidance.
  Fix: update Stage 1 to describe asymmetric query instruction and dual query/keyword embedding.

- Medium — `docs/CONTEXT-INJECTION.md:110`
  Documented defaults are stale. The docs list `AXON_ASK_MIN_RELEVANCE_SCORE` as default `0.1`, but config uses `0.45`; later tables also list stale candidate/context defaults.
  Impact: users may lower quality accidentally or misinterpret why documents are filtered.
  Fix: align docs with `crates/core/config/parse/build_config.rs` and add a lightweight docs check or table source comment.

- Medium — `docs/CONTEXT-INJECTION.md:209`
  The docs say evaluate answer arms stream concurrently, but `evaluate_payload()` currently runs them sequentially in the service/CLI JSON path.
  Impact: performance expectations and troubleshooting guidance are wrong for the primary typed service path.
  Fix: either make the service path concurrent or document the actual behavior.

- Medium — `crates/core/config/cli.rs:224`
  `evaluate --responses-mode` is parsed, but the typed service path forces JSON mode and never reaches the side-by-side/streaming renderer.
  Impact: the CLI flag is effectively dead for the current command path.
  Fix: route CLI evaluate through the streaming renderer or remove/defer the flag until it is wired.

## Critical Issues for Phase 4 Context

- The highest-value regression suite is targeted and mostly unit-level: prompt construction, side-effect-free evaluate, judge-reference filtering, cache-key identity, and context deduplication.
- Docs drift is already visible in current RAG docs, so standards review should include a rule for config-default documentation ownership.
