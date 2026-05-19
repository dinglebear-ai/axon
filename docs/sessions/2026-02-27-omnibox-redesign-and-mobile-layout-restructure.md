# Omnibox Redesign & Pulse Workspace Mobile Layout Restructure

**Date:** 2026-02-27
**Branch:** feat/crawl-download-pack
**Session Type:** Frontend UI refinement + mobile layout restructure

---

## Session Overview

Two-phase session:

1. **Omnibox redesign** — height adjustment, icon scaling, 8 design improvements (processing animations, larger text, animated placeholder, send-button glow, stronger focus ring), followed by a density rollback after screenshot showed elements were too large.

2. **Mobile layout restructure** — Based on an annotated mobile screenshot, major refactor of the Pulse workspace mobile layout: moved document title + pane switcher + SRC button into a fixed mobile header bar aligned with the AXON logo, anchored the omnibox to the viewport bottom, and removed the redundant "Pulse Chat" badge.

---

## Timeline

| Time | Activity |
|------|----------|
| Start | Investigated omnibox height (naturally ~36-38px via `py-2`) |
| Early | Applied `min-h-[60px]`, scaled icons to `size-3.5`/`size-4` |
| Mid | Implemented 8 design improvements (animations, placeholder, glow, radius) |
| Mid | Density rollback — `text-lg`→`text-sm`, icons back to `size-3.5`, `min-h-[44px]` |
| Late | User provided annotated mobile screenshot requesting layout restructure |
| Late | Context strip merged into omnibox bottom (2px gradient), standalone context card removed |
| End | Fixed omnibox (`page.tsx`), fixed mobile header (`pulse-workspace.tsx`), prop refactor (`pulse-chat-pane.tsx`, `pulse-editor-pane.tsx`) |

---

## Key Findings

- **No explicit height** on omnibox previously — driven entirely by `py-2` padding on the textarea (~36-38px natural)
- **`readability: true`** in `build_transform_config()` was the root cause of thin page issues (stripped VitePress sidebar layouts) — fixed in a prior session; unrelated to this one
- **Context strip placement**: The `overflow-hidden` needed for the sweep/progress animation couldn't go on the main container (would clip dropdown menus). Solved by wrapping each effect in its own `pointer-events-none absolute inset-0 overflow-hidden` div
- **`::placeholder` CSS transitions** have poor cross-browser support; solved with absolutely-positioned `<span aria-hidden>` overlay + `placeholder:opacity-0` on the real input
- **Fixed header z-index**: AXON logo in `page.tsx` is `z-10`; mobile header in `pulse-workspace.tsx` is `z-[9]` so logo floats above it correctly
- **`sourcesExpanded` was local state** in `PulseChatPane` — needed to lift it to `PulseWorkspace` so the mobile header's SRC button could control it

---

## Technical Decisions

### Fixed bottom omnibox
- When `isPulseWorkspaceActive`, omnibox is conditionally NOT rendered inside the interface card and instead rendered as a `fixed bottom-0` element outside `<main>`
- `<main>` gets `pb-[80px] sm:pb-[88px]` to prevent workspace content from being hidden behind fixed omnibox
- Chosen over a portal approach to avoid React portal complexity; conditional render is simpler

### Mobile header placement
- Placed in `pulse-workspace.tsx` rather than `page.tsx` because it's logically scoped to the Pulse workspace
- AXON logo (`z-10`) intentionally overlaps the header bar (`z-[9]`) — the header provides background, logo floats above it
- `pt-11` on workspace outer div (when `!isDesktop`) provides clearance for the fixed header
- `PulseToolbar` is now desktop-only; mobile gets the fixed header instead

### `sourcesExpanded` lifted to `PulseWorkspace`
- Previously local state in `PulseChatPane` with localStorage persistence (SOURCE_EXPANDED_STORAGE_KEY)
- Lifted so the mobile header SRC button can control it without prop drilling from `PulseChatPane` up
- localStorage persistence for `sourcesExpanded` removed (the state resets on navigation — acceptable tradeoff for simplicity)

### Animated placeholder
- 6 example commands cycle every 3.5s with 350ms fade
- Implemented as a `<span aria-hidden>` overlay (not native `::placeholder`) for reliable cross-browser transitions
- Native placeholder kept transparent (`placeholder:opacity-0`) for screen reader accessibility

### Context strip
- 2px gradient strip at the bottom of the omnibox container shows context utilization
- Only visible when `!isProcessing && workspaceContext?.turns > 0`
- Processing state shows the sweep shimmer animation instead — mutual exclusivity prevents visual conflicts

---

## Files Modified

| File | Purpose |
|------|---------|
| `apps/web/app/globals.css` | Added `@keyframes omnibox-sweep` and `@keyframes omnibox-progress` with `.animate-omnibox-sweep` and `.animate-omnibox-progress` utility classes |
| `apps/web/components/omnibox.tsx` | Height, icons, 8 design improvements, density rollback, animated placeholder, processing sweep, context strip, removed standalone context card |
| `apps/web/app/page.tsx` | Fixed omnibox at viewport bottom when pulse active; conditional rendering inside vs. outside `<main>` |
| `apps/web/components/pulse/pulse-workspace.tsx` | Fixed mobile header (title + SRC + pane switcher); `PulseToolbar` desktop-only; `sourcesExpanded` state lifted; `ChevronDown` + `PulseMobilePaneSwitcher` imports added |
| `apps/web/components/pulse/pulse-chat-pane.tsx` | Removed "Pulse Chat" label; removed `PulseMobilePaneSwitcher`; removed `mobilePane`/`onMobilePaneChange`/`isDesktop` props; added `sourcesExpanded`/`onSourcesExpandedChange` props; removed local state + localStorage effects for `SOURCE_EXPANDED_STORAGE_KEY` |
| `apps/web/components/pulse/pulse-editor-pane.tsx` | Removed `PulseMobilePaneSwitcher` import + JSX; removed `mobilePane`/`onMobilePaneChange`/`isDesktop` props from interface |

---

## Commands Executed

No shell commands executed in this session — all changes were file edits.

---

## Behavior Changes (Before / After)

| Aspect | Before | After |
|--------|--------|-------|
| Omnibox height | Natural `~36px` (py-2 only) | `min-h-[44px]` |
| Omnibox border radius | `rounded-xl` | `rounded-2xl` |
| Omnibox input text | `text-base` | `text-sm` |
| Omnibox placeholder | Static native placeholder | Animated 6-phrase cycling overlay (3.5s / 350ms fade) |
| Omnibox processing | No visual indicator | Blue/pink sweep shimmer animation across container |
| Omnibox bottom bar | Standalone rounded-md card with progress bar + % text | 2px gradient strip at container bottom (context utilization), or indeterminate progress bar when processing |
| Send button | Plain send icon | `drop-shadow-[0_0_10px_rgba(255,135,175,0.5)]` glow when input is non-empty |
| Omnibox position (Pulse active) | `sticky bottom-0` inside interface card | `fixed bottom-0 left-0 right-0 z-20` — viewport-anchored |
| Mobile Pulse header | `PulseToolbar` (title input only on mobile) | Fixed header: AXON logo space left + title center + SRC button + Chat/Editor tabs right |
| "Pulse Chat" label | Shown in chat pane header | Removed |
| Mobile pane switcher | Rendered in both `PulseChatPane` header and `PulseEditorPane` header | Rendered once, in the fixed mobile header |
| SRC button location | Inside `PulseChatPane` header | Inside `PulseWorkspace` fixed mobile header (mobile) / `PulseChatPane` header (desktop) |
| `sourcesExpanded` state | Local to `PulseChatPane` w/ localStorage | Lifted to `PulseWorkspace`, no localStorage persistence |

---

## Verification Evidence

| Item | Expected | Actual | Status |
|------|----------|--------|--------|
| `pulse-chat-pane.tsx` — no `mobilePane`/`isDesktop` props | Grep returns 0 matches | 0 matches | ✅ |
| `pulse-editor-pane.tsx` — no `mobilePane` props | Grep returns 0 matches | 0 matches | ✅ |
| `pulse-chat-pane.tsx` — no `PulseMobilePaneSwitcher` | Grep returns 0 matches | 0 matches | ✅ |
| `pulse-chat-pane.tsx` — no `setSourcesExpanded` local call | Grep returns 0 matches | 0 matches | ✅ |
| `pulse-chat-pane.tsx` — no `SOURCE_EXPANDED_STORAGE_KEY` import | Import not present | Not present | ✅ |
| Standalone context card in `omnibox.tsx` | Removed | Not present in file | ✅ |
| `page.tsx` — fixed omnibox div present | `fixed bottom-0` div when pulse active | Present | ✅ |

---

## Source IDs + Collections Touched

_(Axon embed section — populated after embed below)_

---

## Risks and Rollback

- **`sourcesExpanded` localStorage persistence removed**: Users who had sources expanded and reload will see them collapsed. Low risk — no data loss, just a UI preference reset.
- **Fixed omnibox**: If a mobile keyboard pushes the viewport, the `fixed bottom-0` omnibox may overlap keyboard. Consider `env(safe-area-inset-bottom)` addition if this causes issues on iOS.
- **Mobile header `z-[9]` vs AXON logo `z-10`**: If additional fixed elements are added at `z-[9]` or `z-10`, stacking order may need revisiting.
- **Rollback**: All changes are additive UI-only. Revert commits to `pulse-workspace.tsx`, `pulse-chat-pane.tsx`, `pulse-editor-pane.tsx`, `page.tsx`, `omnibox.tsx`, and `globals.css`.

---

## Decisions Not Taken

| Alternative | Reason Rejected |
|-------------|----------------|
| Portal for fixed omnibox | More complex React setup; conditional render outside `<main>` achieves same result cleanly |
| Keep "Pulse Chat" label | User explicitly requested removal to reclaim header space |
| Keep `sourcesExpanded` localStorage persistence after lifting | Adds complexity (need to pass `initialValue` or restore in workspace); benefit is minimal |
| Put mobile header in `page.tsx` | Would need to pass `documentTitle`, `mobilePane`, `sourcesExpanded` down as props — better scoped to `pulse-workspace.tsx` |
| CSS `:placeholder-shown` for animated placeholder | `::placeholder` transition support is inconsistent cross-browser; overlay span approach is more reliable |

---

## Open Questions

- Should the mobile header doc title be editable (tap to rename) or read-only? Currently read-only (`<span>` not `<input>`).
- iOS safe-area-inset-bottom for fixed omnibox — `pb-[env(safe-area-inset-bottom)]` may be needed.
- Should `sourcesExpanded` be re-persisted to localStorage from `PulseWorkspace` level? Currently the reset-on-reload tradeoff is accepted but not explicitly confirmed by user.

---

## Next Steps

- Test on actual mobile device to verify fixed header + fixed omnibox layout
- Consider adding `env(safe-area-inset-bottom)` padding to fixed omnibox for iOS home indicator clearance
- Consider making mobile header title tappable/editable (inline input on tap)
- PR review once layout is confirmed visually
