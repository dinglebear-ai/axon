# Systematic Debugging Report: Session Hydration + Prompt Selector Desync
Date: 2026-03-12
Owner: Codex
Status: In Progress

## Scope
1. Assistant chat selected from sidebar sometimes opens a fresh chat view instead of restoring message history.
2. Agent/model selector sometimes shows incorrect model options after agent switch (e.g., Gemini selected but Claude models shown; Codex selected but only agent-like options shown).

## Phase 1: Root Cause Investigation

### Reproduction Evidence (UI via Chrome DevTools MCP)
- Reproduced assistant-mode send/receive: messages render in chat.
- Reproduced sidebar inconsistency: assistant rows visible, but selecting a row can land in "is ready" state rather than restored history.
- Reproduced selector inconsistency: after switching agent, model dropdown sometimes contains wrong option set.

### API/Code-path Evidence
- Session list endpoint supports assistant mode:
  - `apps/web/app/api/sessions/list/route.ts` calls `scanSessions(..., { assistantMode })`.
- Session detail endpoint does **not** use assistant mode from request context:
  - `apps/web/app/api/sessions/[id]/route.ts` currently calls `scanSessions(200)` with no options.
- Session detail fetch caller (`useAxonSession`) currently has no assistant-mode parameter and always calls:
  - `/api/sessions/{id}`
- Desktop assistant session selection path does not clear optimistic live messages before rehydration:
  - `handleSelectAssistantSession` in `apps/web/components/reboot/axon-shell.tsx`.

### Selector Evidence
- Model option derivation relies on heuristic picker:
  - `apps/web/lib/pulse/acp-config.ts#getAcpModelConfigOption`
- Current heuristic can select the wrong option when multiple config options are `category=model` (or model-like), including agent-like options in mixed adapters.

## Phase 2: Pattern Analysis

### Working Pattern
- Session list route correctly threads assistant context using explicit query param and option propagation.
- Main session select path clears live optimistic messages before loading historical messages (`handleSelectSession`).

### Broken Pattern
- Session detail route does not mirror list route context propagation for assistant mode.
- Assistant session select path differs from sessions path and omits live-state reset.
- Model-option selection uses broad heuristics with no guard against selecting agent-picker options.

## Phase 3: Hypotheses

### Hypothesis A (session hydration)
Assistant sessions intermittently fail to restore because detail fetch does not carry assistant context; route scans default stores and can miss assistant-only IDs. Additionally, assistant selection path can retain stale optimistic state.

Test plan:
- Add failing tests for assistant-mode propagation in detail route and fetch URL generation.
- Align assistant select state reset with standard session select path.

### Hypothesis B (selector mismatch)
Model dropdown mismatch occurs when the model option picker selects an agent selector option (or other non-model option) from ACP config options.

Test plan:
- Add failing tests where both agent-like and model-like options are present and ensure the model picker chooses the actual model option.

## Phase 4: Implementation Plan
1. Add failing tests for assistant-mode detail lookup and query propagation.
2. Add failing tests for model-option ambiguity.
3. Implement minimal fixes:
   - Thread `assistant_mode` through session detail fetch path and route lookup.
   - Reset live messages when selecting assistant sessions.
   - Harden model option picker to reject agent-like choices.
4. Run targeted test suites.
5. Re-verify with Chrome DevTools MCP and append final evidence.

## Progress Log
- [x] Phase 1 complete
- [x] Phase 2 complete
- [x] Phase 3 complete
- [x] Phase 4 implementation completed
## Phase 4: Implementation (Completed)

### Fixes Applied

#### A) Assistant session detail lookup/context propagation
- Updated `apps/web/app/api/sessions/[id]/route.ts` to read `assistant_mode=1` query param and pass it to scanner lookup:
  - `scanSessions(200, 30, { assistantMode })`
- Updated `apps/web/hooks/use-axon-session.ts`:
  - Added `assistantMode` option to `fetchSessionWithRetry` and `useAxonSession`
  - Session fetch now calls `/api/sessions/{id}?assistant_mode=1` in assistant mode.
- Updated `apps/web/components/reboot/axon-shell.tsx`:
  - `useAxonSession(chatSessionId, { assistantMode: railMode === 'assistant' })`
  - Assistant session selection now resets optimistic live messages (same behavior as regular sessions).

#### B) Assistant-session render hydration
- Hardened selected-session display path in `apps/web/components/reboot/axon-shell.tsx`:
  - On session switch completion, always adopt selected historical messages.
  - `displayMessages` now falls back to `historicalMessages` when a session is selected and live state is empty.

#### C) Agent/model selector ambiguity
- Hardened model-option resolver in `apps/web/lib/pulse/acp-config.ts`:
  - Added explicit agent-picker detection (`claude/codex/gemini` options and `agent` id/name).
  - `getAcpModelConfigOption` now rejects agent-like options.
  - Returns `undefined` when only agent-like options are present (prevents wrong model menus).

### Test-First Coverage Added/Updated
- `apps/web/__tests__/api/sessions-routes.test.ts`
  - Added assertions that detail route passes `{ assistantMode: true|false }` correctly.
- `apps/web/__tests__/use-axon-session-retry.test.ts`
  - Added assertion that assistant-mode fetch uses `?assistant_mode=1`.
- `apps/web/__tests__/pulse-acp-config.test.ts`
  - Added cases for mixed agent/model options and agent-only option sets.

### Verification Results
- Targeted tests pass:
  - sessions route tests
  - use-axon-session retry tests
  - use-axon-session tests
  - acp-config tests
- Typecheck passes (`apps/web`: `pnpm tsc --noEmit`).
- Chrome DevTools MCP verification:
  - Assistant session selection now restores history content (validated with `assistant mode persistence test from devtools`).
  - Session detail request now includes `assistant_mode=1` and returns 200 with messages.
  - Model selector no longer cross-loads another agent's model list from agent-like options.

## Additional UI Feature Request (Completed)

### Agent logos in conversation list
- Located and integrated logos for:
  - Anthropic (Claude)
  - Google (Gemini)
  - OpenAI (Codex)
- Implementation in `apps/web/components/reboot/axon-sidebar.tsx`:
  - Added `AgentLogo` rendering for both Sessions and Assistant conversation rows.
  - Kept compact letter badge (`C/O/G`) plus logo for quick scanning.
- Dependencies added:
  - `@icons-pack/react-simple-icons` (Anthropic + Google)
  - `react-icons` (OpenAI)

## Final Status
- [x] Assistant session restore bug fixed and validated
- [x] Session detail assistant-mode propagation fixed and validated
- [x] Agent/model selector ambiguity fixed and validated
- [x] Conversation list agent logos added and validated
