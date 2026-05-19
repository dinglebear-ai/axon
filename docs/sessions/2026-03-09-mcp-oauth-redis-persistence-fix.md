# MCP OAuth Redis Persistence Fix

**Date:** 2026-03-09
**Branch:** `refactor/acp-performance-modern-rust`
**Author:** Claude (Sonnet 4.6)

---

## Session Overview

Investigated and fixed a bug where the Axon MCP server required full Google OAuth re-authentication on every restart. The root cause was a single missing call to `normalize_local_service_url()` in the MCP OAuth state initializer, causing Redis to be silently unreachable when `axon mcp` runs as a local process outside Docker. All OAuth state (client registrations, access tokens, refresh tokens) lived exclusively in-memory and evaporated on every restart. Additionally hardened `redis_healthy()` with the same normalization as a defensive measure.

---

## Timeline

1. **Investigation** — User reported re-auth requirement on every MCP server restart despite Redis persistence being implemented.
2. **Root cause identification** — Read `.env` file, identified `AXON_REDIS_URL=redis://:password@axon-redis:6379` uses Docker-internal hostname; read `state.rs` and confirmed URL was passed raw to `redis::Client::open()` without normalization.
3. **Pattern confirmed** — Found `normalize_local_service_url()` in `crates/core/config/parse/docker.rs`; confirmed all other services (Postgres, AMQP, Qdrant, Chrome) already use it; MCP OAuth `from_env()` was the only miss.
4. **Scope expansion** — Dispatched parallel agents to find any other code paths bypassing normalization. Found `redis_healthy()` in `health.rs` takes a raw `&str` — a footgun for future callers (current callers are safe via `cfg.redis_url`).
5. **Implementation** — Three-file patch: `state.rs` (primary fix), `health.rs` (defensive hardening), `tests.rs` (new test).
6. **Verification** — `cargo check`, `cargo test`, `cargo clippy` all clean.

---

## Key Findings

- **`state.rs:28-32`** — `from_env()` read `AXON_REDIS_URL` and passed it directly to `redis::Client::open()`. `redis::Client::open()` is lazy (parses URL only, never connects), so it always returns `Ok()`. At connection time, `axon-redis` DNS fails outside Docker → `.ok()` converts to `None` → all Redis ops silently no-op → pure in-memory state → lost on restart.
- **`crates/core/config/parse/docker.rs:25`** — `normalize_local_service_url()` already existed and rewrites Docker-internal hostnames to `127.0.0.1:PORT` when `/.dockerenv` is absent. `axon-redis:6379` → `127.0.0.1:53379`.
- **All other services safe** — `crates/core/config/parse/build_config.rs` normalizes all URLs into the `Config` struct at parse time. Any code using `cfg.redis_url`, `cfg.pg_url`, etc. is not affected.
- **`health.rs:15`** — `redis_healthy(redis_url: &str)` is `pub` and accepts raw `&str` — current callers pass normalized `cfg.redis_url` but the signature is a footgun.
- **Failure mode is completely silent** — no log warning, no error, no panic. Tokens appeared to be written (no error returned) but were silently discarded.

---

## Technical Decisions

- **Normalize in `state.rs` not in callers** — fix the source, not each call site. Makes it impossible to break by adding a new call site.
- **Add `warn!` log when `redis_client` is `None`** — makes misconfiguration visible at startup rather than silently degrading.
- **Harden `redis_healthy()` defensively** — even though current callers are safe, the `pub &str` signature is a footgun. Inline normalization makes the function contract self-protecting.
- **Chose inline path over `use` import in `health.rs`** — consistent with other `crate::crates::core::config::parse::*` inline calls in the codebase (e.g., `crates/jobs/common.rs`).
- **Test added in `tests.rs`** — verifies `normalize_local_service_url` rewrites `axon-redis:6379` → `127.0.0.1:53379` with credentials preserved. Skips inside Docker where normalization is a no-op.

---

## Files Modified

| File | Change |
|------|--------|
| `crates/mcp/server/oauth_google/state.rs` | Added `use crate::crates::core::config::parse::normalize_local_service_url;`; wrapped Redis URL in `normalize_local_service_url(url)` at line 32; added `warn!` log when `redis_client` is `None` (lines 34-39) |
| `crates/core/health.rs` | Added inline `normalize_local_service_url()` call inside `redis_healthy()` before `redis::Client::open()` (lines 16-18) |
| `crates/mcp/server/oauth_google/tests.rs` | Added test `oauth_redis_url_docker_hostname_is_normalized_to_localhost` (lines 62-97) |

---

## Commands Executed

| Command | Result |
|---------|--------|
| `cargo check --lib` | `Finished dev profile` — zero errors |
| `cargo test --lib oauth` | 7 passed, 0 failed |
| `cargo test --lib health` | 5 passed, 0 failed |
| `cargo clippy --lib` | 2 pre-existing warnings (unrelated `type_complexity` in `youtube.rs`), zero new warnings |

---

## Behavior Changes (Before/After)

**Before:**
- `axon mcp` starts → reads `AXON_REDIS_URL=redis://axon-redis:6379` → `redis::Client::open()` succeeds (lazy parse) → `redis_client = Some(client)` → every connection attempt DNS-fails silently → all OAuth state in-memory only → restart = full re-auth required
- `axon doctor` / health checks: `redis_healthy()` could return `false` incorrectly if called with a Docker-internal URL from outside Docker (defensive concern, not observed in practice)

**After:**
- `axon mcp` starts → URL normalized to `redis://127.0.0.1:53379` outside Docker → Redis connection succeeds → OAuth tokens (client registrations, access tokens, refresh tokens) persisted to Redis → restart is seamless for MCP clients
- If Redis is genuinely unavailable: startup emits `WARN axon.mcp.oauth: no redis client configured for oauth state — tokens will not survive restarts`
- `redis_healthy()` now normalizes its input — safe regardless of caller

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check --lib` | Zero errors | `Finished dev profile` | ✅ PASS |
| `cargo test --lib oauth` | 7 tests pass | 7 passed, 0 failed | ✅ PASS |
| `cargo test --lib health` | 5 tests pass | 5 passed, 0 failed | ✅ PASS |
| `cargo clippy --lib` | No new warnings | 0 new warnings (2 pre-existing unrelated) | ✅ PASS |
| New test `oauth_redis_url_docker_hostname_is_normalized_to_localhost` | Pass | ok | ✅ PASS |

---

## Source IDs + Collections Touched

No Axon embed/retrieve operations performed during this session (code-only changes, no document indexing).

---

## Risks and Rollback

- **Risk:** Inside Docker (`/.dockerenv` present), `normalize_local_service_url()` is a no-op — no behavior change for containerized deployments.
- **Risk:** If `GOOGLE_OAUTH_REDIS_URL` or `AXON_REDIS_URL` is unset, `redis_client` is `None` and behavior is identical to before (in-memory fallback, startup warn log added).
- **Rollback:** `git diff crates/mcp/server/oauth_google/state.rs` — revert line 32 to remove `normalize_local_service_url()` wrapper and remove lines 34-39 (warn block). Revert `health.rs` lines 16-18 to original `redis::Client::open(redis_url)`.

---

## Decisions Not Taken

- **Change `redis_healthy()` signature to accept `&Config`** — would be cleaner but changes the public API and all call sites. Inline normalization achieves safety with minimal churn.
- **Add a startup connectivity probe in `from_env()`** — would catch DNS failures at startup but `from_env()` is synchronous; adding async probing would require significant restructuring.
- **Add `GOOGLE_OAUTH_REDIS_URL` to `.env`** — not needed; `AXON_REDIS_URL` fallback is now correctly normalized.
- **Log the normalized URL at startup** — decided against to avoid leaking Redis credentials into logs.

---

## Open Questions

- After the fix, MCP clients should not need to re-auth across restarts as long as their refresh token hasn't expired (30-day TTL). This has not been manually verified end-to-end (requires restarting `axon mcp` and observing Claude Desktop behavior).
- The `redis-cli` command to verify keys exist post-auth: `redis-cli -p 53379 -a <password> keys "axon:mcp:oauth:*"` — not run this session.
- One pre-existing clippy warning in `crates/ingest/youtube.rs:264` (`if` statement can be collapsed) — not related to this session, should be addressed separately.

---

## Next Steps

1. Manual smoke test: restart `axon mcp`, authenticate via Google OAuth in Claude Desktop, restart `axon mcp` again, confirm no re-auth prompt.
2. Verify Redis keys via `redis-cli` after authentication.
3. Address pre-existing `youtube.rs:264` clippy warning in a separate commit.
4. Consider adding a startup log line (at `info` level, not `debug`) showing the normalized Redis URL being used (without credentials) for operational visibility.
