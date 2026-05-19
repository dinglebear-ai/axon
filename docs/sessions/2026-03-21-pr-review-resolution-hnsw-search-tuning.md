# Session: PR Review Resolution + HNSW Search Tuning
Date: 2026-03-21
Branch: feat/pulse-shell-and-hybrid-search
Version bump: 0.30.0 → 0.30.1

---

## Session Overview

Continued from a context-compacted previous session. The prior session had already addressed 18 PR review threads (commit `bfc4654a`) but `verify_resolution.py` found 7 additional unresolved threads. This session:

1. Completed the in-progress frontend fix (`ws-handler.ts` → `ws-schemas.ts` split) that had failed the monolith pre-commit hook
2. Committed all 7 remaining thread fixes
3. Marked all 7 threads resolved via GitHub API
4. Verified all 147 PR review threads are resolved
5. Applied a follow-up fix: HNSW/quantization params on the wrong arm of the hybrid search query
6. Bumped version to 0.30.1 and pushed

---

## Timeline

| Time | Activity |
|------|----------|
| Session start | Resumed from compacted context; `ws-handler.ts` was 556 lines (limit 500); `ws-schemas.ts` had been created but `ws-handler.ts` not yet updated |
| ~T+5m | Updated `ws-handler.ts` to import from `ws-schemas.ts`, removed inline schema block; file dropped to 440 lines |
| ~T+10m | Staged all 7 thread fix files and committed (`0a30c876` absorbed changes) |
| ~T+12m | Ran `mark_resolved.py` for all 7 thread IDs; all 7 resolved |
| ~T+13m | Ran `verify_resolution.py` → `✓ 147 thread(s) resolved or outdated` |
| ~T+14m | Pushed branch |
| ~T+20m | `/quick-push` on unstaged changes: HNSW params placement fix + new tests |
| ~T+25m | Version bump 0.30.0 → 0.30.1, CHANGELOG updated, committed `476ab832` |
| ~T+27m | Pushed; all pre-commit hooks passed (1483 tests, clippy clean) |

---

## Key Findings

- **HNSW params were on wrong search arm**: `hnsw_ef` and `quantization` rescore params were at the top-level of the `/points/query` fusion body. The fusion stage performs no HNSW traversal — params must be on the `dense` prefetch arm. Fixed in `hybrid.rs`.
- **`synthesis_delta` not handled in frontend**: The WS message type was emitted by the Rust bridge but had no handler in `ws-handler.ts`. Added handler + schema. File grew to 556 lines, requiring extraction to `ws-schemas.ts`.
- **`ws-handler.ts` → `ws-schemas.ts` split**: 132-line Zod schema block extracted to a new file. `ws-handler.ts` went from 556 → 440 lines.
- **`dispatch_vector_search` missing in evaluate**: `scoring.rs` was calling the raw `qdrant_search` (dense-only) instead of the hybrid-aware dispatch path. Fixed to match `query`/`ask` behavior.
- **`anyhow!(e.to_string())` anti-pattern**: Multiple sites in `hybrid.rs` and `search.rs` were converting errors to strings (losing error chain). Replaced with `inspect_err` + `?` and `anyhow::Error::from(e)`.

---

## Technical Decisions

- **Schema extraction over suppression**: Rather than bumping the monolith limit or adding an allowlist entry, extracted all Zod schemas to `ws-schemas.ts`. This is the right move — schemas are a stable, separate concern.
- **`inspect_err` over `map_err` + discard**: Using `.inspect_err(|e| log_warn(...))?.` instead of `.map_err(|e| { log_warn(...); anyhow!(e.to_string()) })?` preserves the original error chain and avoids a gratuitous string allocation.
- **Prefetch arm vs top-level params**: Qdrant's `/points/query` fusion stage fuses pre-ranked lists; it doesn't re-traverse the HNSW graph. Setting `hnsw_ef` at the top level is silently ignored. The fix puts it where HNSW traversal actually happens.

---

## Files Modified

| File | Change | Purpose |
|------|--------|---------|
| `apps/web/hooks/use-axon-acp/ws-handler.ts` | Replaced inline schema block with import from `ws-schemas.ts`; added `synthesis_delta` handler | Thread F — frontend synthesis_delta; monolith fix |
| `apps/web/hooks/use-axon-acp/ws-schemas.ts` | Created (new file, 139 lines) | Extracted Zod schemas; enables ws-handler.ts to stay under 500 lines |
| `crates/cli/commands/research.rs` | `consumer.await` → `tokio::time::timeout(5s, consumer).await` | Thread B — prevent unbounded wait |
| `crates/services/acp_llm.rs` | Wrap completion loop with 300s timeout; drain `result_rx` on channel close | Threads D+E — timeout + error surfacing |
| `crates/services/search.rs` | `warm_session` errors → `Option<WarmAcpSession>` (degraded path); `synthesize_warm` accepts `Option` | Thread C — graceful degradation |
| `crates/vector/ops/input.rs` | Doc comment: "200–2000" → "500–2000" characters | Thread G — doc accuracy |
| `crates/vector/ops/tei/prepare.rs` | Guard `chunk_markdown` against control chars | Thread A — prevent MarkdownSplitter panic |
| `crates/vector/ops/commands/evaluate/scoring.rs` | Use `dispatch_vector_search` instead of `qdrant_search` | Hybrid-aware dispatch in evaluate |
| `crates/vector/ops/qdrant.rs` | Remove `pub(crate) use search::qdrant_search` (no longer exported directly) | Encapsulation |
| `crates/vector/ops/qdrant/hybrid.rs` | Move `hnsw_ef`+quantization to dense prefetch arm; `inspect_err`; fix test assertion | Correct HNSW param placement |
| `crates/vector/ops/qdrant/search.rs` | `inspect_err` pattern; 2 new tests (filter propagation, oversampling) | Error handling + coverage |
| `Cargo.toml` | 0.30.0 → 0.30.1 | Patch version bump |
| `CHANGELOG.md` | Added [0.30.1] section | Document commits |

---

## Commands Executed

```bash
# Count lines after schema extraction
wc -l apps/web/hooks/use-axon-acp/ws-handler.ts
# → 440 (was 557, limit 500)

# Mark all 7 remaining threads resolved
python3 ~/.claude/skills/gh-address-comments/scripts/mark_resolved.py \
  PRRT_kwDORS2O8s5161C9 PRRT_kwDORS2O8s5161C- PRRT_kwDORS2O8s5161C_ \
  PRRT_kwDORS2O8s5161DA PRRT_kwDORS2O8s5161DB PRRT_kwDORS2O8s5161DC \
  PRRT_kwDORS2O8s5161DD
# → Resolved 7/7 threads

# Verify complete resolution
python3 ~/.claude/skills/gh-address-comments/scripts/fetch_comments.py | \
  python3 ~/.claude/skills/gh-address-comments/scripts/verify_resolution.py
# → ✓ 147 thread(s) resolved or outdated

# Pre-commit hook results (commit 476ab832)
# 1483 tests: all passed
# clippy: clean
# monolith: passed (warnings only, no hard failures)
```

---

## Behavior Changes (Before/After)

| Area | Before | After |
|------|--------|-------|
| Hybrid search params | `hnsw_ef`/quantization at fusion top-level (ignored by Qdrant) | On dense prefetch arm where HNSW traversal occurs |
| Frontend `synthesis_delta` | Silently dropped — no handler in `ws-handler.ts` | Appended to `pendingDeltaRef`, flushed as assistant text |
| ACP warm session failure | Hard error propagated up, aborted research | Degrades gracefully — synthesis skipped, search results returned |
| Research consumer task | Unbounded wait on event consumer after `research()` returns | 5s timeout; bounded, non-blocking |
| ACP completion loop | No timeout | 300s timeout; surfaces timeout as error |
| `MarkdownSplitter` on control chars | Potential panic | Falls back to `chunk_text` if control chars detected |
| Evaluate hybrid dispatch | Always used dense-only `qdrant_search` | Uses `dispatch_vector_search` (hybrid-aware) |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `verify_resolution.py` | 0 unresolved threads | `✓ 147 thread(s) resolved or outdated` | PASS |
| `cargo test --lib` | 1483 passing | 1483 passed; 0 failed | PASS |
| `cargo clippy --lib` | 0 errors | Finished with no errors | PASS |
| `wc -l ws-handler.ts` | < 500 lines | 440 lines | PASS |
| Pre-commit hook (476ab832) | All hooks pass | All 12 hooks ✔️ | PASS |

---

## Source IDs + Collections Touched

No Axon embed/retrieve operations were performed during this session (pure code + PR review work).

---

## Risks and Rollback

- **HNSW param placement**: If Qdrant's behavior with params on the prefetch arm differs from expectations, revert `hybrid.rs` to put params at top-level. Risk is low — this is the documented correct placement per Qdrant hybrid search docs.
- **`synthesize_warm` degraded path**: Research will silently skip synthesis if ACP warm session fails. The caller receives search results without the LLM summary. This is intentional — better partial results than a hard error.
- **Rollback**: `git revert 476ab832` (quick-push commit) or `git revert 0a30c876` (PR thread fixes + HNSW config).

---

## Decisions Not Taken

- **Monolith allowlist for `ws-handler.ts`**: Rejected — extracting schemas is the right fix, not suppressing the check.
- **Hard-error on ACP warm session failure**: Rejected — synthesis is a value-add on top of search results; network/process failures shouldn't deny the user their search results.
- **Top-level `hnsw_ef` with a note**: Rejected — the param is silently ignored at the fusion stage, so leaving it there would be misleading and ineffective.

---

## Open Questions

- Whether the `synthesis_delta` handler needs to also set `streamingIdRef` if none is active (currently it guards `!refs.streamingIdRef.current` and silently drops the delta).
- Whether Qdrant's INT8 quantization rescore with `oversampling: 1.5` on the prefetch arm is being applied correctly (no benchmark yet).

---

## Next Steps

- Open a PR or request re-review now that all 147 threads are resolved.
- Benchmark hybrid search quality with HNSW params on the prefetch arm vs. before.
- Consider adding a `synthesis_delta` integration test to the research command.
