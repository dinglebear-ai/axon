# Pulse Op Confirmation Modal + Permission Default Fix

**Date:** 2026-03-03
**Branch:** feat/sidebar

## Session Overview

Fixed the Pulse document operation confirmation dialog — it was rendering inline at the bottom of the chat panel, trapped under the omnibox and unclickable. Converted it to a proper centered portal modal. Also changed the default permission level from `accept-edits` to `bypass-permissions` to match the Docker container's `PULSE_SKIP_PERMISSIONS=true` default, so the confirmation dialog doesn't appear at all unless the user explicitly opts into `accept-edits` mode. Removed backdrop blur from both the confirmation modal and the Cmd+K palette.

## Timeline

1. **Screenshot analysis** — Identified `PulseOpConfirmation` rendering inline in chat flow with `p-3` wrapper
2. **Modal conversion** — Rewrote component to use `createPortal` to `document.body` with fixed overlay
3. **Backdrop blur removal** — User requested no blur; changed from `bg-black/60 backdrop-blur-sm` to `bg-black/40`
4. **Permission default change** — Traced permission flow: `checkPermission('bypass-permissions', ...)` already returns `{ requiresConfirmation: false }`, but default was `accept-edits`. Updated 3 locations.
5. **Cmd+K palette blur removal** — Same treatment: `cmdk-palette-dialog.tsx` backdrop changed to `bg-black/40` without blur

## Key Findings

- `checkPermission()` in `lib/pulse/permissions.ts:20` already handles `bypass-permissions` correctly — returns `{ allowed: true, requiresConfirmation: false }`
- The dialog appeared because the default permission level was `accept-edits` in 3 places
- `PulseOpConfirmation` was a plain `<div>` in the chat flow — no portal, no z-index management

## Technical Decisions

- **Portal to `document.body`** rather than a sibling div — ensures no parent `overflow: hidden` or stacking context can trap the modal
- **`z-[9999]`** — high enough to sit above the omnibox and all other UI layers
- **Keyboard shortcuts** (Enter to apply, Esc to reject) — standard modal UX; click-outside also dismisses
- **Default `bypass-permissions`** — matches `PULSE_SKIP_PERMISSIONS=true` in Docker. Users who want safety checks switch to `accept-edits` in settings.
- **No blur on overlays** — user preference; `bg-black/40` provides enough visual separation without the rendering cost

## Files Modified

| File | Purpose |
|------|---------|
| `apps/web/components/pulse/pulse-op-confirmation.tsx` | Converted from inline div to portal-based centered modal with keyboard shortcuts |
| `apps/web/components/pulse/pulse-workspace.tsx:415-429` | Removed wrapper `<div className="p-3">` around confirmation component |
| `apps/web/hooks/use-ws-messages.ts:73` | Default permission `accept-edits` → `bypass-permissions` |
| `apps/web/lib/pulse/types.ts:54` | Zod schema default `accept-edits` → `bypass-permissions` |
| `apps/web/lib/pulse/workspace-persistence.ts:83` | Fallback default `accept-edits` → `bypass-permissions` |
| `apps/web/components/cmdk-palette/cmdk-palette-dialog.tsx:239` | Removed `backdrop-blur-sm`, lightened to `bg-black/40` |

## Behavior Changes (Before/After)

| Aspect | Before | After |
|--------|--------|-------|
| Confirmation dialog position | Inline at bottom of chat panel, under omnibox | Centered portal modal, above everything |
| Confirmation dialog clickability | Unclickable (trapped under omnibox z-index) | Fully interactive with keyboard shortcuts |
| Default permission level | `accept-edits` (shows confirmation on high-risk ops) | `bypass-permissions` (auto-applies all ops) |
| Confirmation backdrop | `bg-black/60 backdrop-blur-sm` | `bg-black/40` (no blur) |
| Cmd+K palette backdrop | `bg-black/60 backdrop-blur-sm` | `bg-black/40` (no blur) |

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `npx vitest run __tests__/pulse-permissions.test.ts` | 4 tests pass | 4 passed | PASS |
| `npx tsc --noEmit` | No new errors from our changes | 13 pre-existing errors (none from our files) | PASS |

## Risks and Rollback

- **Low risk** — UI-only changes, no backend impact
- **Permission default change** — users with `localStorage` key `axon.web.pulse-permission` set to `accept-edits` will keep their setting (restored on init). Only new users or cleared storage get `bypass-permissions` default.
- **Rollback** — revert the 6 files listed above

## Decisions Not Taken

- **Exposing `PULSE_SKIP_PERMISSIONS` as `NEXT_PUBLIC_` env var** — would let the server control the client default, but overengineering since the UI permission selector already exists and persists to localStorage
- **Removing the confirmation dialog entirely** — kept it for users who opt into `accept-edits` mode

## Open Questions

- Should the permission level selector in settings be more prominent / have a tooltip explaining the relationship to Claude CLI permissions?

## Next Steps

- None — changes are complete and ready for the next deploy
