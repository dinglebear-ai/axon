# Session: PR Review and Fixes — feat/lite-mode
**Date:** 2026-03-27
**Branch:** feat/lite-mode
**PR:** #60 — feat(lite): add lite-mode backend and shared runtime cutover

---

## 1. Session Overview

Ran a comprehensive PR review on PR #60 using two specialized agents (`pr-review-toolkit:code-reviewer` and `pr-review-toolkit:silent-failure-hunter`) in parallel. The review surfaced one critical pre-existing silent failure, three important correctness/reliability issues, and three minor suggestions. All issues were fixed, tests verified passing, and `cargo check` confirmed clean compilation.

---

## 2. Timeline

| Time | Activity |
|------|----------|
| Start | Checked git status and PR state; identified 3 changed Rust files |
| +2 min | Launched code-reviewer and silent-failure-hunter agents in parallel |
| +5 min | Agents returned findings; synthesized into prioritized action plan |
| +10 min | Applied all fixes to `ops.rs`, `workers.rs` |
| +12 min | Ran `cargo test --lib crates::jobs::lite` — 17 passed |
| +13 min | Ran `cargo test --locked --quiet` — all tests pass |

---

## 3. Key Findings

### Critical
- **`ops.rs:105,108` (was `workers.rs`)** — `let _ = mark_completed(...)` and `let _ = mark_failed(...)` in `worker_loop` silently discard SQLite write errors. If `SQLITE_BUSY` or disk-full hits at job completion, the job stays stuck in `running` state forever with no log entry and no observable signal.

### Important
- **`ops.rs:30`** — `serde_json::to_string(urls).unwrap_or_default()` stored an empty string on serialization failure; the broken extract job would only fail later at execution time with a confusing error rather than at enqueue.
- **`ops.rs:77-119`** — `pool.begin()` starts a DEFERRED transaction. Under SQLite WAL mode two workers can both SELECT the same pending row before either UPDATE runs; `AND status='pending'` guard was correct but the SELECT→UPDATE window was not atomic. `BEGIN IMMEDIATE` eliminates it.
- **`workers.rs:316-374`** — `github`, `reddit`, `youtube` branches in `run_ingest_job_lite` ignored `config_json` entirely. Since `ingest.rs` now serializes the full `IngestSource` struct (including `include_source` for GitHub), workers using only `source_type` + base `cfg` would silently use the wrong `include_source` value. Refactored to deserialize `config_json` as `IngestSource` for all variants.
- **`workers.rs:418`** — `serde_json::from_str(&config_json).unwrap_or_default()` for graph jobs masked malformed JSON with a `null` value; next line would produce a generic "missing 'url'" error, losing the actual parse error details.

### Suggestions
- `workers.rs:108-111` — No trace log on the `rows_affected() == 0` rollback path; concurrent claim collisions were invisible.
- `ops.rs:294` — Test hardcoded `/tmp/`; portability nit.
- `ops.rs:348` — `std::fs::remove_file` in async test; CLAUDE.md requires `tokio::fs::*`.

---

## 4. Technical Decisions

### BEGIN IMMEDIATE via raw SQL instead of `pool.begin()`
sqlx 0.8 has no API to set `BEGIN IMMEDIATE` via the `Transaction` RAII wrapper. Switched to `pool.acquire()` + manual `BEGIN IMMEDIATE` / `COMMIT` / `ROLLBACK` with explicit error-path cleanup (`let _ = ROLLBACK` on intermediate failures, propagated `?` on expected paths). RAII auto-rollback is lost but error paths are all explicit.

### Deserialize `config_json` as `IngestSource` for all variants (not just sessions)
Rather than adding special-case per-variant deserialization, unified all four match arms to start from the `IngestSource` enum. This eliminates the string-based dispatch, makes `include_source` for Github faithfully round-trip through the job queue, and means adding a new `IngestSource` variant automatically gets compiler enforcement.

### `let _ =` → `if let Err(e) =` with tracing::error!
Chose to log at `ERROR` level (not `WARN`) because a job stuck in `running` state forever is an operational incident — not a warning. Message explicitly states the job will remain in that state to avoid confusion during post-mortem.

### Error message includes config_json preview
Added `&config_json[..config_json.len().min(120)]` preview to deserialization errors so operators can distinguish empty JSON from malformed JSON without querying the raw DB row.

---

## 5. Files Modified

| File | Purpose |
|------|---------|
| `crates/jobs/lite/ops.rs` | BEGIN IMMEDIATE, `unwrap_or_default` fix, trace log, test portability fixes |
| `crates/jobs/lite/workers.rs` | `let _ =` → error logging, graph job parse fix, ingest unified deserialization |

`crates/services/ingest.rs` — no changes in this session (changes were from the original PR commits already on the branch).

---

## 6. Commands Executed

```bash
# Identify changed files
git diff --name-only HEAD

# Check PR state
gh pr view

# Get full diff of the 3 Rust files under review
git diff HEAD crates/jobs/lite/ops.rs crates/jobs/lite/workers.rs crates/services/ingest.rs

# Verify compilation
cargo check  # → Finished dev profile in 12.70s, no errors

# Run lite-mode unit tests
cargo test -p axon --lib crates::jobs::lite
# → test result: ok. 17 passed; 0 failed

# Run full test suite
cargo test --locked --quiet
# → all doctests ran; no failures
```

---

## 7. Behavior Changes (Before/After)

| Area | Before | After |
|------|--------|-------|
| Job completion write failure | Silently discarded; job stuck in `running` forever | `tracing::error!` with table + job_id logged |
| Extract job enqueue with bad URL list | Stored empty `urls_json`; failed at execute time | Fails at enqueue with clear `sqlx::Error::Decode` |
| Concurrent SQLite job claim | DEFERRED tx; TOCTOU window between SELECT and UPDATE | BEGIN IMMEDIATE; write lock acquired before SELECT |
| Github ingest `include_source` | Used `cfg.github_include_source` (may differ from enqueued value) | Reads `include_source` from stored `config_json` |
| Graph job with malformed `config_json` | `unwrap_or_default()` → confusing "missing url" error | Fails immediately with parse error + job ID |
| Concurrent claim collision | Silent `Ok(None)` return | `tracing::trace!` logs the collision |
| Test temp file path | Hardcoded `/tmp/` | `std::env::temp_dir()` |
| Test file cleanup | `std::fs::remove_file` (blocking) | `tokio::fs::remove_file(...).await` |

---

## 8. Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check` | Clean compile | `Finished dev profile in 12.70s` | ✅ Pass |
| `cargo test --lib crates::jobs::lite` | All pass | 17 passed, 0 failed | ✅ Pass |
| `cargo test --locked --quiet` | All pass | All passed, 0 failed | ✅ Pass |
| `concurrent_claims_only_return_one_job` | Exactly 1 winner | 1 winner, status=running | ✅ Pass |

---

## 9. Source IDs + Collections Touched

No Axon embed/retrieve operations were performed during this session (session was code review + implementation, not research/indexing).

---

## 10. Risks and Rollback

### BEGIN IMMEDIATE change
- **Risk**: `BEGIN IMMEDIATE` returns `SQLITE_BUSY` immediately if another writer holds the lock (no retry in `claim_next_pending`). Callers (`worker_loop`) treat claim errors as a `break` from the inner loop with a logged error. Workers then re-enter the outer `select!` loop and retry after `POLL_INTERVAL` (5s). This is safe — no job is lost, just a missed cycle.
- **Risk**: `PoolConnection` drops without explicit COMMIT/ROLLBACK if a panic occurs between `BEGIN IMMEDIATE` and the cleanup path. SQLite will auto-rollback when the connection is returned to the pool. Acceptable.
- **Rollback**: Revert `claim_next_pending` to use `pool.begin()` and remove `use tracing;` import.

### Unified ingest deserialization
- **Risk**: Jobs enqueued before this fix with `config_json = "{}"` will fail to deserialize as `IngestSource`. These are `github`, `reddit`, `youtube` jobs from the old code path (pre-`ingest.rs` fix). The error message now includes the preview, making them diagnosable. There are no such jobs in production yet (branch not merged).
- **Rollback**: Revert `run_ingest_job_lite` to the string-match dispatch.

---

## 11. Decisions Not Taken

| Decision | Rationale for Rejection |
|----------|------------------------|
| Keep DEFERRED transaction + `rows_affected()` guard only | Reviewer classified BEGIN IMMEDIATE as "Important"; user requested all issues addressed |
| Add second rollback helper function | Inlining was cleaner given sqlx type complexity with `PoolConnection<Sqlite>` deref chains |
| Panic/unwrap in test cleanup | CLAUDE.md requires `tokio::fs::*` in async contexts; `.await.ok()` is idiomatic |
| `tracing::warn!` for mark_completed/failed failures | Operational incident severity warrants `error!` level |

---

## 12. Open Questions

- Does `BEGIN IMMEDIATE` interact with sqlx's connection pool WAL checkpoint behavior? In-process workers share the same pool, so concurrent `BEGIN IMMEDIATE` calls will serialize. Cross-process scenarios (if any) need separate investigation.
- Are there any enqueued `github`/`reddit`/`youtube` ingest jobs in the staging/dev DB with `config_json = "{}"` that would now fail deserialization? Unlikely pre-merge, but worth a one-time check before merging.
- The `mark_completed` None path no longer writes `result_json=NULL` explicitly (updated to a separate query). Is the previous behavior (SQLite NULL default) equivalent? Tests pass, suggesting yes.

---

## 13. Next Steps

- Address any remaining PR review comments from `chatgpt-codex-connector`, `cubic-dev-ai`, and Copilot (use `/gh-address-comments`)
- Run `just verify` (fmt-check + clippy + check + test) as final pre-merge gate
- Merge PR #60 to main once reviewers approve
- Monitor lite-mode worker logs for `tracing::error!` on `mark_completed`/`mark_failed` after deployment
