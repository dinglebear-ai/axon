# Pulse Chat Debug + Consolidation Tracker
Date: 03/11/2026
Scope: Initial sidebar/session/thinking/log issues + full assistant/sessions consolidation

## 1) Original User Requests (Pre-Consolidation)

1. Debug why only one Gemini session is shown in the sidebar list.
2. Review recent screenshots and fix chain-of-thought issues:
   - one word per line
   - disappears when message completes
   - not working for Gemini/Codex
3. Address logs:
   - `requested session_mode 'accept-edits' is not in ACP mode options`
   - `Unexpected case: {"type":"rate_limit_event",...}`
   - ACP decode error for unknown `usage_update`
   - repeated `codex_core::skills::loader: failed to stat skills entry ...`
   - `ACP load_session failed, falling back: Resource not found`

## 2) Root Cause Summary

### 2.1 Sidebar showing one Gemini session
- Web had separate assistant/sessions data pipelines (route/hook/scanner), which diverged behavior.
- ACP persistent connection pinned a single backend session and did not reliably switch per-turn session targets.

### 2.2 Chain-of-thought rendering issues
- Thinking chunks were appended as separate fragments, creating very fragmented rendering in the Chain of Thought UI.
- Session sync could overwrite richer live message metadata (thinking/tool blocks) with minimal historical JSONL messages.

### 2.3 Log noise
- Session mode value mismatch between UI naming (`accept-edits`) and adapter options (`accept_edits` in some adapters).
- `agent-client-protocol` crate version does not fully model `usage_update` payloads, producing decode noise.
- Codex skills loader emits many non-fatal stderr lines for missing symlink targets.
- `rate_limit_event` stderr chatter is non-fatal adapter telemetry.

## 3) Implemented Fixes

### 3.1 Complete sessions pipeline consolidation
- Deleted duplicate assistant hook/API/scanner paths.
- Unified to one sessions API endpoint + one scanner module + one hook path.
- Assistant behavior preserved as optioned filtering within the shared scanner path.

Changed:
- `apps/web/lib/sessions/session-scanner.ts`
- `apps/web/app/api/sessions/list/route.ts`
- `apps/web/hooks/use-recent-sessions.ts`
- `apps/web/components/reboot/axon-shell.tsx`

Removed:
- `apps/web/lib/sessions/assistant-scanner.ts`
- `apps/web/hooks/use-assistant-sessions.ts`
- `apps/web/app/api/assistant/sessions/route.ts`

### 3.2 ACP per-turn session handling
- Updated persistent ACP runtime to load/switch/create session per turn as needed.
- Updated runtime state to track mutable current session id (not single-init lock semantics).

Changed:
- `crates/services/acp/persistent_conn.rs`
- `crates/services/acp/bridge.rs`
- `crates/services/acp/runtime.rs`

### 3.3 Chain-of-thought behavior fixes
- Coalesced thinking delta chunks to reduce one-word-per-line fragmentation.
- Preserved richer live metadata (thinking/tool blocks) during historical sync to prevent post-complete disappearance.

Changed:
- `apps/web/hooks/use-axon-acp.ts`
- `apps/web/components/reboot/live-message-sync.ts`
- `apps/web/components/reboot/axon-shell.tsx`

### 3.4 Log handling improvements
- Added mode alias tolerance (`accept-edits` ↔ `accept_edits`).
- Suppressed ACP decode noise from `agent_client_protocol::rpc`.
- Filtered noisy non-fatal codex skills loader stderr lines.
- Downgraded known non-fatal rate-limit stderr chatter.

Changed:
- `crates/services/acp/persistent_conn.rs`
- `crates/core/logging.rs`
- `crates/services/acp/session.rs`

## 4) Verification Evidence

### 4.1 Rust verification
- `cargo check` passed.
- `cargo test resolve_mode_option_accepts_hyphen_underscore_alias --lib` passed.

### 4.2 Web verification
- `pnpm vitest run __tests__/api/sessions-routes.test.ts __tests__/sessions/unified-assistant-sessions.test.ts __tests__/sessions/scanner.test.ts __tests__/live-message-sync.test.ts` passed.

## 5) Request-by-Request Status

1. One Gemini session in sidebar
- Status: Addressed.
- Reason: shared session pipeline + per-turn ACP session handling now implemented.

2. Chain-of-thought issues
- Status: Addressed in code.
- Reason: delta coalescing + live/historical merge behavior updated.

3. Log issues
- `accept-edits` mode warning: addressed via alias handling.
- `rate_limit_event` unexpected-case noise: handled as non-fatal low-noise log path.
- `usage_update` decode error noise: suppressed at logging filter level.
- codex skills loader spam: filtered from forwarded adapter stderr.
- `load_session failed, falling back`: retained as meaningful fallback signal (still expected to appear when requested session is missing).

## 6) Remaining Follow-Ups (Optional)

1. Rename `apps/web/__tests__/sessions/assistant-scanner.test.ts` to reflect unified scanner naming. ✅
2. Update any docs that still mention assistant-specific API route/hook/scanner files. ✅
3. Add integration test for ACP multi-turn session switch sequence (new session -> existing session -> fallback). ✅
   - Added: `crates/web/execute/tests/acp_ws_event_tests.rs::acp_multi_turn_session_switch_sequence_in_ws_pipeline`
