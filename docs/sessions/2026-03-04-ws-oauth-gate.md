# Session: WebSocket OAuth Gate — crates/web

**Date:** 2026-03-04
**Branch:** feat/sidebar
**Duration:** ~1 session

---

## Session Overview

Audited `crates/web` for unauthenticated endpoints, identified the `/ws` WebSocket upgrade handler as a gap, and implemented MCP OAuth bearer token reuse to gate access. Investigated a confusing edit-reversion pattern (turned out to be a `rustfmt` post-hook creating misleading system reminders). Dispatched two parallel code reviewers and applied all critical and important findings.

---

## Timeline

1. **Security audit** — reviewed `crates/web.rs`, `shell.rs`, `execute/args.rs`, `execute/constants.rs`; identified `/ws` has no auth while `/ws/shell` already had loopback restriction
2. **Architecture review** — read `crates/mcp/server/oauth_google/` to understand token storage model
3. **Design decision** — reuse existing MCP OAuth access tokens via Redis lookup (separate processes, shared Redis) rather than adding a new auth mechanism
4. **Implementation** — added `ConnectionManager`, `validate_bearer_token`, `WsQuery`, startup logging, and the auth gate in `ws_upgrade`
5. **Debugging detour** — investigated why edits appeared to be reverting; confirmed it was the `PostToolUse` rustfmt hook creating misleading "file was modified" system reminders; actual changes persisted (confirmed via `git diff`)
6. **Parallel review** — dispatched `coderabbit:code-reviewer` and `feature-dev:code-reviewer` simultaneously
7. **Fix pass** — applied four actionable findings from both reviewers

---

## Key Findings

### Security Gap (resolved)
- `crates/web.rs:199` — `ws_upgrade` accepted any connection with no authentication. Anyone who could reach port 49000 directly (bypassing the Next.js proxy that enforces `AXON_WEB_API_TOKEN`) could execute any whitelisted command.
- `/ws/shell` had correct loopback-only restriction at `web.rs:152` — only `/ws` was missing auth.

### Root Cause: Hook Confusion
- `.claude/settings.json` `PostToolUse[Edit]` runs `rustfmt` on every `.rs` save.
- System reminders showed the pre-rustfmt state, making it appear content changes were reverted.
- `git diff` confirmed semantic changes persisted throughout; only whitespace/ordering was being normalized.

### Reviewer Findings (CodeRabbit + feature-dev)
- **F-02**: Empty string token was passed to Redis lookup when `?token=` was absent (`unwrap_or("")` → `axon:mcp:oauth:access_token:` key). Theoretical collision risk + wasted round-trip.
- **F-05**: `get_multiplexed_async_connection()` per WS upgrade opened a new TCP connection each time. `ConnectionManager` (already a declared feature dep) is the correct pattern.
- **F-06**: No startup log indicating whether the gate was active or disabled — silent misconfiguration risk.
- **F-07**: No log on 401 rejections — impossible to distinguish an attack from misconfiguration at the operator level.
- **F-03**: `#[allow(dead_code)]` on all security gate components confirmed removed cleanly (compiler saw through axum route registration correctly once all components were wired).

---

## Technical Decisions

### Why reuse MCP OAuth tokens instead of a new mechanism
MCP already issues `atk_<uuid>` tokens stored in Redis at `{GOOGLE_OAUTH_REDIS_PREFIX}:access_token:{token}`. The serve process and MCP process share Redis. No new auth infrastructure needed — just a Redis GET and expiry check. The key format and fallback chain (`GOOGLE_OAUTH_REDIS_URL` → `AXON_REDIS_URL`) match the MCP server exactly.

### Why `?token=` query param (not header)
Browser `WebSocket` API does not support custom headers. The `?token=` query parameter is the standard workaround for browser-originating WS connections.

### Why `ConnectionManager` over `Client`
`redis::Client::get_multiplexed_async_connection()` opens a new TCP connection per call. `ConnectionManager` maintains a persistent connection, handles reconnects automatically, and is `Clone` (cheap share of the underlying connection). The `connection-manager` feature was already enabled in `Cargo.toml`.

### Fail-closed when Redis is down
If Redis is unreachable, `validate_bearer_token` returns `false` → 401. This was a deliberate design choice confirmed by both reviewers. Added `log::warn!` on connection failure so operators can distinguish a Redis outage from a bad token.

### Shell WS left loopback-only (not OAuth-gated)
`/ws/shell` exposes a raw PTY. OAuth would be wrong here — a shell session should only ever be local. The existing loopback-only `ConnectInfo` check is the correct guard.

---

## Files Modified

| File | Purpose |
|------|---------|
| `crates/web.rs` | Full implementation: `AppState` extension, `validate_bearer_token`, `WsQuery`, OAuth gate in `ws_upgrade`, startup log |

No other files were modified.

---

## Implementation Details

### `AppState` extension (`crates/web.rs:23-35`)
```rust
oauth_redis: Option<redis::aio::ConnectionManager>,
oauth_prefix: String,
```

### `validate_bearer_token` (`crates/web.rs:61-79`)
- Clones `ConnectionManager` (cheap) for each call
- Constructs key: `{oauth_prefix}:access_token:{token}`
- Deserializes only `expires_at_unix` from stored JSON (ignores `scope`)
- Expiry check: `unix_now_secs() <= rec.expires_at_unix` (matches MCP's `handlers_protected.rs` boundary semantics exactly)
- `log::warn!` on Redis GET failure

### `start_server` Redis init (`crates/web.rs:87-113`)
- Env priority: `GOOGLE_OAUTH_REDIS_URL` → `AXON_REDIS_URL` (matches MCP `state.rs`)
- Prefix: `GOOGLE_OAUTH_REDIS_PREFIX` (default: `axon:mcp:oauth`)
- `ConnectionManager::new(client).await` — logs on success or connection failure
- Startup log announces gate active/inactive with prefix

### `ws_upgrade` auth gate (`crates/web.rs:225-265`)
- Added `ConnectInfo(addr)` for client IP in logs
- Explicit `None` branch: rejects before Redis call (no empty-key lookup)
- `log::warn!` with `addr.ip()` on both rejection paths

---

## Commands Executed

| Command | Result |
|---------|--------|
| `cargo check --bin axon` | `Finished` — 0 errors, 0 warnings |
| `git diff crates/web.rs` | Confirmed all changes persisted despite hook confusion |
| `grep -n "connection-manager" Cargo.toml` | Feature already enabled: `features = ["tokio-comp", "connection-manager"]` |

---

## Behavior Changes (Before / After)

| Surface | Before | After |
|---------|--------|-------|
| `/ws` upgrade | Accepted any connection, no auth | Requires `?token=<atk_uuid>` when Redis is configured; 401 otherwise |
| Missing token | Accepted | `401 bearer token required` + warn log |
| Invalid/expired token | Accepted | `401 invalid or expired bearer token` + warn log |
| Redis unreachable | N/A | `401` + `log::warn!` (fail-closed) |
| No Redis configured | N/A | Gate disabled, `/ws` unauthenticated (fallthrough, with startup log) |
| Redis connection | New TCP per WS upgrade | Single `ConnectionManager` shared across all upgrades |
| Startup | Silent | Logs gate status and prefix at boot |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check --bin axon` | 0 errors | 0 errors, 0 warnings | ✅ |
| `git diff crates/web.rs \| grep oauth_redis` | `+    oauth_redis: Option<redis::aio::ConnectionManager>` | Present | ✅ |
| `git diff crates/web.rs \| grep ConnectionManager` | `+        Some(client) => match redis::aio::ConnectionManager::new(client).await` | Present | ✅ |
| `git diff crates/web.rs \| grep "filter.*empty"` | `+        match params.token.as_deref().filter(\|t\| !t.is_empty())` | Present | ✅ |
| `git diff crates/web.rs \| grep "log::warn"` | Multiple warn lines | 4 warn sites | ✅ |
| `grep "allow(dead_code)" crates/web.rs` | 0 matches | 0 matches | ✅ |

---

## Source IDs + Collections Touched

None — no Axon embed/retrieve operations were performed during this session. Session doc will be embedded as part of `save-to-md`.

---

## Risks and Rollback

**Risk:** If `axon serve` process does not have `AXON_REDIS_URL` or `GOOGLE_OAUTH_REDIS_URL` set, the gate is silently disabled and `/ws` remains unauthenticated. This is logged at startup but easy to miss.

**Risk:** The `BearerTokenRecord.expires_at_unix` field name is duplicated between `crates/web.rs` and `crates/mcp/server/oauth_google/types.rs`. If the MCP server renames the field, `web.rs` will silently deserialize `0` (default) and all tokens will appear expired. No compile-time coupling exists.

**Rollback:** Revert `crates/web.rs` to the pre-session state — removes all OAuth gate logic. The `/ws` endpoint returns to unauthenticated (original behavior). Single-file rollback, no migration needed.

---

## Decisions Not Taken

| Alternative | Why Rejected |
|-------------|-------------|
| Add `AXON_WEB_API_TOKEN` to WS auth | Already used by the Next.js proxy for `/api/*`. Adding it to WS would be a second secret to manage and doesn't leverage the existing OAuth session. |
| Cross-crate import of `AccessTokenRecord` | `pub(crate)` boundary prevents it. Would require moving the type to `crates/core` — larger refactor than warranted for this change. |
| Fail-open when Redis is unreachable | Both reviewers recommended fail-closed. A transient Redis outage causing 401 is observable and recoverable; a security gate that opens on errors is not acceptable. |
| Per-request `get_multiplexed_async_connection()` | Replaced with `ConnectionManager` — new TCP connection per upgrade is wasteful and slow. |
| Scope check in WS validation | MCP's `require_google_auth` validates scopes. For the WS gate, any valid non-expired token from a logged-in user is sufficient — scope enforcement would be overly restrictive for an internal UI. |

---

## Open Questions

1. **Frontend integration:** The Next.js app (`apps/web`) needs to pass `?token=<atk_uuid>` when constructing the WS URL. This is not yet done — the gate is in place on the Rust side but the UI will get 401 until the frontend is updated.
2. **Shared constant risk:** `"axon:mcp:oauth"` default prefix exists independently in `crates/web.rs:91` and `crates/mcp/server/oauth_google/state.rs:84`. A rename in one won't fail the build. Should be extracted to `crates/core` in a future refactor.
3. **Token format validation:** Should `validate_bearer_token` reject tokens that don't match the `atk_` prefix before doing a Redis lookup? Low risk today but worth adding if token format ever changes.

---

## Next Steps

1. **Update Next.js frontend** — `apps/web` WebSocket connection setup needs to read the OAuth access token (from session/cookie) and append `?token=<atk_uuid>` to the WS URL
2. **Add shared prefix constant** — move `"axon:mcp:oauth"` to a `pub const` in `crates/core` or document the dependency explicitly in both files
3. **Consider origin check** — add an `Origin` header validation to `/ws` to prevent cross-site WebSocket hijacking (CSWSH), especially once the token gate is established
4. **Update `.env.example`** — confirm `GOOGLE_OAUTH_REDIS_PREFIX` is documented
