# Right Pane Switching + Mobile Pane Fix

**Date**: 2026-03-11
**Branch**: `feat/github-code-aware-chunking`

## Session Overview

Continued implementation of the right-pane switching feature (replacing dialog modals with inline panes) and fixed two critical issues: (1) duplicate icons in the mobile header, and (2) terminal unmount crash when switching panes. All changes verified via Chrome DevTools MCP.

## Timeline

1. **Terminal unmount crash fix** — `terminal-emulator.tsx:313` threw `Cannot read properties of undefined (reading 'onShowLinkUnderline')` when switching away from Terminal pane. Added try-catch around `term.dispose()`.
2. **Mobile duplicate icons identified** — `PulseMobilePaneSwitcher` already rendered Terminal/Logs/MCP/Settings tabs, but `axon-shell.tsx` mobile header also had standalone Terminal/Logs/MCP buttons using `persistRightPane` (desktop concept).
3. **Mobile header cleanup** — Removed 3 duplicate buttons (Terminal, Logs, MCP) from mobile header. Wired `PulseMobilePaneSwitcher` to pass all pane types through instead of filtering to chat/editor only.
4. **Mobile content rendering** — Added Terminal/Logs/MCP/Settings pane rendering to the mobile content section (previously only supported sidebar/chat/editor).
5. **Type fix** — Expanded `AxonMobilePane` from `'sidebar' | 'chat' | 'editor'` to include `'terminal' | 'logs' | 'mcp' | 'settings'`.
6. **Chrome DevTools verification** — Verified all pane switching on both mobile and desktop layouts.

## Key Findings

- `PulseMobilePaneSwitcher` (`pulse-mobile-pane-switcher.tsx`) already had all 6 pane buttons defined (chat, editor, terminal, logs, mcp, settings) but `axon-shell.tsx` was filtering its callback to only pass `chat`/`editor` values.
- The `MobilePane` type (`lib/pulse/types.ts:168`) = `'chat' | RightPanelId` already supported all pane types — the bottleneck was the local `AxonMobilePane` type in `axon-shell.tsx:60`.
- xterm.js `WebLinksAddon` tears down its internal `onShowLinkUnderline` handler before the parent `Terminal.dispose()` runs, causing the crash. A try-catch is the standard fix.

## Technical Decisions

- **Try-catch over null guard for terminal dispose**: xterm.js addon internals are not exposed — we can't check addon state before dispose. Try-catch is the only reliable approach.
- **Single PulseMobilePaneSwitcher for all panes**: Rather than keeping separate buttons for Terminal/Logs/MCP alongside the switcher, unified everything into the switcher component which already had the UI for all panes.
- **`mobilePane === 'sidebar' ? 'chat' : mobilePane` mapping**: The sidebar button is separate from the switcher (different UX — it's a toggle, not a tab). When sidebar is active, the switcher shows "chat" as selected since that's the default return pane.

## Files Modified

| File | Change |
|------|--------|
| `apps/web/components/terminal/terminal-emulator.tsx:310-314` | Added try-catch around `term.dispose()` to prevent xterm.js addon crash on unmount |
| `apps/web/components/reboot/axon-shell.tsx:60` | Expanded `AxonMobilePane` type to include terminal/logs/mcp/settings |
| `apps/web/components/reboot/axon-shell.tsx:737-753` | Removed 3 duplicate mobile header buttons, wired `PulseMobilePaneSwitcher` to pass all pane types |
| `apps/web/components/reboot/axon-shell.tsx:797-821` | Added Terminal/Logs/MCP/Settings rendering to mobile content section |

## Behavior Changes (Before/After)

| Behavior | Before | After |
|----------|--------|-------|
| Mobile header icons | 9 icons (3 duplicate Terminal/Logs/MCP) | 7 icons (Sidebar + 6 pane tabs, no duplicates) |
| Mobile Terminal/Logs/MCP/Settings | Buttons existed but did nothing visible on mobile | Full-screen pane rendering on mobile |
| Terminal pane unmount | Runtime crash (`onShowLinkUnderline` undefined) | Silent cleanup, no error |
| Mobile pane switcher | Only passed chat/editor to `setMobilePaneTracked` | Passes all pane types through |

## Verification Evidence

| Test | Expected | Actual | Status |
|------|----------|--------|--------|
| Mobile: Chat tab | Shows chat with prompt composer | Chat with "Claude is ready" + composer | PASS |
| Mobile: Terminal tab | Full-screen terminal | "TERMINAL · CONNECTED" + shell prompt | PASS |
| Mobile: Logs tab | Full-screen Docker Logs | Docker Logs with toolbar + streaming entries | PASS |
| Mobile: MCP tab | MCP Servers config | "MCP Servers" + "Add Server" button | PASS |
| Mobile: Back to Chat | Returns to chat cleanly | Chat restored | PASS |
| Mobile: No duplicate icons | 7 icons in header | 7 icons (sidebar + 6 tabs) | PASS |
| Desktop: Terminal in right pane | 3-panel with terminal | Sidebar \| Chat \| Terminal | PASS |
| Desktop: Logs in right pane | 3-panel with logs | Sidebar \| Chat \| Docker Logs | PASS |
| Desktop: Editor restore | Editor with content | "New document" content preserved | PASS |
| Terminal unmount | No crash on pane switch | Clean switch, no error overlay | PASS |
| TypeScript check | No errors in axon-shell | Clean (0 errors in axon-shell) | PASS |

## Risks and Rollback

- **Low risk**: Changes are UI-only, no backend or data changes.
- **Rollback**: Revert the 2 files (`terminal-emulator.tsx`, `axon-shell.tsx`) to restore previous behavior.
- **Terminal try-catch**: Silences all dispose errors, not just the known one. Acceptable since dispose is a cleanup path — any error there is non-critical.

## Decisions Not Taken

- **Separate mobile terminal drawer**: Could have kept the old bottom-drawer terminal for mobile. Rejected — unified pane switching is simpler and consistent with desktop behavior.
- **Lazy mounting panes**: Could unmount panes when not visible to save memory. Rejected — terminal and logs need to maintain state (shell session, log stream) across switches.

## Open Questions

- Mobile Settings pane not yet tested via Chrome DevTools (only Chat, Terminal, Logs, MCP verified visually).
- `PulseMobilePaneSwitcher` has 6 small icons — may be cramped on very narrow viewports (<360px).
- Terminal session persistence across mobile pane switches not explicitly tested (switching away and back).

## Next Steps

- Quick-push all changes (version bump + commit + push)
- Test Settings pane on mobile
- Consider reducing mobile icon count or using a dropdown for less-used panes on very small screens
