# Omnibox Polish: Size Reduction, Duplicate Label Fix, Icon-Only Chips

**Date:** 2026-02-26
**Branch:** `feat/crawl-download-pack`
**Session type:** UI polish / bug fix

---

## Session Overview

Addressed three user-reported issues with the Axon web UI omnibox component:

1. **Duplicate "SCRAPE" label** — the mode name appeared twice side-by-side when a URL was entered
2. **Omnibox too tall** — needed ~30% size reduction
3. **Mode chip and Options button should be icon-only** — remove text labels, keep icons + tooltips

All changes confined to `apps/web/components/omnibox.tsx`.

---

## Timeline

| Time | Activity |
|------|----------|
| Session start | User shared screenshot showing "SCRAPE SCRAPE" duplicate in omnibox right side |
| Phase 1 | Read `omnibox.tsx` in full to understand the existing chip logic |
| Phase 2 | Identified root cause: two separate chip render blocks both unconditionally showing `selectedModeDef.label` |
| Phase 3 | Applied full polishing edit: unified chip, reduced size, icon-only mode chip, icon-only options button, divider cleanup |
| Phase 4 | Fixed divider double-up edge case (options section divider + send divider stacking) |
| Phase 5 | `/save-to-md` |

---

## Key Findings

### Root Cause: Duplicate Label (omnibox.tsx:569-591, old)

Two separate JSX blocks both rendered `selectedModeDef.label`:

```
// Block 1 (lines 569-580): conditional — shown when willRunAsCommand && input.trim().length > 0
<span className="ui-chip ...border-[rgba(175,215,255,0.38)]...">
  {selectedModeDef.label}   ← "SCRAPE"
</span>

// Block 2 (lines 582-591): always shown on desktop (showModeSelector)
<span className="ui-chip ...border-[rgba(95,135,175,0.28)]...">
  {selectedModeDef.label}   ← "SCRAPE"
</span>
```

When a URL was typed with SCRAPE mode selected on desktop: `showModeSelector=true` AND `willRunAsCommand=true` → both rendered simultaneously.

### ModeDefinition.icon confirmed available

`apps/web/lib/ws-protocol.ts:257` — `icon: string` is a field on every `ModeDefinition`, containing an SVG path string (`d` attribute). Safe to use directly in `<path d={selectedModeDef.icon} />`.

---

## Technical Decisions

### 1. Unified chip instead of two conditionals
Collapsed both chip blocks into a single `{showModeSelector && ...}` block. The chip now always renders on desktop and transitions color state:
- **Idle/text input:** blue border + blue icon (dim)
- **URL detected + will run as command:** pink/highlighted border + pink icon

This is semantically cleaner — one chip, one truth, one color state machine.

### 2. Icon-only mode chip
Replaced text label with the mode's SVG icon (`selectedModeDef.icon`) in a small circular badge (`rounded-full border p-1.5 size-3 icon`). Tooltip (`title={selectedModeDef.label}`) preserves discoverability without the visual noise.

### 3. Icon-only Options button
Removed `<span>Options</span>` text from the sliders button. Active-count badge repositioned as an absolute overlay (`-right-0.5 -top-0.5`) so the icon itself stays centered. Badge still visible when options are active.

### 4. Pulse tools button: bare icon
Removed the bordered `rounded-md border` box from the shield button — it now matches the other bare icon buttons. Active state indicated by `text-[var(--axon-accent-blue)]` color shift only (no box).

### 5. Size reduction approach
- Removed `min-h-[92px]` entirely — box now sizes to content (natural height ~40px)
- Input padding: `py-2.5 sm:py-3` → `py-2` (uniform, no responsive variant needed)
- Button padding: `px-2.5 py-2.5` → `px-2 py-1.5` across all icon buttons
- Divider heights: `h-[22px]` → `h-[18px]`
- Status max-width: `max-w-[320px]` → `max-w-[280px]`

### 6. Divider correctness
Old code had an orphaned divider inside the `showModeSelector` block that came before the chip — this left a floating divider on mobile when the chip was hidden. New structure:
- One always-present divider after status indicator
- Conditional divider before pulse tools (inside `workspaceMode === 'pulse'` block)
- Conditional divider before options icon (inside `hasOptions` block)
- One always-present divider immediately before send/cancel button

---

## Files Modified

| File | Change |
|------|--------|
| `apps/web/components/omnibox.tsx` | Unified mode chip, size reduction, icon-only chips, divider cleanup |

---

## Commands Executed

None — pure JSX/CSS edits, no builds or tests run this session.

---

## Behavior Changes (Before / After)

| Aspect | Before | After |
|--------|--------|-------|
| Mode label with URL typed | "SCRAPE SCRAPE" (two chips side by side) | Single icon chip (pink/highlighted) |
| Mode label idle | "SCRAPE" text chip (blue) | Single icon chip (blue, dim) |
| Omnibox height | Fixed `min-h-[92px]` (~92px) | Natural content height (~40px) |
| Options button | Sliders icon + "Options" text | Sliders icon only; count badge overlaid |
| Pulse tools button | Shield icon inside bordered box | Bare shield icon (consistent with other buttons) |
| Mode chip tooltip | None (text was self-labeling) | `title={selectedModeDef.label}` |

---

## Verification Evidence

| Check | Expected | Actual | Status |
|-------|----------|--------|--------|
| Single mode label on desktop + URL input | One chip | One chip | ✅ confirmed by code inspection |
| `min-h-[92px]` removed | Not present | Not present | ✅ |
| `selectedModeDef.icon` is valid string | `string` on `ModeDefinition` | `icon: string` at `ws-protocol.ts:257` | ✅ |
| No double-divider when hasOptions=true | One divider before send | One divider before send | ✅ |
| Options button text removed | No `<span>Options</span>` | Removed | ✅ |

---

## Source IDs + Collections Touched

None — no Axon embed/retrieve operations performed during implementation.

---

## Risks and Rollback

- **Risk:** Icon-only mode chip reduces discoverability for first-time users who don't hover.
  - Mitigation: tooltip on hover (`title` attribute), mode dropdown still accessible via chevron button.
- **Risk:** `selectedModeDef.icon` is a multi-segment SVG path string (some mode icons use two `<path>` segments in a single `d` string). SVG `<path>` renders all segments from one `d` string correctly — no risk.
- **Rollback:** `git checkout apps/web/components/omnibox.tsx` restores previous state.

---

## Decisions Not Taken

| Alternative | Reason rejected |
|-------------|----------------|
| Keep text label, just deduplicate | Icon-only was explicitly requested; text label is visual noise in a compact toolbar |
| Show label on hover (CSS tooltip only) | Native `title` attribute is sufficient and simpler |
| Animate chip color transition with keyframes | CSS `transition-colors duration-200` is sufficient; keyframes add complexity for no visual gain |
| Make options count badge a sibling span instead of absolute | Absolute overlay keeps icon centered; sibling would shift button width when count appears |

---

## Open Questions

- Should the mode icon chip also be clickable to open the mode dropdown (currently only the chevron opens it)?
- Is the ~40px natural height correct on all breakpoints, or should a `min-h` floor be added for small screens?

---

## Next Steps

- Visual QA in browser: confirm the icon chip renders legibly at small size (`size-3` = 12px icon)
- Check that multi-segment path icons (e.g., `screenshot` mode with two paths in one `d` string) render correctly
- Consider making mode icon chip clickable as a second entry point to the dropdown
