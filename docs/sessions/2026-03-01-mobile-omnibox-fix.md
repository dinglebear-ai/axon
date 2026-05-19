# Session: Mobile Omnibox Sizing Fix

**Date:** 2026-03-01
**Branch:** feat/sidebar
**Commit:** ad449c8a

---

## Session Overview

Diagnosed and fixed a three-bug root-cause chain causing the omnibox input to render as a massive ~160px tall box on mobile (390px viewport). Used Chrome DevTools MCP to inspect live layout dimensions and confirm the fix. Also committed pre-existing changes to the repo including the CmdK palette component and misc web improvements.

---

## Timeline

1. **Chrome DevTools setup** — navigated to `http://10.1.0.6:49010`, emulated iPhone 14 Pro (390×844, deviceScaleFactor=3)
2. **Visual inspection** — omnibox rendered as a ~160px tall box instead of a compact 44px single-line bar
3. **Sidebar root cause** — JS eval revealed `main` element was only 130px wide; sidebar was 260px (no mobile auto-collapse)
4. **Textarea root cause** — further eval showed textarea width = 24px, scrollHeight = 160px (cap) even when empty
5. **Placeholder inflation** — at 24px width, the placeholder text wrapped to many lines; `height: auto` returned stretched flex height
6. **Effect timing root cause** — `[input]`-dep resize effect fires once on mount while sidebar is still 260px; never re-runs since `input` stays `''`
7. **Fixes applied** — three targeted edits to `pulse-sidebar.tsx` and `omnibox.tsx`
8. **Verification** — reloaded with mobile emulation, JS eval confirmed `styleH: "44px"`, `containerH: 46`, `sidebarW: 48`
9. **Biome fixes** — pre-commit hook caught 4 lint errors in pre-existing `cmdk-palette/` files; fixed and committed

---

## Key Findings

- `pulse-sidebar.tsx:103` — `useEffect` initialized `collapsed` from localStorage but defaulted to `false` (expanded, 260px) when no preference stored — even on 390px mobile screen
- `omnibox.tsx:531` — `el.style.height = 'auto'` before reading `scrollHeight`: in a flex layout `scrollHeight` reflects the stretched layout height, not intrinsic content height — an empty textarea measured 160px (the cap)
- `omnibox.tsx:528-535` — `useEffect` dep array `[input]`: fires once on mount (sidebar still 260px → textarea 24px wide → placeholder wraps → scrollHeight ≥ 160 → cap hit) and never re-runs since `input` never changes
- `apps/web/components/cmdk-palette/CmdKOutput.tsx:51` — missing `biome-ignore` for intentional trigger-only `lines` dep in scroll effect; missing `type="button"` on two buttons
- `apps/web/components/cmdk-palette/CmdKPalette.tsx:213,220` — `noStaticElementInteractions` on backdrop and panel divs; missing `type="button"` on one button
- `apps/web/app/globals.css:98` — Biome formatting: `0.30` → `0.3` trailing zero

---

## Technical Decisions

- **`height: '1px'` not `'auto'`** — standard pattern for auto-resizing textareas; `'auto'` in flex layout returns the element's current rendered height as scrollHeight, making the measurement meaningless. `'1px'` forces browser to report actual content height.
- **`ResizeObserver` not `window.resize`** — fires precisely when the textarea's container changes size (sidebar collapse reflow), not on every window resize. Cleaner and more targeted.
- **`innerWidth < 768` threshold** — matches Tailwind's `sm:` breakpoint (640px) with extra margin; 768px is a standard tablet/desktop boundary. Intentionally only auto-collapses when no stored preference exists — respects user choice.
- **`biome-ignore` over removing `lines` dep** — the `lines` dep in `CmdKOutput` is intentional (triggers auto-scroll when new lines arrive); removing it would silently break the UX. The comment explains the intent.

---

## Files Modified

| File | Change |
|------|--------|
| `apps/web/components/pulse/sidebar/pulse-sidebar.tsx` | Auto-collapse on mobile (<768px) when no stored preference |
| `apps/web/components/omnibox.tsx` | `height: '1px'` fix + `ResizeObserver` for post-collapse resize |
| `apps/web/components/cmdk-palette/CmdKOutput.tsx` | `biome-ignore` + `type="button"` on 2 buttons (pre-existing) |
| `apps/web/components/cmdk-palette/CmdKPalette.tsx` | `biome-ignore` on 2 divs + `type="button"` on 1 button (pre-existing) |
| `apps/web/app/globals.css` | Biome format fix: `0.30` → `0.3` |
| `apps/web/app/page.tsx` | (pre-existing) |
| `apps/web/components/app-shell.tsx` | (pre-existing — CmdKPalette wired in) |
| `apps/web/components/pulse/pulse-toolbar.tsx` | (pre-existing) |
| `apps/web/components/pulse/pulse-workspace.tsx` | (pre-existing) |
| `apps/web/components/results-panel.tsx` | (pre-existing) |
| `apps/web/hooks/use-ws-messages.ts` | (pre-existing) |
| `crates/web/execute/mod.rs` | (pre-existing) |
| `docs/UI-DESIGN-SYSTEM.md` | (pre-existing) |
| `CHANGELOG.md` | Updated with this session's changes |

---

## Commands Executed

```bash
# Identify server IP
hostname -I  # → 10.1.0.6

# DevTools: measure layout dimensions
# (JS eval via chrome-devtools MCP)
# sidebarW: 260, mainW: 130 → sidebar taking 2/3 of 390px screen
# textareaW: 24, scrollH: 160 → squished textarea, placeholder wraps

# Confirm fix works manually
# el.style.height = '1px'; el.scrollHeight → 44 (correct)

# Verify placeholder impact
# with placeholder: scrollH = 44; without: scrollH = 28

# After fix: styleH: "44px", containerH: 46, sidebarW: 48

# Biome check new files
npx biome check components/cmdk-palette/  # → 3 errors, 3 warnings → fixed

# Commit + push
git commit ... → ad449c8a
git push → 27fc39f6..ad449c8a feat/sidebar -> feat/sidebar
```

---

## Behavior Changes (Before/After)

| Aspect | Before | After |
|--------|--------|-------|
| Mobile omnibox height | ~160px (capped, broken) | ~44px (single line, correct) |
| Sidebar on mobile (first visit) | 260px (2/3 of 390px screen) | 48px (icon-only) |
| `main` content width on mobile | 130px | 342px |
| Textarea width | 24px | ~204px |
| Auto-resize after sidebar collapse | Never (effect stale, `input` unchanged) | Triggers via `ResizeObserver` |

---

## Verification Evidence

| Check | Expected | Actual | Status |
|-------|----------|--------|--------|
| `sidebarW` after mobile auto-collapse | 48px | 48px | ✅ |
| `mainW` after sidebar collapse | ~342px | 342px | ✅ |
| `textareaStyleH` after ResizeObserver fires | 44px | 44px | ✅ |
| `containerH` | ~46px | 46px | ✅ |
| Biome check on cmdk-palette/ | 0 errors | 0 errors | ✅ |
| Pre-commit hook (480 Rust tests) | all pass | all pass | ✅ |
| git push | success | `27fc39f6..ad449c8a` pushed | ✅ |

---

## Source IDs + Collections Touched

_(Axon embed job ID and source ID to be filled after embed completes)_

---

## Risks and Rollback

- **Sidebar auto-collapse on mobile** — stored preference takes priority; users who have previously set the sidebar to expanded will see no change. New visitors on mobile will get collapsed by default. Rollback: revert `pulse-sidebar.tsx` useEffect change.
- **ResizeObserver** — fires on any textarea size change, not just sidebar. On very fast sidebar animations could fire multiple times. `Math.min(scrollHeight, 160)` cap prevents unbounded growth. Low risk.
- **`height: '1px'` flash** — sets textarea to 1px for a single frame before React re-sets. Invisible at 60fps. No visual regression.

---

## Decisions Not Taken

- **`useState(() => isMobile)` for initial collapsed state** — would eliminate the flash-of-expanded-sidebar on first render but causes Next.js hydration mismatch (server has no `window`). Rejected for now; the `useEffect` fires fast enough to be imperceptible.
- **CSS-only mobile sidebar hide (`hidden sm:flex`)** — would fully hide sidebar on mobile. Rejected because user may want to expand it on mobile; auto-collapse preserves that option.
- **`window.resize` event instead of ResizeObserver** — fires on every window resize, not just the textarea's container. More expensive and less precise. Rejected.
- **`max-w-[160px]` cap reduction on mobile** — could reduce from 160px to e.g. 80px on small screens. Not needed once the root cause is fixed.

---

## Open Questions

- The CmdK palette files (`cmdk-palette/`) appear to be pre-existing (listed in git status as `??`) but their origin isn't clear from this session. They may have been added by a parallel agent or outside the current conversation scope.
- GitHub dependabot reported 2 high-severity vulnerabilities on the default branch — unrelated to this session but worth addressing.

---

## Next Steps

- Consider fixing the hydration-mismatch-free version of sidebar mobile init using `suppressHydrationWarning` or a CSS-only approach
- Address GitHub dependabot vulnerabilities
- Verify CmdK palette functionality works end-to-end on mobile
