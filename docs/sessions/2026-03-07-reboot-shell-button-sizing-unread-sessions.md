# Reboot Shell: Button Sizing Fix & Unread Sessions Indicator

**Date:** 2026-03-07
**Branch:** `feat/services-layer-refactor`

## Session Overview

Two targeted UI fixes to the Reboot Shell: (1) aligned mobile header button sizes for terminal toggle and sidebar buttons to match the Pulse pane switcher's `size-7` (28px), and (2) replaced the split "active sessions" / "recent sessions" sidebar sections with a single flat session list featuring an unread indicator (glowing dot + semibold title).

## Timeline

1. **Button sizing investigation** — Read `PulseMobilePaneSwitcher` (`pulse-mobile-pane-switcher.tsx`) to determine its button size: `size-7` with `size-3.5` icons
2. **Button sizing fix** — Changed terminal toggle and sidebar buttons in `reboot-shell.tsx` from `min-h-[44px] min-w-[44px]` to `size-7`, icons from `size-4` to `size-3.5`
3. **Session list flattening** — Removed `SESSION_SECTIONS` constant from `reboot-mock-data.ts`, added `hasUnread?: boolean` to `SessionItem` type
4. **Unread indicator** — Updated `reboot-sidebar.tsx` to render flat session list with unread dot and bold title styling

## Key Findings

- `PulseMobilePaneSwitcher` buttons use `size-7` (28px) with `size-3.5` icons (`pulse-mobile-pane-switcher.tsx:22,36`)
- The 44px minimum touch target fix from the prior session made terminal/sidebar buttons visually larger than the chat/editor buttons
- `SESSION_SECTIONS` was only consumed in `reboot-sidebar.tsx` — safe to remove without other references

## Technical Decisions

- **`size-7` over `min-h-[44px]`** — visual consistency with existing Pulse pane switcher buttons takes priority; all four mobile header buttons now match exactly
- **Flat session list over sections** — the active/recent split added visual noise without conveying useful information; a single list with an unread indicator is more informative
- **Dot + semibold for unread** — minimal, non-intrusive: a 6px `--axon-primary` colored dot left of the title plus `font-semibold` + `text-[var(--text-primary)]` on the title text. Read sessions use `font-medium` with inherited color.
- **`hasUnread` as optional boolean** — keeps backward compatibility; sessions without the field render as read

## Files Modified

| File | Action | Purpose |
|---|---|---|
| `apps/web/components/reboot/reboot-shell.tsx` | Edited | Terminal toggle and sidebar buttons: `min-h-[44px] min-w-[44px]` → `size-7`, icon `size-4` → `size-3.5` |
| `apps/web/components/reboot/reboot-mock-data.ts` | Edited | Added `hasUnread?: boolean` to `SessionItem`, removed `SESSION_SECTIONS`, marked 2 mock sessions as unread |
| `apps/web/components/reboot/reboot-sidebar.tsx` | Edited | Replaced sectioned session list with flat list + unread dot indicator, import `SESSION_ITEMS` instead of `SESSION_SECTIONS` |

## Behavior Changes (Before/After)

| Area | Before | After |
|---|---|---|
| Mobile header buttons | Terminal/sidebar buttons 44px, chat/editor buttons 28px — visually mismatched | All four buttons `size-7` (28px) — visually consistent |
| Mobile header icons | Terminal/sidebar icons `size-4`, chat/editor icons `size-3.5` | All icons `size-3.5` |
| Session sidebar | Two collapsible sections: "active sessions" (2 items) and "recent sessions" (2 items) | Single flat list with count label: "sessions (4)" |
| Unread indicator | No visual indicator for sessions with new responses | Unread sessions show a `--axon-primary` dot + semibold title |

## Verification Evidence

| Command | Expected | Actual | Status |
|---|---|---|---|
| `grep -c 'SESSION_SECTIONS' apps/web/components/reboot/*.tsx` | 0 | 0 (no references) | PASS |
| `grep 'size-7' apps/web/components/reboot/reboot-shell.tsx` | Terminal + sidebar buttons use size-7 | Found at lines 316, 329 | PASS |

## Risks and Rollback

- **Low risk** — all changes are within the `/reboot` prototype route
- **Touch target regression** — buttons are now 28px, below the 44px WCAG minimum. This matches the existing `PulseMobilePaneSwitcher` buttons which the user considers the correct size. If touch targets become a concern later, padding-based hit areas can be added without changing visual size.
- **Rollback**: `git checkout HEAD -- apps/web/components/reboot/reboot-shell.tsx apps/web/components/reboot/reboot-mock-data.ts apps/web/components/reboot/reboot-sidebar.tsx`

## Decisions Not Taken

- **Padding-based touch targets** — could keep 28px visual size while providing 44px tap area via transparent padding. Not implemented because user explicitly wanted visual matching, not touch compliance.
- **Separate unread count badge** — a numeric badge showing count of unread messages per session. Deferred — the dot indicator is sufficient for the prototype.
- **Animated unread dot** — a pulsing or glowing animation on the unread dot. Kept static to avoid visual noise.

## Open Questions

- Should the unread state be cleared when a session is selected (clicked)?
- Should the unread dot color differ by agent (e.g., different colors per agent)?
- Will sessions eventually come from a real data source that tracks read/unread state?

## Next Steps

- Wire unread state management (clear on session select, set on incoming message)
- Connect mock session data to real WebSocket session list
- Consider adding exit animations for pane collapse (AnimatePresence or CSS `data-state`)
- Consider swipe gestures for mobile pane switching
