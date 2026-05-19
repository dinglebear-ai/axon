# YouTube Playlist Ingest: Resume + Concurrency
**Date**: 2026-03-09
**Branch**: `refactor/acp-performance-modern-rust`
**Session type**: Feature implementation from written plan

---

## Session Overview

Implemented three improvements to YouTube playlist/channel ingestion:

1. **Resume support** — persists `completed_urls` in `result_json` so a killed/restarted ingest worker skips already-done videos instead of restarting from video 1.
2. **Concurrent processing** — replaced sequential loop with `FuturesUnordered` at N=5 concurrency, ~5× speedup.
3. **429 retry with backoff** — retries with 10s/20s/40s exponential backoff instead of silently skipping rate-limited videos.
4. **Polite per-request delay** — added `--sleep-requests 1` to the yt-dlp subprocess to reduce server-side rate-limit pressure across concurrent invocations.

Expected result: 278-video channel goes from ~23 min → ~3 min.

---

## Timeline

| Time | Activity |
|------|----------|
| Start | Read plan, reviewed `process.rs` and `youtube.rs` |
| Phase 1 | Added `--sleep-requests 1` to yt-dlp args in `youtube.rs` |
| Phase 2 | Rewrote `ingest_youtube_playlist` with `FuturesUnordered` + resume |
| Phase 3 | Fixed compile error: type unification — boxed futures with explicit `Pin<Box<dyn Future + Send>>` type |
| Phase 4 | Fixed `Send` bound error — `Box<dyn Error>` held across await; fixed with `.map_err(|e| e.to_string())` before match |
| Phase 5 | Fixed `unused_qualifications` warning — `std::future::Future` → `Future` |
| Verify | `cargo check` clean, 112 ingest tests pass |
| Attempt | Tried to re-index `@SpaceinvaderOne` channel via MCP — axon token expired |

---

## Key Findings

- **Sequential + 2s delay = ~23 min** for 278-video channel. Root cause: `PLAYLIST_VIDEO_DELAY` constant (2s blanket sleep) applied after every video, sequential loop.
- **`FuturesUnordered` type unification**: Two `push` call sites with different anonymous `async` block types — Rust can't unify them. Fix: `Pin<Box<dyn Future<Output=...> + Send>>` explicit type annotation with `Box::pin(...)` at both call sites.
- **Non-Send across await**: `Box<dyn Error>` (not `Send`) held across `tokio::time::sleep` await in the retry loop. Even explicit `drop(e)` doesn't satisfy the compiler's async state machine analysis. Fix: `.map_err(|e| e.to_string())` eagerly converts to `String` before the `match`, so no non-Send type ever exists in the state machine across an await.
- **`completed_urls` size**: ~45 chars × 278 URLs ≈ 12.5 KB in JSONB — well within PostgreSQL limits.
- **Final `mark_completed` overwrites `result_json`**: The resume `completed_urls` field is only needed during the run; `mark_completed` writes `{"chunks_embedded": N}` at job finish, which is correct behavior.

---

## Technical Decisions

| Decision | Rationale |
|----------|-----------|
| `FuturesUnordered` over `buffer_unordered` | More explicit control over refill logic; easier to combine with DB progress writes after each completion |
| `Box::pin` + explicit future type | Avoids creating a new helper function just to unify async block types; keeps all logic in one function |
| `.map_err(|e| e.to_string())` pattern | Converts non-Send `Box<dyn Error>` to `String` at the await boundary, cleanest fix without restructuring function |
| `PLAYLIST_CONCURRENCY = 5` | Conservative — 5 concurrent yt-dlp subprocesses reduce risk of YouTube IP-level rate limiting while giving ~5× speedup |
| `completed_urls` in `result_json` | Re-uses existing DB column (JSONB); no schema migration needed; field is ephemeral (overwritten at job completion) |
| `--sleep-requests 1` in yt-dlp | Polite 1s inter-request delay within each yt-dlp invocation; mitigates 429s when 5 concurrent processes are hitting YouTube |

**Alternative not taken**: `tokio::task::JoinSet` — requires owned `Config` clones anyway, and `FuturesUnordered` gives same semantics with slightly less boilerplate for this pattern.

---

## Files Modified

| File | Change | Line count |
|------|--------|-----------|
| `crates/ingest/youtube.rs` | Added `"--sleep-requests", "1"` to yt-dlp subprocess args (after `--no-warnings`) | 433 lines (unchanged) |
| `crates/jobs/ingest/process.rs` | Full rewrite: removed `PLAYLIST_VIDEO_DELAY`, added `load_playlist_progress`, `ingest_video_with_retry`, rewrote `ingest_youtube_playlist` with `FuturesUnordered` | 264 lines (was 152) |

---

## Commands Executed

```bash
# Verify compilation
cargo check
# → Clean (0 errors, 0 warnings after all fixes)

# Run ingest unit tests
cargo test ingest --lib
# → 112 passed; 0 failed

# Check file sizes (monolith policy: ≤500 lines)
wc -l crates/jobs/ingest/process.rs crates/ingest/youtube.rs
# → 264 + 433 — both within limit
```

---

## Behavior Changes (Before/After)

| Scenario | Before | After |
|----------|--------|-------|
| 278-video channel, avg 3s/video | ~23 min (sequential + 2s blanket delay) | ~3 min (N=5 concurrent, 1s per-request sleep) |
| Worker killed mid-run, restarted | Restarts from video 1, re-embeds everything | Loads `completed_urls` from DB, skips done videos |
| 429 from yt-dlp | Video silently skipped (lost) | Retried up to 3× with 10s/20s/40s backoff |
| Live progress display (`axon ingest status`) | `videos_done / videos_total, chunks_embedded` | Same — now updated after each concurrent completion |

---

## `result_json` Schema (resume-compatible)

```json
{
  "videos_done": 47,
  "videos_total": 278,
  "chunks_embedded": 312,
  "completed_urls": [
    "https://www.youtube.com/watch?v=abc123",
    "..."
  ]
}
```

On final `mark_completed`, this is overwritten with `{"chunks_embedded": N}` — correct, since the job is finished.

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check` | 0 errors, 0 warnings | 0 errors, 0 warnings | ✅ PASS |
| `cargo test ingest --lib` | All pass | 112 passed, 0 failed | ✅ PASS |
| `wc -l process.rs` | ≤500 lines | 264 lines | ✅ PASS |
| `wc -l youtube.rs` | ≤500 lines | 433 lines | ✅ PASS |
| MCP ingest `@SpaceinvaderOne` | Job queued | Token expired | ⏳ BLOCKED |

---

## Compile Errors Encountered and Fixed

### Error 1: Type Unification (`E0308`)
```
error[E0308]: mismatched types
  --> crates/jobs/ingest/process.rs:169:27
```
**Cause**: Two `inflight.push(async move { ... })` call sites produce different anonymous future types. `FuturesUnordered<_>` infers type from first push, rejects second.
**Fix**: Explicit type annotation `FuturesUnordered<Pin<Box<dyn Future<Output=(String, Result<usize,String>)> + Send>>>` with `Box::pin(...)` at both sites.

### Error 2: Send Bound (`future cannot be sent between threads safely`)
```
error: future cannot be sent between threads safely
note: has type `Box<dyn StdError>` which is not `Send`
note: await occurs here, with `e` maybe used later
```
**Cause**: `match ingest_youtube(...).await { Err(e) => { ... sleep(...).await } }` — `e: Box<dyn Error>` (not Send) exists in async state machine across the sleep await.
**Fix**: `.map_err(|e| e.to_string())` before the match arm — converts to `String` (Send) at the await boundary.

### Warning: Unused Qualification
```
warning: unnecessary qualification
  Box<dyn std::future::Future<...>>
```
**Fix**: Shortened to `Box<dyn Future<...>>` — `Future` is in scope via Rust 2021 prelude.

---

## Source IDs + Collections Touched

| Action | Source | Collection | Outcome |
|--------|--------|------------|---------|
| YouTube ingest `@SpaceinvaderOne` | `https://www.youtube.com/@SpaceinvaderOne` | `cortex` | ⏳ Blocked (MCP token expired) |

---

## Risks and Rollback

- **Risk**: N=5 concurrent yt-dlp processes could trigger IP-level YouTube rate limiting. Mitigated by `--sleep-requests 1`. If 429s persist, reduce `PLAYLIST_CONCURRENCY` to 2–3.
- **Rollback**: `git checkout crates/jobs/ingest/process.rs crates/ingest/youtube.rs` restores sequential behavior.
- **DB state**: `completed_urls` in `result_json` is purely additive; existing jobs are unaffected. No schema migration.

---

## Decisions Not Taken

| Alternative | Rejected because |
|-------------|-----------------|
| `tokio::task::JoinSet` | Same ownership requirements as `FuturesUnordered`, no meaningful advantage for this pattern |
| `buffer_unordered` stream adapter | Less control over per-completion DB write + refill; `FuturesUnordered` is more explicit |
| `drop(e)` before await | Insufficient — async state machine generator is more conservative than NLL; `.map_err` is the correct fix |
| Reduce concurrency to 3 | Kept at 5 with `--sleep-requests 1` as the primary rate-limit mitigation; can tune down if 429s persist |

---

## Open Questions

- Will N=5 trigger YouTube IP bans at scale? Needs monitoring on the `@SpaceinvaderOne` run.
- Should `PLAYLIST_CONCURRENCY` be an env var (`AXON_PLAYLIST_CONCURRENCY`) for runtime tuning without recompile?
- The `ingest errors <uuid>` subcommand is still unhandled (known gap from prior session) — no fix in this session.

---

## Next Steps

1. Re-authorize axon MCP token (`/mcp reconnect axon`) and trigger `@SpaceinvaderOne` ingest
2. Monitor job progress with `axon ingest status <job_id>` — verify resume + concurrent progress display
3. If 429s still occur, consider lowering `PLAYLIST_CONCURRENCY` to 3 or adding `--sleep-requests 2`
4. Consider making `PLAYLIST_CONCURRENCY` an env-var override for runtime tuning
