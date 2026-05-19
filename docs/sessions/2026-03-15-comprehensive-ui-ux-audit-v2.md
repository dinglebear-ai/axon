# Comprehensive UI/UX Audit & Remediation — Axon Web

**Date:** 2026-03-15
**Scope:** `apps/web/` — full UI/UX audit and fix cycle
**Method:** 3-phase parallel agent workflow (audit → fix → review → fix → review)

---

## Session Overview

Conducted a comprehensive UI/UX audit of the Axon web application (`apps/web/`) using 3 parallel review agents, then dispatched 4 parallel fix agents to remediate all findings, followed by 2 review cycles and a final cleanup pass. The entire audit-fix-review pipeline was completed in a single session.

**Result:** 69 files modified, 2499 lines added, 1440 removed. Zero TypeScript errors. Zero remaining blockers.

---

## Timeline

1. **Phase 1 — Audit** (3 parallel agents):
   - `shadcn-reviewer`: shadcn/ui component patterns, Radix primitives, theme tokens
   - `shadcn-code-reviewer`: code quality, cn() usage, CSS conflicts, accessibility, performance
   - `frontend-design-reviewer`: visual hierarchy, responsive design, states, animations, coherence

2. **Phase 2 — Fixes** (4 parallel agents, non-overlapping file ownership):
   - `token-fixer`: globals.css tokens, color alignment, prefers-reduced-motion, install 6 shadcn components
   - `ai-elements-fixer`: Critical C-1/C-2 fixes, TooltipProvider, context-menu/hover-card, dialogs
   - `shell-fixer`: Toolbar tooltips, raw button conversions, skeleton loaders, DockerStats, mobile touch targets
   - `pane-fixer`: AlertDialog, toast system, form element conversions, Card/Collapsible, Badge, virtualization

3. **Phase 3 — Review Round 1**: Found 3 blockers, 7 warnings

4. **Phase 4 — Fix Round 2** (3 parallel agents):
   - `token-fixer-2`: Missing `--axon-primary-bg` token, delete dead `sonner.tsx`
   - `shell-fixer-2`: MCP DialogDescription, retry button, terminal close, file-action buttons
   - `sidebar-fixer-2`: Sidebar focus-visible states, prompt composer 7 raw buttons

5. **Phase 5 — Review Round 2**: 0 blockers, 0 warnings, 14 passes. 7 raw buttons + 4 INFO items noted.

6. **Phase 6 — Final Cleanup**: Addressed all 7 remaining raw buttons + 4 INFO items.

---

## Key Findings (Audit Phase)

### Critical (2)
- **C-1** `tool.tsx:98`: `ToolHeader` had local `useState(true)` disconnected from Radix `Collapsible` — chevron desynced from actual state
- **C-2** `message.tsx:146-148`: `MessageBranchContent` infinite re-render loop — `childrenArray` new ref every render triggered `setBranches` in `useEffect`

### High Priority (10)
- **H-1**: ~100+ hardcoded `rgba()` values instead of CSS custom properties
- **H-2**: Dual color system divergence — shadcn `oklch` vs Axon `rgba/hex`
- **H-3**: 91 raw `<button>` elements bypassing shadcn `Button`
- **H-4**: No toast/notification system
- **H-5**: `--text-dim` (#4d6a8a) failed WCAG AA (~3.2:1 contrast)
- **H-6**: No skeleton loaders for primary views (chat, Mission Control)
- **H-7**: Duplicated `DeleteConfirmModal` — not a real dialog (no focus trap, no ARIA)
- **H-8**: 7 icon-only toolbar buttons with no visible tooltips
- **H-9**: `context-menu.tsx` styling inconsistent with `dropdown-menu.tsx`
- **H-10**: Nested `TooltipProvider` per `MessageAction`

### Positive Patterns Found
- Correct `cn()` usage, `data-slot` on all primitives, `focus-visible:` over `:focus`
- `React.memo()` on heavy components, `sr-only` on icon buttons
- Virtualized log viewer via `@tanstack/react-virtual`
- Semantic HTML on resize divider, proper tab roles on mobile switcher

---

## Technical Decisions

| Decision | Rationale |
|----------|-----------|
| Keep 7 raw buttons as raw | Complex layout items (session list), radio-like selectors, `CollapsibleTrigger asChild`, invisible backdrop — `<Button>` CVA would fight custom styling |
| Align shadcn tokens to Axon (not vice versa) | Axon tokens are the established system; shadcn tokens are the newcomers. Less churn. |
| Delete `sonner.tsx` wrapper, import `sonner` directly | Wrapper used `next-themes` (not installed). App hardcodes dark mode. Direct import is cleaner. |
| Add ARIA radio pattern to density/canvas selectors | Radio-like toggle buttons need `role="radio"` + `aria-checked` for screen reader semantics |
| Extract Toaster inline styles to CSS | Design tokens should drive all theming; inline styles bypass the token system |
| 11px text floor in density presets | WCAG readability — 9-10px text is functionally illegible for many users |

---

## Files Modified

### New shadcn Components (6)
- `components/ui/card.tsx` — Card compound component
- `components/ui/alert-dialog.tsx` — Destructive confirmation dialog
- `components/ui/skeleton.tsx` — Loading placeholder
- `components/ui/progress.tsx` — Progress bar
- `components/ui/sheet.tsx` — Slide-over panel (4 side variants)
- `components/ui/sonner.tsx` — CREATED then DELETED (dead code, used `next-themes`)

### CSS/Foundation (2)
- `app/globals.css` — 10 new tokens, shadcn/Axon alignment, `--text-dim` contrast fix, `--surface-sunken`, `--axon-primary-bg`, `.axon-wordmark`, `prefers-reduced-motion`, sonner toast styles
- `app/density-high.css` — 11px text size floor enforcement

### AI Elements (4)
- `components/ai-elements/tool.tsx` — C-1: removed disconnected state, Radix data-attribute chevron
- `components/ai-elements/message.tsx` — C-2: memoized childrenArray, H-10: removed nested TooltipProvider, L-6: memoized context value
- `components/ai-elements/conversation.tsx` — M-8: aria-label on scroll button
- `components/ai-elements/confirmation.tsx` — tabIndex={-1}, bg-transparent on backdrop
- `components/ai-elements/queue.tsx` — focus-visible ring on CollapsibleTrigger

### Shell Components (18)
- `axon-shell.tsx` — H-8: tooltips on 7 toolbar buttons, H-3: raw buttons → Button, M-3: .axon-wordmark
- `axon-sidebar.tsx` — H-3: raw buttons → Button, M-3: .axon-wordmark, W-4: focus-visible on session items
- `axon-mobile-pane-switcher.tsx` — H-3: buttons, M-7: touch targets 24px → 40px
- `axon-message-list.tsx` — H-6: skeleton loader, B-3: retry button, W-3: file-action buttons, I-2: border token
- `axon-mcp-pane.tsx` — H-7: AlertDialog, H-4: toast notifications
- `axon-mcp-dialog.tsx` — H-7: AlertDialog, H-4: toasts, B-2: DialogDescription
- `axon-settings-pane.tsx` — H-4: toasts, M-5: validation feedback
- `axon-settings-dialog.tsx` — M-9: DialogDescription
- `axon-logs-dialog.tsx` — M-9: DialogDescription
- `axon-terminal-dialog.tsx` — M-9: DialogDescription, W-2: close button → Button
- `axon-terminal-pane.tsx` — search close button → Button with X icon
- `axon-shell-resize-divider.tsx` — aria-valuenow fix, role="slider"
- `axon-prompt-composer.tsx` — W-5: 7 raw buttons → Button
- `axon-pane-handle.tsx` — expand button → Button
- `density-selector.tsx` — ARIA radiogroup/radio pattern
- `canvas-profile-selector.tsx` — ARIA radiogroup/radio pattern
- `docker-stats.tsx` — M-1: shadcn tokens → Axon tokens
- `mission-control-pane.tsx` — H-6: skeleton loader

### Other Components (10)
- `landing-cards.tsx` — M-4: Card/Collapsible refactor, L-7: AbortController, status tokens
- `logs-toolbar.tsx` — M-5: shadcn Select/Input/Button
- `command-options-panel.tsx` — M-5: shadcn Checkbox/Input/Select
- `table-primitives.tsx` — M-5: shadcn Input, L-5: Badge for StatusBadge
- `doctor-report.tsx` — L-5: Badge for StatusPill
- `job-cells.tsx` — L-5: Badge/Button
- `job-detail-ui.tsx` — L-8: dead /jobs link → /, M-12: virtualized ShowMoreList
- `action-rail.tsx` — H-3: Button conversions
- `omnibox-input-bar.tsx` — H-3: Button conversions
- `omnibox-dropdowns.tsx` — H-3: Button conversions
- `editor-tab-bar.tsx` — H-3: Button conversions

### Infrastructure (3)
- `app/providers.tsx` — H-4: Toaster from sonner, I-1: removed inline styles
- `hooks/use-is-mobile.ts` — M-6: useIsPhone/useIsTablet/useIsDesktop hooks
- `components/ui/context-menu.tsx` — H-9: glass styling alignment
- `components/ui/hover-card.tsx` — L-3: entrance/exit animations
- `components/ui/button.tsx` — reformatted by shadcn CLI (no functional changes)

### NPM Dependencies Added
- `sonner` — toast notification library
- `@radix-ui/react-alert-dialog` — AlertDialog primitive (via shadcn)

---

## Verification Evidence

| Check | Expected | Actual | Status |
|-------|----------|--------|--------|
| `npx tsc --noEmit` | 0 errors | 0 errors | PASS |
| Round 1 review blockers | 0 | 3 found → fixed | PASS |
| Round 2 review blockers | 0 | 0 | PASS |
| Round 2 review warnings | 0 | 0 | PASS |
| Dangling sonner.tsx imports | 0 | 0 | PASS |
| Raw buttons in shell/ needing conversion | 0 | 0 (7 justified) | PASS |

---

## Risks and Rollback

- **Risk**: Shadcn token alignment (H-2) changes `--primary`, `--destructive`, `--background` in `.dark` block — any component using shadcn semantic tokens (`bg-primary`, `text-destructive`) will render different colors than before.
  - **Mitigation**: This was intentional — the old values were inconsistent with the Axon palette. All affected components were verified.
- **Risk**: `prefers-reduced-motion` wrapping all keyframes may affect users who had animations enabled by default.
  - **Mitigation**: The `no-preference` media query is the default state for users who haven't set a preference. Only users who explicitly set `reduce` will see changes.
- **Rollback**: `git checkout -- apps/web/` to revert all web changes.

---

## Open Questions

- Should the Axon design tokens migrate from `rgba` to `oklch` for consistency with shadcn's color space?
- Should `useIsTablet()` be wired into the shell layout for a three-tier responsive approach?
- The MCP pane and MCP dialog still share duplicated CRUD logic (W-6 from review) — worth a DRY pass?
- Font choice (Noto Sans) was flagged as generic for the neural/cyber aesthetic — worth exploring Geist/Outfit/Sora?

---

## Next Steps

1. Consider migrating hardcoded `rgba()` values in component classNames to the new CSS custom properties (tokens defined but component migration not done for all files)
2. Wire `useIsTablet()` into shell layout for tablet-specific responsive behavior
3. DRY up MCP pane/dialog duplicated EmptyState and CRUD logic
4. Evaluate display font options for stronger brand identity
5. Add `sonner` toast calls to remaining user-facing actions (session create, WS reconnect)
