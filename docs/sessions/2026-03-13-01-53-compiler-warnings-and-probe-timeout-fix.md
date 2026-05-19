# Session: Compiler Warnings Audit + Pulse Chat Probe Timeout Fix
**Date:** 2026-03-13 | **Branch:** feat/github-code-aware-chunking

---

## Session Overview

Two tasks were addressed:

1. **Compiler warnings audit (interrupted)** — User requested all 27 `cargo build` warnings be resolved in `crates/vector/ops/commands/evaluate*` and `crates/vector/ops/commands/streaming.rs`. Investigation was underway when the user interrupted with a runtime error.

2. **`pulse_chat_probe` timeout fix (completed)** — Diagnosed and fixed a 502 error caused by the Gemini ACP adapter exceeding the 60-second JS-side timeout during cold-start session probing. Fixed by increasing the timeout to 120s and adding graceful degradation.

---

## Timeline

| Time | Activity |
|------|----------|
| Session start | User reported 27 compiler warnings from `cargo build` output |
| ~T+5m | Read `evaluate.rs`, `evaluate/display.rs`, `evaluate/streaming.rs`, `streaming.rs` |
| ~T+8m | User interrupted — posted `pulse_chat_probe` 502 timeout error |
| ~T+12m | Read `route.ts` (pulse/config), `pulse_chat.rs`, `acp.rs`, `runtime.rs`, `adapters.rs` |
| ~T+18m | Identified root cause: Gemini cold-start exceeds 60s JS timeout |
| ~T+20m | Applied fix: 120s timeout + graceful degradation in `route.ts` |
| ~T+22m | Added 2 new tests to `pulse-config-route.test.ts` |
| ~T+25m | Verified: `pnpm vitest run` — 4/4 tests pass |

---

## Key Findings

### Compiler Warnings (Unresolved — Task Interrupted)
All 27 warnings are in the `evaluate` command subsystem:
- `crates/vector/ops/commands/evaluate.rs` — unused imports: `EvaluateResponsesMode`, `emit_analysis_header`, `emit_context_header`, `emit_evaluate_output`, `emit_event`, `run_parallel_answers_streaming`; unused struct `SideBySideBuffer` with methods `new` + `push`
- `crates/vector/ops/commands/evaluate/display.rs` — 13 unused `pub(super)` functions including `emit_event`, `terminal_width`, `char_len`, `pad_to_width`, `wrap_fixed_width`, `build_side_by_side_frame`, `repaint_frame`, `emit_context_header`, `emit_analysis_header`, `emit_json_output`, `emit_events_output`, `emit_terminal_output`, `emit_evaluate_output`
- `crates/vector/ops/commands/evaluate/streaming.rs` — 2 unused consts (`STREAM_WITH_CONTEXT`, `STREAM_WITHOUT_CONTEXT`); 4 unused functions (`build_parallel_futures`, `handle_token_inline`, `handle_token_side_by_side`, `run_parallel_answers_streaming`)
- `crates/vector/ops/commands/streaming.rs` — 2 unused fields on `TaggedToken` (`stream`, `delta`); 2 unused `pub(crate)` functions (`ask_llm_streaming_tagged`, `baseline_llm_streaming_tagged`)

**Critical observation:** Many of the "unused" items in `display.rs` and `streaming.rs` ARE actually used — they are called from `evaluate.rs` and `evaluate/streaming.rs`. The warnings are caused by incorrect/missing import paths in `evaluate.rs`, not dead code. Specifically, `emit_event`, `emit_context_header`, `emit_analysis_header`, `emit_evaluate_output` are imported in `evaluate.rs:14-16` but the compiler reports them unused because `evaluate/streaming.rs` references them via `super::display::*` already — the import in the parent module is redundant.

**Root pattern:** `SideBySideBuffer` and `run_parallel_answers_streaming` in `evaluate.rs` are used in `evaluate/streaming.rs` (which imports `super::SideBySideBuffer`). The `streaming.rs` tagged functions are used by `evaluate/streaming.rs`. The warnings suggest the public surface is wider than what the current callers require.

### Pulse Chat Probe Timeout
- **Error:** `Timeout waiting for axon pulse_chat_probe (60000ms)` → HTTP 502
- **Location:** `apps/web/lib/axon-ws-exec.ts:346` — JS-side timeout fires after 60s
- **Route:** `apps/web/app/api/pulse/config/route.ts:90` — `timeoutMs: 60_000`
- **Probe path:** `handle_pulse_chat_probe` → `AcpClientScaffold::start_session_probe` → `run_acp_event_loop` (300s internal timeout) → `establish_acp_session` (spawn → init → session setup → config apply)
- **Agent affected:** Gemini only (cold-start involves Google API authentication + session initialization)
- **Internal Rust timeout:** 300s (`ACP_ADAPTER_TIMEOUT` in `crates/services/acp.rs:93`) — never reached because JS fires first at 60s
- **Cache:** Probe results are cached 60s per `cacheKey = agent:model:sessionId`; in-flight requests are coalesced (`IN_FLIGHT` map)

---

## Technical Decisions

### Why 120s (not 90s or 180s)?
Gemini cold-start involves: process spawn, ACP handshake, Google OAuth token validation, `NewSessionRequest`, `ConfigOptionUpdate` round-trip. 60s was too tight; 120s doubles the budget without making the settings panel feel completely unresponsive. The inner Rust timeout remains 300s, so 120s is safe.

### Why graceful degradation instead of just longer timeout?
If Gemini is still unreachable at 120s (network outage, quota exceeded), a hard 502 causes the settings panel to show an error state. Empty `configOptions: []` renders the panel gracefully with no options — the user can still use the agent, and the next request will retry fresh (timeout responses are intentionally not cached).

### Why not suppress compiler warnings with `#[allow(dead_code)]`?
Dead code should be removed, not silenced. However, the warnings are more nuanced — much of the code in `display.rs` and `evaluate/streaming.rs` IS used. The real fix requires:
1. Remove unused items from the parent `evaluate.rs` import list (not the implementations)
2. Delete `SideBySideBuffer::new()` (use `Default::default()` directly)
3. Remove tagged streaming functions from `streaming.rs` only if confirmed unused across all call sites

This investigation was interrupted before changes were applied.

---

## Files Modified

| File | Change | Purpose |
|------|--------|---------|
| `apps/web/app/api/pulse/config/route.ts` | `timeoutMs` 60_000 → 120_000; timeout graceful degradation | Fix Gemini probe 502 |
| `apps/web/__tests__/api/pulse-config-route.test.ts` | +2 tests | Verify timeout returns `[]` + assert 120s timeout |

---

## Commands Executed

```bash
# Confirmed test files pass
pnpm vitest run __tests__/api/pulse-config-route.test.ts
# Result: 4 passed ✓
```

---

## Behavior Changes (Before/After)

### pulse/config probe timeout
| Scenario | Before | After |
|----------|--------|-------|
| Gemini probe takes >60s | HTTP 502 `ACP config probe failed` | HTTP 200 `{ configOptions: [] }` |
| Gemini probe takes 61-120s | 502 (always) | 200 (succeeds) |
| Probe error (not timeout) | HTTP 502 | HTTP 502 (unchanged) |
| Timeout result cached? | N/A (502 path) | No — intentionally skipped |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `pnpm vitest run __tests__/api/pulse-config-route.test.ts` | 4 passed | 4 passed | ✅ |
| Test: timeout returns 200 + empty configOptions | pass | pass | ✅ |
| Test: timeoutMs equals 120_000 | pass | pass | ✅ |
| Test: codex config options from probe | pass | pass | ✅ |
| Test: claude agent config options | pass | pass | ✅ |

Pre-existing failures (unrelated): `ws-messages-handlers.test.ts` (4 tests) and `use-ws-messages.test.ts` (1 test) — not caused by this session.

---

## Source IDs + Collections Touched

None — no Axon embed/retrieve operations were performed during this session's work.

---

## Risks and Rollback

### Timeout increase (low risk)
- **Risk:** 120s probe means the settings panel waits up to 2 minutes before showing options on cold Gemini start
- **Rollback:** Revert `timeoutMs` to 60_000 in `route.ts:90`

### Graceful timeout degradation (low risk)
- **Risk:** Silent empty config — user might not know Gemini is slow/unreachable
- **Mitigation:** `console.warn` is logged server-side with errorId for debugging
- **Rollback:** Remove the `message.includes('Timeout waiting for axon')` branch from the catch block

---

## Decisions Not Taken

1. **Return 502 with a shorter-timeout error code** — Rejected: hard errors in the settings panel degrade UX unnecessarily; empty config is a safe fallback
2. **Cache empty configOptions on timeout** — Rejected: would lock the agent into broken state for 60s; next request should retry fresh
3. **Add `#[allow(dead_code)]` to suppress compiler warnings** — Rejected: suppression hides real issues; proper fix is removal of genuinely unused code
4. **Per-agent timeout** — Rejected: overengineering; if Claude/Codex are also slow, they benefit from 120s too

---

## Open Questions

1. **Compiler warnings root cause** — Are `ask_llm_streaming_tagged` and `baseline_llm_streaming_tagged` in `streaming.rs` genuinely unused, or are they used by some codepath not yet explored? They have `pub(crate)` visibility suggesting intent to be called from outside.
2. **Gemini cold-start duration** — Is the typical Gemini probe duration 60-90s or 90-120s? Knowing this would help tune the timeout more precisely.
3. **Settings panel behavior on empty configOptions** — Does the Pulse settings UI handle `configOptions: []` gracefully? Not verified in this session.
4. **Pre-existing test failures** — `ws-messages-handlers.test.ts` and `use-ws-messages.test.ts` have 5 failing tests unrelated to this session. These need a separate investigation.

---

## Next Steps

1. **Resume compiler warnings fix** — Properly audit each warned symbol, determine which are truly dead vs. incorrectly imported, and either delete or fix import paths
2. **Investigate pre-existing test failures** — `ws-messages-handlers.test.ts:698` (`setCurrentJobIdTracked` not called) and `use-ws-messages.test.ts:428` (stdoutJson shape mismatch)
3. **Monitor Gemini probe timing** — Check server logs after this fix to see if 120s is sufficient or if further tuning is needed
4. **Verify settings panel empty-state UX** — Manually test with a slow/unreachable Gemini adapter to confirm `configOptions: []` renders correctly
