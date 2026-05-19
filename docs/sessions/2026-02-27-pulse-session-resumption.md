# Pulse Session Resumption
**Date:** 2026-02-27
**Branch:** feat/crawl-download-pack

---

## Session Overview

Implemented persistent Claude Code session resumption for the Pulse chat workspace. When a user closes and reopens the Pulse workspace, subsequent messages are now sent to the *same* Claude Code session (via `--resume <session_id>`) rather than spawning a fresh subprocess every time. This allows Claude to maintain conversation continuity across page refreshes and reconnects without any additional storage infrastructure.

---

## Timeline

1. **Investigation** — User asked whether Pulse sessions are saved to `~/.claude/projects`. Confirmed yes: every `claude -p` invocation auto-writes a session file there.
2. **Architecture audit** — Read `route.ts`, `stream-parser.ts`, `claude-stream-types.ts`, `types.ts`, and `pulse-workspace.tsx` to map the full data flow.
3. **Key finding** — The entire frontend pipeline was already in place (`sessionId` in schema, workspace sending/storing it); only the backend was broken (`session_id` from the CLI was never captured, `--resume` was never passed).
4. **TDD — RED** — Added 3 failing tests to `pulse-chat-route-streaming.test.ts`: session_id propagation, `--resume` presence, and `--resume` absence.
5. **TDD — GREEN** — Made minimal changes to `stream-parser.ts` and `route.ts` to pass all 3 tests.
6. **Full suite** — Verified 110/110 passing (107 baseline + 3 new).

---

## Key Findings

- `apps/web/app/api/pulse/chat/route.ts:95-98` — Had a comment "Do NOT --resume" that was based on a CLAUDE.md contamination concern. The concern was already mitigated (`cwd: os.tmpdir()`, `CLAUDECODE` env stripped), so the block was incorrect.
- `apps/web/app/api/pulse/chat/route.ts:350` — `sessionId: undefined` was hardcoded in all three `done` event emissions — never populated regardless of what the claude CLI returned.
- `apps/web/app/api/pulse/chat/stream-parser.ts:8` — `StreamParserState` had no `sessionId` field; the `result` event handler (`stream-parser.ts:152-155`) stored `event.result` but discarded `event.session_id`.
- `apps/web/lib/pulse/types.ts:36` — `sessionId: z.string().min(1).max(256).optional()` already present in `PulseChatRequestSchema` — schema was ready.
- `apps/web/components/pulse/pulse-workspace.tsx:401,678-680` — Frontend already sent `sessionId` in every request and stored `data.sessionId` from responses. Workspace also already persisted/restored it via localStorage (`chatSessionId` field).

---

## Technical Decisions

- **No new storage layer needed** — `~/.claude/projects` already persists the full session. The only missing piece was the session ID round-trip (capture → return → store → send → resume).
- **`--resume` in `route.ts`, not in `buildClaudeArgs`** — `buildClaudeArgs` in `claude-stream-types.ts` is a pure helper for base args. Session ID is request-specific, so it stays in the route handler after the helper call.
- **`parserState.sessionId ?? undefined`** — Used in all three done-event branches (abort, memory fallback, normal) so the ID is returned whenever it was received, even on partial/fallback responses.
- **Stale comment removed** — The "Do NOT --resume" comment was replaced with an accurate one explaining why the original concern doesn't apply.

---

## Files Modified

| File | Change |
|------|--------|
| `apps/web/app/api/pulse/chat/stream-parser.ts` | Added `sessionId: string \| null` to `StreamParserState`; set from `event.session_id` in result handler |
| `apps/web/app/api/pulse/chat/route.ts` | Push `--resume <sessionId>` after `buildClaudeArgs` when `req.sessionId` is set; replaced all 3 `sessionId: undefined` with `parserState.sessionId ?? undefined` |
| `apps/web/__tests__/pulse-chat-route-streaming.test.ts` | Added 3 new tests (RED→GREEN): session_id in done response, `--resume` present, `--resume` absent |

---

## Commands Executed

```bash
# Baseline verification
pnpm test  # 107 passing

# RED phase — confirmed 2 failures before implementation
pnpm test __tests__/pulse-chat-route-streaming.test.ts
# FAIL: 2 failed | 6 passed

# GREEN phase — confirmed all pass after implementation
pnpm test __tests__/pulse-chat-route-streaming.test.ts
# PASS: 8 passed

# Full suite regression check
pnpm test
# PASS: 110 passed (21 test files)
```

---

## Behavior Changes (Before/After)

| Aspect | Before | After |
|--------|--------|-------|
| `session_id` in done response | Always `undefined` | Populated from claude CLI `result` event |
| Claude spawn args | Never included `--resume` | Includes `--resume <id>` when client sends a sessionId |
| Page refresh behavior | Fresh Claude session on every message | Resumes same Claude session from previous visit |
| localStorage `chatSessionId` | Stored but never used for API calls | Sent to API and triggers `--resume` |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `pnpm test` (baseline) | 107 passing | 107 passing | ✅ |
| New tests (RED) | 2 fail, 1 pass | 2 fail, 1 pass | ✅ |
| New tests (GREEN) | 8 passing | 8 passing | ✅ |
| Full suite (final) | 110 passing | 110 passing | ✅ |

---

## Source IDs + Collections Touched

None — no Axon embed/retrieve operations were performed during this session (pure code implementation).

---

## Risks and Rollback

- **Risk:** If the `session_id` returned by the claude CLI changes format or becomes absent in future CLI versions, `parserState.sessionId` will be `null` and the system degrades gracefully to `sessionId: undefined` (no resume, same behavior as before).
- **Rollback:** Revert `stream-parser.ts` and `route.ts` changes. The frontend (workspace, types) requires no rollback — it was already written to handle optional `sessionId`.
- **No database migrations** — purely in-memory state changes.

---

## Decisions Not Taken

- **Adding `--resume` to `buildClaudeArgs`** — Rejected; `buildClaudeArgs` is a pure, request-agnostic helper. Session ID is request-specific and belongs at the call site.
- **Server-side session storage** — The previous exploration suggested Postgres tables for session history. Rejected in favor of the zero-infrastructure approach: claude CLI already handles persistence in `~/.claude/projects`.
- **Keeping the "Do NOT --resume" comment** — Removed; the original concern (CLAUDE.md contamination from project cwd) was already mitigated by `cwd: os.tmpdir()` and the `CLAUDECODE` env strip that were already in place.

---

## Open Questions

- When a resumed session's context grows beyond the 200k token window, the claude CLI will fail or truncate. The current code sends full `conversationHistory` in the system prompt AND uses `--resume`. This could cause double-context on very long sessions. May need investigation if users hit context errors.
- The `chatSessionId` in localStorage is per-workspace (single slot). There is no multi-session history UI (list of past sessions to pick from). That is a separate, larger feature.

---

## Next Steps

- Monitor for any issues with context overflow on long sessions (double-counting between `--resume` and manual `conversationHistory` in system prompt).
- Consider eventually adding a "New Session" button to the Pulse toolbar that clears `chatSessionId` from localStorage, giving users explicit control over when to start fresh vs resume.
