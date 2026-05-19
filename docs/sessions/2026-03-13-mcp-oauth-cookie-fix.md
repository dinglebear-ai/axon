# Session: MCP OAuth Constant Re-Auth Bug Fix
**Date:** 2026-03-13
**Branch:** feat/github-code-aware-chunking
**Duration:** Single focused debugging session

---

## Session Overview

User reported constant re-authentication prompts when using the Axon MCP server via Claude Code. Investigated the OAuth flow, identified a browser cookie security prefix violation as the root cause, and fixed it with a 4-test verification suite.

---

## Timeline

| Time | Activity |
|------|----------|
| Start | User reports constant MCP re-auth even with Redis configured |
| +5m | Investigated OAuth token TTLs — access token 1hr, refresh 30d — ruled out as primary cause |
| +10m | Traced `__Host-` cookie prefix requirements — found root cause |
| +15m | Implemented fix: dynamic cookie name based on broker scheme |
| +20m | Added 4 regression tests, all passing |

---

## Key Findings

### Root Cause: `__Host-` Cookie Security Prefix on HTTP

**File:** `crates/mcp/server/oauth_google/types.rs:5`

```rust
pub(crate) const OAUTH_SESSION_COOKIE: &str = "__Host-axon_oauth_session";
```

The `__Host-` prefix is a [browser cookie security prefix](https://developer.mozilla.org/en-US/docs/Web/HTTP/Cookies#cookie_prefixes) that **mandates** the `Secure` flag. Without it, browsers **silently drop** the cookie — no error, no warning, just gone.

**File:** `crates/mcp/server/oauth_google/helpers.rs` (`is_secure_cookie`)
The `Secure` flag is only added when `broker_issuer.starts_with("https://")`. In local dev (HTTP), it's never added.

**Consequence:**
1. User authenticates with Google OAuth
2. Callback sets `__Host-axon_oauth_session` cookie **without** `Secure`
3. Browser silently drops the cookie
4. Next `/oauth/authorize` call: no session cookie found → "not authenticated" → redirect to Google login
5. Repeat forever — even with Redis fully operational

**Why Redis didn't help:** The session token was being persisted to Redis correctly. The problem was upstream — the browser never sent the cookie back, so `is_authenticated()` always returned `false`, always redirecting to login before the token could be looked up.

### Secondary Observation: 1-Hour Access Token TTL

`handlers_protected.rs:149` — `expires_in = 3600_u64`. If the MCP client doesn't auto-refresh via the `refresh_token` grant, re-auth would still happen every hour. This is a separate, lower-priority issue.

---

## Technical Decisions

### Dynamic Cookie Name vs. Dropping `__Host-` Entirely

**Chosen:** `session_cookie_name(state: &GoogleOAuthState) -> &'static str` — returns `"__Host-axon_oauth_session"` on HTTPS and `"axon_oauth_session"` on HTTP.

**Rationale:** Preserves the security enhancement (`__Host-` + `Secure`) for users running behind HTTPS (e.g., Tailscale funnel, reverse proxy). Fixes the HTTP case without regressing HTTPS deployments.

**Alternative rejected:** Change the constant to `"axon_oauth_session"` unconditionally. Simpler, but loses `__Host-` security hardening on HTTPS setups.

### Placement of `session_cookie_name`

Added to `helpers.rs` (not `types.rs`) alongside `is_secure_cookie` since it's a derived value, not a type constant. Both `build_session_set_cookie` and `build_session_clear_cookie` already live in `helpers.rs`, so colocation is natural.

---

## Files Modified

| File | Change |
|------|--------|
| `crates/mcp/server/oauth_google/types.rs` | Replaced single `OAUTH_SESSION_COOKIE` const with `OAUTH_SESSION_COOKIE_SECURE` (`__Host-axon_oauth_session`) and `OAUTH_SESSION_COOKIE_PLAIN` (`axon_oauth_session`) |
| `crates/mcp/server/oauth_google/helpers.rs` | Added `session_cookie_name(state)` function; updated `build_session_set_cookie` and `build_session_clear_cookie` to use it |
| `crates/mcp/server/oauth_google/handlers_google.rs` | Replaced 3 uses of static `OAUTH_SESSION_COOKIE` with `session_cookie_name(&state)`; updated import |
| `crates/mcp/server/oauth_google/handlers_broker.rs` | Added `session_cookie_name` to helpers import; 1 usage already rewritten by linter |
| `crates/mcp/server/oauth_google/tests.rs` | Added 4 new regression tests + 2 state builder helpers (`make_http_oauth_state`, `make_https_oauth_state`) |

---

## Behavior Changes (Before/After)

| Scenario | Before | After |
|----------|--------|-------|
| Local dev (HTTP broker) | `Set-Cookie: __Host-axon_oauth_session=...; HttpOnly` — browser drops it, loops forever | `Set-Cookie: axon_oauth_session=...; HttpOnly; SameSite=Lax` — browser stores it, auth persists |
| HTTPS deployment | `Set-Cookie: __Host-axon_oauth_session=...; Secure; HttpOnly` — works | Unchanged — same behavior |
| Redis-backed sessions | Stored correctly, but never read because cookie never sent | Sessions now properly round-trip through browser ↔ server |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check --bin axon` | 0 errors | `Finished dev` | ✅ |
| `cargo test --lib -- crates::mcp::server::oauth_google::tests` | 16 passed, 0 failed | `16 passed; 0 failed` | ✅ |
| `session_cookie_name_is_plain_on_http` | `axon_oauth_session` | `axon_oauth_session` | ✅ |
| `session_cookie_name_uses_host_prefix_on_https` | `__Host-axon_oauth_session` | `__Host-axon_oauth_session` | ✅ |
| `build_session_set_cookie_on_http_has_no_secure_flag_and_plain_name` | plain name, no `Secure` | plain name, no `Secure` | ✅ |
| `build_session_set_cookie_on_https_has_secure_flag_and_host_prefix` | `__Host-` + `Secure` | `__Host-` + `Secure` | ✅ |

---

## Source IDs + Collections Touched

None — no Axon embed/retrieve operations performed during this session.

---

## Risks and Rollback

**Risk:** Clients that authenticated before this fix stored a cookie named `__Host-axon_oauth_session`. After the fix, the server looks for `axon_oauth_session`. Any existing valid session is effectively invalidated (one-time re-auth on next connection). This is acceptable and expected.

**Rollback:** Revert the 5 modified files. The re-auth loop returns, but no data is lost.

**No risk to:** Redis state, Qdrant, job queues, or any other subsystem. The change is purely in cookie name selection.

---

## Decisions Not Taken

- **`AXON_MCP_API_KEY` static key**: Suggested as a workaround early in the session. User confirmed Redis was configured, indicating OAuth was the intended auth path, not a static key. Fix addresses root cause instead.
- **Increase access token TTL**: The 1-hour `atk_` TTL is a separate concern. Addressing it would require verifying that Claude Code's MCP client implements `refresh_token` grant properly. Not in scope for this session.
- **`__Host-` dropped entirely**: Would simplify the code but lose HTTPS security benefit. Rejected in favor of dynamic selection.

---

## Open Questions

- Does Claude Code's MCP client automatically use the `refresh_token` grant when an `atk_` expires (1-hour TTL)? If not, re-auth every hour is a separate outstanding issue.
- Are there any other MCP clients (e.g., Cursor, other tools) connecting to this server that might have cached the old `__Host-axon_oauth_session` cookie name and need a forced re-auth after deploy?

---

## Next Steps

1. Rebuild and restart `axon mcp` to pick up the fix
2. Re-authenticate once (existing sessions invalidated by cookie name change — expected)
3. Verify auth persists across multiple tool calls without re-prompt
4. If hourly re-auth still occurs: investigate whether Claude Code sends `refresh_token` grant to `/oauth/token`, and if not, consider increasing `expires_in` in `handlers_protected.rs:149`
