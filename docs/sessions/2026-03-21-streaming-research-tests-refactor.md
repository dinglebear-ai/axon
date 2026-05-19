# Session: Streaming Research Synthesis, Quality Tests, and Refactors
**Date:** 2026-03-21
**Branch:** feat/pulse-shell-and-hybrid-search
**Commit:** 758837ce
**Version:** 0.29.0 → 0.30.0

---

## Session Overview

Continued from the previous session (which fixed hybrid search and added Tier 1 embedding). This session:

1. **Locked in hybrid search behavior with tests** — 3 `QdrantQueryResponse` deserialization tests, 1 `prepend_query_instruction` test, 4 `chunk_markdown` property tests
2. **Added streaming research synthesis** — `ServiceEvent::SynthesisDelta` events stream LLM tokens to CLI stderr and the web WS layer in real time
3. **ACP eager warm-up** — `spawn_eager` starts adapter cold-start concurrently with Tavily search to hide latency
4. **Refactored duplicate query instruction format** — extracted `prepend_query_instruction()` helper used at 3 call sites
5. **Efficiency and quality cleanup** — two `/simplify` passes eliminated dead renames, redundant assertions, a full `Value` deserialization, and a `format!()` allocation per loop iteration

---

## Timeline

| Time | Activity |
|------|----------|
| Start | Resumed from prior context: task was "lock this behavior in with some tests" |
| Phase 1 | Audited existing test coverage; identified 3 gaps (QdrantQueryResponse, prepend logic, chunk_markdown proptests) |
| Phase 2 | Wrote tests: 3 deserialization tests in `types.rs`, 1 prepend test in `query.rs`, 4 proptests in `input_proptest.rs` |
| Phase 3 | First `/simplify` pass — agents found: reuse opp (`prepend_query_instruction`), quality issues (duplicate comments, redundant assertion, `tx_for_deltas`), efficiency issues (`try_send` silent drop, `format!()` in loop) |
| Phase 4 | Applied fixes from first pass; second `/simplify` pass — agents found test calling raw `format!` instead of the helper, `parse_synthesis_response` using full `Value` deser |
| Phase 5 | Applied fixes from second pass |
| Phase 6 | `/quick-push` — version bump, CHANGELOG, commit, push |

---

## Key Findings

- **`/points/query` vs `/points/search` response shapes**: `/points/search` → `{"result":[...]}` (flat array); `/points/query` → `{"result":{"points":[...]}}` (nested). This bug silently killed all hybrid RRF search since introduction. Fixed in prior session; tests written this session to prevent regression (`crates/vector/ops/qdrant/types.rs:55–91`).
- **`MarkdownSplitter` panics on Unicode control characters**: Proptest's `.{0,10000}` regex generates arbitrary Unicode including chars like `\x00`–`\x1F` that cause panics inside `text-splitter`'s regex engine. Solution: `markdown_safe_input()` strategy constrained to printable ASCII + `\n` + `\t` (`input_proptest.rs:218`).
- **`ChunkConfig::new(200..2000).with_overlap(200)` panics**: overlap must be strictly less than min chunk size. Fixed in prior session to `500..2000` with overlap 200.
- **`try_send` silent drop**: Synthesis delta `try_send` was silently discarding tokens when channel full. Now logs via `log_warn` (`search.rs:291`).
- **`format!()` in loop allocates per iteration**: `push_str(&format!(...))` creates a temp `String` per extraction. Fixed to `write!(context, ...)` (`search.rs:313`).

---

## Technical Decisions

- **`markdown_safe_input()` uses `prop::char::ranges(vec![...])` not `is_ascii_graphic()`**: proptest has no built-in for "printable ASCII + newline + tab"; `ranges()` is the documented approach. Self-explanatory range literals (`' '..='~'`, `'\n'..='\n'`) retained with inline comments for ASCII table clarity.
- **`QUERY_INSTRUCTION` re-exported as `#[cfg(test)]` only**: After extracting `prepend_query_instruction()`, the constant is only needed in tests. `#[cfg(test)] pub(crate) use tei_client::QUERY_INSTRUCTION` in `tei.rs` avoids exposing it in production builds while keeping it accessible from `query.rs` tests via the module path.
- **Typed struct in `parse_synthesis_response`**: Local `#[derive(serde::Deserialize)] struct SynthesisJson { summary: String }` tells serde to skip all other JSON fields — avoids full `Value` allocation when only one field is needed.
- **`spawn_eager` vs `spawn`**: `spawn` defers adapter setup to first `run_turn`; `spawn_eager` starts setup immediately so the cold-start (subprocess spawn → init → session setup) overlaps with the Tavily search. Channel capacity 16 queues turns received before setup completes.
- **`try_send` retained (not `send().await`)**: The streaming callback runs inside the ACP response handler — blocking is not an option. `try_send` with logging is the correct pattern; channel capacity 256 handles typical synthesis bursts (~200–500 events).

---

## Files Modified

| File | Change |
|------|--------|
| `crates/vector/ops/qdrant/types.rs` | +3 deserialization tests for `QdrantQueryResponse` nested shape |
| `crates/vector/ops/commands/query.rs` | +2 tests for `QUERY_INSTRUCTION`; call site → `prepend_query_instruction()` |
| `crates/vector/ops/commands/evaluate/scoring.rs` | Call site → `prepend_query_instruction()` |
| `crates/vector/ops/commands/ask/context/retrieval.rs` | Call site → `prepend_query_instruction()` |
| `crates/vector/ops/tei/tei_client.rs` | +`prepend_query_instruction()` helper |
| `crates/vector/ops/tei.rs` | Re-export `prepend_query_instruction`; `#[cfg(test)]` guard on `QUERY_INSTRUCTION` |
| `crates/vector/ops/input_proptest.rs` | +4 `chunk_markdown` property tests + `markdown_safe_input()` strategy |
| `crates/services/events.rs` | +`ServiceEvent::SynthesisDelta { text: String }` variant |
| `crates/services/search.rs` | `synthesize_warm()` with streaming; `build_synthesis_context()` with `write!()`; `parse_synthesis_response()` with typed struct; removed `tx_for_deltas` rename |
| `crates/services/acp/persistent_conn.rs` | +`spawn_eager()` + `adapter_loop_eager()` |
| `crates/cli/commands/research.rs` | Event consumer loop for `SynthesisDelta` + phase log events; removed `AtomicBool` ticker |
| `crates/web/execute/sync_mode/pulse_chat/events.rs` | +`SynthesisDelta` arm → `send_json_owned` WS frame |
| `Cargo.toml` | Version 0.29.0 → 0.30.0 |
| `CHANGELOG.md` | v0.30.0 entry |

---

## Commands Executed

```bash
# Verify test coverage before writing
cargo test --lib chunk_markdown      # 8 tests pass
cargo test --lib qdrant_query_response  # 3 tests pass
cargo test --lib query_instruction   # 2 tests pass

# Full gate after each simplify pass
just verify   # fmt-check + clippy + check + test — all green

# Version bump confirmation
cargo check   # Checking axon v0.30.0 — Finished in 17.54s

# Push
git push      # dd6a7b68..758837ce  feat/pulse-shell-and-hybrid-search
```

---

## Behavior Changes (Before/After)

| Behavior | Before | After |
|----------|--------|-------|
| Research command output | Printed periodic "in_progress elapsed_ms=N" lines every 2s | Prints `phase:searching` and `phase:synthesizing results=N` markers, then streams LLM tokens inline to stderr |
| ACP adapter startup during research | Cold-start happened after Tavily search completed | Cold-start begins immediately, overlaps with search |
| Hybrid search (named collections) | Silent deserialization failure — returned 0 results | Correctly parses `{"result":{"points":[]}}` and returns hits |
| `synthesis_delta` channel full | Tokens silently dropped | Tokens dropped with `log_warn("synthesis_delta dropped: …")` |
| `build_synthesis_context` | One temp `String` allocation per extraction via `push_str(&format!())` | Zero temp allocations — `write!(context, ...)` appends directly |
| Query instruction prepend | 3 identical `format!("{}{query}", QUERY_INSTRUCTION)` sites | Single `prepend_query_instruction(query)` call at each site |
| Web research synthesis | No streaming; client got complete response only | Receives `{"type":"synthesis_delta","text":"..."}` WS frames per token |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo test --lib chunk_markdown` | 8 tests pass | 8 passed, 0 failed | ✅ |
| `cargo test --lib qdrant_query_response` | 3 tests pass | 3 passed, 0 failed | ✅ |
| `cargo test --lib query_instruction` | 2 tests pass | 2 passed, 0 failed | ✅ |
| `just verify` (first simplify pass) | All green | All green, 0 errors | ✅ |
| `just verify` (second simplify pass) | All green | All green, 0 errors | ✅ |
| `cargo check` after version bump | `Checking axon v0.30.0` | `Finished dev profile` | ✅ |
| `git push` | Pushed to remote | `dd6a7b68..758837ce` | ✅ |

---

## Source IDs + Collections Touched

No Axon embed/retrieve operations performed during this session (session was code/test work only).

---

## Risks and Rollback

- **`spawn_eager` failure path**: if `establish_acp_session` fails, queued `RunTurn` messages are drained and failed via `try_recv`. Subsequent `run_turn` calls see a closed-channel error. Rollback: revert `persistent_conn.rs` and `search.rs` to use `spawn` + `complete_text`.
- **`SynthesisDelta` token drops**: `try_send` drops tokens when channel at capacity (256). Risk is cosmetic (incomplete streaming output) — underlying synthesis still completes. Channel sized for typical ~200–500 token bursts; overflow only under heavy consumer lag.
- **`#[cfg(test)]` re-export of `QUERY_INSTRUCTION`**: if a non-test caller needs the constant in the future, the `#[cfg(test)]` guard must be removed from `tei.rs`. Low risk — `prepend_query_instruction()` covers all known call sites.

---

## Decisions Not Taken

- **Content-preservation proptest for `chunk_markdown`**: `chunk_text` has a reassembly test (skip OVERLAP chars from each subsequent chunk). `chunk_markdown` splits on semantic boundaries with no fixed overlap — the skip trick doesn't apply. Omitted.
- **`extraction_fields()` helper**: Agents flagged shared field-access pattern between `build_synthesis_context` and `fallback_summary_from_extractions`. Rejected — the defaults differ (`""` vs `"untitled"` for title) and `fallback` doesn't use `url`, so a common helper would need different defaults per caller, adding indirection without clarity.
- **`send().await` for `SynthesisDelta`**: Using blocking send in the streaming callback would deadlock (callback runs inside response handler). `try_send` with logging is correct.
- **Full `Value` deserialization preserved**: One agent suggested `StreamDeserializer` or a JSON pointer crate for `parse_synthesis_response`. Rejected — the local typed struct approach is simpler and achieves the same skip-unwanted-fields benefit without adding a new dependency.

---

## Open Questions

- **Bulk reindex**: The `axon` collection has ~26 test points. The `cortex` collection (~7M points) was embedded with the wrong instruction prefix (no `QUERY_INSTRUCTION`). Re-embedding into `axon` using `chunk_markdown` + correct asymmetric encoding would improve search quality.
- **`chunk_text("")` fast-path**: Returns `vec![""]` (one empty string) instead of `vec![]`. The `pipeline.rs` `retain` guard catches it, but the behavior is surprising. No fix applied — behavior change would affect all callers.
- **Live `axon ask` end-to-end verification**: `prepend_query_instruction` in `retrieval.rs` and `scoring.rs` is unit-tested but not validated against a live collection with correct embeddings.
- **`research_payload` function length**: Pre-commit hook warned `research_payload()` is 102 lines (warning threshold 80, limit 120). Not split — still under hard limit.

---

## Next Steps

1. Start bulk reindex: `axon embed` all previously indexed content into `axon` collection with correct `chunk_markdown` + `QUERY_INSTRUCTION` pipeline
2. Verify `axon ask` end-to-end against `axon` collection
3. Consider splitting `research_payload()` if it grows past 120 lines
4. PR: merge `feat/pulse-shell-and-hybrid-search` → `main`
