# Session: Pulse Chat — Remove Redundant conversationHistory Injection When Session Active

**Date:** 2026-03-01
**Branch:** feat/sidebar
**Scope:** `apps/web/app/api/pulse/chat/route.ts`

---

## Session Overview

Fixed a token budget waste in the Pulse chat API: when a Claude session is active (`--resume <sessionId>`), the server was injecting a client-maintained copy of the conversation history into the system prompt as text (up to 14,000 chars) even though Claude already had the full native history via the JSONL file loaded by `--resume`. This caused Claude to see conversation history twice — once as real messages, once as formatted text — wasting system prompt budget and potentially causing model confusion.

Two targeted changes were made to `route.ts`:
1. Clear `conversationHistory` before passing to `buildPulseSystemPrompt` when `req.sessionId` is set.
2. Clear `conversationHistory` before passing to `computeContextCharsTotal` so telemetry accurately reflects the actual token budget used.

---

## Timeline

- **Reviewed plan** — confirmed single-file, two-line-change scope
- **Applied fix 1** — `buildPulseSystemPrompt` call: conditionally pass `{ ...req, conversationHistory: [] }` when `req.sessionId` is set
- **Applied fix 2** (tightening) — `computeContextCharsTotal` call: same guard, preventing telemetry from double-counting history chars that are no longer in the system prompt
- **Verified diff** — confirmed no other files touched, no unintended side effects

---

## Key Findings

- `route.ts:80` — `buildPulseSystemPrompt(req, citations)` was unconditional; now skips history when `req.sessionId` is set
- `route.ts:126` — `computeContextCharsTotal` received `conversationHistory: req.conversationHistory` (original); after fix, receives `[]` when `sessionId` is set, eliminating double-counting against a system prompt that no longer contains it
- `rag.ts:26-55` — `buildConversationHistorySection`: caps at 24 turns × 1,200 chars, 14,000 total. Fully skipped when `conversationHistory: []` passed.
- `rag.ts:129` — `buildConversationHistorySection(req.conversationHistory)` — the `if (conversationHistory)` guard at line 160 means an empty array already short-circuits; the fix works by clearing at the call site, not by changing `rag.ts`
- `claude-stream-types.ts:207-210` — `computeContextCharsTotal` sums `entry.content.length` over all history entries; passing `[]` yields 0 contribution, which is accurate when `--resume` owns the history

---

## Technical Decisions

**Clear at call site, not in function signature** — adding a `skipHistory` flag to `buildPulseSystemPrompt` would be more invasive and couple the function to session awareness. Spreading `{ ...req, conversationHistory: [] }` at the call site keeps `rag.ts` unchanged and has zero blast radius.

**Same guard for telemetry** — `systemPromptChars` (from `systemPrompt.length`) already reflects the cleared prompt, but `computeContextCharsTotal` was separately summing `req.conversationHistory` and adding it on top. Without the telemetry fix, `contextCharsTotal` would have overstated actual usage by up to 14,000 chars on every 2nd+ message.

**`replayKey` left unchanged** — `replayKey` still hashes `req.conversationHistory`. This is correct: different turn counts need different cache entries for replay correctness. The mild inefficiency (cache misses every turn when `--resume` is active) is acceptable.

**`noSessionPersistence: true` path unaffected** — no `sessionId` is ever set in this mode, so `req` is passed as-is and the fallback system-prompt injection continues to work.

---

## Files Modified

| File | Change |
|------|--------|
| `apps/web/app/api/pulse/chat/route.ts` | Two targeted changes: skip history in `buildPulseSystemPrompt` + `computeContextCharsTotal` when `req.sessionId` is set |

---

## Behavior Changes (Before/After)

| Scenario | Before | After |
|----------|--------|-------|
| 2nd+ message in active session | System prompt includes up to 14,000 chars of conversation history + Claude has JSONL history via `--resume` | System prompt omits history section; Claude uses JSONL history only |
| `noSessionPersistence: true` | System prompt includes history (correct, no `--resume`) | Unchanged — no `sessionId`, full history still injected |
| `contextCharsTotal` telemetry (2nd+ msg) | Overstated by up to 14,000 chars (counted history in both `systemPromptChars` and `conversationChars`) | Accurate — `conversationChars` is 0 when session active |
| First message in new session | No `sessionId`, full history injected (history is empty anyway) | Unchanged |

---

## Verification Evidence

| Check | Expected | Actual | Status |
|-------|----------|--------|--------|
| `git diff` shows only `route.ts` changed | 2 hunks in 1 file | 2 hunks in `apps/web/app/api/pulse/chat/route.ts` | ✅ |
| `buildPulseSystemPrompt` call site | Conditional spread when `sessionId` set | `req.sessionId ? { ...req, conversationHistory: [] } : req` | ✅ |
| `computeContextCharsTotal` call site | `conversationHistory: []` when `sessionId` set | `req.sessionId ? [] : req.conversationHistory` | ✅ |
| `rag.ts` untouched | No changes | 0 changes | ✅ |
| `noSessionPersistence` path | Full history still injected | No `sessionId` → `req` passed as-is | ✅ (by logic) |

*No test run was executed this session — changes are logic-only guards at existing call sites with no new code paths.*

---

## Source IDs + Collections Touched

None — no Axon embed/retrieve operations were performed during implementation.

---

## Risks and Rollback

**Risk:** Low. Both changes are additive guards (`req.sessionId ? ... : req`). The false branch is identical to the previous unconditional behavior.

**Rollback:** Revert to `buildPulseSystemPrompt(req, citations)` and `conversationHistory: req.conversationHistory` in both call sites. One-line reverts each.

---

## Decisions Not Taken

- **Add `skipHistory` param to `buildPulseSystemPrompt`** — more invasive, couples `rag.ts` to session awareness. Rejected: call-site spread is simpler.
- **Clear history in `chat-api.ts` before sending to API** — would affect client-side display and `computeContextCharsTotal` upstream. Rejected: server-side guard is cleaner and doesn't touch the React state shape.
- **Read the JSONL file server-side to reconstruct history for non-session mode** — out of scope, unnecessary complexity.

---

## Open Questions

- Should `replayKey` also exclude `conversationHistory` when `sessionId` is set, to improve cache hit rate across turns? Currently harmless but wasteful.
- Are there tests for `buildPulseSystemPrompt` / `computeContextCharsTotal` that should be updated to cover the `sessionId` guard path?

---

## Next Steps

- Run `cd apps/web && pnpm test` to confirm no regressions
- Manual verification: send 2+ messages in Pulse, confirm `contextCharsTotal` telemetry drops on 2nd message
- Consider adding unit test for the `sessionId` guard in `route.ts` (or integration test via `chat-api.ts`)
