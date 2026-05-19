# Session: Tier 2 Qdrant Index Configuration + Review

**Date:** 2026-03-21
**Branch:** `feat/pulse-shell-and-hybrid-search`
**Plan:** `docs/superpowers/plans/2026-03-19-qdrant-index-config-tier2.md`

---

## Session Overview

Completed the Tier 2 Qdrant index configuration plan (Tasks 2–4, Task 1 was done prior session), then dispatched four parallel review agents (comprehensive-review, superpowers, rust-reviewer, systems-programming:rust-pro) with `/rust-code-review`, `/rust-best-practices`, and `/rust-async-patterns` skills. Agents found and fixed several real bugs — most critically a search routing bug in `evaluate/scoring.rs` and incorrect placement of `params` inside hybrid search prefetch arms.

Total commits this session: **7** (4 plan + 3 agent fixes + 1 doc fix)

---

## Timeline

| Time | Activity |
|------|----------|
| Session start | Resumed from prior context — Task 2 in progress (qdrant_search extraction) |
| ~T+5m | Fixed compile error: `commands.rs` still called `super::client::qdrant_search` after extraction |
| ~T+10m | Fixed failing test: `json_body_includes` pattern needed `{"params":{"quantization":...}}` not `{"quantization":...}` |
| ~T+15m | Committed Task 2 (extraction + search.rs monolith fix) |
| ~T+20m | Task 3: Added hnsw_ef + quantization params to hybrid.rs (TDD: 3 red → green) |
| ~T+30m | Task 4: `just verify` clean, `.env.example` updated with AXON_HNSW_EF_SEARCH vars |
| ~T+45m | Dispatched 4 review agents in parallel |
| ~T+60m | Agents returned findings; all issues committed (41b3d799, 476ab832, 58ff8c4e) |
| ~T+65m | Fixed stale CLAUDE.md doc (OnceLock<Mutex> → OnceLock<RwLock>) |

---

## Key Findings

1. **params placement bug in hybrid search** (`hybrid.rs:39-63`): `hnsw_ef` and `quantization` params were placed at the top-level `/points/query` body. HNSW traversal happens during prefetch, not at RRF fusion — correct placement is inside the dense prefetch arm object only. Sparse BM42 arm has no HNSW index and correctly gets no params.

2. **evaluate routing bug** (`evaluate/scoring.rs:17`): `build_judge_reference` was calling `qdrant::qdrant_search()` directly (legacy `/points/search` with flat `"vector"` field). Qdrant rejects that for Named-mode collections with a 400 error. Fixed to `qdrant::dispatch_vector_search()` — matches `query` and `ask` command behavior and correctly routes to hybrid/named-dense for new collections.

3. **Dead re-export** (`qdrant.rs`): After `dispatch_vector_search` was the only external caller, `pub(crate) use search::qdrant_search` at the module level was dead and would break CI under `-D warnings`. Removed.

4. **Error chain flattening** (`search.rs`, `hybrid.rs`): `.map_err(|e| anyhow!(e.to_string()))` serialized `reqwest::Error` to string and discarded URL, status code, timeout flag, and backtrace. Fixed to `.inspect_err(|e| log_warn(...))? ` for transport errors and `anyhow::Error::from(e)` for status errors.

5. **Stale doc** (`crates/vector/CLAUDE.md:53`): Said `OnceLock<Mutex<HashMap>>` — implementation uses `OnceLock<RwLock<HashMap>>` since the full-review refactor. Corrected.

---

## Technical Decisions

- **`params` inside dense prefetch arm (not top-level for hybrid)**: The Qdrant `/points/query` schema accepts `params` at both levels, but HNSW traversal only happens during each prefetch arm's ANN search. Top-level params apply to the final fusion/reranking stage which does pure arithmetic, not graph traversal — placing `hnsw_ef` there was semantically meaningless.

- **`hnsw_ef` default 128 for named-mode vs 64 for legacy**: Named collections use quantization with `rescore: true`, which oversamples 1.5× and then rescores against full-precision vectors. Larger initial candidate window costs little given the reranking budget. Legacy unnamed collections match prior behavior at 64.

- **`env_usize_clamped()` called per-request (not cached)**: Sub-microsecond vs network RTT; adds operational flexibility for live tuning without restart. No measurable impact, no need for LazyLock caching.

- **`dispatch_vector_search()` in evaluate**: Makes `evaluate` route identically to `query`/`ask`. judge-reference retrieval now gets hybrid RRF for Named collections — more relevant reference chunks → more accurate LLM judge scores.

---

## Files Modified

| File | Change | Purpose |
|------|--------|---------|
| `crates/vector/ops/qdrant/search.rs` | **CREATED** | Extracted `qdrant_search()` from `client.rs`; added `hnsw_ef` + quantization params; 5 tests |
| `crates/vector/ops/qdrant/client.rs` | Removed `qdrant_search()`, removed unused `log_debug` import | Restore monolith budget (517→461 lines) |
| `crates/vector/ops/qdrant.rs` | Added `mod search;`; removed dead `pub(crate) use search::qdrant_search` re-export | Module wiring |
| `crates/vector/ops/qdrant/commands.rs` | Updated call site `super::client::qdrant_search` → `super::search::qdrant_search` | Compile fix |
| `crates/vector/ops/qdrant/tests.rs` | Updated import for `qdrant_search` to `super::search` | Import fix |
| `crates/vector/ops/qdrant/hybrid.rs` | Added `hnsw_ef` + quantization to both functions; moved params to dense prefetch arm; `inspect_err` + `anyhow::Error::from`; 3 new tests | Tier 2 + review fixes |
| `crates/vector/ops/tei/qdrant_store.rs` | Added HNSW config + INT8 quantization to `ensure_collection()` create path | Tier 2 Task 1 (prior session) |
| `crates/vector/ops/tei/qdrant_store/tests.rs` | 4 new tests for collection creation shape | Task 1 coverage |
| `crates/vector/ops/commands/evaluate/scoring.rs` | `qdrant_search()` → `dispatch_vector_search()` | Runtime fix for Named collections |
| `crates/core/config/types/config.rs` | Added `ask_hybrid_candidates: usize` field (default 150) | Agent addition |
| `.env.example` | Added `AXON_HNSW_EF_SEARCH=128` and `AXON_HNSW_EF_SEARCH_LEGACY=64` | Task 4 documentation |
| `crates/vector/CLAUDE.md` | Fixed `OnceLock<Mutex>` → `OnceLock<RwLock>` | Doc accuracy |

---

## Commands Executed

```bash
# Compile verification after extraction
cargo check -p axon

# TDD red phase — confirmed 3 failing tests before implementation
cargo test -p axon -- qdrant_hybrid_search_sends_hnsw qdrant_named_dense_search_sends_hnsw qdrant_named_dense_search_sends_quantization

# Full verification gate
just verify  # fmt-check + clippy + check + test → 1480 passed, 0 failed

# Post-review verification
cargo test -p axon --lib  # 1472 passed, 0 failed, 11 ignored
cargo clippy -p axon       # 0 warnings
cargo fmt --check          # clean
```

---

## Behavior Changes (Before/After)

| Surface | Before | After |
|---------|--------|-------|
| New collection creation | No HNSW/quantization config sent | Creates with `m=32, ef_construct=256, int8 scalar, quantile=0.99, always_ram=true` |
| Legacy `/points/search` | No search params | Sends `hnsw_ef=64` (env-tunable), `rescore=true, oversampling=1.5` |
| Hybrid `/points/query` | No search params; params would have been at wrong level | `hnsw_ef=128` on dense prefetch arm only; sparse arm unchanged |
| Named-dense `/points/query` | No search params | `hnsw_ef=128, rescore=true, oversampling=1.5` at top level |
| `evaluate` judge-reference | Direct `qdrant_search()` — silently fails for Named collections | `dispatch_vector_search()` — hybrid for Named, legacy for Unnamed |
| Error messages from search | `reqwest::Error` flattened to string | Full error chain preserved (URL, status, timeout flag) |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check -p axon` | 0 errors | 0 errors | ✅ |
| `cargo test -p axon --lib` | All pass | 1472 passed, 0 failed, 11 ignored | ✅ |
| `cargo clippy -p axon` | 0 warnings | 0 warnings | ✅ |
| `cargo fmt --check` | Clean | Clean | ✅ |
| `just verify` | All gates pass | All pass (1480 tests at gate time) | ✅ |
| Monolith check (pre-commit hook) | All files ≤500 lines | `client.rs` 461L, `search.rs` 182L, `hybrid.rs` 440L | ✅ |

---

## Source IDs + Collections Touched

No Axon embed/retrieve operations were performed during this session (pure code implementation session).

---

## Risks and Rollback

- **New collections only**: HNSW/quantization config only applies at `ensure_collection()` create time (GET-first pattern). Existing `cortex` collection (Named, 7M+ points) is unaffected — it was created before this change. Re-index into a new collection to get the new config.
- **`evaluate` routing change**: `dispatch_vector_search` now calls TEI for sparse vector computation on Named collections. If TEI is unavailable, `evaluate` will fail rather than fall back to dense-only. Acceptable — dense fallback would produce misleading judge scores.
- **Rollback**: `git revert` any of the 7 commits is safe. The plan commits are independent of the agent fix commits.

---

## Decisions Not Taken

- **Caching `hnsw_ef` in a `LazyLock<usize>`**: Adds complexity for immeasurable gain (env read is sub-µs vs network RTT). Rejected.
- **Adding `params` to sparse BM42 prefetch arm**: BM42 is an inverted-index lookup with no HNSW graph traversal and no quantization. `hnsw_ef` and `quantization.rescore` would be silently ignored by Qdrant on the sparse arm. Rejected.
- **Returning `Err` on 404 in `get_or_fetch_vector_mode`**: Already implemented (explicit 404 handling exists). Not changed.
- **Per-request `reqwest::Client::new()`**: Project standard is the shared `http_client()` singleton. Not changed.

---

## Open Questions

- Does the `ask_hybrid_candidates: usize` field added by an agent (commit `58ff8c4e`) conflict with any existing `hybrid_search_candidates` config field? Both may exist with similar semantics — needs audit.
- `cortex` collection (7M+ points, Named mode from prior migration) does not have HNSW m=32/ef_construct=256 or INT8 quantization — only new collections created after this PR get those. Is a collection update API call needed, or is re-index into `cortex_v3` the intended path?

---

## Next Steps

1. Audit `ask_hybrid_candidates` vs `hybrid_search_candidates` config fields for duplication
2. Push branch and open PR against `main`
3. After merge, update `AXON_COLLECTION` in `.env` to trigger new collection creation with Tier 2 config on next embed
4. Consider `axon migrate` to copy existing `cortex` → `cortex_v2` with named vectors if not already done
