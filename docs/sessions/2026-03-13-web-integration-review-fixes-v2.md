# Session: Web Integration Review — Full Issue Resolution
**Date:** 2026-03-13
**Branch:** `feat/web-integration-review-fixes`
**Session duration:** ~1.5 hours

---

## Session Overview

Starting from `WEB-INTEGRATION-REVIEW.md` (45 issues: 5 Critical, 12 High, 16 Medium, 12 Low), this session:
1. Dispatched two Haiku verification agents to audit which issues were already addressed from the prior session
2. Identified 23 fixed, 13 partial, 9 not-fixed
3. Split remaining work across 3 parallel specialized agents (2 Rust, 1 TypeScript) with strict file ownership boundaries to prevent conflicts
4. All agents completed cleanly — `cargo check` and `pnpm tsc --noEmit` both passed

---

## Timeline

| Time | Activity |
|------|----------|
| Start | User asked to verify all WEB-INTEGRATION-REVIEW.md issues were addressed |
| ~5 min | Dispatched 2 Haiku verification agents (Critical+High vs Medium+Low) |
| ~15 min | Received verification results: 23 ✅ / 13 ⚠️ / 9 ❌ |
| ~20 min | User asked if remaining issues were all Rust — determined split: ~8 Rust, 2 TS, 1 docs, 1 mixed |
| ~25 min | Dispatched 3 parallel specialized agents with disjoint file ownership |
| ~60 min | All agents returned: `cargo check` clean, `pnpm tsc --noEmit` clean, 863 tests passing |
| End | User asked about H-6 migration, then requested save-to-md |

---

## Key Findings

### Verification Results (before fix agents)

| Severity | Fixed | Partial | Not Fixed |
|----------|-------|---------|-----------|
| Critical (5) | 5 | 0 | 0 |
| High (12) | 11 | 3 (H-1, H-5, H-6) | 0 |
| Medium (16) | 6 | 6 | 4 (M-5, M-6, M-12, M-14) |
| Low (12) | 1 | 4 | 7 |

### H-1 Was Already Fixed
The verification found `permission_request` was already fully wired in TypeScript (`ws-protocol.ts`, `WsServerMsg`, `parseAcpWsMessage`, `isAcpRelevantWsMessage`) — the partial status was a false negative from the first verification pass.

### M-16 / L-2 Were Already Fixed
`biased;` already present in main forward loop. `crawl_files` detection already using typed `MsgType` deserialization — prior session had done both.

### M-14 Stronger Than Expected
The loopback bypass wasn't just logged — it was removed entirely, replaced with uniform token authentication. Stronger than the originally requested fix.

### H-6 Remains Architectural Debt
The `AXON_EDITOR_SYSTEM_PROMPT_PREAMBLE` was made `pub` and documented with a TODO, but the full migration into `crates/services/acp/` was not done. WS path (Rust) and SSE path (TypeScript `buildPulseSystemPrompt`) remain divergent.

---

## Technical Decisions

**Split Rust work across 2 agents by file domain** — `ws_handler.rs`/`docker_stats.rs`/`session_cache.rs` vs `web.rs`/`execute/`. Zero file overlap guaranteed no conflicts.

**TypeScript agent also handled docs/env** — L-1, M-7, M-3 env additions were consolidated into one agent pass over `.env.example` and `CLAUDE.md`.

**H-5 orphaned types removed** — Chose full removal over documented dead code. Dead variants in an enum are noise; poll-only semantics documented in `CLAUDE.md` instead.

**L-3 duplicate terminal event removed** — `command.done` no longer emitted on cancel. `job.cancel.response` is sole terminal event for cancels.

**L-8 `unsafe-inline` gated to dev** — Production CSP `script-src` now `'self'` only. Breaking change for any staging that relied on inline scripts.

---

## Files Modified

### Rust files (Agents 1 & 2)
| File | Issues Addressed |
|------|-----------------|
| `crates/web/ws_handler.rs` | M-5 (crawl_job_ids Vec), M-6 (non-destructive replay), M-12 (stats opt-in subscription), M-16 (biased; confirmed), L-2 (typed detection confirmed) |
| `crates/web/docker_stats.rs` | M-12 (no changes needed — filtering happens per-connection) |
| `crates/services/acp/session_cache.rs` | M-6 (`read_replay_buffer()` non-destructive method added) |
| `crates/web.rs` | L-6 (comment), L-7 (`AXON_MAX_WS_CONNECTIONS`), L-12 (`AXON_MAX_SHELL_CONNECTIONS`), M-14 (already removed) |
| `crates/web/execute/events.rs` | H-5 (removed `JobStatusPayload`, `JobProgressPayload`, dead enum variants) |
| `crates/web/execute/tests/ws_event_v2_tests.rs` | H-5 (removed associated dead tests) |
| `crates/web/execute/sync_mode/pulse_chat.rs` | H-6 (made `pub`, added TODO migration comment) |
| `crates/web/execute/sync_mode/service_calls.rs` | M-10 (`sanitize_svc_error()` — logs full error, returns generic to client) |
| `crates/web/execute.rs` | M-11 (queued status event before semaphore wait) |
| `crates/web/execute/cancel.rs` | L-3 (removed `send_done_dual` from cancel path) |

### TypeScript/docs files (Agent 3)
| File | Issues Addressed |
|------|-----------------|
| `apps/web/lib/ws-protocol.ts` | M-2 (removed `job_id?` from cancel variant) |
| `apps/web/components/omnibox/hooks/use-omnibox-execution.ts` | M-2 (removed dead `job_id` property from cancel call) |
| `apps/web/components/results/job-lifecycle-renderer.tsx` | M-2 (removed dead `job_id` property from cancel call) |
| `apps/web/next.config.ts` | M-3 (`SHELL_SERVER_HOST` env var for shell WS rewrite) |
| `apps/web/lib/server/csp.ts` | L-8 (`unsafe-inline` gated to dev-only) |
| `apps/web/lib/download-urls.ts` | L-4 (sync-warning comment, `DOWNLOAD_URL_PATTERNS` export) |
| `apps/web/__tests__/download-urls.test.ts` | L-4 (new structural test) |
| `.env.example` | L-1, M-3, M-7 (`AXON_WEB_BROWSER_API_TOKEN`, `SHELL_SERVER_HOST` documented) |
| `CLAUDE.md` | M-7 (three-token table replacing "one token" claim) |
| `apps/web/CLAUDE.md` | M-7 (accurate three-token description) |

---

## Behavior Changes (Before/After)

| Area | Before | After |
|------|--------|-------|
| Multi-crawl cancel | Only last job ID tracked; implicit cancel hit wrong job | All job IDs accumulated; implicit cancel hits all |
| ACP session reconnect | Second reconnect got empty replay buffer | Replay buffer non-destructive; cursor-based read |
| Docker stats | All WS clients receive stats every 500ms | Opt-in via `subscribe_stats` message; unsubscribed clients receive nothing |
| Service errors to browser | Full `e.to_string()` — file paths, SQL, AMQP URIs exposed | Generic "Internal service error"; full detail logged server-side |
| ACP semaphore wait | Silent 30s hang | Immediate `{"type":"status","phase":"queued"}` event sent to client |
| Cancel terminal events | Both `job.cancel.response` AND `command.done` emitted | Only `job.cancel.response` on cancel |
| WS connection limit | Unlimited — FD exhaustion possible | 503 when `AXON_MAX_WS_CONNECTIONS` (default 100) exceeded |
| Shell connection limit | Unlimited PTY subprocesses | 503 when `AXON_MAX_SHELL_CONNECTIONS` (default 10) exceeded |
| Production CSP | `script-src 'self' 'unsafe-inline'` | `script-src 'self'` in prod; `unsafe-inline` dev-only |
| cancel type (`job_id?`) | TypeScript had `job_id?` field Rust silently ignored | Field removed from TS type; `id` is canonical cancel key |

---

## Verification Evidence

| Check | Expected | Actual | Status |
|-------|----------|--------|--------|
| `cargo check` (Rust Agent 1) | 0 errors, 0 warnings | 0 errors, 0 warnings | ✅ |
| `cargo check` (Rust Agent 2) | 0 errors, 0 warnings | 0 errors, 0 warnings | ✅ |
| `pnpm tsc --noEmit` | 0 errors | 0 errors | ✅ |
| `pnpm test -- download-urls` | 8 passing | 8 passing | ✅ |
| Full TS test suite | 863 passing | 863 passing | ✅ |
| Rust Agent 1 related tests | 27 passing | 27 passing | ✅ |

---

## Risks and Rollback

**L-8 CSP change** — Removing `unsafe-inline` from prod `script-src` is a breaking change if any production path uses inline scripts. Monitor for CSP violations in browser console after deploy. Rollback: revert `csp.ts:71`.

**L-3 cancel terminal events** — Frontend components that listened for `command.done` to clean up streaming state on cancel will no longer receive it. They must handle `job.cancel.response` as terminal. Verify all cancel UI flows in browser after deploy.

**M-12 stats opt-in** — Any frontend code that renders stats without first sending `subscribe_stats` will see no stats data. The apps/web stats widget must be updated to send the subscription message on mount.

---

## Decisions Not Taken

- **H-6 full services-layer migration** — Moving `AXON_EDITOR_SYSTEM_PROMPT_PREAMBLE` into `crates/services/acp/` requires understanding SSE path bridging and may require cross-language constant sharing. Deferred to next sprint. Unblocked with `pub` visibility + TODO comment.
- **H-5 implement Redis pub/sub push** — Full push model for job.status/job.progress would require Redis pub/sub integration in the WS path. Chose removal of dead types over partial implementation.
- **M-12 server-side subscription tracking** — Could have tracked subscriptions in Rust only, or also required TypeScript to explicitly manage. Went with Rust-side `AtomicBool` per connection, TypeScript sends subscribe/unsubscribe messages.

---

## Open Questions

- Does `apps/web` stats widget send `subscribe_stats` after M-12? If not, stats widget will be blank after deploy.
- H-6 migration: does the TypeScript SSE path (`apps/web/app/api/pulse/chat/route.ts`) route through any Rust service, or is it fully TypeScript-owned? This determines whether a shared Rust constant is even possible.
- L-4 integration test: the new `download-urls.test.ts` is structural only — no running server required. A true round-trip test against the Rust routes would require a test server fixture.

---

## Next Steps

1. **Verify stats widget** — Check `apps/web` stats component sends `subscribe_stats` on mount (M-12 behavioral change)
2. **Verify cancel UI** — Test all cancel flows in browser to confirm `job.cancel.response` is handled as terminal (L-3 change)
3. **H-6 investigation** — Dispatch investigation agents to map the SSE path and design the services-layer migration (user was about to do this when save-to-md was requested)
4. **`cargo test`** — Full test suite run not executed this session; run before merge
5. **PR** — Branch `feat/web-integration-review-fixes` ready for PR after test suite passes
