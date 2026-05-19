# Session: Pulse Chat Omnibox Overlap Fix

**Date**: 2026-03-06
**Branch**: `feat/services-layer-refactor`
**Duration**: ~15 minutes

## Session Overview

Fixed a UI bug where Pulse workspace chat messages scrolled behind the fixed-position omnibox dock at the bottom of the viewport, making the last several messages unreadable.

## Timeline

1. Analyzed screenshot showing chat messages clipped behind the omnibox input bar
2. Explored the component tree: `page.tsx` → `ResultsPanel` → `PulseWorkspace` → `PulseChatPane` + fixed omnibox dock
3. Identified root cause: workspace overlay used `bottom: 0` while omnibox dock floated on top with `z-20`
4. Implemented fix using `ResizeObserver` + CSS custom property to dynamically size the gap
5. Verified no new TypeScript errors introduced (4 pre-existing errors remain)

## Key Findings

- **Root cause** (`page.tsx:165-171`): The workspace overlay was `fixed top-0 bottom-0`, extending the full viewport height. The omnibox dock at `fixed bottom-0 z-20` sat on top, clipping chat content underneath.
- The omnibox height varies by viewport (`min-h-[36px]` mobile, `min-h-[44px]` desktop) plus padding (`pb-3` / `sm:pb-4`) plus border/inner padding — not a fixed value.
- `PulseChatPane` (`pulse-chat-pane.tsx:215`) uses `flex h-full min-h-0 flex-col` — it correctly fills its parent, so the fix needed to be at the parent constraint level, not inside the chat pane.

## Technical Decisions

- **ResizeObserver over static padding**: The omnibox height is not constant (mobile vs desktop breakpoints, processing state changes). A `ResizeObserver` on the dock element dynamically tracks the real height and sets `--omnibox-dock-h` on `<html>`.
- **CSS custom property**: `--omnibox-dock-h` with `72px` fallback covers the initial render frame before the observer fires. The workspace overlay uses `bottom: var(--omnibox-dock-h, 72px)` instead of `bottom: 0`.
- **No changes to child components**: The fix is entirely in `page.tsx` — the workspace overlay constraint. No modifications needed in `PulseChatPane`, `PulseWorkspace`, or `OmniboxInputBar`.

## Files Modified

| File | Change |
|------|--------|
| `apps/web/app/page.tsx` | Added `omniboxDockRef`, `ResizeObserver` effect, changed workspace overlay `bottom` from `0` to `var(--omnibox-dock-h, 72px)`, attached ref to omnibox dock div |

## Behavior Changes (Before/After)

| Aspect | Before | After |
|--------|--------|-------|
| Chat messages near bottom | Clipped behind omnibox, unreadable | Fully visible above omnibox |
| Workspace overlay height | Full viewport (`top:0` to `bottom:0`) | Stops above omnibox dock (`bottom: var(--omnibox-dock-h)`) |
| Responsive behavior | Same clipping at all sizes | Omnibox height tracked dynamically via ResizeObserver |

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `npx tsc --noEmit` | No new errors | 4 pre-existing errors, none from changes | PASS |

## Risks and Rollback

- **Low risk**: CSS-only layout change scoped to the Pulse workspace active state. Landing page / non-Pulse views untouched.
- **Rollback**: Revert `page.tsx` — change `bottom: 'var(--omnibox-dock-h, 72px)'` back to `bottom-0` class, remove the `ResizeObserver` effect and `omniboxDockRef`.
- **Edge case**: If `ResizeObserver` is unavailable (very old browsers), the `72px` fallback applies — still better than `0`.

## Decisions Not Taken

- **Static `pb-[72px]` padding on workspace**: Rejected because omnibox height varies with viewport and processing state.
- **Modifying `PulseChatPane` scroll container**: The chat pane correctly fills its parent — the constraint was at the wrong level (parent overlay), not inside the chat.

## Open Questions

- The 4 pre-existing TypeScript errors in `claude-stream-types.ts`, `route.ts`, `settings-sections.tsx`, and `use-ws-messages.ts` are unrelated but should be addressed.
- Linter also modified `omnibox-input-bar.tsx` (removed Claude-specific model branching in `modelOptions` and `useEffect`) — this was a separate user/linter change, not part of this fix.

## Next Steps

- Visual verification in browser that chat messages are fully readable above the omnibox
- Test with long conversation threads to confirm scroll-to-bottom and "Jump to latest" still work correctly
