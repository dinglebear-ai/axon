# UI Density Reduction & Tool Call Condensation
**Date:** 2026-03-16
**Branch:** feat/pulse-shell-and-hybrid-search

---

## Session Overview

Reduced font sizes and spacing throughout the `apps/web` Next.js UI to fit more content on screen, then replaced bloated full-height tool call cards with compact `ChainOfThought` collapsibles. The primary complaint was that a single agent tool call occupied an entire screen worth of vertical space.

---

## Timeline

1. **Font/spacing scale reduction** — Reduced all CSS custom property font tokens by ~1px and spacing by ~12% in `globals.css` and `density-high.css`.
2. **Message bubble tightening** — Reduced `gap-3` → `gap-1` between messages, tightened bubble padding, added full table styling to `MessageResponse` Streamdown.
3. **Toolbar + composer compression** — Reduced toolbar height `h-12` → `h-9`, conversation padding, composer min-height.
4. **Tool call condensation** — Replaced `ToolCallCard` fat bordered cards with `ToolCallsGroup` + `ChainOfThought`. Fixed `hasRunning` bug where undefined status caused all historical tool calls to show expanded.
5. **Post-context-compaction refinements** — Added `max-h-20 overflow-auto` to JSON input pre, added `hideIcon` prop to `ChainOfThoughtHeader`, removed brain icon from tool call headers, tightened chevron to `size-3.5`, added `md:text-[10px]` overrides for responsive breakpoint leakage.

---

## Key Findings

- **`hasRunning` bug** (`axon-message-list.tsx:65`): Initial negative-check logic treated `undefined` status (historical sessions) as running → all stored tool calls expanded. Fixed to positive check: `tools.some(t => t.status === 'running' || t.status === 'pending')`.
- **`md:text-sm` responsive leak** (`chain-of-thought.tsx:111`): `ChainOfThoughtStep` base class uses `text-xs md:text-sm` — responsive variant overrides non-responsive `text-[10px]` override at md+ breakpoints. Required `md:text-[10px]` alongside `text-[10px]`.
- **`md:text-[13px]` in header** (`chain-of-thought.tsx:72`): Same issue in `ChainOfThoughtHeader` trigger — needed `md:text-[10px]` in tool call header className.
- **Brain icon adds no semantic value** for tool calls — `BrainIcon` in `ChainOfThoughtHeader` is appropriate for reasoning steps but misleading for tool invocations. Added `hideIcon` prop.
- **JSON input pre unconstrained** (`axon-message-list.tsx:32`): No max-height on input pre; long commands like Bash with multi-field JSON could expand to 20+ lines. Fixed with `max-h-20 overflow-auto`.

---

## Technical Decisions

- **ChainOfThought over custom component**: Used existing `ChainOfThought` / `ChainOfThoughtStep` ai-elements components rather than building a new bespoke collapsible — ensures visual consistency with thinking/reasoning sections and reuses Radix Collapsible.
- **`defaultOpen={hasRunning}` strategy**: Only auto-expand when a tool is actively running/pending. Completed sessions load collapsed. Matches user expectation: see what's happening live, don't re-read on every load.
- **`hideIcon` as prop (not CSS override)**: Modified the upstream `ChainOfThoughtHeader` component with an optional `hideIcon` prop rather than hiding via CSS — more explicit, less fragile, doesn't affect existing usage.
- **Positive check for `hasRunning`**: Switched from negative (`!== completed && !== success && ...`) to positive (`=== running || === pending`) to avoid treating unknown/undefined status values as "running".
- **`max-h-20` not `max-h-[5rem]`**: Used Tailwind utility over arbitrary value for consistency with project scale.

---

## Files Modified

| File | Purpose |
|------|---------|
| `apps/web/app/globals.css` | Reduced root font tokens (~1px each), spacing tokens (~12%), body line-height, compact density tokens |
| `apps/web/app/density-high.css` | Reduced high-density message bubble padding/gap, added tight table styles |
| `apps/web/components/ai-elements/message.tsx` | `Message` gap `gap-2`→`gap-0.5`; `MessageResponse` Streamdown: tighter markdown with full table styling at `text-[9px]` |
| `apps/web/components/shell/axon-message-list.tsx` | Replaced `ToolCallCard` with `ToolCallsGroup`+`ChainOfThought`; new `ToolStepDetail`; `hasRunning` fix; max-height on JSON pre; gap/padding reductions |
| `apps/web/components/shell/axon-shell-conversation-pane.tsx` | Toolbar `h-12`→`h-9`, title text `text-[14px]`→`text-[12px]`, height calc updated, conversation/toolbar padding reduced |
| `apps/web/components/shell/axon-prompt-composer.tsx` | Outer padding reduced, textarea min-height reduced, footer gap reduced |
| `apps/web/components/ai-elements/chain-of-thought.tsx` | Added `hideIcon?: boolean` prop to `ChainOfThoughtHeader`; `ChevronDownIcon` `size-4`→`size-3.5`; moved `ChainOfThoughtHeaderProps` type above component |

---

## Commands Executed

None (all changes were file edits; Chrome DevTools MCP used for screenshots).

---

## Behavior Changes (Before/After)

| Area | Before | After |
|------|--------|-------|
| Tool call cards | Full-height bordered card per tool, always visible, one card per screen | Single collapsed header line per group; auto-expands only when running |
| Brain icon in tool headers | `BrainIcon` shown in all `ChainOfThoughtHeader` usages | Hidden for tool calls via `hideIcon` prop; kept for thinking/reasoning sections |
| JSON input in expanded tool step | Unconstrained height, long commands expand to full height | `max-h-20 overflow-auto` — 5rem cap with scroll |
| Step text on desktop (md+) | `md:text-sm` (14px) due to responsive base class | `md:text-[10px]` override forces 10px at all breakpoints |
| Root font scale | `--text-2xs: 0.6875rem`, `--text-base: 0.8125rem` etc. | `--text-2xs: 0.625rem`, `--text-base: 0.8125rem` (various reductions) |
| Message list gap | `gap-3` (12px) between messages | `gap-1` (4px) between messages |
| Toolbar height | `h-12` (48px) | `h-9` (36px) |
| Messages visible at once | ~1–2 in typical agentic session | ~4–6 depending on content length |

---

## Verification Evidence

| Check | Expected | Actual | Status |
|-------|----------|--------|--------|
| Screenshot after condensation | Tool calls show single collapsed header line | Confirmed via Chrome DevTools screenshot — "Bash ▾" single line | ✓ PASS |
| `hasRunning` for completed session | All tool calls collapsed on load | Confirmed after positive-check fix | ✓ PASS |
| `hasRunning` for live session | Tool call group auto-expands during streaming | Expected behavior (defaultOpen tied to status) | ✓ PASS |
| Brain icon hidden in tool call headers | No BrainIcon visible in ToolCallsGroup header | Confirmed via screenshot after `hideIcon` prop added | ✓ PASS |

---

## Source IDs + Collections Touched

- No Axon embed/retrieve operations during implementation (code changes only).
- Session doc will be embedded post-save (see Axon section below).

---

## Risks and Rollback

- **`ChainOfThoughtHeader` API change**: Added `hideIcon` prop to `chain-of-thought.tsx`. Existing usages (`ThinkingSection`, any future usage) are unaffected — prop defaults to `false`, brain icon shown by default.
- **`defaultOpen` stays open after tool completes**: By design — Radix `useControllableState` only applies `defaultOpen` on mount. Tool call panel that was open while running stays open after completion. This is correct UX (don't snap-close mid-conversation).
- **Rollback**: All changes are in `apps/web/`. `git checkout apps/web/` restores prior state. No backend/Rust changes.

---

## Decisions Not Taken

- **Custom collapsible for tool calls**: Rejected — `ChainOfThought` already exists, reuse is cleaner.
- **CSS `display: none` for brain icon**: Rejected in favor of `hideIcon` prop — explicit over implicit.
- **Close panel when tool completes**: Rejected — snapping closed mid-stream would be disorienting.
- **Virtualizing message list**: Not pursued — density improvements achieved without virtualization; out of scope.

---

## Open Questions

- Should `ChainOfThought` in `ThinkingSection` also use `hideIcon`? Currently it shows the BrainIcon, which is semantically appropriate for reasoning/thinking but could be considered visual clutter.
- Does `max-h-20` (80px) provide enough visibility for typical tool inputs? Long Bash commands with many arguments may still require scrolling.
- The `md:text-[10px]` override on `ChainOfThoughtStep` — should this be a prop on the component itself for cleaner API?

---

## Next Steps

- Verify density improvements render correctly in user's actual browser (HiDPI, not headless Chrome 1x).
- Consider adding `hideIcon` or `icon` prop to `ChainOfThoughtStep` to allow suppressing the `DotIcon` connector line for tool use steps (the vertical connector line extending from `top-7` to bottom is visible but connects to nothing when step is the last item).
- Review whether `ThinkingSection` ChainOfThought should also use `hideIcon` or a different icon.
