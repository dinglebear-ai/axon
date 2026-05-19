# Reboot UI Cutover — Phase 2: Consolidate Cortex + Remove Pages Rail

**Date:** 2026-03-11
**Branch:** `feat/github-code-aware-chunking`

## Session Overview

Completed the Reboot UI cutover by: (1) removing the agents rail mode from the sidebar dropdown, (2) creating an `AxonCortexPane` component that consolidates all 5 cortex dashboards + jobs dashboard into a single right pane with internal tabs, (3) moving Cortex from sidebar page links into the chat header as a toggleable right pane (alongside Terminal, Logs, MCP, Settings), (4) removing the Pages rail mode entirely since all its links were moved, leaving only Sessions and Files in the sidebar, and (5) fixing pre-existing missing lucide icon imports in `editor-pane.tsx`.

## Timeline

1. **Removed agents rail mode** — Deleted `'agents'` from `RailMode` type, `RAIL_MODES` array, `AGENT_ITEMS`/`AgentItem`, updated `readStoredRailMode` validation. Preserved `AGENT_BADGE` (used by sessions rail for agent labels on session items).
2. **Explored codebase** — Dispatched 3 parallel agents to analyze cortex routes, right pane architecture, and cortex dashboard components.
3. **Created `AxonCortexPane`** — New tabbed component with 6 tabs (Status, Doctor, Sources, Domains, Stats, Jobs) importing existing dashboard components directly.
4. **Wired cortex into right pane system** — Added `'cortex'` to `RightPane` type, `VALID_RIGHT_PANES`, `AxonMobilePane`, `RightPanelId`, `VALID_PANELS`. Added Brain icon toggle button in chat header. Added rendering in both desktop and mobile layouts.
5. **Removed Pages rail mode** — Deleted `'pages'` from `RailMode`, removed `PAGE_ITEMS`/`PageItem`, cleaned sidebar of pages rendering and `Queue*` component imports, removed `pathname` prop chain.
6. **Deleted dead routes** — Removed `app/cortex/` (layout + 5 sub-pages) and `app/jobs/page.tsx`. Kept `app/jobs/[id]/page.tsx` for standalone job detail.
7. **Fixed editor-pane icons** — Added missing `Quote`, `Braces`, `List`, `ListOrdered`, `Subscript`, `Superscript` to lucide-react imports in `editor-pane.tsx`.
8. **Build verification** — `pnpm build` passes clean.

## Key Findings

- Cortex layout (`app/cortex/layout.tsx`) was a simple tab bar wrapper — easily replicated as internal state in the pane component.
- All 5 cortex dashboards and the jobs dashboard are self-contained (no props, manage own data fetching/state) — trivial to embed in any container.
- Jobs detail page (`app/jobs/[id]/page.tsx`) is 674 lines with complex artifact rendering — too heavy to inline, kept as standalone route.
- `AGENT_BADGE` constant is used by the sessions rail (not agents rail) — accidentally deleted it initially, had to restore.
- `editor-pane.tsx` had 6 missing lucide icon imports (`Quote`, `Braces`, `List`, `ListOrdered`, `Subscript`, `Superscript`) — pre-existing bug that blocked `pnpm build`.

## Technical Decisions

- **Internal tabs vs URL routing for cortex pane**: Used internal `useState<CortexTab>` rather than URL routing since it's a right pane, not a standalone page. No URL changes when switching cortex sub-tabs.
- **Keep `/jobs/[id]` as route**: The job detail page is too complex (674 lines, artifact visualization, live polling) to inline. Jobs dashboard links still navigate to `/jobs/{id}`.
- **Brain icon for Cortex**: Consistent with existing cortex branding in the codebase (previously used in `PAGE_ITEMS`).
- **Cortex button placement**: First in header button row (before Terminal) — it's the most feature-rich pane and deserves prominence.
- **Removed `pathname` from sidebar**: Was only used by the pages rail for active link detection. With pages gone, the entire `pathname` / `usePathname()` chain was removed from both sidebar and shell.

## Files Modified

| File | Action | Purpose |
|------|--------|---------|
| `components/reboot/axon-cortex-pane.tsx` | **Created** | Tabbed cortex/jobs pane with 6 sub-tabs |
| `components/reboot/axon-shell.tsx` | Modified | Added cortex to RightPane, header button, mobile/desktop rendering, removed pathname |
| `components/reboot/axon-mobile-pane-switcher.tsx` | Modified | Added cortex to PANE_BUTTONS with Brain icon |
| `components/reboot/axon-ui-config.ts` | Modified | Removed agents rail, pages rail, PAGE_ITEMS, AgentItem, AGENT_ITEMS |
| `components/reboot/axon-sidebar.tsx` | Modified | Removed pages rendering, Queue imports, pathname prop, simplified to sessions+files |
| `lib/pulse/types.ts:168` | Modified | Added `'cortex'` to `RightPanelId` |
| `hooks/use-split-pane.ts:12` | Modified | Added `'cortex'` to `VALID_PANELS` |
| `components/editor/editor-pane.tsx` | Modified | Fixed missing lucide icon imports |
| `app/cortex/` | **Deleted** | Layout + 5 sub-page routes (moved to pane) |
| `app/jobs/page.tsx` | **Deleted** | Jobs list page (moved to cortex pane) |

## Behavior Changes (Before/After)

| Area | Before | After |
|------|--------|-------|
| Sidebar rail modes | Sessions, Files, Pages, Agents | Sessions, Files |
| Sidebar Pages tab | Links to /, /jobs, /cortex/status | Removed |
| Cortex access | Sidebar page link → `/cortex/status` route | Chat header Brain icon → right pane |
| Jobs list access | Sidebar page link → `/jobs` route | Cortex pane → Jobs tab |
| Jobs detail | `/jobs/[id]` route | Unchanged — still `/jobs/[id]` |
| Mobile pane switcher | 6 buttons (chat, editor, terminal, logs, mcp, settings) | 7 buttons (+ cortex) |
| Default rail mode | Sessions (unchanged) | Sessions (unchanged) |

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `npx tsc --noEmit` (filtered) | No new errors | Only pre-existing cortex-routes.test.ts tuple errors | PASS |
| `pnpm build` | Clean build | Build succeeds, routes: /, /jobs/[id], all /api/* | PASS |
| `.next/types/validator.ts` stale | Errors from deleted routes | Cleared by removing `.next/types/` cache | PASS |

## Risks and Rollback

- **Low risk**: All changes are frontend-only. API routes unchanged. No backend modifications.
- **Rollback**: `git checkout -- apps/web/` restores all files. Re-create `app/cortex/` and `app/jobs/page.tsx` from git.
- **localStorage migration**: Users with `'pages'` or `'agents'` stored as rail mode will gracefully fall back to `'sessions'` default.
- **Jobs dashboard navigation**: `JobsDashboard` uses `router.push('/jobs/{id}')` — this still works since `/jobs/[id]` route is preserved.

## Decisions Not Taken

- **Inline job detail in cortex pane**: Rejected — 674-line component with live polling, artifact visualization, and complex state. Too heavy for a pane sub-view.
- **Keep cortex as route + pane dual-mode**: Rejected — unnecessary complexity. The pane fully replaces the route.
- **Remove rail mode dropdown entirely (only 2 modes)**: Not requested — the dropdown still works fine with 2 options and leaves room for future additions.

## Open Questions

- Should the jobs dashboard in the cortex pane be adapted to open job detail inline instead of navigating to `/jobs/[id]`? Currently navigates away from the shell.
- The cortex pane `max-w-5xl` container may be too narrow in a wide right pane — may need responsive width adjustment.

## Next Steps

- Test the cortex pane in the running app to verify all 6 dashboard tabs render correctly
- Consider whether `/jobs/[id]` should render inside the shell layout or as a standalone page
- Update `apps/web/CLAUDE.md` to reflect the new architecture (routes table, Pages section)
