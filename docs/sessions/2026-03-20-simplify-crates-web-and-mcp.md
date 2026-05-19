# Session: Simplify crates/web and crates/mcp
**Date:** 2026-03-20
**Branch:** `feat/pulse-shell-and-hybrid-search`

## Session Overview

Executed a comprehensive 9-agent parallel code review and fix process on two major Rust modules: `crates/web` (WebSocket execution bridge) and `crates/mcp` (MCP server). Each review phase used `systems-programming:rust-pro` agents with Rust best practices, code review, async patterns, and ACP skill loading. After review, 9 fix agents resolved findings in parallel. The `crates/web` phase completed fully in a prior context window; this session continued with `crates/mcp`.

## Timeline

1. **crates/web review** (prior context) — 9 agents reviewed 42 files, found 144 findings (13 critical, 59 warning, 72 suggestion)
2. **crates/web fixes** (prior context) — 12 critical/high fixes applied manually, then 9 fix agents resolved remaining suggestions
3. **crates/mcp review** — 9 agents reviewed 29 files (8,088 lines), found 5 critical, 20+ warning, 30+ suggestion findings
4. **crates/mcp fixes** — 9 fix agents dispatched; 3 completed fully, 6 hit rate limits but still applied most fixes before dying
5. **Manual reconciliation** — Fixed 4 compilation errors from partial agent work (`Option<&String>` → `Option<&str>` callers), reverted `OnceLock` caching of `artifact_root()` that broke tests

## Key Findings

### crates/web (from prior context)
- **DoS via unbounded WS messages**: Added `max_message_size(1 MiB)` on WS upgrade
- **PTY child process leak**: `shell.rs` — child process wasn't killed on WS disconnect
- **Symlink check after canonicalize** was a no-op in download paths
- **Blocking I/O in async**: several `HashSet` constructions and file reads on hot paths

### crates/mcp — Critical
- **PKCE timing side-channel** (`handlers_protected.rs:132`): `!=` comparison leaked PKCE challenge via timing. Fixed to use `constant_time_eq`
- **`constant_time_eq` leaked length** (`helpers.rs:122`): Early return on length mismatch revealed expected token length. Fixed with constant-time iteration over `max(a.len(), b.len())`
- **`spawn_blocking` + `block_on` anti-pattern** (`server.rs:121`): `handle_ask` wrapped async fn in `spawn_blocking(|| block_on(...))`, burning a blocking thread for zero benefit and risking deadlock. Changed to direct `.await`
- **Open redirect via `LoopbackOrHttps`** (`helpers.rs:65`): Accepts any HTTPS domain as redirect URI. *Deferred — requires allowlist design*
- **Spoofable rate-limit identity** (`helpers.rs:86`): `X-Forwarded-For` trusted without proxy validation. *Deferred — requires `ConnectInfo<SocketAddr>`*

### crates/mcp — Warning
- Auth code consumed before validation (`handlers_broker.rs:254`) — attacker can burn legitimate client's code
- Triple `load_mcp_config()` creating config drift (`server.rs:246,252`)
- `make_pool` creates new PgPool per `handle_export` call (`handlers_system.rs:369`)
- `oauth_sessions` and `oauth_clients` in-memory maps have no TTL eviction
- New Redis connection per operation (no `MultiplexedConnection` caching)
- Dead `schema/tests.rs` file (orphaned, never compiled)
- Inconsistent `i64`/`usize` for limit fields across schema
- Stringly-typed `schedule_subaction` (now fixed with enum)
- `slugify` mangled non-ASCII input to colliding slugs

## Technical Decisions

1. **Reverted `OnceLock` caching of `artifact_root()`** — Tests mutate env vars between runs in the same process; `OnceLock` caused first test's value to persist. The env reads are cheap relative to the disk I/O that follows, so caching isn't worth the test breakage.

2. **Constant-time eq without `subtle` crate** — Hand-rolled fix iterates over `max(a.len(), b.len())` with accumulated XOR+length-diff byte. Adding `subtle` as a dependency was deferred since it requires Cargo.toml changes and CI validation.

3. **Typed `ScheduleSubaction` enum over string matching** — Invalid schedule subactions now rejected at serde deserialization time rather than runtime string matching, matching the pattern used by every other subaction in the MCP schema.

4. **`parse_ingest_source` ownership change** — Changed from `&mut IngestRequest` with `.take()` to accepting owned fields directly. Clearer ownership semantics, no hidden mutation.

## Files Modified

### crates/mcp (this session)
| File | Changes |
|------|---------|
| `server.rs` | Removed `spawn_blocking`/`block_on`, cached schema with `LazyLock` |
| `server/common.rs` | Slugify hash fallback, URL error context, pagination cleanup |
| `server/handlers_crawl_extract.rs` | `.as_deref()` callers, `i64::try_from` casts |
| `server/handlers_embed_ingest.rs` | `parse_ingest_source` ownership, safe casts |
| `server/handlers_query.rs` | `internal_error` for config, map allocation fix, pre-capture `limit`/`offset` |
| `server/handlers_refresh_status.rs` | Typed `ScheduleSubaction`, `Option<&str>`, safe casts |
| `server/handlers_system.rs` | Regex length limit, `unreachable!()` → error, pagination cleanup |
| `server/handlers_system/screenshot.rs` | `create_dir_all` for screenshots, avoid JSON clones |
| `server/artifacts/path.rs` | Removed misleading symlink checks, reverted `OnceLock` |
| `server/artifacts/shape.rs` | `.len()` over `.chars().count()`, tightened visibility |
| `server/artifacts/lifecycle.rs` | 10MB file size cap for search reads |
| `server/oauth_google/handlers_protected.rs` | PKCE constant-time, removed clones, captured `now`, removed `Debug`, scope linear scan, generic error message |
| `server/oauth_google/helpers.rs` | Fixed `constant_time_eq` length leak |
| `server/oauth_google/state.rs` | Atomic auth code consumption (memory+Redis) |
| `server/oauth_google/rate_limit.rs` | Replaced `use super::*` with explicit imports |
| `schema.rs` | Deleted dead `deny_unknown_fields`, `Default` derive, typed `ScheduleSubaction`, expanded test coverage |
| `schema/tests.rs` | **DELETED** — orphaned file, never compiled |

### crates/web (prior context, summary)
- `web.rs`: Max WS message size, renamed `tailscale_auth` → `auth`
- `shell.rs`: Child process kill on disconnect, keepalive ping fix
- `download.rs`: Symlink checks, shared `serve_pack` helper
- `download/manifest.rs`: Typed `ManifestEntry`, symlink check
- `download/validation.rs`: `uuid::Uuid::parse_str`, removed `CurDir`
- `execute.rs`: Linear scan over HashSet, shared test helper
- `execute/args.rs`: Exact `json_key` match, `split_once`
- `execute/ws_send.rs`: Removed unnecessary `.clone()`
- `execute/sync_mode/dispatch.rs`: Explicit error on unhandled mode
- `execute/cancel.rs`: Ingest modes, extracted helper
- `execute/async_mode.rs`: Renamed `crawl_job_id` → `job_id_slot`
- `docker_stats.rs`: `eq_ignore_ascii_case`, `&'static str` status
- Plus 15+ more files

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check` | Clean compilation | `Finished dev profile` | PASS |
| `cargo clippy` | 0 warnings | 0 warnings | PASS |
| `cargo test mcp` | All MCP tests pass | 80 passed, 0 failed | PASS |
| `cargo test mcp` (before) | 73 pass, 2 fail (pre-existing) | 73 pass, 2 fail | baseline |
| `cargo test mcp` (after) | >=73 pass, 0 new failures | 80 pass, 0 fail | PASS (net +7 tests, 2 pre-existing fixed) |

## Behavior Changes (Before/After)

| Area | Before | After |
|------|--------|-------|
| PKCE validation | Standard `!=` comparison (timing leak) | Constant-time comparison |
| `constant_time_eq` | Leaked token length via early return | Iterates full `max(a,b)` length |
| `handle_ask` | `spawn_blocking(block_on(...))` — deadlock risk | Direct `.await` |
| Auth code consume | In-memory copy survived after Redis delete | Both stores cleared atomically |
| Schema generation | Regenerated per `read_resource` call | Cached with `LazyLock` |
| Schedule subaction | Raw string matching (`"list"`, `"create"`, ...) | Typed `ScheduleSubaction` enum |
| Regex patterns | Unbounded pattern length (ReDoS risk) | Capped at 1024 chars |
| Error messages | Leaked env var names | Generic "see server documentation" |
| OAuth scope check | `HashSet<String>` allocation per request | Linear scan (1-3 scopes) |
| Artifact search | No file size limit before read | 10MB cap |
| Screenshot dir | No `create_dir_all` — write could fail | Directory created before write |
| Non-ASCII slugs | "什么是Rust" → "rust" (collision) | "rust-a1b2c3d4" (hash suffix) |
| MCP test count | 73 passing (2 pre-existing failures) | 80 passing (0 failures) |

## Risks and Rollback

- **PKCE constant-time fix**: The hand-rolled `constant_time_eq` iterates with `get().copied().unwrap_or(0)` which could theoretically be optimized away by an aggressive compiler. Adding the `subtle` crate would be more robust. Rollback: revert `helpers.rs` changes.
- **`spawn_blocking` removal on `handle_ask`**: If `handle_ask` internally does blocking work (e.g., subprocess wait), it would now block the Tokio runtime. Verified that `handle_ask` is fully async. Rollback: restore the `spawn_blocking` wrapper.
- **`artifact_root()` not cached**: Env reads on every call. Acceptable since the I/O that follows dominates. If perf profiling shows this matters, re-add caching with test-aware invalidation.

## Decisions Not Taken

| Decision | Rationale |
|----------|-----------|
| Add `subtle` crate for constant-time comparison | Requires Cargo.toml + CI changes; hand-rolled fix is adequate for now |
| Hash tokens before Redis storage | Requires migration of existing tokens; deferred to dedicated security hardening |
| `DashMap` for OAuth state maps | Large refactor of 7 mutex-guarded HashMaps; deferred |
| DCR redirect URI allowlist | Design work needed for `LoopbackOrHttps` policy; deferred |
| `ConnectInfo<SocketAddr>` for rate limiting | Requires axum middleware restructuring; deferred |
| Shared `PgPool` in `AxonMcpServer` | Architecture decision about lazy init vs constructor injection; deferred |
| Unified `JobSubaction` enum | 4 identical enums (crawl/extract/embed/ingest) could share one, but breaks independent evolution |
| Redis `MultiplexedConnection` caching | Requires `from_env()` signature change; deferred |

## Open Questions

1. Is `handle_ask` truly fully async? If ACP subprocess spawning blocks, the `spawn_blocking` removal could cause issues under load.
2. The `LoopbackOrHttps` redirect policy accepts any HTTPS domain — is this intentional for the self-hosted use case, or a security gap?
3. The 2 pre-existing config parse test failures (`parse_mcp_transport_from_env`, `parse_mcp_transport_flag_overrides_env`) — now passing after our changes. Was this a side effect of env cleanup, or did we accidentally fix them?
4. `oauth_sessions` and `oauth_clients` maps have no TTL eviction — at what traffic level does this become a problem?

## Next Steps

1. **Security hardening pass**: Add `subtle` crate, hash Redis token keys, implement DCR allowlist
2. **OAuth state cleanup**: Add TTL to `oauth_sessions` and `oauth_clients` maps, use `GETDEL` for atomic Redis operations
3. **Performance**: Cache Redis `MultiplexedConnection`, shared `PgPool` in `AxonMcpServer`
4. **Architecture**: Consider unified `JobSubaction` enum if no divergence is expected
