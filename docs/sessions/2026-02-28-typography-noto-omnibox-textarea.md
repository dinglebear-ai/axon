# Session: Typography Overhaul — Noto Sans/Mono, Omnibox Refinements, Textarea Auto-Expand
**Date:** 2026-02-28
**Branch:** feat/crawl-download-pack

---

## Session Overview

Four focused UI/typography tasks across the Axon web app:

1. **Font stack replaced** — Sora + Space Mono + JetBrains Mono → Noto Sans + Noto Sans Mono
2. **Typography system tightened** — optical sizing, tabular numbers, heading weight, mono size bump
3. **Omnibox semantic fix** — input/placeholder switched from `font-mono` to `font-sans`; AXON wordmark tracking reduced
4. **Omnibox auto-expand** — `<input>` replaced with auto-resizing `<textarea>` (1–6 lines, then scrolls)

---

## Timeline

1. **Font switch** — Removed `Sora`, `Space_Mono`, `JetBrains_Mono` from `layout.tsx`; loaded `Noto_Sans` + `Noto_Sans_Mono`. Rewired `--font-display`, `--font-sans`, `--font-mono` CSS variables in `globals.css`.
2. **Typography audit** — Analyzed how Noto Sans differs from Sora/Space Mono: larger x-height, proportional letterforms need heading weight differentiation, Noto Sans Mono runs optically smaller than JetBrains Mono.
3. **Refinements applied** — Added `font-optical-sizing: auto`, `font-variant-numeric: tabular-nums`, `"calt"` + `"tnum"` feature settings to `body`. Tightened `body` `letter-spacing` from `0.01em` → `0`. Added `font-weight: 700` and `letter-spacing: -0.03em` to `.font-display` + `h1–h4`. Bumped `.ui-table-dense` and `.ui-mono` from `--text-sm` (12px) → `--text-md` (13px).
4. **Omnibox semantic fix** — Identified omnibox input + placeholder using `font-mono` (semantically wrong for natural-language search). Switched to `font-sans`. Reduced AXON wordmark `tracking-[6px]` → `tracking-[3px]`.
5. **Omnibox auto-expand** — Replaced `<input>` with `<textarea rows={1}>` + `useEffect` auto-resize. Max height 160px (~6 lines), then `overflowY: auto`. TypeScript ref retyped to `HTMLTextAreaElement`.

---

## Key Findings

- **Space Mono was heading font** — `--font-display` mapped to Space Mono, applied globally to `h1–h4`. Monospace letterforms created visual distinction from body (Sora). Switching both to Noto Sans without adding weight/tracking would make headings indistinguishable from body text.
- **Noto Sans Mono runs smaller optically** — JetBrains Mono has tall x-height and heavy stroke weight; Noto Sans Mono is more neutrally proportioned. `.ui-mono` and `.ui-table-dense` at `--text-sm` (12px) would feel smaller after the switch — bumped one step to `--text-md` (13px).
- **Omnibox input was `font-mono`** — `omnibox.tsx:636` had `font-mono text-sm` on the search input. Placeholder at `:643` also used `font-mono`. Search/chat boxes are not mono contexts.
- **`tracking-[6px]` on proportional font breaks** — Space Mono's equal-width glyphs hold at extreme tracking. Noto Sans' proportional letterforms (N wide, O round, A diagonal) produce uneven gap distribution at 6px. Reduced to 3px (`page.tsx:134`).
- **`handleKeyDown` prevents textarea newlines** — Both bare `Enter` and `Cmd+Enter` paths call `e.preventDefault()` before returning, so the `<textarea>` never inserts a newline. No behavior change on submission.

---

## Technical Decisions

- **`font-optical-sizing: auto` on body** — Noto Sans is variable-font-aware and responds to this property. Improves stroke weight rendering at both small label sizes (10px labels) and larger headings automatically. Free quality improvement.
- **`font-variant-numeric: tabular-nums` globally** — App displays chunk counts, timestamps, stats, token counts. Tabular figures prevent column jitter when numbers update. More impactful than it looks in a RAG data-heavy UI.
- **`letter-spacing: 0` on body** — Sora needed `0.01em` nudge for natural spacing. Noto Sans is correctly calibrated out of the box. Removing avoids over-tracking at small sizes.
- **`-0.03em` tracking on headings** — Noto Sans at display sizes is slightly wide optically. Tightening to -0.03em (slightly more than the -0.02em initially set) gives intentional typographic feel without looking squeezed.
- **Auto-resize `useEffect` pattern** — `el.style.height = 'auto'` collapses to intrinsic (scrollHeight reflects true content), then set to `Math.min(scrollHeight, 160)`. `overflowY` toggled dynamically at the 160px threshold. Standard CSS height transition not added — textarea growth is inherently instant and feels more natural than animated.
- **`items-center` retained on omnibox container** — Considered switching to `items-end` (Claude.ai pattern) for button alignment in multi-line state. Rejected: `min-h-[52px]` with `items-end` creates dead space at top in single-line state (buttons bottom-aligned, 14px gap above). `items-center` distributes naturally and looks correct for both states.
- **`rows={1}` + `overflow-y: hidden` initial state** — Prevents flash of scrollbar on mount. The `useEffect` runs on every `input` change including mount, so height is set correctly immediately.

---

## Files Modified

| File | Change |
|------|--------|
| `apps/web/app/layout.tsx` | Removed `Sora`, `Space_Mono`, `JetBrains_Mono` imports; added `Noto_Sans`, `Noto_Sans_Mono`; updated CSS variable names and body className |
| `apps/web/app/globals.css` | Rewired `--font-display/sans/mono` to Noto variables; added `font-optical-sizing`, `font-variant-numeric`, `"calt"/"tnum"` features; `letter-spacing: 0` on body; `font-weight: 700` + `-0.03em` tracking on `.font-display` / `h1–h4`; `.ui-table-dense` + `.ui-mono` from `--text-sm` → `--text-md`; updated fallback font strings |
| `apps/web/components/omnibox.tsx` | `inputRef` retyped to `HTMLTextAreaElement`; added auto-resize `useEffect`; `<input>` → `<textarea rows={1}>` with `resize-none`; input/placeholder class `font-mono` → `font-sans` |
| `apps/web/app/page.tsx` | AXON wordmark `tracking-[6px]` → `tracking-[3px]` |

---

## Commands Executed

```bash
# TypeScript validation after each change
cd /home/jmagar/workspace/axon_rust/apps/web && npx tsc --noEmit 2>&1 | grep "error TS"
# Result: (empty) — zero errors after all changes

# Font usage audit
grep -n "font-space-mono\|font-sora\|font-jetbrains\|font-display\|font-sans\|font-mono" apps/web/app/globals.css
# Confirmed all references updated to Noto variables

# Mono usage inventory
grep -rn 'font-mono' apps/web --include="*.tsx" | grep -v node_modules | grep -v ".next"
# Identified 37 uses; confirmed all are semantically appropriate mono contexts
# except omnibox input/placeholder (corrected)

# Tracking usage inventory
grep -rn 'tracking-\|letter-spacing' apps/web --include="*.tsx" | grep -v node_modules
# Identified AXON wordmark at page.tsx:134 using tracking-[6px] — reduced to tracking-[3px]
```

---

## Behavior Changes (Before/After)

| Area | Before | After |
|------|--------|-------|
| **UI sans font** | Sora (300–700) | Noto Sans (300–700) — wider x-height, more neutral, better for dense data UIs |
| **Heading/display font** | Space Mono (monospace aesthetic) | Noto Sans 700 with -0.03em tracking — clean, weight-differentiated |
| **Mono font** | JetBrains Mono | Noto Sans Mono — less programming-editor, more neutral; sized up 1 step in tables |
| **Body letter-spacing** | `0.01em` | `0` — Noto Sans is correctly spaced; removing the nudge avoids over-tracking |
| **Numbers in UI** | Proportional figures (widths shift) | Tabular figures via `font-variant-numeric: tabular-nums` — stable column width |
| **Omnibox input font** | `font-mono` (JetBrains→Noto Sans Mono) | `font-sans` (Noto Sans) — search/chat input is not a mono context |
| **AXON wordmark** | `tracking-[6px]` (wide, uneven on proportional font) | `tracking-[3px]` — still distinctive, letterforms don't spread unevenly |
| **Omnibox height** | Fixed single-line `<input>` (never grows) | Auto-expanding `<textarea>` (grows 1→~6 lines as text wraps, then scrolls) |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `npx tsc --noEmit` (after font switch) | 0 errors | 0 errors | ✅ |
| `npx tsc --noEmit` (after typography refinements) | 0 errors | 0 errors | ✅ |
| `npx tsc --noEmit` (after omnibox font fix) | 0 errors | 0 errors | ✅ |
| `npx tsc --noEmit` (after textarea swap) | 0 errors | 0 errors | ✅ |
| grep for old font names in globals.css | 0 matches | 0 matches (Sora/Space Mono/JetBrains) | ✅ |

---

## Source IDs + Collections Touched

No Axon embed/retrieve/query operations performed during this session (pure frontend code changes).

---

## Risks and Rollback

- **Font switch (all files)**: Noto Sans/Mono are widely supported Google Fonts. Loaded via `next/font/google` with same subset and weight config as predecessors. Rollback: revert `layout.tsx` and `globals.css` font sections.
- **`font-variant-numeric: tabular-nums` globally**: Affects all number rendering. Tabular figures have fixed width; proportional figures have variable width. In very rare cases (condensed stylistic contexts) tabular figures look slightly out of place. Rollback: remove from `body` rule; add selectively to data display contexts.
- **Omnibox `<textarea>` swap**: If a downstream component or test queries by `input[type]` selector or `HTMLInputElement`-specific properties, it will fail. `inputRef` is now `HTMLTextAreaElement`. `handleKeyDown` intercepts Enter so no newline behavior change. Rollback: revert `omnibox.tsx` to `<input>` and remove auto-resize effect.
- **AXON wordmark tracking**: Visual-only change. Rollback: change `tracking-[3px]` back to `tracking-[6px]` in `page.tsx:134`.

---

## Decisions Not Taken

- **`items-end` on omnibox container** — Would put send button at bottom-right of multi-line textarea (Claude.ai pattern). Rejected because `min-h-[52px]` creates dead space at top in single-line state when combined with `items-end`. `items-center` distributes naturally.
- **CSS height transition on textarea** — `transition: height 150ms ease` considered for smooth growth animation. Rejected because textarea height transitions are unreliable across browsers (layout reflow timing), and instant growth feels more natural for typed input.
- **Separate `font-display` font (e.g. Noto Serif)** — Could pair Noto Serif for headings with Noto Sans for body (same superfamily, different style). Rejected — user asked for clean modern sans; serif headings would shift the aesthetic significantly.
- **`font-optical-sizing` only on headings** — Could limit to `.font-display` / `h1–h4`. Applied to body globally instead because small UI labels (10px) benefit as much as large headings.
- **Per-component mono size bump** — Rather than bumping `.ui-table-dense` and `.ui-mono` globally, could bump individual components. Rejected for consistency — any context using the utility class should get the same correction.

---

## Open Questions

- **Noto Sans Mono perceived size** — The one-step bump in `.ui-table-dense` / `.ui-mono` (12px → 13px) is a calibration guess. Visual verification needed against live running UI to confirm it matches the previous JetBrains Mono perceived size.
- **`tracking-[3px]` on AXON wordmark** — 3px may still feel wide or may feel too narrow depending on the gradient render at specific screen densities. 2px–4px is the reasonable range; user should verify visually.
- **Upstream Space Mono / Sora usage in `.next/standalone`** — `standalone/` dir has copied source files that still reference old font names. These are build artifacts and not source-of-truth; they'll be overwritten on next build. No action needed but worth noting.

---

## Next Steps

- Visual verification in running `axon-web` container — check heading weight contrast, mono element size, omnibox textarea expansion behavior
- Consider adding `font-optical-sizing: auto` to Plate.js editor prose content specifically if content-viewer renders large headings
- Potential: `tracking-[2px]` or `tracking-[4px]` trial on AXON wordmark based on visual preference
- PR #5 (`feat/web): ship pulse workspace foundation and omnibox`) — these typography changes are additive and can be included in or follow the current PR
