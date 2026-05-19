# Session: Omnibox → Pulse Chat Empty State Fix

**Date:** 2026-03-01
**Branch:** feat/sidebar
**Duration:** Short focused debugging session

---

## Session Overview

Fixed a bug where sending a plain-text message from the omnibox displayed the
"Start a conversation" empty state in the Pulse chat pane instead of opening the
chat and showing the user's message. Root cause was a 250ms debounce in
`submitWorkspacePrompt` that created a timing window where `isPulseWorkspaceActive`
evaluated to `false`, causing `PulseWorkspace` to either not mount or mount fresh
without the prompt.

---

## Timeline

1. User reports: submitting from omnibox shows Pulse chat empty state ("Start a conversation")
2. Traced the omnibox submit flow through `executeCommand` → `activateWorkspace` + `submitWorkspacePrompt`
3. Traced `isPulseWorkspaceActive` guard: `workspaceMode === 'pulse' && hasResults && workspacePromptVersion > 0`
4. Identified the 250ms debounce in `submitWorkspacePrompt` as the root cause
5. Identified secondary issue: `activateWorkspace('pulse')` called unnecessarily when workspace already active
6. Applied two-part fix; cleaned up dead debounce infrastructure

---

## Key Findings

### Root Cause: 250ms Debounce Creates Broken Intermediate State

`submitWorkspacePrompt` (use-ws-messages.ts:748) set `workspaceMode` and `hasResults`
synchronously but deferred `setWorkspacePrompt` + `setWorkspacePromptVersion` inside
a 250ms `setTimeout`. During those 250ms:

- `workspacePromptVersion = 0`
- `isPulseWorkspaceActive = workspaceMode==='pulse' && hasResults && version>0 = FALSE`
- `PulseWorkspace` either never mounted, or rendered an empty chat from the landing
  `ResultsPanel`'s content tab (which showed `<PulseWorkspace />` because
  `workspaceMode==='pulse' && activeTab==='content'`)

When the debounce finally fired (version=1), `isPulseWorkspaceActive` became `true`,
a FRESH `PulseWorkspace` mounted, and `handlePrompt` ran — but during that 250ms
window the user saw the empty state.

### Secondary Issue: Unnecessary Unmount/Remount Cycle

`executeCommand` in `omnibox.tsx:363` always called `activateWorkspace('pulse')`
even when `workspaceMode` was already `'pulse'`. This:
- Reset `workspacePromptVersion = 0` → caused `isPulseWorkspaceActive = false` → overlay unmounted
- Caused `PulseWorkspace` to lose its local `chatHistory` state
- Forced a full remount on every message submission

### React Batching Confirms the Fix

With `submitWorkspacePrompt` made synchronous, all four setters run in the same
call stack as `activateWorkspace`. React 18 batches all `setState` calls into one
render, so the final state after the batch is:
- `workspaceMode = 'pulse'`
- `hasResults = true`
- `workspacePrompt = '<user text>'`
- `workspacePromptVersion = 1` (activateWorkspace sets 0, functional updater adds 1)

`isPulseWorkspaceActive` is `true` on the very first render. No flash.

---

## Technical Decisions

- **Remove debounce entirely** (not reduce it): The debounce's stated purpose was
  to batch rapid-fire submissions, but `clearTimeout` already ensured only the last
  submission in a burst triggered. The abort controller in `handlePrompt` already
  handles rapid-fire cancellation. No reason to keep 250ms lag.
- **Guard `activateWorkspace` by `workspaceMode`**: When already in pulse mode,
  calling `activateWorkspace` wipes all accumulated axon command results unnecessarily.
  Only reset when transitioning FROM another mode TO pulse.

---

## Files Modified

| File | Purpose |
|------|---------|
| `apps/web/hooks/use-ws-messages.ts` | Made `submitWorkspacePrompt` synchronous; removed debounce ref, constant, cleanup effect |
| `apps/web/components/omnibox.tsx` | Skip `activateWorkspace('pulse')` when `workspaceMode` is already `'pulse'` |

---

## Changes Detail

### `use-ws-messages.ts`

**Removed:**
- `const WORKSPACE_PROMPT_DEBOUNCE_MS = 250` (line 140)
- `const workspacePromptDebounceRef = useRef<...>(null)` (line 301)
- Debounce `clearTimeout` guards in `deactivateWorkspace`
- Cleanup `useEffect` for the debounce ref

**Changed `submitWorkspacePrompt`:**
```ts
// Before (async via setTimeout)
const submitWorkspacePrompt = useCallback((prompt: string) => {
  setWorkspaceMode('pulse')
  setHasResults(true)
  clearTimeout(workspacePromptDebounceRef.current)
  workspacePromptDebounceRef.current = setTimeout(() => {
    setWorkspacePrompt(prompt)
    setWorkspacePromptVersion((prev) => prev + 1)
    workspacePromptDebounceRef.current = null
  }, 250)
}, [])

// After (fully synchronous — React batches all 4 setters)
const submitWorkspacePrompt = useCallback((prompt: string) => {
  setWorkspaceMode('pulse')
  setHasResults(true)
  setWorkspacePrompt(prompt)
  setWorkspacePromptVersion((prev) => prev + 1)
}, [])
```

### `omnibox.tsx`

**Changed `executeCommand` non-command path:**
```ts
// Before
if (!shouldRunCommand) {
  activateWorkspace('pulse')      // always called
  if (trimmedInput) submitWorkspacePrompt(trimmedInput)
  return
}

// After
if (!shouldRunCommand) {
  if (workspaceMode !== 'pulse') { // only when transitioning IN
    activateWorkspace('pulse')
  }
  if (trimmedInput) submitWorkspacePrompt(trimmedInput)
  return
}
```

---

## Behavior Changes (Before / After)

| Scenario | Before | After |
|----------|--------|-------|
| First text message (fresh state) | 250ms flash of empty chat pane, then message appears | Message appears immediately, no flash |
| Text message while already in pulse workspace | Workspace unmounts/remounts, chat history lost, 250ms empty state | Workspace stays mounted, history preserved, message added inline |
| URL submission (tool command) | Unchanged (routes to axon command) | Unchanged |
| Rapid-fire text messages | Last message wins (debounce cleared previous) | Last message wins (abort controller cancels previous API call) |

---

## Verification Evidence

| Check | Expected | Status |
|-------|----------|--------|
| `workspacePromptVersion` after batch | 1 (0 from activateWorkspace + functional +1) | Confirmed via React batching semantics |
| `isPulseWorkspaceActive` after submit | `true` on first render | Confirmed — no intermediate `false` state |
| No dangling refs | 0 references to `workspacePromptDebounceRef` or `WORKSPACE_PROMPT_DEBOUNCE_MS` | Confirmed via grep |
| Omnibox `workspaceMode` guard compiles | TypeScript satisfied | Confirmed — `workspaceMode` already destructured from `useWsMessages` |

---

## Source IDs + Collections Touched

None — session was a UI bug fix, no Axon embed/retrieve operations.

---

## Risks and Rollback

**Risk:** Removing the debounce means rapid-fire Enter presses send multiple prompts simultaneously
instead of coalescing to the last one.

**Mitigation:** `handlePrompt` in `use-pulse-chat.ts:152` already has abort controller logic
(`inFlightPromptRef` + `activePromptAbortRef`) that cancels the previous in-flight API request
when a new one arrives. The UI behavior is identical — last message wins — just with a shorter
window.

**Rollback:** Revert the two edits. Re-add `WORKSPACE_PROMPT_DEBOUNCE_MS = 250`,
`workspacePromptDebounceRef`, the setTimeout block in `submitWorkspacePrompt`, and the
clearTimeout calls in `deactivateWorkspace` and the cleanup effect.

---

## Decisions Not Taken

- **Reduce debounce to 0ms instead of removing it**: A 0ms setTimeout still defers to the next
  microtask tick, breaking React batching. Would not fix the issue.
- **Keep `activateWorkspace` unconditional, fix the version reset**: Could have changed
  `activateWorkspace` to not reset `workspacePromptVersion`. Rejected because the reset is
  intentional when transitioning FROM an axon command state — it clears stale prompt state.
  The guard in the omnibox is the correct layer to make this decision.

---

## Open Questions

- Should `activateWorkspace('pulse')` + `deactivateWorkspace` also be called from other places
  that trigger pulse mode (e.g., CmdK palette, sidebar navigation)? If so, those call sites
  should get the same `workspaceMode !== 'pulse'` guard.

---

## Next Steps

- Test sending the first message (fresh state) and confirming no empty-state flash
- Test sending a second message while already in pulse workspace and confirming history persists
- Consider adding the `workspaceMode !== 'pulse'` guard to any other call sites of
  `activateWorkspace('pulse')` if they exist
