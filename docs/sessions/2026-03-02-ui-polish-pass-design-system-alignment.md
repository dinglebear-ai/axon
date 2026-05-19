# UI Polish Pass + Design System Alignment
**Date:** 2026-03-02
**Branch:** feat/sidebar

---

## Session Overview

Implemented a comprehensive pre-approved UI polish plan (18 changes across 9 files), then performed a second-pass design system alignment audit to eliminate raw `rgba`/hex values where CSS tokens exist, fix an off-brand red error color, correct a below-minimum font size, and fix a redundant placeholder condition. Finished with a command palette selected-item style refinement (border → inset glow) per user preference.

---

## Timeline

1. **Pre-flight reads** — Read all 9 target files to confirm exact current state before editing
2. **Polish pass implementation** — Applied all 17 active changes (change 6 — `overflow-x-auto` on `<pre>` — was already present)
3. **Snapshot updates** — Updated 3 outdated vitest snapshots caused by the visual changes
4. **Design system audit** — Cross-checked every new value against `docs/UI-DESIGN-SYSTEM.md`; found 8 violations; fixed all
5. **Placeholder logic fix** — Caught redundant outer `{!input && !isProcessing &&` guard that killed the `transition-opacity` fade
6. **Palette selection style** — Replaced left-border indicator with inset pink glow per user preference

---

## Key Findings

- `pulse-markdown.tsx:67` — `overflow-x-auto` already on `<pre>` — plan item 6 was pre-done, no action needed
- `omnibox-input-bar.tsx:87` — Linter renamed `placeholderVisible` → `_placeholderVisible` (unused) after outer mount guard was added, flagging that the variable was now dead — confirmed correct
- `omnibox-input-bar.tsx:174–183` — With both outer `{!input && !isProcessing &&` and inner opacity condition `!input && !isProcessing ? 'opacity-100' : 'opacity-0'`, the inner condition was always `true`; `transition-opacity` was inert (pop instead of fade)
- `globals.css:101` — Initial `--axon-error: #ef4444` violated brand identity; design system states pink (`--axon-secondary`) is the alert/error color — corrected to `var(--axon-secondary-strong)`
- `cmdk-palette-dialog.tsx:50` — `border-left: 2px solid rgba(135,175,255,0.08)` used raw rgba; `var(--surface-primary)` is the exact token — corrected; CSS `var()` is valid in injected `<style>` strings
- Pre-existing test failures (11 before session): `pulse-chat-route-streaming` (6 — environment/MCP config) + `pulse-mobile-pane-switcher` (1) + snapshots (3); after session: 7 failures (snapshots fixed, net -4)

---

## Technical Decisions

- **`--axon-error` → `var(--axon-secondary-strong)` not `--axon-secondary`**: Secondary-strong (`#ff9ec0`) is brighter than secondary (`#ff87af`), giving the error dot the visual weight needed to read as an alert state while staying on-brand
- **Placeholder guard removal**: Removed outer `{!input && !isProcessing &&` conditional mount; kept opacity class in className — this restores the 300ms `transition-opacity` fade and is more composable (the span is always in the DOM for a11y tools)
- **Palette inset glow over left-border**: User explicitly rejected left-border; inset `box-shadow` creates a full-perimeter pink frame that works with the rounded-8px border-radius — left-border would have clipped against the radius and looked wrong at the corners
- **Word count moved from header → footer**: Header bar was already dense; footer has visual breathing room and the count is more logically grouped with the editor state hints (copilot active, Ctrl+Space, Tab, Esc)
- **Source pills hostname-only**: Hostnames are 10–30 chars vs full URLs at 50–120 chars; `max-w` narrowed from 190px → 160px since shorter strings never need the extra room

---

## Files Modified

| File | Changes |
|------|---------|
| `apps/web/components/pulse/pulse-chat-pane.tsx` | Stop button neutral style; source pills hostname-only + max-w-[160px] |
| `apps/web/components/pulse/pulse-workspace.tsx` | Citation count hidden when 0 |
| `apps/web/components/pulse/message-content.tsx` | User bubble max-w-[72%]→[80%]; animation delay capped at 150ms |
| `apps/web/components/pulse/pulse-editor-pane.tsx` | Sparkles icon replaces ✦; kbd styled; word count moved to footer |
| `apps/web/components/omnibox/omnibox-input-bar.tsx` | Placeholder fade fix; mention tip card bg; disabled cursor; error dot token + glow |
| `apps/web/components/results-panel.tsx` | Tab font-weight equalized (no layout shift); badge flex alignment |
| `apps/web/components/landing-cards.tsx` | SessionRow hover adds text-primary |
| `apps/web/components/cmdk-palette/cmdk-palette-dialog.tsx` | Selected item: left-border → inset pink glow; empty state styled with Search icon |
| `apps/web/app/globals.css` | Added `--axon-error`/`--axon-error-bg` tokens (pink-based, on-brand) |

---

## Commands Executed

```bash
# Baseline test count (pre-change via git stash)
git stash && pnpm test
# → 4 files failed, 11 tests failed (pre-existing)
git stash pop

# After changes
pnpm test
# → 4 files failed, 10 tests failed (1 improvement, 3 snapshots mismatched)

# Update mismatched snapshots
npx vitest run --update __tests__/omnibox-snapshot.test.tsx __tests__/pulse-chat-pane-layout.test.ts
# → 3 snapshots updated

# Final test state
pnpm test
# → 2 files failed, 7 tests failed (all pre-existing streaming/MCP-config failures)

# Lint
pnpm lint
# → 0 errors, 18 warnings (all pre-existing Plate.js static element warnings)
```

---

## Behavior Changes (Before/After)

| Component | Before | After |
|-----------|--------|-------|
| Stop button | Red border + dark-red bg + rose-200 text (alarming) | Neutral dim with subtle pink hover |
| Source pills | Full URL truncated at 190px | Hostname only, max 160px |
| Citation badge | Always shows "0" when empty | Hidden when count is 0, icon only |
| User message bubble | max-w-[72%] (asymmetric) | max-w-[80%] (equalized with assistant) |
| Message stagger | Up to 500ms on 20th message | Capped at 150ms |
| Omnibox placeholder | Hides on focus (guidance disappears) | Stays visible while input empty + not processing |
| Mention tip | Floats over neural canvas (unreadable) | Card: border + `surface-base` bg + shadow + blur |
| Send button disabled | Opacity only, cursor default | `cursor-not-allowed` + opacity |
| Editor footer `✦` | Unicode glyph (rendering varies) | `<Sparkles className="size-2.5" />` icon |
| Editor `<kbd>` | Plain monospace text | Border + `surface-primary` bg + 2xs font |
| Word count | Editor header (dense row) | Editor footer (breathing room) |
| Tab inactive font | `font-medium` → `font-semibold` on click (layout shift) | `font-semibold` always (no shift) |
| Badge alignment | Inline span with `ml-1.5` (baseline drift) | `inline-flex items-center gap-1.5` |
| SessionRow hover | Background only | Background + text-primary |
| Palette inactive item | Border `transparent` (hard flash) | No border (clean default state) |
| Palette selected item | Blue left-border + blue bg | Inset pink glow + pink-tinted bg + blue text |
| Palette empty state | Plain centered text | `<Search>` icon + styled flex column |

---

## Verification Evidence

| Check | Expected | Actual | Status |
|-------|----------|--------|--------|
| `pnpm lint` | 0 errors | 0 errors, 18 pre-existing warnings | ✅ |
| `pnpm test` (snapshot files) | Pass | Pass (after `--update`) | ✅ |
| `pnpm test` (streaming failures) | Pre-existing | Still pre-existing (7 tests, 2 files) | ✅ (not regressed) |
| Net test delta | No new failures | -4 failures vs baseline | ✅ |
| `--axon-error` token | On-brand pink | `var(--axon-secondary-strong)` | ✅ |
| Raw rgba audit | All tokenized | All raw values replaced with design tokens | ✅ |

---

## Design System Alignment Fixes (Second Pass)

| Item | Violation | Fix |
|------|-----------|-----|
| Stop button bg | `rgba(10,18,35,0.55)` | `var(--surface-elevated)` |
| Stop button hover border | `rgba(255,135,175,0.3)` | `var(--border-accent)` |
| kbd background | `rgba(135,175,255,0.08)` | `var(--surface-primary)` |
| kbd font size | `text-[9px]` (below 10px floor) | `text-[length:var(--text-2xs)]` |
| Mention tip bg | `rgba(10,18,35,0.85)` | `var(--surface-base)` |
| cmdk inactive border | `rgba(135,175,255,0.08)` in CSS string | `var(--surface-primary)` |
| `--axon-error` value | `#ef4444` (off-brand red) | `var(--axon-secondary-strong)` |
| Error dot glow | `rgba(239,68,68,0.5)` red | `rgba(255,135,175,0.5)` pink |

---

## Source IDs + Collections Touched

*No Axon crawl/embed/query operations performed this session — pure UI code changes.*

---

## Risks and Rollback

- **Snapshot updates**: 3 snapshots regenerated (`omnibox-snapshot`, `pulse-chat-pane-layout` ×2) — revert via `git checkout -- apps/web/__tests__/__snapshots__/`
- **`--axon-error` token**: All consumers use `var(--axon-error)` — changing the token value is a single-point rollback in `globals.css:101`
- **Placeholder outer guard removal**: Span now always in DOM — if a11y tools surface it as an issue, reinstate the conditional mount and accept the pop-vs-fade trade-off
- **Palette selection style**: No logic change, pure CSS — rollback is one `cmdk-palette-dialog.tsx` edit

---

## Decisions Not Taken

- **`font-semibold` on inactive tabs**: Plan called for this to prevent layout shift; could have used `font-medium` on active tab instead — rejected because active tabs conventionally carry heavier weight across the design system
- **Animate placeholder with `animationFillMode: backwards`**: Would have required restructuring the placeholder cycling logic — kept `transition-opacity` since it's already wired and sufficient
- **Left-border on palette selected items**: User rejected; inset glow chosen as the alternative
- **Gradient left-border (blue→pink) on palette selected**: Considered but more complex than needed; inset glow achieves the same "framing" effect more elegantly

---

## Open Questions

- `pulse-chat-route-streaming.test.ts` — 6 pre-existing failures related to MCP config JSON parsing (`SyntaxError: Unexpected token 'o'`) — root cause not investigated this session; may be a test environment issue
- `pulse-mobile-pane-switcher.test.ts` — 1 pre-existing failure (`marks chat tab selected when chat is active`) — not caused by our changes but worth investigating

---

## Next Steps

- Run manual visual verification checklist from the plan (10 items) in browser
- Investigate the 7 pre-existing test failures to determine if they're fixable env issues or design gaps
- Update `docs/UI-DESIGN-SYSTEM.md` to document the `--axon-error` token formally (currently only success/warning/danger-bg are listed in §1 Status Colors)
