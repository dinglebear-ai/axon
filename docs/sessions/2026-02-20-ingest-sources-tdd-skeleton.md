# Session: GitHub / Reddit / YouTube Ingest Sources — TDD Skeleton → GREEN

**Date:** 2026-02-20
**Branch:** `feat/ingest-sources` (plan: `compiled-hopping-quilt.md`)
**Duration:** ~2 context windows (compacted mid-session)

---

## 1. Session Overview

Completed the TDD RED → GREEN cycle for three new top-level ingest commands (`github`, `reddit`, `youtube`). The previous session wrote the skeleton with `todo!()` stubs and was cut short before `cargo check` finished. This session:

1. Verified compilation (1 non-exhaustive match error, quickly fixed)
2. Confirmed RED phase (31 tests panicking at `todo!()`)
3. Dispatched 3 parallel sub-agents for GREEN phase implementation
4. Cleaned all `cargo clippy` warnings
5. Left state: 131 tests passing, 0 warnings, 0 errors, `cargo fmt` clean

The three network-dependent async functions (`ingest_github`, `ingest_reddit`, `ingest_youtube`) remain as `todo!()` — next implementation target.

---

## 2. Timeline

| Time | Activity |
|------|----------|
| Start | `cargo check --bin axon` — 1 error (non-exhaustive match), 1 warning (unused import) |
| +5m | Fixed `mod.rs` match arms + `ingest_jobs.rs` import |
| +10m | `cargo check` clean; ran `cargo test --lib -- ingest` → 31 FAILED (RED confirmed) |
| +15m | Dispatched 3 parallel sub-agents (GitHub, Reddit, YouTube pure-function GREEN) |
| +35m | All agents reported done; 131 tests passing |
| +40m | `cargo clippy` cleanup (stub `_` params, `stream_ended` fix in worker_lane) |
| +45m | `cargo fmt` pass |
| End | `/save-to-md` invoked |

---

## 3. Key Findings

- **Non-exhaustive match**: The linter had pre-inserted a placeholder arm during the prior session. Replaced it with real dispatch to `run_github`, `run_reddit`, `run_youtube`.
- **`worker_lane.rs` `stream_ended` warning**: Pre-existing clippy warning from unused assignment. Root cause: the variable tracked whether the AMQP consumer stream ended so an error could be returned after the loop — but clippy (correctly) flagged `let mut x = false; loop { ... x = true ... }` as dead. Fix: changed `Ok(None)` arm to `break`, post-loop `Err(...)` is now statically reachable.
- **Parallel agent efficiency**: 3 independent agents on different files with no shared state completed in ~20 minutes vs sequential ~60 minutes.
- **Stub param convention**: All `todo!()` function params prefixed with `_` to silence unused-variable warnings while preserving signatures for future implementation.

---

## 4. Technical Decisions

| Decision | Rationale |
|----------|-----------|
| Shared `axon_ingest_jobs` table (Approach C) | One table, one worker, one AMQP queue — minimal Docker footprint; all three sources are low-volume relative to crawl/embed |
| `IngestSource` tagged enum with serde | Enables both in-process dispatch and JSON persistence in `config_json` column without separate type discriminator columns |
| `embed_text_with_metadata` new function | Ingest sources need `source_type` + `title` in Qdrant payload for filtered queries; wrapping chunking + TEI + upsert in one call keeps ingest code clean |
| No `octocrab` added yet | Deferring until `ingest_github` implementation; `Cargo.toml` not modified until the dep is actually used (YAGNI) |
| `worker_lane::run_job_worker` reused | Generic worker pattern already handles AMQP lanes, polling fallback, stale sweeps — no new infrastructure needed |

---

## 5. Files Modified

| File | Change | Lines |
|------|--------|-------|
| `mod.rs` | Added Github/Reddit/Youtube dispatch + `is_async_enqueue_mode` arms | +10 |
| `crates/cli/commands/mod.rs` | Added 3 module declarations + pub use exports | +6 |
| `crates/cli/commands/github.rs` | **Created** — full job subcommand plumbing + `run_github` | ~220 |
| `crates/cli/commands/reddit.rs` | **Created** — full job subcommand plumbing + `run_reddit` | ~220 |
| `crates/cli/commands/youtube.rs` | **Created** — full job subcommand plumbing + `run_youtube` | ~220 |
| `crates/jobs/ingest_jobs.rs` | Removed unused import; added `recover_stale_ingest_jobs` | +15 |
| `crates/jobs/worker_lane.rs` | Fixed `stream_ended` clippy warning; refactored loop exit | ~5 |
| `crates/ingest/github.rs` | Implemented `is_indexable_source_path`, `is_indexable_doc_path`, `parse_github_repo` | +40 |
| `crates/ingest/reddit.rs` | Implemented `classify_target` (Thread vs Subreddit routing) | +20 |
| `crates/ingest/youtube.rs` | Implemented `parse_vtt_to_text`, `extract_video_id` | +50 |

---

## 6. Commands Executed

```bash
# Initial state check
cargo check --bin axon
# → Error: non-exhaustive patterns for Github/Reddit/Youtube in mod.rs match

# After fix
cargo check --bin axon
# → warning: unused import `spider::tokio` in ingest_jobs.rs
# → Finished (0 errors)

# RED phase confirmation
cargo test --lib -- ingest
# → test result: FAILED. 0 passed; 31 failed

# After parallel agent GREEN phase
cargo test --lib
# → test result: ok. 131 passed; 0 failed

# Clippy pass
cargo clippy --bin axon
# → warning: variable `stream_ended` is assigned but not used [worker_lane.rs]
# (then fixed)
cargo clippy --bin axon
# → Finished (0 errors, 0 warnings)

# Format
cargo fmt
cargo fmt --check
# → clean
```

---

## 7. Behavior Changes (Before/After)

| Aspect | Before | After |
|--------|--------|-------|
| `axon github owner/repo` | Command unknown / panic | Enqueues ingest job or runs sync |
| `axon reddit rust` | Command unknown / panic | Enqueues ingest job or runs sync |
| `axon youtube <url>` | Command unknown / panic | Enqueues ingest job or runs sync |
| `axon github list` | Command unknown | Lists ingest jobs filtered to Github source |
| `axon github worker` | Command unknown | Starts ingest worker (shared with Reddit/YouTube) |
| Test count | 100 passing | 131 passing (+31 ingest pure-logic tests) |
| Clippy warnings | 1 (`stream_ended`) | 0 |

---

## 8. Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check --bin axon` | 0 errors | 0 errors | ✅ |
| `cargo test --lib -- ingest` | 31 passed | 31 passed | ✅ |
| `cargo test --lib` | 131 passed | 131 passed | ✅ |
| `cargo clippy --bin axon` | 0 warnings | 0 warnings | ✅ |
| `cargo fmt --check` | clean | clean | ✅ |

---

## 9. Risks and Rollback

- **Low risk**: No existing behavior changed. Three new commands added; all existing commands unmodified.
- **`ingest_github/reddit/youtube` are still `todo!()`**: Calling these with `--wait true` will panic. Default (no `--wait`) enqueues a job that will fail when the worker picks it up — this is acceptable test-in-progress behavior.
- **Rollback**: `git revert HEAD` on any commit in this branch, or `git checkout main` abandons the feature branch entirely.

---

## 10. Decisions Not Taken

| Option | Rejected Because |
|--------|-----------------|
| Separate AMQP queues per source | One more queue to manage, one more worker process — overkill for low-volume ingest sources |
| `octocrab` added to Cargo.toml now | YAGNI — dep bloat before the code that uses it exists |
| Implement `ingest_github` network logic now | Better to ship skeleton → tests green → review → then implement network logic with proper error handling |
| Single `ingest.rs` flat file | Three independent fetchers with different dep trees belong in separate files |

---

## 11. Open Questions

- **`yt-dlp` Docker installation**: Must be added to `Dockerfile` — not yet done. `RUN pip3 install yt-dlp` or static binary download?
- **Reddit OAuth**: `REDDIT_CLIENT_ID` / `REDDIT_CLIENT_SECRET` — how does user supply these? Via `.env` already updated in `config/types.rs` but `.env.example` not yet updated.
- **GitHub pagination**: `octocrab` handles this — but what's the right default page limit for issues/PRs on large repos like `rust-lang/rust` (50k+ issues)?
- **YouTube playlist/channel**: yt-dlp glob `*.vtt` in `/tmp/` could collide if multiple workers run concurrently. Should temp dir be per-job-id?

---

## 12. Next Steps

1. **Implement `ingest_youtube`** (simplest — no OAuth, just subprocess + file parse):
   - `tokio::process::Command::new("yt-dlp")` with VTT flags
   - Glob `/tmp/axon-*.vtt`, call `parse_vtt_to_text`, call `embed_text_with_metadata`
   - Use job-id-scoped temp dir to prevent collision

2. **Implement `ingest_reddit`** (medium — OAuth2 + reqwest):
   - `get_access_token` from `REDDIT_CLIENT_ID` / `REDDIT_CLIENT_SECRET`
   - Route on `classify_target` result
   - Fetch posts + comments, build document per post

3. **Implement `ingest_github`** (most complex — pagination, optional source files):
   - Add `octocrab` to `Cargo.toml`
   - Tree walk for `.md` + optional source files
   - Issues + PRs with pagination

4. **Docker**: Add `yt-dlp` install to `Dockerfile`; add `ingest-worker` s6 service scripts

5. **Config**: Update `.env.example` with `GITHUB_TOKEN`, `REDDIT_CLIENT_ID`, `REDDIT_CLIENT_SECRET`, `AXON_INGEST_QUEUE`
