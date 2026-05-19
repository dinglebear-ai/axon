# `crates/jobs/` Test Coverage Analysis

**Date:** 2026-03-15
**Scope:** 66 `.rs` files, ~16.8k lines, 148 test functions across 33 files
**Test LOC:** ~2,901 in dedicated test files + ~1,200 in inline `#[cfg(test)]` modules

---

## Executive Summary

The jobs crate has a solid foundation of integration tests covering the core job lifecycle (claim, complete, fail, cancel) and AMQP/Redis/Postgres connectivity. The test pyramid leans heavily toward integration tests that require live infrastructure (Postgres, Redis, RabbitMQ), with a smaller set of pure unit tests for watchdog logic, backoff sequences, and data transforms.

**Key strengths:** Job lifecycle state transitions are well-tested. Watchdog two-pass confirmation logic has thorough unit tests. Schema migration concurrency is tested with 8-way parallel races. Deduplication of active jobs is tested across crawl, embed, and extract.

**Key gaps:** No tests for the graph worker's Neo4j-per-job instantiation pattern, no tests for the extract worker's Redis-down failure mode, no security tests for API key redaction, no concurrent claim contention tests, no tests for unbounded cleanup loops, and the entire `ingest/process.rs` (367 lines) has zero test coverage.

---

## 1. Test Coverage Map

### Well-Covered Code Paths

| Code Path | Tests | Quality |
|-----------|-------|---------|
| `claim_next_pending` FIFO ordering | `pool_integration::claim_next_pending_claims_oldest_job_first` | Good: serial test, cleans pre-existing pending rows |
| `mark_job_completed` idempotency | `db_lifecycle::mark_job_completed_is_idempotent` | Good: verifies second call returns false |
| `cancel_pending_or_running_job` | `db_lifecycle::cancel_pending_or_running_job_lifecycle` | Good: covers pending, running, completed, and re-cancel |
| `touch_running_job` no-op on non-running | `db_lifecycle::touch_running_job_is_noop_for_non_running` | Good |
| `spawn_heartbeat_task` advances `updated_at` | `heartbeat::spawn_heartbeat_task_advances_updated_at` | Good: uses 1s interval, 3s wait, checks DB timestamp |
| Watchdog two-pass payload/confirm | 8 unit tests in `common/tests/watchdog.rs` | Excellent: mismatch, malformed, window, preservation, reset |
| Stale job recovery (embed, crawl, extract) | 3 integration tests | Good: simulate 20-min stale, verify status=failed |
| Job deduplication (pending + running) | embed has 3 tests (pending, fresh running, stale running) | Good: covers the stale-running bypass case |
| AMQP publish/consume round-trip | 4 tests in `amqp_integration.rs` | Good: batch, single, purge, queue empty verification |
| Redis cancel key isolation | 5 tests in `redis_integration.rs` | Good: namespace isolation, TTL expiry, multi-key |
| Schema migration concurrency | 3 tests (crawl, embed, extract) with 8 concurrent spawns | Good: validates advisory lock serialization |
| `JobStatus` enum | 3 unit tests | Good: as_str, Display, unique strings |
| Polling backoff sequence | `worker_lane/tests.rs` | Good: verifies double/cap/reset sequence |
| AMQP reconnect backoff | `worker_lane/tests.rs` | Good: verifies double/cap at 60s |
| Refresh `fetch_and_process_url` | 5 tests with `httpmock` | Excellent: 304, 200-matching-hash, 200-new, 404, first-fetch |
| Refresh schedule claiming | Integration test with 3 fixture rows | Good: verifies due vs future filtering |
| Extract aggregation helpers | 4 unit tests in `extract/worker.rs` | Good: parser_hits, page counts, result shape |

### Partially Covered

| Code Path | What Exists | What's Missing |
|-----------|-------------|----------------|
| `reclaim_stale_running_jobs` (watchdog) | Two-pass confirm unit tests + integration tests | No test for `MAX_WATCHDOG_RECLAIM_ATTEMPTS` exhaustion path |
| `wrap_with_heartbeat` | Basic call-through test | No test that heartbeat actually fires DB touches in the wrapped context |
| `process_graph_job` | `merge_candidates`, `partition_by_ambiguity` unit tests | No test for the actual graph processing pipeline or Neo4j writes |
| `run_job_worker` | Indirect E2E tests (marked `#[ignore]`) | No test for `lane_count=0` error, no test for AMQP-to-polling fallback |
| Crawl worker E2E | 1 test marked `#[ignore]` | Test exists but is not run in normal CI |

### Untested Code Paths

| Code Path | Lines | Severity | Risk |
|-----------|-------|----------|------|
| `ingest/process.rs` | 367 | **Critical** | YouTube playlist resume, 429 retry, GitHub progress tracking, sessions dispatch |
| `ingest/ops.rs` | ~80 | High | `mark_completed`, `start_ingest_job`, `list_ingest_jobs` |
| `graph/worker.rs` `process_claimed_graph_job` | ~70 | High | Neo4j client-per-job instantiation (Phase 2 issue #3) |
| `extract/worker.rs` `process_extract_job` cancel path | ~30 | High | Redis connect failure makes extract jobs un-cancelable (Phase 2 issue #4) |
| `worker_lane.rs` `reenqueue_orphaned_pending_jobs` | ~25 | Medium | Startup re-enqueue of lost AMQP messages |
| `crawl/runtime/worker/loops.rs` reconnect loop | ~60 | Medium | Backoff reset semantics on clean vs error exit |
| `common/stats.rs` `count_stale_and_pending_jobs` | ~40 | Low | Wrapper that creates pool per call |
| `common/pool.rs` `AXON_PG_POOL_SIZE` env override | ~5 | Low | Custom pool size via env var |

---

## 2. Test Quality Assessment

### Strengths

**Behavior-focused assertions.** Tests verify observable outcomes (DB status column values, AMQP message bodies, Redis key presence) rather than internal implementation details. Example: `claim_and_fail_lifecycle_transitions_are_state_guarded` checks actual DB values after a sequence of operations.

**Proper cleanup.** Every integration test that inserts rows cleans up after itself with `DELETE` statements. The refresh schedule test uses a cleanup closure that runs regardless of assertion outcome.

**Test isolation for infrastructure.** Tests use `AXON_TEST_PG_URL` / `AXON_TEST_AMQP_URL` / `AXON_TEST_REDIS_URL` to avoid accidentally hitting production infrastructure. Tests gracefully skip via `return Ok(())` when env vars are unset.

**Unique test data.** Tests use `Uuid::new_v4()` for queue names, URLs, and input text to prevent cross-test contamination in shared databases.

### Weaknesses

**Silent skip pattern hides failures.** Every integration test does `let Some(pg_url) = resolve_test_pg_url() else { return Ok(()); }`. If the test environment is misconfigured, all integration tests pass silently with zero coverage. CI should assert that at least one infrastructure-dependent test actually ran.

**Duplicated schema DDL.** The `CREATE TABLE IF NOT EXISTS axon_embed_jobs` DDL is copy-pasted into 6+ test functions. A shared `ensure_test_schema()` helper would reduce maintenance burden and prevent schema drift between test DDL and production DDL.

**`serial_test` overuse.** Some tests marked `#[serial]` don't actually need serialization (e.g., `embed_ensure_schema_is_concurrency_safe` uses unique UUIDs and advisory locks). Unnecessary serialization slows test execution.

**E2E tests are `#[ignore]`.** The three worker E2E tests (`crawl_worker_e2e`, `embed_worker_e2e`, `extract_worker_e2e`) are permanently ignored. They test the most critical path (full job processing) but never run.

---

## 3. Test Pyramid Assessment

```
          /\
         /  \       E2E: 3 (all #[ignore])
        /    \
       /------\     Integration: ~65 (require Postgres/Redis/AMQP)
      /        \
     /----------\   Unit: ~80 (pure logic, no infrastructure)
    /            \
```

**Verdict:** The pyramid is inverted for the critical path. The most important behavior (full job processing through the worker pipeline) is only covered by ignored E2E tests. The integration tests cover the data layer well, but the orchestration layer (worker loops, reconnect logic, heartbeat-wrapped processing) is largely untested.

---

## 4. Findings by Severity

### Critical

#### C-1: `ingest/process.rs` has zero test coverage (367 lines)

The ingest job processor handles YouTube playlist/channel resume logic (`load_playlist_progress_with_pool`, `drain_playlist_videos_with_pool`), 429 retry backoff (`ingest_video_with_retry`), GitHub progress streaming, and sessions dispatch. None of this is tested.

**Risk:** Playlist resume logic involves `HashSet<String>` serialization to JSONB and deserialization on restart. A serialization mismatch would cause videos to be re-processed or silently skipped.

**Recommended tests:**
```rust
#[test]
fn ingest_video_with_retry_returns_ok_on_first_success()
#[test]
fn ingest_video_with_retry_retries_on_429_up_to_max_attempts()
#[test]
fn ingest_video_with_retry_does_not_retry_non_429_errors()
#[test]
fn load_playlist_progress_returns_default_on_missing_result_json()
#[test]
fn load_playlist_progress_deserializes_completed_urls_correctly()
#[test]
fn update_ingest_progress_merges_with_existing_result_json()
```

#### C-2: No concurrent claim contention test

`claim_next_pending` uses `FOR UPDATE SKIP LOCKED`, which is the correct pattern for concurrent claiming. However, there is no test that actually exercises concurrent claiming. The existing FIFO test (`claim_next_pending_claims_oldest_job_first`) is `#[serial]` and runs claims sequentially.

**Risk:** A future refactor could accidentally drop the `SKIP LOCKED` clause or change the CTE structure, breaking concurrency safety silently.

**Recommended test:**
```rust
#[tokio::test]
async fn concurrent_claim_next_pending_never_returns_same_job_id()
// Insert 10 pending jobs, spawn 10 concurrent claim tasks, assert
// no two tasks receive the same UUID and all 10 are claimed.
```

#### C-3: Watchdog `MAX_WATCHDOG_RECLAIM_ATTEMPTS` exhaustion path untested

The watchdog has a 3-attempt reclaim limit. After 3 reclaims, a job is permanently marked `failed` instead of reset to `pending`. This code path (`watchdog_reclaim_count >= MAX_WATCHDOG_RECLAIM_ATTEMPTS`) has no dedicated test. The integration test (`reclaim_stale_running_jobs_two_pass_flow_marks_then_reclaims`) only tests one reclaim cycle.

**Risk:** A crash loop could be silently infinite if the reclaim count logic has a bug (e.g., off-by-one, count never increments).

**Recommended tests:**
```rust
#[test]
fn watchdog_reclaim_count_extracts_from_result_json()
#[test]
fn watchdog_reclaim_payload_increments_count()
#[test]
fn watchdog_reclaim_payload_removes_watchdog_marker()
#[tokio::test]
async fn reclaim_exhausted_job_is_permanently_failed_after_max_attempts()
```

### High

#### H-1: Extract worker Redis-down makes jobs un-cancelable (Phase 2 issue #4)

In `extract/worker.rs`, `process_extract_job` connects to Redis with a 3s timeout. If Redis is down, it returns a `Box<dyn Error>` and the job fails. But the embed worker (`embed/worker.rs`) handles Redis failure gracefully: `open_embed_redis` returns `None`, and `check_embed_canceled` returns `false` (fail-safe). This asymmetry is not tested.

**Risk:** Extract workers crash on Redis timeout instead of continuing without cancel support. No test verifies the embed worker's fail-safe Redis behavior either.

**Recommended tests:**
```rust
#[tokio::test]
async fn embed_worker_continues_when_redis_unavailable()
// Mock redis connection failure, verify job completes (not fails)

#[tokio::test]
async fn extract_worker_fails_on_redis_unavailable()
// Verify current behavior (or fix it to match embed's fail-safe pattern)
```

#### H-2: Graph worker creates Neo4j client per job (Phase 2 issue #3)

`process_claimed_graph_job` calls `Neo4jClient::from_config(&cfg)` for every job. The `run_graph_worker` function already creates a client at startup but doesn't pass it to the per-job handler. No test verifies this behavior or its performance implications.

**Recommended test:**
```rust
#[test]
fn graph_worker_config_requires_neo4j_url()
// Verify run_graph_worker returns error when AXON_NEO4J_URL is empty
```

#### H-3: No test for API key redaction in error paths (Phase 2 issue #5)

`process_ingest_job` has a comment `SEC-M-6` documenting that `cfg` is never serialized into `error_text`. But there's no test enforcing this. If someone adds `format!("{cfg:?}")` to an error path, `openai_api_key` would leak into the database.

**Risk:** Credential leakage to persistent storage.

**Recommended tests:**
```rust
#[test]
fn config_debug_does_not_contain_api_key()
// Create Config with known api_key, assert format!("{:?}", cfg) does not contain it

#[test]
fn error_text_in_failed_jobs_does_not_contain_secrets()
// After mark_job_failed, verify error_text column doesn't contain API key patterns
```

#### H-4: `PgPool` created per CRUD call in some paths (Phase 2 issue #1)

`count_stale_and_pending_jobs` (in `stats.rs`) calls `make_pool(cfg)` which creates a new pool per invocation. The `_with_pool` variant exists and is tested, but nothing verifies that the pool-per-call variant is only used as a convenience wrapper (not in hot loops).

**Recommended test:**
```rust
#[test]
fn count_stale_and_pending_jobs_delegates_to_with_pool_variant()
// Structural verification that the non-pool function calls make_pool then delegates
```

### Medium

#### M-1: No test for AMQP-to-polling fallback in `run_job_worker`

When AMQP is unavailable at startup, `run_job_worker` falls back to SQL polling. This critical resilience path has no test. The polling mode is entirely exercised only in production.

**Recommended test:**
```rust
#[tokio::test]
async fn run_job_worker_falls_back_to_polling_when_amqp_unavailable()
// Configure cfg with invalid AMQP URL, verify worker starts in polling mode
// (would need to capture log output or add a mode indicator)
```

#### M-2: No test for `orphaned_pending_threshold_secs` with real job data

The function is tested with boundary values (0, 30, 60, 300, i64::MAX) but never with actual DB rows. The `reenqueue_orphaned_pending_jobs` function that uses it has no test at all.

**Recommended test:**
```rust
#[tokio::test]
async fn reenqueue_orphaned_pending_jobs_publishes_old_pending_to_amqp()
// Insert a pending job older than threshold, verify it appears in AMQP queue
```

#### M-3: No test for `resolve_lane_count` env var override

`resolve_lane_count` falls back to CPU count when the env var is unset and respects min/max clamping. No test exists.

**Recommended test:**
```rust
#[test]
fn resolve_lane_count_respects_env_override()
#[test]
fn resolve_lane_count_clamps_to_min_max()
#[test]
fn resolve_lane_count_ignores_zero_env_value()
```

#### M-4: Crawl worker reconnect backoff semantics differ from embed/extract (documented but untested)

The CLAUDE.md documents that crawl resets backoff on every successful reconnect, while embed/extract only reset after 60s alive. Neither reconnect loop has a test verifying its specific reset behavior.

**Recommended test:**
```rust
#[test]
fn crawl_reconnect_resets_backoff_on_every_success()
#[test]
fn generic_lane_reconnect_only_resets_after_long_lived_connection()
```

#### M-5: Refresh `url_processor.rs` GitHub `pushed_at` gating has minimal test coverage

The `url_processor.rs` has 3 inline tests but the `pushed_at` gating logic (check if a GitHub repo has been updated since last refresh) is likely tested only at the HTTP mock level, not the decision logic.

### Low

#### L-1: `durable_queue_options()` has no test

The function returns a struct with specific boolean fields. A test would prevent accidental changes to queue durability settings.

```rust
#[test]
fn durable_queue_options_are_durable_not_exclusive_not_autodelete()
```

#### L-2: `test_config` helper hardcodes infrastructure URLs

If the test config's default URLs (e.g., `redis://127.0.0.1:1`) ever match a real service, tests could have unintended side effects. The URLs use port 1, which is unlikely to be in use, but this is not documented.

#### L-3: `parse_dotenv_content` edge cases

The dotenv parser has 3 tests but doesn't test: empty values (`FOO=`), values with internal quotes (`FOO="it's here"`), or inline comments (`FOO=bar # comment`). These are defensive tests given the parser is test-only code, but could prevent surprises.

---

## 5. Security Test Gaps

| Gap | Risk | Recommendation |
|-----|------|----------------|
| No test for `openai_api_key` redaction in `Config` Debug/Display | API key in logs or DB error_text | Add `config_debug_redacts_api_key` test |
| No test that `error_text` column never contains credentials | Persistent credential leakage | Audit all `mark_job_failed` call sites in a test |
| No SSRF test for ingest URLs | Ingest jobs accept user URLs | The `http.rs` SSRF tests exist elsewhere; verify `validate_url` is called before ingest |
| No test for Redis cancel key TTL enforcement | Stale cancel keys could cancel future jobs | The TTL test in `redis_integration.rs` covers the mechanism but not the production code path |

---

## 6. Performance Test Gaps

| Gap | Risk | Recommendation |
|-----|------|----------------|
| No test for unbounded cleanup loops (Phase 2 issue #2) | `reclaim_stale_running_jobs` has `LIMIT 50` but callers may loop | Add test verifying batch limit is respected |
| No test for pool exhaustion under concurrent claiming | Could cause worker deadlocks | Add test with pool_size=2 and 5 concurrent claims |
| No test for AMQP channel reuse vs per-job creation | `batch_enqueue_jobs` opens/closes per batch; verify no per-message connections | Already documented by `enqueue_job_delegates_to_batch` but not measured |
| No benchmark for `claim_next_pending` with large pending queues | CTE + SKIP LOCKED performance degrades with many locked rows | Add benchmark with 10k+ pending rows |

---

## 7. Flaky Test Indicators

| Test | Risk Factor | Mitigation |
|------|-------------|------------|
| `spawn_heartbeat_task_advances_updated_at` | Uses `sleep(3s)` to wait for heartbeat tick | Acceptable: 3x the 1s interval provides margin |
| `redis_cancel_key_with_short_expiry_disappears_after_ttl` | Uses `sleep(1.2s)` for 1s TTL | Marginal: Redis TTL precision is 1s; under load this could flake |
| `claim_next_pending_claims_oldest_job_first` | `#[serial]` + deletes all pending rows at start | Correct pattern but brittle if another test inserts during the window |
| All `#[ignore]` E2E tests | 90s timeout, poll every 100ms | If ever un-ignored, these could be slow and fragile |

---

## 8. Recommended Priority Actions

1. **[Critical] Add `ingest/process.rs` unit tests** for retry logic, playlist resume serialization, and progress update merging. These are pure-logic functions that can be tested without infrastructure.

2. **[Critical] Add concurrent claim contention test** using `tokio::spawn` with multiple workers racing to claim from the same pool. This validates the `FOR UPDATE SKIP LOCKED` invariant.

3. **[Critical] Add watchdog exhaustion test** that runs 4 reclaim cycles and verifies the job is permanently failed on the 4th attempt.

4. **[High] Add Config Debug redaction test** that creates a Config with a known API key and asserts it doesn't appear in `format!("{:?}", cfg)`.

5. **[High] Fix extract worker Redis asymmetry** to match embed's fail-safe pattern, then add tests for both.

6. **[Medium] Extract duplicated schema DDL** into a shared `ensure_test_embed_schema(&pool)` helper to prevent drift.

7. **[Medium] Add CI assertion** that at least N infrastructure-dependent tests actually executed (not silently skipped).

8. **[Low] Un-ignore E2E tests** in a dedicated CI job with full infrastructure, or document why they are permanently disabled.
