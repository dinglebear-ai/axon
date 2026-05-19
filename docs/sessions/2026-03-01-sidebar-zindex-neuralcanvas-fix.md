# Session: Sidebar Z-Index / NeuralCanvas Fix

**Date:** 2026-03-01
**Branch:** feat/crawl-download-pack
**Working Dir:** `apps/web`

---

## Session Overview

Investigated and fixed two layered issues with the `PulseSidebar`:

1. **Crawl files not populating** — `app-shell.tsx` had been stripped of `useWsMessages()` and `PulseSidebar` props; `pulse-sidebar.tsx` was a stale flat-nav version without section architecture or `ExtractedSection` wiring.
2. **Sidebar invisible after NeuralCanvas addition** — Adding `<NeuralCanvas>` to `AppShell` caused the home page's own canvas (already rendered inside the content wrapper) to cover the sidebar at the same z-index.

Both issues are now resolved and verified via Chrome DevTools screenshot.

---

## Timeline

| Time | Activity |
|------|----------|
| Session start | User reported crawled files not appearing in sidebar |
| Investigation | Used `/check` screenshot skill + Chrome DevTools to confirm sidebar was in DOM but invisible |
| Root cause 1 | `app-shell.tsx` missing `useWsMessages()` + `PulseSidebar` props; `pulse-sidebar.tsx` was stale flat version |
| Fix 1 | Restored both files from `.next/standalone` build (authoritative copy) |
| User feedback | Restored version overwrote user's changes: AXON logo was `<span>` not `<Link>`, Tags nav item present, Terminal/Creator/Tasks/Jobs/Logs in separate bottom section |
| Fix 2 | Re-applied user's changes: `<Link href="/">` logo, removed Tags, merged page links inline in nav |
| NeuralCanvas added | Added `<NeuralCanvas profile="subtle" />` to `AppShell` so canvas shows on all pages |
| Root cause 2 | Sidebar disappeared: two canvas elements at sidebar coordinates; home page's own `NeuralCanvas` (inside z-[1] content wrapper stacking context) painted over sidebar (also z-[1]) via DOM order |
| Fix 3 | Bumped sidebar container from `z-[1]` → `z-[2]` |
| Verification | Chrome DevTools screenshot confirmed sidebar fully visible with NeuralCanvas behind it |

---

## Key Findings

- **`app-shell.tsx` was stripped** — the working version was in `.next/standalone/components/app-shell.tsx` (had `useWsMessages()` + full `PulseSidebar` props)
- **Two NeuralCanvas elements at sidebar position** — `elementsFromPoint(130, 250)` returned both AppShell's canvas and the home page's own canvas stacked over the sidebar
- **CSS stacking context rule** — `relative z-[1]` on both sidebar and content wrapper puts them at the same stacking level; later DOM order (content wrapper) wins, making its fixed canvas child visible over the sidebar
- **Fix**: `z-[2]` on sidebar is sufficient — it places the sidebar above the content wrapper's stacking context entirely

---

## Technical Decisions

- **`z-[2]` not higher** — only need to be above `z-[1]` content wrapper; no reason to go higher and risk covering modals/dropdowns
- **Restored from `.next/standalone`** — authoritative source for the section-based sidebar architecture that was accidentally deleted; `.next/standalone` reflected the last known-good state
- **Kept NeuralCanvas in AppShell** — `subtle` profile canvas in AppShell means every page gets the background without each page needing its own canvas; the home page's existing canvas became redundant (but not a problem since sidebar is now z-[2])

---

## Files Modified

| File | Change |
|------|--------|
| `apps/web/components/app-shell.tsx` | Restored `useWsMessages()`, `PulseSidebar` with full props, `NeuralCanvas` |
| `apps/web/components/pulse/sidebar/pulse-sidebar.tsx` | Restored section-based architecture; fixed AXON logo to `<Link>`, removed Tags, merged page links inline, bumped container from `z-[1]` to `z-[2]` |

---

## Behavior Changes (Before/After)

| Behavior | Before | After |
|----------|--------|-------|
| Crawl files in sidebar | Never populated (props not wired) | Populates via `useWsMessages()` → `ExtractedSection` |
| Sidebar visibility | Invisible on home page (covered by canvas) | Fully visible on all pages |
| NeuralCanvas background | Only on home page | All pages via AppShell |
| AXON logo | `<span>` (not clickable) | `<Link href="/">` (navigates home) |
| Tags nav item | Present | Removed |
| Page links (Creator/Tasks/Jobs/Logs/Terminal) | Separate bottom section | Inline in main nav after section tabs |

---

## Verification Evidence

| Check | Expected | Actual | Status |
|-------|----------|--------|--------|
| Chrome DevTools screenshot | Sidebar visible with nav items | AXON logo + Files/Starred/Recents/Skills/Workspace/Creator/Tasks/Jobs/Logs/Terminal visible | ✅ PASS |
| NeuralCanvas behind sidebar | Canvas visible through page content | Neural network animation visible in right-side content area | ✅ PASS |
| Sidebar `elementsFromPoint` | Sidebar elements in hit-test | DIV `z-[2]` sidebar present and topmost at sidebar coords | ✅ PASS |

---

## Risks and Rollback

- **Risk**: `z-[2]` on sidebar may appear above modals/dropdowns if those use `z-[1]` or `z-[2]`. Check any floating UI components — they should be `z-50`+ (Tailwind convention).
- **Rollback**: Revert `pulse-sidebar.tsx` outer div class from `z-[2]` back to `z-[1]` if sidebar starts overlapping modals.

---

## Decisions Not Taken

- **Remove home page's own `<NeuralCanvas>`** — Would have fixed the z-stacking conflict without needing `z-[2]`, but the home page canvas is `profile="default"` (more prominent) vs AppShell's `profile="subtle"`. Removing it would change the home page's visual.
- **Use `isolation: isolate`** — CSS `isolation` on the content wrapper would have prevented its fixed children from escaping its stacking context, but that's a less obvious fix and could have other layout side effects.

---

## Open Questions

- Does the home page still render its own NeuralCanvas in addition to AppShell's? If so, both run simultaneously (doubles canvas overhead). Worth checking `apps/web/app/page.tsx` to see if its canvas can be removed now that AppShell provides one.
- Are there other pages with their own `<NeuralCanvas>` that would cause the same z-stacking issue on those routes?

---

## Next Steps

- Audit all pages for standalone `<NeuralCanvas>` usage — if AppShell now provides the canvas universally, page-level canvases can be removed
- Test sidebar populates correctly with live crawl: run `axon crawl <url>` and verify files appear in the Files section of the sidebar
- Check modal/dropdown z-indexes to ensure nothing is below `z-[2]`
