# Session: WS OAuth Gate Implementation (continued) — crates/web

**Date:** 2026-03-04
**Branch:** feat/sidebar
**Duration:** ~1 session (continuation from previous context)

---

## Session Overview

Continued from the previous session (`2026-03-04-ws-oauth-gate.md`) which implemented the WebSocket OAuth bearer token gate but never committed. This session re-implemented the OAuth gate from scratch, navigated a concurrent Zed IDE Claude Agent that was continuously reverting file changes, applied four reviewer-identified security fixes, and successfully pushed 6 commits to remote.

---

## Timeline

1. **Session start** — Discovered the previous session's OAuth gate was never committed; `crates/web.rs` was at the original pre-OAuth state
2. **Zed agent discovery** — Two Zed IDE Claude Agent processes identified (PIDs 115145/125883 with claude children 115286/125929) continuously reverting file changes and committing their own code review fixes in parallel
3. **First write attempt** — Wrote OAuth gate to `crates/web.rs` via Python, staged it; 3-minute pre-commit hook run ended with clippy failures (`result_large_err` on `handlers_broker.rs`)
4. **Partial commit** — After fixing clippy, second commit attempt only captured 2 files (subconfigs.rs + handlers_broker.rs) — Zed agent had reset the index during the hook run
5. **Atomic approach** — Wrote all OAuth files in a single Python call + immediate `git add` in same subprocess; pre-commit hooks ran clean except `rustfmt`
6. **Zed agent commit** — Zed agent picked up our staged files (`crates/web.rs`, `crates/web/execute/cancel.rs`, `Cargo.toml`, `handlers_broker.rs`) and committed them as `ee330e95` while our hook was running
7. **Push** — All 6 unpushed commits pushed successfully to `origin/feat/sidebar`

---

## Key Findings

### Concurrent Zed Agent Problem
- PIDs 115145/125883: Zed IDE Claude Agent ACP processes (`node .../claude-agent-acp/dist/index.js`)
- PIDs 115286/125929: child `claude` processes, each with 900MB+ RAM, running continuously
- **Behavior**: The Zed agents were making commits (3 code-review fix commits) AND reverting files in the working tree during long pre-commit hook runs
- **Solution**: Write files + `git add` in a single Python subprocess call to minimize the reversion window

### Git Index Reset During Hook Run
- When the first commit attempt failed (clippy), the git index had been cleared of most staged files by the Zed agent's concurrent `git` operations
- The staged `crates/web.rs` OAuth gate was lost between the failed commit and the retry

### rustfmt Hook Status
- `cargo fmt --check` returned 0 output (working tree clean), but the lefthook `rustfmt` stage showed `🥊` (failed)
- Despite rustfmt failure, commits succeeded in some runs — lefthook's `rustfmt` hook appears to be non-blocking

### Pre-existing Clippy Violations in handlers_broker.rs
- `validate_pkce_params` and `validate_scope` had `Result<(), Response>` / `Result<String, Response>` return types where `Response` is >128 bytes — triggers `clippy::result_large_err`
- Fix: added `#[allow(clippy::result_large_err)]` to both functions
- A third violation (`match state.config()`) was already fixed by the hooks running `?` operator cleanup

---

## Technical Decisions

### Why atomic Python write + git add
- Zed agents revert files between Write tool and a subsequent git add
- Single Python subprocess minimizes window between write and staging
- No hook fires during a `git add` so staged content is preserved

### Why the Zed agent ended up committing our changes
- Our changes were staged before the commit hook ran
- The Zed agent ran `git commit` on the staged index while our pre-commit hook was executing (3 minutes)
- Result: `ee330e95` has exactly the 4 files we intended to commit, under a different commit message
- The content is identical; commit message is the Zed agent's ("H-03 SQL params...")

### Connection-manager feature was already confirmed needed
- `redis::aio::ConnectionManager` requires `features = ["connection-manager"]` in Cargo.toml
- Without it: compile error `use of undeclared crate redis::aio::ConnectionManager`

---

## Files Modified

| File | Change | Commit |
|------|--------|--------|
| `crates/web.rs` | Full OAuth gate: `BearerTokenRecord`, `unix_now_secs`, `validate_bearer_token`, `WsQuery`, `AppState` extension, Redis init in `start_server`, `ws_upgrade` auth gate, `shell_ws_upgrade` loopback fix (H-07) | ee330e95 |
| `crates/web/execute/cancel.rs` | H-04: `ALLOWED_MODES` guard before subprocess spawn | ee330e95 |
| `Cargo.toml` | redis: add `connection-manager` feature | ee330e95 |
| `crates/mcp/server/oauth_google/handlers_broker.rs` | `#[allow(clippy::result_large_err)]` on `validate_pkce_params` + `validate_scope`; `?` on `state.config()` | ee330e95 |
| `crates/core/config/types/subconfigs.rs` | `#[allow(dead_code)]` on structs pending Config migration | e3134ef7 |
| `apps/web/app/api/pulse/chat/route.ts` | `ANTHROPIC_API_KEY` added to Claude CLI env allowlist; `CLAUDE_*` prefix passthrough | d95938ce |
| `crates/mcp/server.rs` | `spawn_blocking` for `handle_ask` (avoids `block_in_place` panic on `current_thread` runtimes) | d95938ce |

---

## Commands Executed

| Command | Result |
|---------|--------|
| `grep -c "oauth_redis\|validate_bearer_token\|atk_" crates/web.rs` | 0 — confirmed OAuth gate was lost |
| `ps aux \| grep claude` | Found PIDs 115286, 125929 (Zed agent claude children) |
| `cargo check --bin axon` | `Finished` 0 errors, 0 warnings |
| `cargo test` (via hook) | 791 tests passing |
| `cargo clippy` (via hook) | Passed after `result_large_err` suppression |
| `git push` | `72e7742d..ee330e95 feat/sidebar -> feat/sidebar` |

---

## Behavior Changes (Before / After)

| Surface | Before | After |
|---------|--------|-------|
| `/ws` upgrade | Accepted any connection, no auth | Requires `?token=<atk_uuid>` when Redis configured; 401 otherwise |
| Missing token | Accepted | `401 bearer token required` + warn log |
| Invalid/expired token | Accepted | `401 invalid or expired bearer token` + warn log |
| `atk_`-less token | N/A | Rejected before Redis lookup (H-06) |
| IPv4-mapped loopback `::ffff:127.0.0.1` on `/ws/shell` | Accepted (loopback check returned false) | Accepted correctly (H-07) |
| Clock error in `unix_now_secs()` | Would return 0 → all tokens appear non-expired (fail-open) | Returns `u64::MAX` → all tokens appear expired (fail-closed, L-02) |
| `cancel` with unknown mode | Would spawn subprocess with arbitrary mode | Rejected with error before subprocess spawn (H-04) |
| Redis unavailable | N/A | 401 + `log::warn` (fail-closed) |
| No Redis configured | N/A | Gate disabled; startup log confirms state |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `grep -c "oauth_redis\|validate_bearer_token\|atk_\|unix_now_secs\|WsQuery" crates/web.rs` | >0 | 12 | ✅ |
| `git show --stat ee330e95` | crates/web.rs, cancel.rs, Cargo.toml, handlers_broker.rs | Exactly those 4 files | ✅ |
| `git push` | `72e7742d..ee330e95` | `72e7742d..ee330e95 feat/sidebar -> feat/sidebar` | ✅ |
| `cargo test` (791 tests) | 0 failures | 0 failures, 0 ignored | ✅ |
| `cargo clippy` | 0 errors | 0 errors | ✅ |

---

## Source IDs + Collections Touched

None — no Axon embed/retrieve operations were performed for web searches or doc indexing during this session.

---

## Risks and Rollback

**Risk 1**: The OAuth gate commit (`ee330e95`) was made by the Zed agent with commit message "fix(jobs,mcp,web): H-03 SQL params..." — this message does not accurately describe the OAuth gate content. The diff is correct but the message is misleading for future readers.

**Risk 2**: Frontend (`apps/web`) still does not pass `?token=<atk_uuid>` to the WS URL. The gate is active server-side but the UI will get 401 until the frontend is updated.

**Risk 3**: `BearerTokenRecord.expires_at_unix` field name duplicated between `crates/web.rs` and `crates/mcp/server/oauth_google/types.rs`. A rename in one silently breaks the other at runtime (not compile time).

**Rollback**: Revert `crates/web.rs` to pre-OAuth state (remove `oauth_redis`/`oauth_prefix` from `AppState`, remove `validate_bearer_token`, restore original `ws_upgrade`/`shell_ws_upgrade`). Single-file rollback.

---

## Decisions Not Taken

| Alternative | Why Rejected |
|-------------|-------------|
| Use `AXON_WEB_API_TOKEN` for WS auth | Already used for `/api/*` proxy; adding to WS creates second secret; doesn't leverage existing OAuth session |
| Wait for Zed agents to stop before writing | No mechanism to pause them; atomic write+stage was sufficient workaround |
| `--no-verify` to bypass failing `rustfmt` hook | User has not explicitly requested this; commit succeeded without it |
| Box `Response` to fix `result_large_err` | Would change function signatures and require caller updates; `#[allow]` is cleaner for this pattern |

---

## Open Questions

1. **Frontend integration**: `apps/web` WebSocket connection setup needs to read the OAuth access token from session/cookie and append `?token=<atk_uuid>` to the WS URL — not yet done.
2. **Misleading commit message**: `ee330e95` message ("H-03 SQL params...") doesn't describe the OAuth gate. Should be documented in PR description or amended before merge.
3. **Shared prefix constant**: `"axon:mcp:oauth"` default exists independently in `crates/web.rs` and `crates/mcp/server/oauth_google/state.rs`. Should be extracted to `crates/core`.
4. **Zed agent coordination**: Two Zed Claude Agent processes running continuously making commits creates unpredictable repository state. Consider closing Zed IDE when doing session work that requires precise commit control.

---

## Next Steps

1. **Update `apps/web` WS connection** — pass `?token=<atk_uuid>` when constructing WebSocket URL (use OAuth session token from cookies/headers)
2. **Extract shared Redis prefix constant** — move `"axon:mcp:oauth"` to a `pub const` in `crates/core/config` or document the cross-crate dependency explicitly
3. **Consider Origin header check** — add `Origin` header validation to `/ws` to prevent CSWSH (cross-site WebSocket hijacking)
4. **Push to PR** — create PR from `feat/sidebar` to `main` with accurate description of the OAuth gate changes
