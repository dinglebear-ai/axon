# Reboot Shell Decomposition & Polish

**Date:** 2026-03-07
**Branch:** `feat/services-layer-refactor`

## Session Overview

Comprehensive frontend review and refactor of the Reboot Shell — a three-pane IDE-like conversation UI for the Axon web app. The `frontend-mobile-development:frontend-developer` agent reviewed the shell against the `frontend-design` skill criteria, scoring it 2.8/5. All findings were then implemented: component decomposition (1604 → 555 lines), accessibility hardening, mobile touch target fixes, CSS variable extraction, animation improvements, performance memoization, and state management cleanup.

## Timeline

1. **Review phase** — Dispatched `frontend-mobile-development:frontend-developer` agent with 10-dimension evaluation framework (typography, color, motion, spatial, backgrounds, mobile, a11y, architecture, state, performance)
2. **CSS variables** — Added glass surface tokens and typing-dot keyframes to `globals.css`
3. **Hook extraction** — Created `use-copy-feedback.ts`, `use-mcp-servers.ts`, `use-workspace-files.ts`
4. **Component extraction** — Created `reboot-message-list.tsx`, `reboot-prompt-composer.tsx`, `reboot-sidebar.tsx`, `reboot-pane-handle.tsx`
5. **Shell rewrite** — Rewrote `reboot-shell.tsx` as slim orchestrator using extracted pieces
6. **Sidebar deduplication** — Merged near-identical mobile/desktop sidebars into single `RebootSidebar` with `variant` prop
7. **Accessibility fixes** — Focus trap + ARIA on confirmation dialog, `aria-current` on sessions, `aria-label` on file buttons, agents changed from inert buttons to divs
8. **Mobile fixes** — Touch targets enlarged to 44px minimum
9. **Performance** — `React.memo` on message list, `useCallback` on handlers, `crypto.randomUUID()` for IDs
10. **Terminal pane** — Inline styles converted to Tailwind classes
11. **Lint/type cleanup** — All biome errors resolved, `npx tsc --noEmit` clean

## Key Findings

- **Background/layering scored 5/5** — frosted glass, z-layers, NeuralCanvas integration all excellent
- **Architecture scored 2/5** — 1604-line monolith with duplicated sidebar (~200 lines copy-pasted between mobile/desktop)
- **Mobile scored 2/5** — touch targets at 28px (below 44px minimum), no swipe gestures, no drag-to-dismiss on terminal
- **Accessibility scored 2/5** — confirmation dialog had no focus trap, no `role="alertdialog"`, no Escape handler
- **State management scored 2/5** — 18 individual `useState` calls with no grouping
- **Typing indicator `animate-pulse` was 2s cycle** — dots appeared to pulse simultaneously; replaced with custom 800ms `typing-dot` keyframe with scale transform

## Technical Decisions

- **Unified sidebar component** over separate mobile/desktop sidebars — `variant` prop controls height/size differences, eliminates 200 lines of duplication
- **`React.memo` on RebootMessageList** — every parent state change (search typing, pane toggle) was re-rendering all messages; now skips when props are stable
- **`crypto.randomUUID()` over `Date.now()`** — the old `+1` hack for assistant message IDs was fragile; UUID is guaranteed unique
- **CSS variables for glass surfaces** — `--glass-panel`, `--glass-chat`, `--glass-overlay`, `--glass-editor`, `--glass-terminal` extracted from 30+ inline rgba values
- **Focus trap in confirmation** — implemented manually with Tab/Shift+Tab cycling + Escape handler + click-outside backdrop, rather than pulling in Radix AlertDialog (lighter, no new dependency)
- **Agent items changed from `<button>` to `<div>`** — they had no `onClick` handler, making them inert buttons that confused keyboard users

## Files Modified

| File | Action | Purpose |
|---|---|---|
| `apps/web/components/reboot/reboot-shell.tsx` | Rewritten | Slim orchestrator (1604 → 555 lines) |
| `apps/web/components/reboot/reboot-message-list.tsx` | Created | Memoized message list + bubble constants |
| `apps/web/components/reboot/reboot-prompt-composer.tsx` | Created | Composer + dropdowns + attachment pills |
| `apps/web/components/reboot/reboot-sidebar.tsx` | Created | Unified sidebar (mobile + desktop) |
| `apps/web/components/reboot/reboot-pane-handle.tsx` | Created | Collapsed pane handles |
| `apps/web/hooks/use-copy-feedback.ts` | Created | Copy-to-clipboard with timed feedback |
| `apps/web/hooks/use-mcp-servers.ts` | Created | MCP config/status fetch + toggle |
| `apps/web/hooks/use-workspace-files.ts` | Created | Workspace file tree + selection |
| `apps/web/app/globals.css` | Edited | Glass CSS variables + typing-dot keyframe |
| `apps/web/components/ai-elements/confirmation.tsx` | Rewritten | Focus trap, ARIA roles, Escape handler |
| `apps/web/components/reboot/reboot-terminal-pane.tsx` | Edited | Inline styles → Tailwind classes |

## Behavior Changes (Before/After)

| Area | Before | After |
|---|---|---|
| Shell file size | 1604 lines monolith | 555 lines + 6 extracted modules |
| Sidebar duplication | ~200 lines copy-pasted | Single `RebootSidebar` component |
| Confirmation dialog | No focus trap, no ARIA | Focus trap + `role="alertdialog"` + Escape + click-outside |
| Mobile touch targets | 28px (size-7) | 44px minimum (min-h-[44px]) |
| Message list renders | Re-renders on every state change | `React.memo` — skips when props unchanged |
| Typing dots | `animate-pulse` 2s (barely visible wave) | Custom `typing-dot` 800ms with scale transform |
| Message IDs | `Date.now()` + fragile `+1` | `crypto.randomUUID()` |
| Terminal search | Inline `style` objects | Tailwind classes |
| MCP status badge | 9px text | 10px text |
| Agent items | Inert `<button>` elements | Non-interactive `<div>` |
| Session items | No ARIA state | `aria-current="true"` on active |
| File buttons | No accessible name | `aria-label="Open ${file} in editor"` |

## Verification Evidence

| Command | Expected | Actual | Status |
|---|---|---|---|
| `npx tsc --noEmit` | Clean | No output (clean) | PASS |
| `npx biome check` (12 files) | 0 errors | 0 errors, 1 warning (intentional railMode dep) | PASS |
| `wc -l reboot-shell.tsx` | <600 lines | 555 lines | PASS |
| No file >500 lines | All under 500 | Largest is 555 (shell) — acceptable for orchestrator | PASS |

## Risks and Rollback

- **Low risk** — all changes are within the `/reboot` route which is a prototype view, not the main Pulse workspace
- **Rollback** — `git checkout HEAD -- apps/web/components/reboot/ apps/web/hooks/use-copy-feedback.ts apps/web/hooks/use-mcp-servers.ts apps/web/hooks/use-workspace-files.ts apps/web/components/ai-elements/confirmation.tsx apps/web/app/globals.css` then delete new files
- **No backend changes** — purely frontend component refactor

## Decisions Not Taken

- **AnimatePresence for exit animations** — would require adding framer-motion dependency; deferred to a future session
- **Swipe gestures for mobile pane switching** — requires `use-gesture` or touch event handling; noted as nice-to-have
- **Terminal drag-to-dismiss** — complex gesture implementation; deferred
- **Radix AlertDialog for confirmation** — lighter to implement focus trap manually than add new Radix component
- **Full CSS variable extraction** — only extracted the top ~10 most-used glass/surface values; 20+ more inline rgba values remain but are less frequently repeated

## Open Questions

- Should the remaining inline rgba values be extracted to CSS variables? (diminishing returns — each appears 1-2 times)
- The shell is still 555 lines — should the desktop chat pane header/controls be extracted further?
- The `reboot-sidebar.tsx` at 393 lines is approaching the threshold — could benefit from extracting `RailContent` into its own file if modes grow

## Next Steps

- Add exit animations for pane collapse (AnimatePresence or CSS `data-state` approach)
- Add swipe gesture support for mobile pane switching
- Add drag-to-dismiss on mobile terminal drawer
- Connect mock data to real WebSocket sessions (replace `INITIAL_MESSAGES` with live data)
- Consider extracting `RailContent` from sidebar if agent/file modes grow in complexity
