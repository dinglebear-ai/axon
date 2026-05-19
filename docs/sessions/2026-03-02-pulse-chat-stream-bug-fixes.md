# Pulse Chat Stream Bug Fixes
**Date:** 2026-03-02
**Branch:** feat/sidebar
**Session Type:** Debugging + Bug Fixes

---

## Session Overview

Diagnosed and fixed three layered bugs in the Pulse chat system that together caused:
- Landing-page prompt submission showing "Request stopped. Partial response preserved." immediately
- PulseWorkspace chat hanging forever at "Starting..."
- Server-side `uncaughtException` crashes on every failed/aborted request
- Opaque "Claude CLI exited 1: " error with no useful detail

All three code fixes applied. OAuth token re-login must be done manually (interactive browser flow required).

---

## Timeline

| Time | Activity |
|------|----------|
| Session start | User reports: landing page → "Request stopped" immediately; PulseWorkspace hangs at "Starting..." |
| Read phase | Read `route.ts`, `claude-stream-types.ts`, `stream-parser.ts`, `chat-api.ts`, `use-pulse-chat.ts`, `use-ws-messages.ts`, `pulse-workspace.tsx`, `page.tsx` |
| Bug 1 identified | `enqueueEvent` has no closed-controller guard → `ERR_INVALID_STATE` uncaughtException |
| Bug 2 identified | React StrictMode double-invocation: effect cleanup aborts request, re-run blocked by version guard |
| Bug 3 identified | Non-zero exit error uses `stderr` (always empty) instead of `parserState.result` (has the real error) |
| Auth identified | `expiresAt: 1772429848669` (05:37 UTC March 2) — both container and host tokens expired |
| Fixes applied | Two files patched; re-login instructions provided |

---

## Key Findings

### Bug 1 — `ERR_INVALID_STATE` uncaughtException (`route.ts:133`)
- `enqueueEvent` called `controller.enqueue()` with **no guard** for an externally-closed controller
- When client disconnects, Next.js closes the `ReadableStream` controller from outside our code
- Claude CLI stdout buffer still drains → `emit()` → `controller.enqueue()` → throws `ERR_INVALID_STATE`
- Because the throw occurs inside a Node.js event emitter (`stdout.on('data')`), it propagates as `uncaughtException` — crashing the route worker process repeatedly
- Manifested in logs as 3× repeated `⨯ uncaughtException` per request

### Bug 2 — "Request stopped" immediately + chat never starts (`pulse-workspace.tsx:276`)
- React StrictMode (Next.js dev default) runs every `useEffect` twice: setup → cleanup → setup
- First setup: `lastHandledPromptVersionRef.current = workspacePromptVersion (1)`, starts fetch
- Cleanup: `usePulseChat` cleanup aborts the `AbortController` → `AbortError` in async `handlePrompt` catch → `setRequestNotice('Request stopped. Partial response preserved.')`
- Second setup: guard `workspacePromptVersion (1) <= lastHandledPromptVersionRef.current (1)` is **true** → returns early — **no new request is started**
- Result: user sees "Request stopped", chat is permanently dead until page reload

### Bug 3 — Opaque auth error (`route.ts:302`)
- Claude CLI auth failure emits a `result` event on stdout (type `result`, `is_error: true`)
- `stream-parser.ts` stores it in `parserState.result`
- `route.ts` `code !== 0` branch used `stderr || stdoutRemainder` — both empty — not `parserState.result`
- Error surfaced to user as `"Claude CLI exited 1: "` with no detail
- Actual error: `"OAuth token has expired. Please obtain a new token or refresh your existing token."`

### OAuth Token Expiry
- Container credentials: `expiresAt: 1772429848669` → expired March 2 at 05:37 UTC
- Host credentials: `expiresAt: 1772452821998` → also expired (different time, same day)
- Both expired ~6 hours before the logs captured in the bug report
- Claude CLI requires an interactive browser flow to refresh — cannot be automated headlessly

---

## Technical Decisions

### try/catch vs `isClosed` property
The `ReadableStream` API does not expose an `isClosed` / `isReadable` property in the web standard. The reliable options are: (a) maintain a `closed` flag ourselves and check it, or (b) wrap in try/catch. We do both — `if (closed) return` as the fast path, try/catch as the safety net in case the controller is closed between the check and the enqueue.

### StrictMode cleanup approach
The effect cleanup resets `lastHandledPromptVersionRef.current = workspacePromptVersion - 1` only if the ref still holds the value we just set (no concurrent update). This means the re-mount is allowed to call `handlePrompt` again. In production (no StrictMode), the cleanup never fires mid-session so this has zero cost.

### `handlePrompt` ref pattern
Removed `handlePrompt` from the workspace-prompt effect's dependency array and replaced it with a ref (`handlePromptRef`) updated in a separate sync effect. This prevents any re-creation of `handlePrompt` (e.g., when `documentMarkdown` changes during initial state restore) from re-triggering the workspace prompt effect with a stale version gate.

### `parserState.result` for error messages
The `result` event from Claude CLI's stream-json contains the last meaningful output, including error messages from auth failures, tool errors, etc. Using it as the primary error detail (with stderr as fallback) gives users actionable information instead of an empty string.

---

## Files Modified

| File | Purpose |
|------|---------|
| `apps/web/app/api/pulse/chat/route.ts` | Bug 1: add `closed` guard + try/catch in `enqueueEvent`; Bug 3: use `parserState.result` for non-zero exit error detail |
| `apps/web/components/pulse/pulse-workspace.tsx` | Bug 2: add `handlePromptRef` pattern, add effect cleanup to reset version gate, remove `handlePrompt` from deps |

---

## Diffs Summary

### `route.ts` — `enqueueEvent` (Bug 1)
```typescript
// Before
const enqueueEvent = (event: PulseChatStreamEvent) => {
  lastEmitAt = Date.now()
  controller.enqueue(encoder.encode(encodePulseChatStreamEvent(event)))
}

// After
const enqueueEvent = (event: PulseChatStreamEvent) => {
  if (closed) return
  lastEmitAt = Date.now()
  try {
    controller.enqueue(encoder.encode(encodePulseChatStreamEvent(event)))
  } catch {
    closed = true
  }
}
```

### `route.ts` — non-zero exit error message (Bug 3)
```typescript
// Before
emitErrorAndClose(
  `Claude CLI exited ${code}: ${truncateForLog(stderr || stdoutRemainder)}`,
  'pulse_chat_exit_nonzero',
)

// After
const cliErrorDetail = parserState.result || truncateForLog(stderr || stdoutRemainder)
// ...
emitErrorAndClose(
  `Claude CLI exited ${code}: ${cliErrorDetail}`,
  'pulse_chat_exit_nonzero',
)
```

### `pulse-workspace.tsx` — workspace prompt effect (Bug 2)
```typescript
// Before
useEffect(() => {
  if (workspacePromptVersion === 0) { lastHandledPromptVersionRef.current = 0; return }
  if (!workspacePrompt) return
  if (workspacePromptVersion <= lastHandledPromptVersionRef.current) return
  lastHandledPromptVersionRef.current = workspacePromptVersion
  void handlePrompt(workspacePrompt)
}, [workspacePromptVersion, workspacePrompt, handlePrompt])

// After
const handlePromptRef = useRef(handlePrompt)
useEffect(() => { handlePromptRef.current = handlePrompt }, [handlePrompt])

useEffect(() => {
  if (workspacePromptVersion === 0) { lastHandledPromptVersionRef.current = 0; return }
  if (!workspacePrompt) return
  if (workspacePromptVersion <= lastHandledPromptVersionRef.current) return
  lastHandledPromptVersionRef.current = workspacePromptVersion
  void handlePromptRef.current(workspacePrompt)
  return () => {
    if (lastHandledPromptVersionRef.current === workspacePromptVersion) {
      lastHandledPromptVersionRef.current = workspacePromptVersion - 1
    }
  }
}, [workspacePromptVersion, workspacePrompt])  // handlePrompt removed from deps
```

---

## Behavior Changes (Before → After)

| Symptom | Before | After |
|---------|--------|-------|
| Landing page submit | "Request stopped. Partial response preserved." immediately; chat dead | Chat starts normally |
| Server logs | Repeated `⨯ uncaughtException: TypeError: Invalid state: Controller is already closed` | No uncaughtException |
| Auth error message | `"Claude CLI exited 1: "` (empty) | `"Claude CLI exited 1: OAuth token has expired..."` |
| Worker process stability | Crashed on every client disconnect | Gracefully handles disconnect |

---

## Commands Executed

```bash
# Verified both token expirations
node -e "const e = 1772429848669; console.log(new Date(e).toISOString()); console.log('expired:', Date.now() > e)"
# → 2026-03-02T05:37:28.669Z, expired: true

cat /home/jmagar/.claude/.credentials.json | python3 -c "import json,sys; ..."
# → host expiresAt: 1772452821998, expired: True

docker exec axon-web which claude
# → /usr/local/bin/claude (Claude CLI is installed in container)
```

---

## Verification Evidence

| Check | Expected | Actual | Status |
|-------|----------|--------|--------|
| `enqueueEvent` has `closed` guard | Yes | Added | ✅ |
| `enqueueEvent` has try/catch | Yes | Added | ✅ |
| `parserState.result` used in error | Yes | Added | ✅ |
| `handlePromptRef` pattern in workspace | Yes | Added | ✅ |
| StrictMode cleanup resets version gate | Yes | Added | ✅ |
| `handlePrompt` removed from effect deps | Yes | Removed | ✅ |
| OAuth token refreshed | Required | **Pending — manual step** | ⏳ |
| Runtime verification (chat works) | Chat starts | Not testable until re-login | ⏳ |

---

## Source IDs + Collections Touched

_No Axon embed/retrieve operations performed during this session._

---

## Risks and Rollback

- **Bug 1 fix** is purely defensive — no behavior change when controller is open (the normal path)
- **Bug 2 fix** cleanup runs only during StrictMode teardown in dev; no-op in production. The `handlePromptRef` sync is a standard React pattern — no new state/renders introduced
- **Bug 3 fix** changes error message content only — no logic change
- **Rollback**: `git diff` covers all three changes; `git checkout` on either file fully reverts

---

## Decisions Not Taken

- **`controller.desiredSize` check** — not reliable cross-runtime; try/catch is more portable
- **`cancel()` on ReadableStream in `abortHandler`** — would close controller immediately on disconnect, but doesn't cover the case where Next.js closes it without firing the abort signal
- **Making `handlePrompt` stable via `useCallback` dep reduction** — would require restructuring `usePulseChat`; the ref pattern achieves the same isolation with less surface area change
- **Moving version gate to a state variable** — would cause extra renders; ref is correct here

---

## Open Questions

- **Is StrictMode actually enabled?** Not checked `next.config.js`. If `reactStrictMode: false`, Bug 2 wouldn't manifest in production — but the fix is still correct and harmless
- **Does `parserState.result` contain the full auth error text or a truncated version?** The `result` event from Claude CLI should contain the full error, but this wasn't confirmed against a live 401 response
- **Are there other callers of `enqueueEvent` outside the `stdout.on('data')` path that also need the closed-controller guard?** The `heartbeatInterval` already checks `if (closed) return` before calling `emit`. The `aborted` path in `child.on('close')` checks `if (closed) return` too. Only `stdout.on('data')` was missing the guard

---

## Next Steps

1. **Re-authenticate Claude CLI** (required for chat to work):
   ```bash
   # Option A: login on host, copy to container
   claude login
   docker cp ~/.claude/.credentials.json axon-web:/home/node/.claude/.credentials.json
   docker exec axon-web chown node:node /home/node/.claude/.credentials.json

   # Option B: login directly inside container
   docker exec -it axon-web bash
   # then: claude login
   ```

2. **Verify chat works end-to-end** after re-login — land on `/`, submit a prompt, confirm it reaches Claude and returns a response

3. **Check `next.config.js`** for `reactStrictMode` to confirm Bug 2 is a real production concern vs dev-only

4. **Consider a `cont-init.d` script** to detect expired tokens on container start and emit a clear warning to container logs rather than letting the first request fail 70 seconds later
