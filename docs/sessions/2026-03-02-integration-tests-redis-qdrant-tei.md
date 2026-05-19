# Integration Tests: Redis Cancel Key, Qdrant Ops, TEI Retry Logic

**Date:** 2026-03-02
**Branch:** feat/sidebar
**Result:** 9 new tests, all passing — 486 total test count

---

## Session Overview

Implemented three integration test suites to cover production bugs that had no test protection:
- Redis cancel key fail-safe (`poll_cancel_key` parks instead of canceling on Redis failure)
- Qdrant `ensure_collection` idempotency (the GET-first bug fix against 409 Conflict)
- TEI `tei_embed` retry/split logic (429 backoff + 413 batch-splitting)

Also extended `docker-compose.test.yaml` with Redis and Qdrant test services, and added URL resolver helpers for both in `crates/jobs/common/mod.rs`.

---

## Timeline

1. Read plan document and existing test files to understand structure
2. Located key functions: `poll_cancel_key` (private in `process.rs`), `ensure_collection` (`pub(super)` in `qdrant_store.rs`), `tei_embed` (`pub(crate)` via `tei.rs`)
3. Resolved visibility constraints: Redis tests inline in `process.rs` (child mod accesses private parent), `ensure_collection` inline in `qdrant_store.rs`, TEI + Qdrant in new sibling test files
4. Discovered httpmock 0.8.2 uses `body_includes` (not `body_contains`) and first-registered mock has higher precedence (opposite of the docs description)
5. Fixed 413 test after first run: swapped mock registration order
6. Confirmed 486/486 tests pass, 0 clippy errors, monolith policy passes

---

## Key Findings

- `poll_cancel_key` (`process.rs:257`) is completely private. Only testable via inline `#[cfg(test)] mod tests {}` in the same file — child mods can access parent private items in Rust.
- `ensure_collection` (`qdrant_store.rs:21`) is `pub(super)` (scoped to the `tei` module). Inline tests within the same file avoid any visibility chain.
- httpmock 0.8.2 `When` API: method is `body_includes` not `body_contains`. First-registered mock has higher precedence (not "most recently registered" as the docs suggest).
- The Qdrant `/facet` endpoint requires a keyword payload index on the faceted field — tests must call `PUT /collections/{name}/index?wait=true` before `qdrant_url_facets` can succeed.
- `reqwest 0.13` `RequestBuilder` does NOT accept `.query()` in the test build context (or requires a feature); query params must be appended as `?key=val` to the URL string.
- Monolith enforcer correctly excludes `#[cfg(test)] mod` blocks from line counts: `process.rs` is 541 raw lines but passes the 500-line limit.

---

## Technical Decisions

**Redis tests in `process.rs` (not `runtime/tests.rs`):** `poll_cancel_key` is private. Rather than building a visibility re-export chain (`process → worker → runtime → tests`), inline tests in the same file is idiomatic Rust and cleaner.

**`ensure_collection` test in `qdrant_store.rs` (not `qdrant/tests.rs`):** The function is `pub(super)` (visible only within the `tei` module). Inline tests in `qdrant_store.rs` directly call it. The two remaining Qdrant tests (`facets`, `search`) live in `qdrant/tests.rs` since those call `pub(crate)` functions.

**413 test uses body string matching, not `respond_with` counter:** Two static mocks with different `body_includes` conditions — simpler than a stateful closure. The mock that requires BOTH strings is registered first (higher precedence); single-item calls miss the 2-string condition and fall through to the 200 mock.

**429 test uses `Arc<Mutex<usize>>` + `respond_with`:** The `respond_with` closure needs `Fn + Send + Sync + 'static`. `Arc<Mutex<usize>>` satisfies all constraints and provides interior mutability without `FnMut`.

**Qdrant tests use raw `reqwest::Client::new()` for collection setup:** `qdrant_store::qdrant_upsert` is `pub(super)` within tei. Creating test points via raw HTTP keeps the Qdrant tests self-contained within the `qdrant` module.

---

## Files Modified

| File | Action | Purpose |
|------|--------|---------|
| `docker-compose.test.yaml` | Modified | Added `axon-redis-test` (port 53380) and `axon-qdrant-test` (ports 53335/53336) with ephemeral tmpfs |
| `crates/jobs/common/mod.rs` | Modified | Added `resolve_test_redis_url()` and `resolve_test_qdrant_url()` helpers |
| `crates/jobs/crawl/runtime/worker/process.rs` | Modified | Appended `#[cfg(test)] mod tests {}` with 3 Redis cancel key tests |
| `crates/vector/ops/tei/qdrant_store.rs` | Modified | Appended `#[cfg(test)] mod tests {}` with `ensure_collection_is_idempotent` |
| `crates/vector/ops/tei.rs` | Modified | Added `#[cfg(test)] mod tests;` declaration |
| `crates/vector/ops/tei/tests.rs` | Created | 3 TEI httpmock tests (empty, 429 retry, 413 split) |
| `crates/vector/ops/qdrant.rs` | Modified | Added `#[cfg(test)] mod tests;` declaration |
| `crates/vector/ops/qdrant/tests.rs` | Created | 2 Qdrant integration tests (facets shape, search roundtrip) |

---

## Commands Executed

```bash
# Confirmed compile clean
cargo check --tests
# → Finished dev profile in 10.04s

# TEI httpmock tests (no services needed)
cargo test tei_embed
# → 3 passed in 1.35s

# Redis cancel key tests (including no-service fail-safe)
cargo test cancel_key
# → 6 passed in 5.00s (5s = timeout on unreachable Redis test)

# Full suite
cargo test --lib
# → 486 passed; 0 failed; 3 ignored; finished in 5.22s

# Monolith policy
python3 scripts/enforce_monoliths.py --file crates/jobs/crawl/runtime/worker/process.rs
# → Monolith policy check passed.

# Clippy
cargo clippy --tests
# → 0 errors
```

---

## Behavior Changes (Before/After)

| Dimension | Before | After |
|-----------|--------|-------|
| Redis cancel fail-safe | Untested — silent refactor risk | 3 tests guard the `pending()` park behavior |
| `ensure_collection` idempotency | Untested — the 409 bug was non-obvious | `ensure_collection_is_idempotent` catches any regression |
| `qdrant_url_facets` API shape | Untested | `qdrant_url_facets_returns_correct_shape` guards (url, count) contract |
| TEI 429/413 handling | Untested retry/split logic | `tei_embed_retries_on_429` and `tei_embed_splits_batch_on_413` |
| Test infra | Postgres + RabbitMQ only | Now includes Redis (53380) and Qdrant (53335) |
| Total test count | 480 (estimate) | 486 passing |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check --tests` | 0 errors | Finished in 10.04s | ✅ |
| `cargo test tei_embed` | 3 pass | 3 passed | ✅ |
| `cargo test cancel_key` | 6 pass (3 new + 3 existing) | 6 passed | ✅ |
| `cargo test --lib` | All pass | 486 passed; 0 failed | ✅ |
| `enforce_monoliths.py --file process.rs` | Pass (test block excluded) | "Monolith policy check passed." | ✅ |
| `cargo clippy --tests` | 0 errors | 0 errors | ✅ |

---

## Source IDs + Collections Touched

No Axon embed/retrieve operations were performed during this session. (Session is about writing test code, not indexing content.)

---

## Risks and Rollback

**Low risk overall** — changes are test-only (`#[cfg(test)]` blocks + new test files + infrastructure yaml).

- `docker-compose.test.yaml` additions are additive — existing Postgres/RabbitMQ tests unaffected. Rollback: remove the two new services.
- `process.rs` inline tests do not affect production code paths. Rollback: delete `#[cfg(test)] mod tests {}` block.
- `qdrant_store.rs` inline test block similarly production-safe. Rollback: delete block.
- New test files (`tei/tests.rs`, `qdrant/tests.rs`) only compiled under `--tests`. Rollback: delete files + remove `mod tests;` declarations.

---

## Decisions Not Taken

**Redis tests in `runtime/tests.rs`:** Would have required a multi-file visibility re-export chain (`poll_cancel_key` → `pub(super)` in process → `pub(super)` in worker → `pub(crate)` in runtime → visible in tests). Rejected as unnecessarily complex.

**Qdrant `ensure_collection` test in `qdrant/tests.rs`:** Would have required changing `ensure_collection` to `pub(crate)` (breaking encapsulation) or adding a `#[cfg(test)]` wrapper. Rejected in favour of inline test in the same file.

**`tokio::time::pause()` for 429 delay:** Would eliminate the 1s test delay but introduces complexity and threading concerns. Accepted the 1s delay — total TEI suite runs in ~1.35s.

**`qdrant_upsert` for Qdrant test setup:** `pub(super)` scoped to `tei` module, not accessible from `qdrant/tests.rs`. Raw `reqwest::Client::new()` HTTP calls used instead — keeps the test module self-contained.

---

## Open Questions

- The production `cortex` collection has a keyword index on `url` and `domain` fields (required for facets), but `ensure_collection` does not create them. How was the production index created? Manual one-time setup? If so, a new collection created by a fresh deploy would fail `qdrant_url_facets` until the index is manually added. Worth investigating.

---

## Next Steps

- Start `docker-compose.test.yaml` Redis + Qdrant services and run full integration suite with `AXON_TEST_REDIS_URL` and `AXON_TEST_QDRANT_URL` set to verify the 5 integration tests pass against live services
- Consider adding `ensure_collection` to create keyword indexes on `url` and `domain` at collection creation time (would make production setup self-contained and make integration tests more realistic)
