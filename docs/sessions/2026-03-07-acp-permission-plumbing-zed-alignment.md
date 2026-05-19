# ACP Permission Plumbing + Zed Alignment Patterns

**Date:** 2026-03-07
**Branch:** `feat/services-layer-refactor`

## Session Overview

Completed ACP (Agent Client Protocol) permission response plumbing — the final integration gap after 5 parallel agents implemented Zed alignment patterns. Wired `permission_response` WebSocket messages from the Next.js frontend through the Rust WS handler, execute bridge, and into the ACP bridge client. Also fixed 3 pre-existing TypeScript build errors.

## Timeline

1. **Verified 5-agent outputs** — Confirmed all parallel agent changes (session list/resume, tool call terminal, permission UI, process exit monitoring, targeted entry updates) compiled cleanly with no conflicts
2. **Identified remaining gap** — Traced the full call chain and found `permission_response` WS messages had no Rust handler
3. **Implemented permission plumbing** — Added `PermissionResponderMap` type, cross-runtime oneshot channels, auto-approve fallback with 60s timeout
4. **Fixed TS build errors** — `route.ts:91` model type, `claude-stream-types.ts:169` model lookup, `pulse-chat-helpers.ts` agent type
5. **Verification** — `cargo check`, `cargo test` (853 pass), `pnpm build`, `pnpm test` (647 pass)

## Key Findings

- **Cross-runtime communication**: ACP SDK runs on `current_thread` tokio runtime inside `spawn_blocking`; WS handler runs on multi-threaded runtime. Must use `std::sync::Mutex` (not `tokio::sync`) for the shared map
- **`PermissionResponderMap`** type: `Arc<std::sync::Mutex<HashMap<String, tokio::sync::oneshot::Sender<String>>>>` — per-connection, created in `handle_ws`
- **Auto-approve fallback**: 60s timeout on frontend response prevents session hangs; controlled by `AXON_ACP_AUTO_APPROVE` env var (default `true`)
- **`Arc<str>` comparison**: `opt.option_id.0 == option_id` fails — need `*opt.option_id.0 == *option_id` for deref comparison (`crates/services/acp.rs:1502`)

## Technical Decisions

| Decision | Rationale |
|----------|-----------|
| `std::sync::Mutex` over `tokio::sync::Mutex` | Inner runtime is `current_thread` — tokio mutex would deadlock |
| `oneshot` channel per permission request | Each request gets exactly one response; channel is dropped after use |
| 60s timeout with auto-approve fallback | Prevents indefinite session hangs if frontend disconnects |
| `AXON_ACP_AUTO_APPROVE` env var | Allows disabling frontend permission flow entirely (default: auto-approve) |
| Per-connection `PermissionResponderMap` | Permissions are scoped to a WS connection, not global |

## Files Modified

| File | Purpose |
|------|---------|
| `crates/services/acp.rs` | Added `PermissionResponderMap` type, `resolve_acp_auto_approve()`, `auto_approve_outcome()`, rewrote `request_permission()` with oneshot channel + timeout |
| `crates/services/types.rs` | Re-export of `PermissionResponderMap` |
| `crates/web.rs` | Added `permission_response` WS message handler, `tool_call_id`/`option_id` fields to `WsClientMsg`, per-connection responder map |
| `crates/web/execute.rs` | Added `permission_responders` parameter to `handle_command` |
| `crates/web/execute/sync_mode.rs` | Threaded `permission_responders` through `handle_sync_direct` -> `dispatch_service` -> `handle_pulse_chat` -> `start_prompt_turn` |
| `apps/web/app/api/pulse/chat/route.ts:91` | Fixed `model: req.model` -> `model: req.model ?? ''` |
| `apps/web/app/api/pulse/chat/claude-stream-types.ts:169` | Fixed model lookup null safety |
| `apps/web/hooks/pulse-chat-helpers.ts` | Changed `agent: string` -> `agent: PulseAgent` in `PromptConfig` |

## Behavior Changes (Before/After)

| Behavior | Before | After |
|----------|--------|-------|
| ACP permission requests | Always auto-approved server-side | Can route to frontend for user decision (with `AXON_ACP_AUTO_APPROVE=false`) |
| Frontend `permission_response` WS messages | Silently ignored (no handler) | Routed to correct ACP session via oneshot channel |
| Permission timeout | N/A | 60s timeout falls back to auto-approve |
| `pnpm build` | Failed on 3 type errors | Passes cleanly |

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check --locked` | PASS | PASS | PASS |
| `cargo test --locked` | All pass | 853 passed, 1 pre-existing (Qdrant) | PASS |
| `pnpm build` | PASS | PASS | PASS |
| `pnpm test` | PASS | 647/647 pass | PASS |

## Risks and Rollback

- **Low risk**: Permission plumbing is behind `AXON_ACP_AUTO_APPROVE` (default `true`), so existing behavior is unchanged unless explicitly opted in
- **Rollback**: Revert the commit; auto-approve was the previous behavior and remains the default

## Decisions Not Taken

- **Bidirectional streaming for permissions**: Considered using mpsc channels for multi-response flows — rejected because ACP permissions are request/response (oneshot is correct)
- **Global permission map**: Considered a single shared map for all connections — rejected because permissions are per-session/per-connection

## Open Questions

- `AXON_ACP_AUTO_APPROVE` not yet added to `.env.example` or deployment docs
- Frontend permission UI rendering (the 5th agent's work) needs integration testing with real ACP adapter
- Whether 60s timeout is appropriate for all permission types (some may need longer user consideration)

## Next Steps

- Add `AXON_ACP_AUTO_APPROVE` to `.env.example` and `docs/DEPLOYMENT.md`
- Integration test: end-to-end permission flow with real ACP adapter subprocess
- Commit all changes on `feat/services-layer-refactor` branch
