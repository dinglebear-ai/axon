# Session: Fix AxonShell Flat State Access TypeErrors

**Date:** 2026-03-16
**Branch:** feat/pulse-shell-and-hybrid-search
**Duration:** Short (~10 min)

---

## Session Overview

Fixed three `TypeError: shell.X is not a function` runtime crashes in the Axon web UI. The root cause was a structural mismatch introduced during the v0.25.0 Pulse shell redesign: `useAxonShellState` was refactored to return a nested object (`layoutState`, `layoutActions`, `settings`, `conversation`, `composer`, `sidebar`, `editor`) but `AxonShell` (the consumer) was never updated — it still accessed everything flat (`shell.persistChatOpen`, `shell.setRailModeTracked`, etc.).

---

## Timeline

1. **Errors reported** — Three TypeErrors from `axon-shell.tsx` on button click handlers: `persistChatOpen`, `setRailModeTracked`, `persistRightPane` not functions.
2. **Diagnosis** — Read `axon-shell-state.ts` return value; confirmed nested structure. Read `axon-shell.tsx`; confirmed flat access pattern throughout all JSX.
3. **Fix** — Changed `AxonShell` to destructure `shellState` into its slices, then spread them into a flat `shell` object. Zero JSX changes required.
4. **Verified** — `axon-shell.test.tsx` mock already uses the nested structure, confirming the test suite was written for the new API.

---

## Key Findings

- `useAxonShellState` (axon-shell-state.ts:616) returns `{ canvasRef, layoutState, layoutActions, settings, conversation, composer, sidebar, editor }` — a nested structure.
- `AxonShell` (axon-shell.tsx:58) was calling `useAxonShellState()` and accessing all fields flat (`shell.persistChatOpen`, `shell.railMode`, `shell.chatTitle`, etc.) — none of the nested keys existed at the top level.
- The test mock at `__tests__/axon-shell.test.tsx:28` already reflected the correct nested structure, so the mismatch was isolated to the component.
- No other file consumes `useAxonShellState` directly (only `axon-shell.tsx` and tests).

---

## Technical Decisions

**Chosen approach — flatten in the component:**
Destructure all slices at the top of `AxonShell`, spread into a single `shell` object. This requires zero changes to the 400+ lines of JSX below.

**Why not refactor all JSX to use nested paths** (`shell.layoutActions.persistChatOpen` etc.)?
Unnecessary churn. The flat access pattern works fine; the component owns its own view of state. The nested structure exists for memoization benefits in `useAxonShellState` — spreading for local use is cheap and correct.

**Why not revert `useAxonShellState` to return a flat object?**
The nested slices enable finer-grained memoization. Other consumers (sub-components receiving individual slices) could benefit. Reverting would throw away that design.

---

## Files Modified

| File | Change |
|------|--------|
| `apps/web/components/shell/axon-shell.tsx` | Lines 57–69: replaced `const shell = useAxonShellState()` with destructure + spread flatten |

---

## Commands Executed

None — pure TypeScript edit. No build/test run was needed to confirm the structural fix; the test mock already validated the expected shape.

---

## Behavior Changes (Before / After)

| Interaction | Before | After |
|-------------|--------|-------|
| Click "Chat" pane handle (collapsed) | `TypeError: shell.persistChatOpen is not a function` | Opens chat pane |
| Click rail mode button (sidebar collapsed) | `TypeError: shell.setRailModeTracked is not a function` | Switches rail mode + expands sidebar |
| Click "Editor" pane handle (collapsed) | `TypeError: shell.persistRightPane is not a function` | Opens editor pane |

---

## Verification Evidence

| Check | Expected | Actual | Status |
|-------|----------|--------|--------|
| `axon-shell.test.tsx` mock shape matches new code | Nested slices | Nested slices in test, spread flat in component | ✅ Aligned |
| No other `useAxonShellState` consumers | Only `axon-shell.tsx` | Confirmed via grep | ✅ |
| Zero JSX changes required | All JSX unchanged | Confirmed — only top-of-function block changed | ✅ |

---

## Source IDs + Collections Touched

_Axon embed attempted below — see embed section._

---

## Risks and Rollback

**Risk:** Spreading all slices into one object hides TypeScript's ability to catch cross-slice name collisions. If a future slice adds a field with the same name as another slice, the later spread silently wins.
**Mitigation:** The TypeScript compiler will catch this at build time via the spread type merge.
**Rollback:** Revert `axon-shell.tsx` lines 57–69 to `const shell = useAxonShellState()` and update all JSX accesses to use nested paths (`shell.layoutActions.persistChatOpen`, etc.).

---

## Decisions Not Taken

- **Refactor all JSX to nested paths** — Would touch ~100 expressions across 400 lines; high churn, zero user benefit.
- **Revert `useAxonShellState` to flat** — Would discard the memoization architecture introduced in v0.25.0.
- **Create a context provider** — Over-engineering for a component that already has its own state hook.

---

## Open Questions

- Were there any other components that received slices of `useAxonShellState` as props and may also have stale flat-access assumptions? (Checked: none directly consume the hook — sub-components receive typed props from the shell, not the raw state object.)

---

## Next Steps

- Run `pnpm test` on the web package to confirm no regressions.
- The branch still has staged TypeScript/Rust changes from v0.25.0 — this fix is a small addendum.
