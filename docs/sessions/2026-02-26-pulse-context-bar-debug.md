# Session: Pulse Context Bar Debugging
**Date:** 2026-02-26
**Branch:** feat/crawl-download-pack
**Focus:** Context bar between omnibox and keyboard hints — rendering, data accuracy, and visibility

---

## Session Overview

Continued from previous session. The Pulse context bar (a narrow progress bar between the omnibox and the "Enter send · @mode switch…" hints line) existed in the DOM but was either invisible or not rendering. This session diagnosed why the bar was not visible, fixed three separate bugs, and added a percentage label to make it unambiguous.

---

## Timeline

| Time | Activity |
|------|----------|
| Start | Resumed from context-compacted session. Previous work had removed a duplicate toolbar context bar and switched to char-based context measurement. |
| Early | Attempted Chrome DevTools MCP verification — discovered remote Chrome (100.120.242.29:9222) is a completely separate instance from the user's browser. Cannot inspect user's session DOM. |
| Mid | Identified root cause #1: `workspaceMode === 'pulse'` guard on the bar was false because bar was added to omnibox which checks the _omnibox_ mode badge (e.g. "SCRAPE"), not the Pulse workspace state. |
| Mid | Identified root cause #2: bar track was nearly invisible at `rgba(255,135,175,0.12)` opacity. |
| Mid | Identified root cause #3: at 4.6% width with no `min-width`, the fill could collapse to sub-pixel. |
| Late | Added `contextUtilizationPercent.toFixed(1)%` text label next to bar to make it unambiguous regardless of bar width. |
| End | Verified API returns correct data (`contextCharsTotal: 36640, contextBudgetChars: 800000`) via DevTools `fetch()` call. |

---

## Key Findings

- **Remote Chrome ≠ User's Browser**: The `chrome-devtools-mcp` is connected to `http://100.120.242.29:9222` (Tailscale Axon Chrome). This is NOT the user's desktop Chrome at `dookie:3000`. Cannot inspect user's DOM via DevTools MCP.
- **`workspaceMode` in omnibox is the omnibox badge state** (`omnibox.tsx:47`), NOT whether PulseWorkspace is active. When user switches back to Scrape mode, `workspaceMode !== 'pulse'` so bar was hidden even with active Pulse session.
- **`workspaceContext` is only non-null when PulseWorkspace is mounted** — set via `updateWorkspaceContext` in `pulse-workspace.tsx:559` after each chat turn, and cleared on unmount at `pulse-workspace.tsx:583`.
- **API confirmed working**: `fetch('/api/pulse/chat', ...)` returned `metadata: { contextCharsTotal: 36640, contextBudgetChars: 800000, elapsedMs: 11927 }` in DevTools eval.
- **Bar was rendering but hairline-thin**: At 4.6% utilization on a ~600px wide bar = ~28px of fill. With nearly-transparent track (`0.12` opacity), fill was nearly invisible. User confirmed seeing "a faint pink line."

---

## Technical Decisions

1. **Removed `workspaceMode === 'pulse'` guard** from bar condition (`omnibox.tsx:880`): Changed to `workspaceContext && workspaceContext.turns > 0`. Safe because `workspaceContext` is null when PulseWorkspace is unmounted.
2. **Added `minWidth: '3px'`** when utilization > 0 — ensures sub-1% usage is still visually present.
3. **Brightened track background**: `rgba(255,135,175,0.12)` → `rgba(255,135,175,0.2)` for better contrast against dark background.
4. **Added percentage text label**: `{contextUtilizationPercent.toFixed(1)}%` in `font-mono text-[10px]` next to bar — makes context usage unambiguous regardless of bar fill width.
5. **Updated tooltip**: Changed "tokens" → "chars (X%)" to accurately reflect char-based measurement.

---

## Files Modified

| File | Change |
|------|--------|
| `apps/web/components/omnibox.tsx:880` | Removed `workspaceMode === 'pulse'` guard from bar render condition |
| `apps/web/components/omnibox.tsx:889` | Brightened track background opacity 0.12 → 0.2 |
| `apps/web/components/omnibox.tsx:894-896` | Added `minWidth: '3px'` when utilization > 0 |
| `apps/web/components/omnibox.tsx:885` | Updated tooltip: "tokens" → "chars (X%)" |
| `apps/web/components/omnibox.tsx:889-899` | Added flex wrapper + `{contextUtilizationPercent.toFixed(1)}%` text label |

*(All changes to `pulse-toolbar.tsx`, `pulse-workspace.tsx`, `use-ws-messages.ts`, `types.ts`, `route.ts` were made in the previous session before context compaction.)*

---

## Commands Executed

```bash
# Confirmed app is running
curl -s -o /dev/null -w "%{http_code}" http://localhost:3000/
# → 200

# Confirmed Next.js process
ps aux | grep next | grep -v grep
# → PID 1718080 node .../next dev

# API verified via DevTools eval
fetch('/api/pulse/chat', { method: 'POST', ... })
# → { metadata: { contextCharsTotal: 36640, contextBudgetChars: 800000, elapsedMs: 11927 } }
```

---

## Behavior Changes (Before/After)

| Aspect | Before | After |
|--------|--------|-------|
| Bar visibility condition | `workspaceMode === 'pulse' && workspaceContext && turns > 0` | `workspaceContext && turns > 0` |
| Bar when omnibox in Scrape mode | Hidden (bug) | Visible if Pulse session active |
| Track opacity | 0.12 (nearly invisible) | 0.20 (visible) |
| Min fill width | None (sub-pixel at low %) | 3px when utilization > 0 |
| Percentage display | Tooltip only (hover) | Visible `4.6%` text label next to bar |
| Tooltip text | "N tokens" | "N / M chars (X%)" |

---

## Verification Evidence

| Check | Expected | Actual | Status |
|-------|----------|--------|--------|
| API `/api/pulse/chat` returns `contextCharsTotal` | Non-zero number | 36,640 | ✅ |
| API returns `contextBudgetChars` | 800,000 | 800,000 | ✅ |
| Bar renders after 1 turn | DOM element present | Unverified in user's browser | ⚠️ |
| Percentage label visible | `4.6%` text next to bar | Unverified in user's browser | ⚠️ |
| Bar hides when no turns | No DOM element | Confirmed in remote Chrome | ✅ |

---

## Source IDs + Collections Touched

None — no Axon embed/retrieve operations during this session.

---

## Risks and Rollback

- **Bar shows during non-Pulse sessions**: Mitigated — `workspaceContext` is null unless PulseWorkspace is mounted, and PulseWorkspace only mounts when `workspaceMode === 'pulse'` in `results-panel.tsx:282`.
- **Rollback**: `git diff apps/web/components/omnibox.tsx` to restore old condition.

---

## Decisions Not Taken

- **Token-based accounting (Claude CLI `usage.input_tokens`)**: CLI only reports ~77-91 tokens per turn (current exchange only), not full context. Rejected in favor of char-based measurement.
- **Hardcoded per-model budgets**: Rejected — user explicitly rejected hardcoded values. Using `800,000 chars` (200k token window × ~4 chars/token) as the universal budget.
- **Using Chrome DevTools to inject React state**: Attempted but React 19 / Next.js 16 Turbopack doesn't expose `__reactFiber` keys on DOM elements in the remote Chrome session. Abandoned.
- **Increasing the bar height**: Bar is `h-1.5` (6px). Not changed — adding a text label is more informative than a taller bar.

---

## Open Questions

- Does the bar actually render visibly in the user's browser after the `workspaceMode` fix? The user's browser session could not be inspected via DevTools MCP (different Chrome instance).
- Is `workspaceMode` in the omnibox `'pulse'` or something else when the Pulse workspace is active but the user selects a different mode badge? Need user confirmation.
- The `workspacePromptVersion > 0` guard in `page.tsx:105` means `isPulseWorkspaceActive` is false on fresh load — but `workspaceMode` defaults to `'pulse'` in `use-ws-messages.ts:294`. Need to confirm whether `PulseWorkspace` renders on fresh load without a submitted prompt.

---

## Next Steps

1. User should send a Pulse chat message and confirm the `X.X%` label appears next to the context bar.
2. Send a second message with more content and confirm the percentage increases.
3. If bar still not showing, add `console.log` to the `updateWorkspaceContext` call in `pulse-workspace.tsx:559` to trace whether it's being called with non-zero `turns`.
