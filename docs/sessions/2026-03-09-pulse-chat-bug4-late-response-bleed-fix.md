# Session: Pulse Chat Bug 4 — Late Response Bleed Fix
**Date:** 2026-03-09
**Branch:** `refactor/acp-performance-modern-rust`

---

## Session Overview

Investigated and fixed Bug 4: a late response from a timed-out Gemini turn was bleeding into the subsequent Claude turn's session state, causing a Gemini-written response to appear briefly in the Claude chat before disappearing and being replaced by Claude's response.

---

## Timeline

| Time | Activity |
|------|----------|
| Session start | User reported Bug 4 after verifying Bugs 1–3 were fixed |
| Investigation | Traced root cause through `use-axon-acp.ts` timeout handler and `result` event handler |
| Fix | Two changes to `apps/web/hooks/use-axon-acp.ts` |
| Completion | Fix applied; no build/verification run (Next.js hot-reload applies automatically) |

---

## Key Findings

### Root Cause: Unconditional `result` Handler After Timeout

- **`STREAMING_TIMEOUT_MS = 60_000`** (`use-axon-acp.ts:7`): 60s frontend timeout — too short for slow agents like Gemini.
- When timeout fires (`use-axon-acp.ts:145–162`): `streamingIdRef.current` cleared to `null`, `isStreaming` set to `false`, error message shown. **Backend keeps running** — `conn.prompt()` is still awaiting Gemini in the background.
- The `result` handler (`use-axon-acp.ts:79–97`) fired `onTurnComplete()` and `onSessionIdChange(gemini_session_id)` **unconditionally** — even when `streamingIdRef.current` was already `null` (turn had timed out).
- **Consequence**: Late Gemini `result` event arriving during an active Claude turn called `onSessionIdChange(gemini_session_id)`, hijacking the active session to the Gemini session. `reloadSession()` then loaded Gemini's messages into the Claude chat UI.
- Claude's subsequent `result` fired its own `onTurnComplete()` → `reloadSession()` → overwrote with Claude session messages. This produced the "appeared then disappeared, replaced by Claude response" pattern.

### Why `assistant_delta` Events Didn't Bleed
- `assistant_delta` and `thinking_content` handlers check `if (!sid) return` where `sid = streamingIdRef.current` (`use-axon-acp.ts:50,60`). When `streamingIdRef.current` is `null` (timed out), deltas are correctly dropped.
- The `result` handler had no equivalent guard — this asymmetry was the bug.

---

## Files Modified

| File | Change |
|------|--------|
| `apps/web/hooks/use-axon-acp.ts` | `STREAMING_TIMEOUT_MS`: `60_000` → `300_000` (line 7); added `wasActiveTurn` guard in `result` handler (lines 80–95) |

---

## Technical Decisions

### Fix 1: Increase timeout to 300s
- Matches the Claude CLI `/api/pulse/chat` timeout (`CLAUDE_TIMEOUT_MS = 300_000` in `app/api/pulse/chat/route.ts`)
- Gemini responses can take 60–120s for long prompts; 60s was routinely too short
- The fallback "⚠ No response received — check agent configuration" message is still available as a safety net at 300s

### Fix 2: `wasActiveTurn` guard in `result` handler
```typescript
const wasActiveTurn = streamingIdRef.current !== null  // read BEFORE clearing
// ... clear streamingIdRef ...
if (wasActiveTurn) {
  onTurnComplete?.()
  if (newSessionId) onSessionIdChange(newSessionId)
}
```
- Reads `streamingIdRef.current` before the handler clears it
- If already `null` (timeout already fired), skips `onTurnComplete()` and `onSessionIdChange()`
- Late results from any previous/replaced agent turn can no longer hijack the active session

### Turn Nonce Alternative (Rejected)
- A more robust fix would assign a UUID per `submitPrompt` call and only process `result`/`error` events matching the current nonce
- Rejected for this session: more invasive, requires changes in multiple event handlers and the WS send payload; the `wasActiveTurn` guard solves the specific bleed scenario with minimal surface area

---

## Behavior Changes (Before/After)

| Scenario | Before | After |
|----------|--------|-------|
| Slow Gemini turn (>60s) | Timeout at 60s, "check agent configuration" error | Waits up to 300s before timeout |
| Late result after timeout + agent switch | Late `result` fires `onSessionIdChange`, loads old agent's session messages into new chat | Late `result` is ignored — `wasActiveTurn` false, session state unchanged |
| Normal Claude/Gemini turns (<300s) | No change | No change |

---

## Commands Executed

None — Next.js hot-reload automatically applies frontend changes without a build step.

---

## Verification Evidence

| Test | Expected | Actual | Status |
|------|----------|--------|--------|
| Code review of fix | `wasActiveTurn` correctly reads ref before clearing | Confirmed at lines 83–95 | ✓ PASS |
| `STREAMING_TIMEOUT_MS` value | `300_000` | `300_000` at line 7 | ✓ PASS |
| Functional E2E of Bug 4 scenario | Not tested this session | — | PENDING (requires slow-agent reproduction) |

---

## Source IDs + Collections Touched

None — no Axon embed/retrieve operations performed during this session (beyond session doc embed below).

---

## Risks and Rollback

- **300s timeout**: If a turn genuinely hangs (adapter crash, network drop), users wait 5 minutes before seeing the error. Mitigated by: backend returns `error` events on adapter exit, which the `error` handler processes immediately (no wait for timeout).
- **`wasActiveTurn` guard**: If a result arrives before `streamingIdRef` is cleared (normal flow), `wasActiveTurn = true` — normal path unchanged.
- **Rollback**: Revert `STREAMING_TIMEOUT_MS` to `60_000` and remove the `wasActiveTurn` guard. No Rust changes required.

---

## Decisions Not Taken

- **Backend timeout on `conn.prompt()`**: `tokio::time::timeout` around the prompt in `run_turn_on_conn` would let the server proactively cancel slow turns. Deferred — requires Rust changes and determining an appropriate timeout value per-agent.
- **Turn nonce**: More robust but more invasive; `wasActiveTurn` guard covers the observed failure mode with minimal change.
- **`error` handler parity**: The `error` handler (`use-axon-acp.ts:99–107`) has a similar unconditional pattern but does NOT call `onSessionIdChange`, so it cannot cause the session-hijack symptom. Left unchanged.

---

## Open Questions

- What is the correct per-agent timeout for the backend? Gemini may be slower than Claude; Codex may differ again. Should `STREAMING_TIMEOUT_MS` be per-agent?
- Should a backend-level `tokio::time::timeout` be added to `run_turn_on_conn` so the adapter loop can clean up rather than run indefinitely?
- Was Gemini's 60s+ response time a one-off or reproducible? Needs more data.

---

## Next Steps

1. Monitor whether 300s timeout is sufficient for Gemini turns in practice.
2. Consider per-agent timeout configuration if some agents are consistently slower.
3. Consider backend-level `tokio::time::timeout` on `conn.prompt()` as defense-in-depth.
4. Commit all four bug fixes (Bugs 1–4) once the MCP build errors on the branch are resolved.
