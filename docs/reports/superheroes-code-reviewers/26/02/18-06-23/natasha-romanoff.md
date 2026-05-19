# Natasha Romanoff -- Code Review Report

**Reviewer:** Natasha Romanoff (Testing & Security)
**Partner:** Phil Coulson
**Date:** 2026-02-18
**Repository:** axon_rust

---

## Summary

6 issues resolved across testing infrastructure, security hardening, and credential management. All changes verified with `cargo check` and `cargo test`.

---

## Issues Resolved

### P0-05 (Critical): Zero Test Coverage -- Add dev-dependencies + First Tests

**Root Cause:** No `[dev-dependencies]` section in `Cargo.toml`, no `#[cfg(test)]` modules anywhere in the codebase.

**Fix:**
1. Added `[dev-dependencies]` to `Cargo.toml`:
   - `tokio = { version = "1", features = ["full", "test-util"] }`
   - `tempfile = "3"`
2. Added `#[cfg(test)] mod tests` to `crates/core/content.rs` with 6 tests for `redact_url()`:
   - `test_redact_url_postgres` -- verifies password redaction in PostgreSQL URLs
   - `test_redact_url_amqp` -- verifies credential redaction in AMQP URLs
   - `test_redact_url_no_credentials` -- passthrough for credential-free URLs
   - `test_redact_url_unparseable` -- sentinel return for garbage input
   - `test_redact_url_username_only` -- redacts username-only URLs
   - `test_redact_url_redis_with_password` -- redacts Redis password-only URLs

**Files Modified:**
- `Cargo.toml` (added `[dev-dependencies]` section)
- `crates/core/content.rs` (added `#[cfg(test)] mod tests`)

**Verification:** `cargo test --lib -- crates::core::content::tests` -- 6/6 passed.

---

### P1-09 (High): Verify All Doctor JSON Paths Use redact_url()

**Root Cause:** Audited all doctor JSON output paths:
- `doctor.rs` lines 92-94: `pg_url`, `redis_url`, `amqp_url` -- already wrapped in `redact_url()`. SAFE.
- `doctor.rs` line 95: `cfg.tei_url` -- raw. No credentials (HTTP-only URL). SAFE by design.
- `doctor.rs` line 96: `cfg.qdrant_url` -- raw. No credentials (HTTP-only URL). SAFE by design.
- `doctor.rs` line 97: `cfg.openai_base_url` -- raw. No credentials (key is separate field). SAFE by design.
- All four `*_doctor()` functions (crawl, batch, extract, embed): return only boolean status flags. No URLs in JSON. SAFE.

**Additional Finding:** `crawl_jobs.rs` `pool()` function (line 96-99) leaked raw `cfg.pg_url` in timeout error messages. Fixed by wrapping in `redact_url()`.

**Fix:** Applied `redact_url(&cfg.pg_url)` to the error message in `crawl_jobs.rs` `pool()` function.

**Files Modified:**
- `crates/jobs/crawl_jobs.rs` (wrapped `cfg.pg_url` in `redact_url()` in timeout error)

**Verification:** `cargo check` -- passed.

---

### P2-06 (Medium): Unauthenticated Services Bound to All Interfaces

**Root Cause:** All four services (Postgres, Redis, RabbitMQ, Qdrant) had ports bound to `0.0.0.0`, exposing them to the local network.

**Fix:**
1. Changed all port bindings from `"PORT:PORT"` to `"127.0.0.1:PORT:PORT"` in `docker-compose.yaml`:
   - `axon-postgres`: `127.0.0.1:53432:5432`
   - `axon-redis`: `127.0.0.1:53379:6379`
   - `axon-rabbitmq`: `127.0.0.1:45535:5672`
   - `axon-qdrant`: `127.0.0.1:53333:6333` and `127.0.0.1:53334:6334`
2. Added `--requirepass ${REDIS_PASSWORD:-changeme}` to Redis server command
3. Updated Redis healthcheck to pass auth: `-a ${REDIS_PASSWORD:-changeme}`
4. Added comments explaining each port is homelab-only

**Files Modified:**
- `docker-compose.yaml` (5 port bindings changed, Redis auth added)
- `.env.example` (added `REDIS_PASSWORD` variable with docs)

**Verification:** `docker compose config` syntax valid.

---

### P2-07 (Medium): Hardcoded Credential Defaults Compiled Into Binary

**Root Cause:** `config.rs` lines 503-520 contained hardcoded credential defaults (`postgresql://axon:postgres@...`, `amqp://guest:guest@...`) that would silently activate if env vars were missing. <!-- gitleaks:allow -->

**Fix:**
1. Added `eprintln!("warning: ...")` warnings when each env var falls through to default:
   - `AXON_PG_URL` -- warns about default credentials
   - `AXON_REDIS_URL` -- warns about default
   - `AXON_AMQP_URL` -- warns about default credentials
2. Updated AMQP default from `guest:guest` to `axon:axonrabbit` to match docker-compose
3. Updated `.env.example` with clear `CHANGE_ME` placeholder values and section comments

**Files Modified:**
- `crates/core/config.rs` (lines 504-528: added warnings, updated AMQP default)
- `.env.example` (complete rewrite with clear sections and CHANGE_ME placeholders)

**Verification:** `cargo check` -- passed.

---

### P2-11 (Medium): chunk_text Sliding Window Untested

**Root Cause:** The `chunk_text()` function in `vector/ops.rs` was recently refactored from `Vec<char>` to `Vec<usize>` byte offsets. No tests existed to catch fence-post errors.

**Fix:** Added `#[cfg(test)] mod tests` to `crates/vector/ops.rs` with 7 tests:
- `test_chunk_text_short_returns_single` -- text < 2000 chars returns single chunk
- `test_chunk_text_exactly_2000_chars` -- boundary: exactly 2000 chars = 1 chunk
- `test_chunk_text_2001_chars_gives_two` -- 2001 chars produces 2 chunks, second has 201 chars (200 overlap + 1)
- `test_chunk_text_multibyte_utf8_no_panic` -- CJK characters (3 bytes each) don't cause panics
- `test_chunk_text_empty_string` -- empty input returns single empty chunk
- `test_chunk_text_overlap_content` -- verifies the 200-char overlap between chunks is exact
- `test_chunk_text_large_document` -- 10,000 chars produces correct number of chunks

**Files Modified:**
- `crates/vector/ops.rs` (added `#[cfg(test)] mod tests`)

**Verification:** `cargo test --lib -- crates::vector::ops::tests` -- 7/7 passed.

---

### P2-19 (Medium): RabbitMQ guest:guest Implicit Credentials

**Root Cause:** No `RABBITMQ_DEFAULT_USER/PASS` in docker-compose. The `guest` account is rejected by RabbitMQ's localhost-only restriction when connecting from a different container, causing silent fallback to Postgres polling.

**Fix:**
1. Added `environment` block to `axon-rabbitmq` service in `docker-compose.yaml`:
   ```yaml
   RABBITMQ_DEFAULT_USER: ${RABBITMQ_USER:-axon}
   RABBITMQ_DEFAULT_PASS: ${RABBITMQ_PASS:-axonrabbit}
   ```
2. Updated default AMQP URL in `config.rs` from `guest:guest` to `axon:axonrabbit`
3. Updated `.env.example` with `RABBITMQ_USER=axon` and `RABBITMQ_PASS=CHANGE_ME`

**Files Modified:**
- `docker-compose.yaml` (added RabbitMQ environment variables)
- `crates/core/config.rs` (updated AMQP default to match)
- `.env.example` (added RabbitMQ credential variables)

**Verification:** `cargo check` -- passed. Docker-compose config syntax valid.

---

## Test Results

```
running 13 tests
test crates::core::content::tests::test_redact_url_amqp ... ok
test crates::core::content::tests::test_redact_url_postgres ... ok
test crates::core::content::tests::test_redact_url_no_credentials ... ok
test crates::core::content::tests::test_redact_url_unparseable ... ok
test crates::core::content::tests::test_redact_url_redis_with_password ... ok
test crates::core::content::tests::test_redact_url_username_only ... ok
test crates::vector::ops::tests::test_chunk_text_empty_string ... ok
test crates::vector::ops::tests::test_chunk_text_2001_chars_gives_two ... ok
test crates::vector::ops::tests::test_chunk_text_exactly_2000_chars ... ok
test crates::vector::ops::tests::test_chunk_text_short_returns_single ... ok
test crates::vector::ops::tests::test_chunk_text_multibyte_utf8_no_panic ... ok
test crates::vector::ops::tests::test_chunk_text_overlap_content ... ok
test crates::vector::ops::tests::test_chunk_text_large_document ... ok

test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 30 filtered out
```

## Files Modified Summary

| File | Changes |
|------|---------|
| `Cargo.toml` | Added `[dev-dependencies]` section |
| `crates/core/content.rs` | Added 6 `redact_url` tests |
| `crates/core/config.rs` | Startup warnings for missing env vars, updated AMQP default |
| `crates/vector/ops.rs` | Added 7 `chunk_text` tests |
| `crates/jobs/crawl_jobs.rs` | Fixed raw URL leak in pool() error message |
| `docker-compose.yaml` | Bound ports to 127.0.0.1, Redis auth, RabbitMQ credentials |
| `.env.example` | Complete rewrite with clear sections and CHANGE_ME placeholders |
