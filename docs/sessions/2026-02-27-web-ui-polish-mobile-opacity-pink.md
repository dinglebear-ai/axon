# Web UI Polish — Mobile, Opacity & Pink Accent Pass
**Date:** 2026-02-27
**Branch:** feat/crawl-download-pack

---

## Session Overview

Visual polish pass across the Axon web UI. Six targeted improvements: mobile omnibox sizing, editor tab navigation on landing page, pink accent color on settings/shield icons in the omnibox, "Recent Sessions" heading and session name colors, and a global opacity bump across all surface layers. Settings page (`/settings`) received a matching opacity pass.

---

## Timeline

| Time | Activity |
|------|----------|
| Start | Read omnibox.tsx, page.tsx, recent-sessions.tsx, globals.css, settings/page.tsx |
| Mid | Applied all 6 changes across 6 files |
| End | Verified via git diff; settings page opacity pass added as follow-up |

---

## Key Findings

- **CSS variable naming is inverted**: `--axon-accent-blue: #ff87af` is visually the hot pink; `--axon-accent-pink: #afd7ff` is visually the light blue. "Our pink" = `var(--axon-accent-blue)` = `#ff87af`.
- **`landingMobilePane` was wired to nothing**: The state tracked editor/chat tab selection in `page.tsx` but was never used to toggle pane visibility — editor tab click had zero visible effect on mobile.
- **`--axon-surface-3`** was `rgba(15,23,42,0.12)` — effectively invisible (12% opacity). This drove the "100% see through" feeling on the main interface card.
- **Settings page** had all its own inline `rgba` values separate from the CSS variables, requiring a dedicated pass.
- The `PulseMobilePaneSwitcher` component itself was correct; the issue was entirely in the parent `page.tsx` not consuming `landingMobilePane`.

---

## Technical Decisions

- **Editor tab placeholder**: When `landingMobilePane === 'editor'` and `!hasResults`, show "Run a command to see results here" rather than an empty screen. This makes the tab feel intentional without requiring a ResultsPanel API change.
- **`hidden lg:block`** pattern: On mobile in editor pane, hide the omnibox section entirely rather than stacking it below results. Desktop layout is unaffected (`lg:block` restores it).
- **Pink = `var(--axon-accent-blue)`**: Used this CSS variable (not hardcoded `#ff87af`) for Settings icon and Shield icon in omnibox so it tracks theme changes.
- **"Recent Sessions" header** uses hardcoded `#ff87af` (not the variable) because the CSS variable name `--axon-accent-blue` is confusing and the header is a one-off label, not an interactive element.
- **Opacity bumps are modest** (~+0.12–0.16 per surface) per user's "a LIL BIT" qualifier — not dramatic enough to lose the glass morphism aesthetic.

---

## Files Modified

| File | Change |
|------|--------|
| `apps/web/components/recent-sessions.tsx` | "Recent Sessions" header → `#ff87af`; session name/preview text → `white` |
| `apps/web/components/omnibox.tsx` | Mobile min-height `44px→52px`; input padding `py-2→py-3 sm:py-2`; Settings icon → pink; Shield icon → pink; bg opacity `0.65→0.80` |
| `apps/web/app/page.tsx` | Wire `landingMobilePane` to hide omnibox on mobile in editor mode; add empty-state placeholder; bump fixed omnibox bg `0.72→0.85` |
| `apps/web/app/globals.css` | `--axon-surface-1`: `0.50→0.62`; `--axon-surface-2`: `0.35→0.48`; `--axon-surface-3`: `0.12→0.28` |
| `apps/web/components/results-panel.tsx` | Content/stats panel bg `rgba(3,7,18,0.25)→0.42` (both instances) |
| `apps/web/app/settings/page.tsx` | Header bar `0.72→0.86`; sidebar `0.55→0.70`; toggle rows/info panels `0.38→0.58`; inputs `0.5→0.65`; focus state `0.7→0.82` |

---

## Behavior Changes (Before → After)

| Area | Before | After |
|------|--------|-------|
| Omnibox on mobile | 44px min-height, `py-2` padding | 52px min-height, `py-3` padding on mobile |
| Editor tab on landing (mobile) | Click does nothing — pane state orphaned | Hides omnibox section; shows results or placeholder |
| Settings icon in omnibox | Muted gray, turns blue on hover | Hot pink at rest, white on hover |
| Shield/model icon in omnibox | Muted gray, turns blue on hover | Hot pink at rest, white on hover |
| "Recent Sessions" heading | Dim gray (`--axon-text-dim`) | Hot pink (`#ff87af`) |
| Session names in Recent Sessions | Muted gray (`--axon-text-muted`) | White |
| Session details (time, size) | Subtle gray | Unchanged |
| Main interface card | 12% opacity (nearly invisible) | 28% opacity |
| Surface-1 elements | 50% opacity | 62% opacity |
| Surface-2 elements | 35% opacity | 48% opacity |
| Settings page header | 72% opacity | 86% opacity |
| Settings sidebar | 55% opacity | 70% opacity |
| Settings inputs/selects | 50% opacity | 65% opacity |
| Results/stats panel bg | 25% opacity | 42% opacity |

---

## Verification Evidence

| Check | Expected | Actual | Status |
|-------|----------|--------|--------|
| `git diff --stat` shows 6 files changed | 6 specific files | 6 files in diff (plus pre-existing branch changes) | ✅ |
| `--axon-surface-3` in globals.css | `rgba(15,23,42,0.28)` | Confirmed via diff | ✅ |
| Settings icon class in omnibox.tsx | `text-[var(--axon-accent-blue)]` | Confirmed via diff | ✅ |
| `min-h-[52px] sm:min-h-[44px]` in omnibox | Present | Confirmed via diff | ✅ |
| Recent Sessions header color | `#ff87af` | Confirmed via diff | ✅ |
| Session name color | `white` | Confirmed via diff | ✅ |
| `hidden lg:block` on omnibox section in editor mode | Present in page.tsx | Confirmed via diff | ✅ |

---

## Source IDs + Collections Touched

None — no Axon embed/retrieve operations were performed during this session (pure frontend code edits).

---

## Risks and Rollback

- **Mobile editor pane placeholder**: Low risk — only shown on `< lg` viewport when `landingMobilePane === 'editor'` and `!hasResults`. Desktop layout is completely unaffected.
- **Opacity increase**: Purely cosmetic. If too opaque, decrease the values in `globals.css` lines 89-91 and revert inline values in the affected files.
- **Rollback**: `git checkout -- apps/web/components/omnibox.tsx apps/web/components/recent-sessions.tsx apps/web/app/page.tsx apps/web/app/globals.css apps/web/components/results-panel.tsx apps/web/app/settings/page.tsx`

---

## Decisions Not Taken

- **Add `mobilePane` prop to `ResultsPanel`**: Would have been cleaner but required a component API change. Inline `hidden lg:block` in `page.tsx` achieves the same without touching ResultsPanel.
- **Use `var(--axon-accent-blue)` for "Recent Sessions" header**: Avoided because the inverted naming (`--axon-accent-blue` = pink) is confusing for a read-only label. Used hardcoded `#ff87af` for clarity.
- **Rename CSS variables**: Fixing `--axon-accent-blue`/`--axon-accent-pink` naming inversion would break many usages across the codebase — deferred.

---

## Open Questions

- The CSS variable names `--axon-accent-blue` and `--axon-accent-pink` are inverted (blue var = pink color, pink var = blue color). Should these be renamed in a dedicated cleanup PR? Many files reference them.
- Should the "Run a command to see results here" placeholder have an arrow or visual indicator pointing to the omnibox (which is hidden in editor mode on mobile)?

---

## Next Steps

- Smoke-test mobile layout at 375px viewport (iPhone SE) to confirm omnibox size and tab switching feel right
- Consider a dedicated CSS variable naming cleanup PR to fix the blue/pink inversion
- The `_CANVAS_PROFILE_LABELS` and `_handleCanvasProfileChange` dead code removed from `page.tsx` in this branch — confirm that was pre-existing cleanup and not something we accidentally broke
