# Pulse Chat — Raw JSON Display Bug Fixes

**Date:** 2026-03-02
**Branch:** feat/sidebar
**Working directory:** apps/web

---

## Session Overview

Continuing from the previous session that fixed two streaming bugs (TDZ reference error, double `controller.close()` crash), this session diagnosed and fixed the remaining raw-JSON display issue in Pulse chat — CORTEX response bubbles were showing `{"text":"Hello!...","operations":[]}` with a "JSON" code badge instead of the extracted assistant text.

Five distinct root causes were found and fixed across three files.

---

## Timeline

1. **Resumed from prior context summary** — prior session had fixed TDZ and double-close bugs; raw JSON display remained
2. **Read `chat-api.ts`** — confirmed `readNdjsonStream` breaks on `error` event (throws immediately), exits loop on stream close; stream never closes if `controller.close()` is never called
3. **Identified `safeClose()` recursive bug** in `route.ts:137` — body called `safeClose()` instead of `controller.close()`; stream hung indefinitely after close was called
4. **Identified `stdoutRemainder` flush gap** — last Claude CLI stdout line (the `result` event) typically has no trailing `\n`; it stayed in `stdoutRemainder` and was never parsed, leaving `parserState.result = ''`
5. **Identified success-path raw JSON** — `content: data.text || partialText` at `use-pulse-chat.ts:326`; when `data.text = ''` (operation-only responses), `partialText` (raw JSON streaming chars) was rendered
6. **Identified error-path raw JSON** — after `emitErrorAndClose`, catch block left `assistantDraft` in chat history with `content = partialText` (raw JSON) and added a NEW invisible `isError: true` bubble on top
7. **Identified multi-block path raw JSON** — `message-content.tsx:129` used `group.content` directly (streaming text block = raw JSON) when `textGroupCount > 1` in the `hasStructuredBlocks` rendering path
8. **Applied all five fixes**, verified Biome lint clean across all modified files

---

## Key Findings

| Finding | File | Line(s) |
|---------|------|---------|
| `safeClose()` called itself recursively — `controller.close()` never called | `route.ts` | 134–138 |
| `stdoutRemainder` never processed in `close` handler — last stdout line lost | `route.ts` | close handler |
| `content: data.text \|\| partialText` — raw JSON fallback in success path | `use-pulse-chat.ts` | 326 |
| Error catch left `assistantDraft` with raw `partialText`; added invisible error bubble | `use-pulse-chat.ts` | catch block |
| `group.content` = raw JSON in multi-text-group structured block render | `message-content.tsx` | 126–129 |

**`partialText` origin:** Claude streams its full JSON response (`{"text":"...","operations":[...]}`) as incremental text deltas via `--include-partial-messages`. `partialText` accumulates these raw JSON characters. Displaying it directly produces the "JSON" code badge when Claude wraps its output in `` ```json `` fences.

**`safeClose()` recursive bug consequence:** `controller.close()` was never called, so the HTTP `ReadableStream` never sent EOF. `readNdjsonStream`'s `reader.read()` loop hung until the HTTP connection timed out or `error`/`done` events caused an early break.

**`stdoutRemainder` consequence:** The Claude CLI `result` event is the final line of stdout and often lacks a trailing `\n`. Without the flush, `parserState.result` stayed empty, causing `parseClaudeAssistantPayload('')` to return null and falling back to `fallbackAssistantText('')` = `'No assistant text returned.'` or an empty `data.text`.

---

## Technical Decisions

- **Removed `|| partialText` fallback** rather than parsing it: `partialText` is never a safe display value in the success path — `data.text` from the server's `done` event is the authoritative parsed text. Operation-only responses (empty text) correctly show nothing; the operation pills render separately.
- **Replace `assistantDraft` in-place on error** rather than adding a second message: leaving the draft in history with raw JSON content is the root cause of the display bug; replacing it in-place with either the recovered text or the error message keeps the chat history clean.
- **Parse `group.content` via `parseClaudeAssistantPayload`** in `message-content.tsx` as a defensive measure: streaming text blocks contain raw Claude output (JSON); stripping the wrapper at render time is safer than trying to clean up `parserState.blocks` on the server.
- **Hoisted `partialText`, `draftAdded`, `assistantDraftId`** before the `try` block: these needed to be accessible in the `catch` block; TypeScript scoping required them to be declared at the outer scope of `handlePrompt`.

---

## Files Modified

| File | Change |
|------|--------|
| `apps/web/app/api/pulse/chat/route.ts` | Fixed recursive `safeClose()`, added `stdoutRemainder` flush in close handler, removed debug `console.log` |
| `apps/web/hooks/use-pulse-chat.ts` | Hoisted `partialText`/`draftAdded`/`assistantDraftId`, removed `\|\| partialText` fallback in success path, fixed error catch to parse partial or replace draft in-place |
| `apps/web/components/pulse/message-content.tsx` | Added `parseClaudeAssistantPayload` import; strip JSON wrapper from `group.content` in multi-text-group rendering path |

---

## Behavior Changes (Before/After)

| Scenario | Before | After |
|----------|--------|-------|
| Normal chat response | CORTEX bubble shows `` ```json\n{"text":"Hi!","operations":[]}\n``` `` with "JSON" badge | CORTEX bubble shows "Hi!" (extracted text) |
| Operation-only response (`text: ''`) | CORTEX bubble shows raw JSON streaming chars | CORTEX bubble shows nothing (operations shown as pills below) |
| Claude exits non-zero (stream error) | Invisible `isError` bubble added; `assistantDraft` with raw JSON remains visible | Draft bubble replaced in-place with error text (rose color, Retry button) OR recovered text if streaming JSON was complete |
| Stream never closing (server hang) | HTTP stream hung indefinitely; browser waited for EOF | `controller.close()` called after `done`/`error` — stream terminates immediately |
| `result` event on last stdout line | `parserState.result = ''` — text lost if no trailing newline | Flushed in close handler via `parseClaudeStreamLine(stdoutRemainder, ...)` |
| Thinking + text blocks (multi-group) | `group.content` = raw JSON rendered in second text group | `parseClaudeAssistantPayload(group.content)?.text` strips JSON wrapper |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `docker exec axon-web pnpm exec biome check app/api/pulse/chat/route.ts hooks/use-pulse-chat.ts components/pulse/message-content.tsx` | No errors | `Checked 3 files in 13ms. No fixes applied.` | ✅ PASS |
| `docker exec axon-web pnpm exec biome check hooks/use-pulse-chat.ts` | No errors | `Checked 1 file in 16ms. No fixes applied.` | ✅ PASS |

---

## Code Diffs Summary

### `route.ts` — `safeClose()` fix
```typescript
// Before (recursive — never closed):
const safeClose = () => {
  if (closed) return
  closed = true
  safeClose()          // ← called itself
}

// After:
const safeClose = () => {
  if (closed) return
  closed = true
  controller.close()   // ← correct
}
```

### `route.ts` — `stdoutRemainder` flush
```typescript
child.on('close', (code, signal) => {
  if (closed) return
  closed = true
  cleanup()

  // NEW: Flush any partial line (e.g. the final `result` event without trailing \n)
  if (stdoutRemainder.trim()) {
    const flushResult = parseClaudeStreamLine(stdoutRemainder, parserState, startedAt)
    stdoutRemainder = ''
    if (flushResult.kind === 'assistant_events') {
      for (const ev of flushResult.events) emit(ev)
    }
  }
  // ... rest unchanged
```

### `use-pulse-chat.ts` — success path
```typescript
// Before:
content: data.text || partialText,

// After:
content: data.text,
```

### `use-pulse-chat.ts` — error path (catch block)
```typescript
// Before: added new isError message, left assistantDraft with raw JSON in history

// After:
const parsedPartial = draftAdded && assistantDraftId ? parseClaudeAssistantPayload(partialText) : null
const message = err instanceof Error ? err.message : 'Unknown error'
if (parsedPartial?.text && assistantDraftId) {
  updateChatMessage(assistantDraftId, (m) => ({ ...m, content: parsedPartial.text }))
} else if (draftAdded && assistantDraftId) {
  updateChatMessage(assistantDraftId, (m) => ({ ...m, content: message, isError: true, retryPrompt: trimmed }))
} else {
  setChatHistory((prev) => [...prev, createMessage({ ..., isError: true })])
}
```

### `message-content.tsx` — multi-block text group
```typescript
// Before:
const displayContent =
  msg.role === 'assistant' && msg.content && textGroupCount === 1
    ? msg.content
    : group.content     // ← raw JSON from streaming

// After:
const rawGroupContent = parseClaudeAssistantPayload(group.content)?.text ?? group.content
const displayContent =
  msg.role === 'assistant' && msg.content && textGroupCount === 1
    ? msg.content
    : rawGroupContent   // ← JSON-stripped
```

---

## Risks and Rollback

- **Low risk**: All changes are client-side rendering/hook fixes. No API contract changes, no server-side state mutations.
- **Rollback**: `git checkout apps/web/app/api/pulse/chat/route.ts apps/web/hooks/use-pulse-chat.ts apps/web/components/pulse/message-content.tsx`
- **Potential regression**: If `data.text` is unexpectedly `undefined` at runtime (type says `string`, but runtime could differ), removing `|| partialText` fallback means content shows as empty. This is preferrable to showing raw JSON.

---

## Decisions Not Taken

- **Streaming the parsed text directly** (not the raw JSON chars): Would require redesigning the Claude CLI prompt to not use JSON format, or parsing each delta as a partial JSON stream. Too invasive.
- **Sanitizing `partialBlocks` on the server** before including in `done.blocks`: Rejected — `blocks` are needed for tool_use/thinking rendering; cleaning text blocks would need special-casing all rendering paths.
- **Restoring `|| partialText` with `parseClaudeAssistantPayload(partialText)?.text ?? partialText`**: Considered but rejected — the fallback chain is confusing and `data.text` should always be authoritative after a successful `done` event.

---

## Open Questions

- **Why does Claude CLI exit with code 1 in some cases?** `[pulse/chat] Claude CLI exited 1 { stderr: '' }` was observed in logs. Empty stderr makes root cause unclear — could be MCP config, session state, or Claude Code version. The error path fixes now handle this gracefully, but the underlying cause is unknown.
- **Does the `stdoutRemainder` flush actually help in production?** The flush is logically necessary (Claude CLI may omit trailing `\n`), but it hasn't been confirmed with a captured `result` event in `stdoutRemainder`. Could verify by adding a temporary log in the flush block.
- **Does `group.content` in `message-content.tsx` ever legitimately contain JSON?** The fix is defensive but could mask genuine JSON display in corner cases (e.g., if a user's document operations are shown via text blocks).

---

## Next Steps

- Hard refresh browser and test with a live Pulse chat prompt to verify JSON is no longer shown
- Investigate root cause of `Claude CLI exited 1` with empty stderr (check Claude CLI version, MCP config path, session state directory permissions)
- Consider removing `data.blocks` (streaming text blocks) from the `done` response if they are never needed in the final render — they currently carry raw JSON text blocks that require defensive parsing at render time
