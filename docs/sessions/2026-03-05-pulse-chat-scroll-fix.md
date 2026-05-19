# Pulse Chat Scroll Fix

**Date:** 2026-03-05
**Branch:** `feat/services-layer-refactor`

## Session Overview

Diagnosed and fixed a critical scrolling bug in the Pulse chat workspace where conversations could not be scrolled. Root cause was a broken CSS height chain in `PulseErrorBoundary`, compounded by two JS workarounds (`ScrollRescue` and `onWheelCapture` handlers) that killed native browser scrolling. Also added smooth scroll behavior for programmatic scrolls.

## Timeline

1. Investigated scroll-related code across Pulse components (chat pane, editor pane, workspace)
2. Identified three compounding issues: broken height chain, `ScrollRescue`, `onWheelCapture` handlers
3. Applied fixes to 4 files
4. Verified via Chrome DevTools MCP: height chain intact, overflow triggers scrollbar, native scroll unblocked
5. Added smooth scrolling (`scrollTo({ behavior: 'smooth' })`) for "Jump to latest" and auto-scroll-on-new-message
6. Verified smooth scroll produces native easing curve: `0 -> 208 -> 753 -> 1137` over ~80ms

## Key Findings

- **`pulse-error-boundary.tsx:43`**: Wrapper `<div>` had no `className="h-full"`, breaking the height chain from the fixed overlay (viewport height) through to the chat scroll container. CSS `height: 100%` on a child of `height: auto` parent resolves to `auto`, causing all flex children to grow unbounded instead of overflowing.
- **`scroll-rescue.tsx`** (untracked file mounted in `app-shell.tsx:15`): Window-level capture-phase wheel listener called `preventDefault()` + `stopPropagation()`, destroying native smooth scrolling, trackpad momentum, and ignoring `deltaMode` (Firefox sends line units).
- **`pulse-chat-pane.tsx:342-354`** and **`pulse-editor-pane.tsx:250-262`**: `onWheelCapture` handlers manually set `scrollTop += deltaY` and called `preventDefault()`, killing native scroll behavior. These were workarounds for the height chain break.
- Flex items with `flex-shrink: 1` (default) in a column container don't prevent overflow because `min-height: auto` resolves to content minimum — real text messages can't shrink below their content height.

## Technical Decisions

- **`h-full` on ErrorBoundary wrapper** rather than restructuring the component: minimal change, fixes the root cause without touching the error boundary's reset logic.
- **Removed `ScrollRescue` entirely** rather than fixing it: it was a band-aid for the height chain break. With the height chain fixed, native scrolling works and no JS interception is needed.
- **Removed `onWheelCapture`** from both chat and editor panes: replaced with CSS `overscroll-y-contain` on the chat scroll container for native scroll containment (prevents scroll leaking to parent).
- **`scrollTo({ behavior: 'smooth' })`** for programmatic scrolls: uses browser's GPU-composited easing rather than JS animation. Instant scroll (`scrollTop = value`) retained for scroll-position restore on mount.

## Files Modified

| File | Change |
|------|--------|
| `apps/web/components/pulse/pulse-error-boundary.tsx` | Added `className="h-full"` to wrapper div (line 43) |
| `apps/web/components/app-shell.tsx` | Removed `ScrollRescue` import and mount |
| `apps/web/components/pulse/pulse-chat-pane.tsx` | Removed `onWheelCapture` handler, added `overscroll-y-contain`, smooth `scrollTo` for programmatic scrolls |
| `apps/web/components/pulse/pulse-editor-pane.tsx` | Removed `onWheelCapture` handler |

## Behavior Changes (Before/After)

| Behavior | Before | After |
|----------|--------|-------|
| Chat scroll | Cannot scroll — content clipped at viewport edge, no scrollbar | Native browser scrolling with scrollbar |
| Editor scroll | Manual JS scroll via `onWheelCapture` — no smooth/momentum | Native browser scrolling |
| "Jump to latest" button | Instant teleport to bottom | Smooth animated scroll to bottom |
| Auto-scroll on new message | Instant snap to bottom | Smooth animated scroll to bottom |
| Scroll containment | JS-based via `stopPropagation()` | CSS-native via `overscroll-behavior-y: contain` |
| Trackpad momentum | Killed by `preventDefault()` | Native browser momentum scrolling |

## Verification Evidence

| Check | Expected | Actual | Status |
|-------|----------|--------|--------|
| Height chain (ErrorBoundary h-full) | All elements show pixel heights | 503px -> 447px chain intact | PASS |
| Scroll container constrained | clientHeight stays fixed | 447px regardless of content | PASS |
| Content overflow triggers scroll | scrollHeight > clientHeight | 1702 > 447 with 15 messages | PASS |
| preventDefault not blocking | wasDefaultPrevented = false | false | PASS |
| Programmatic scroll works | scrollTop = 300 applied | scrollTop = 300 | PASS |
| Smooth scroll animation | Intermediate positions in samples | 0 -> 208 -> 753 -> 1137 | PASS |

## Risks and Rollback

- **Low risk**: Changes are CSS and event handler removals. Rollback: `git checkout` the 4 files.
- **Edge case**: If any other component relied on `ScrollRescue`'s global wheel interception, it would lose that. But `ScrollRescue` was untracked and only mounted in `AppShell` — no other consumers.
- **`overscroll-y-contain`**: Well-supported (Chrome 63+, Firefox 59+, Safari 16+). No concern for the target environment.

## Decisions Not Taken

- **Did not add `scroll-behavior: smooth` CSS property**: This would make ALL scrolls smooth including scroll-position restore on mount, causing a visible animation on page load. Instead, used `scrollTo({ behavior: 'smooth' })` selectively.
- **Did not add `will-change: scroll-position`**: Browser already optimizes overflow-y:auto containers. Adding this hint provides negligible benefit and increases memory usage.
- **Did not fix `ScrollRescue`**: Deleted it instead. The component was a workaround — fixing the root cause made it unnecessary.

## Open Questions

- Editor pane scroll smoothness: `onWheelCapture` was removed but no `overscroll-y-contain` was added to the editor's `EditorContainer`. May need the same treatment if scroll leaking is observed.
- The `scroll-rescue.tsx` file is still on disk (untracked). Can be deleted.

## Next Steps

- Fix scroll in the editor pane (user requested)
- Delete `apps/web/components/scroll-rescue.tsx` (dead code)
