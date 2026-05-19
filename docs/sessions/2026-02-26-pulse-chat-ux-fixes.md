# Pulse Chat UX Fixes
**Date:** 2026-02-26
**Branch:** feat/crawl-download-pack
**Duration:** ~1 hour

---

## Session Overview

Addressed three UX issues in the Pulse chat interface:
1. Redundant "starting" status text displayed twice during loading
2. Claude subprocess killed by SIGTERM after 90s timeout during long research tasks
3. Tool call display redesigned from large expandable cards to compact icon-only badges with hover/click popover

Also discussed architecture for passing Claude Code slash commands through the web UI — decided on `claude --print` one-shot approach over PTY relay.

---

## Timeline

| Time | Activity |
|------|----------|
| 22:03 | Reviewed screenshot — identified redundant "Claude starting..." + "Status: Starting" |
| 22:05 | Removed duplicate status line, reviewed second screenshot confirming tool calls visible |
| 22:06 | User reported SIGTERM crash after extended research task |
| 22:07 | Diagnosed 90s hard timeout in `route.ts:26`, bumped to 300s |
| 22:15 | Redesigned tool call display — icon-only badges with popover |
| 22:25 | Extended redesign to `DocOpBadge` (Replace doc / Append / Insert section pills) |

---

## Key Findings

- **SIGTERM root cause** — `apps/web/app/api/pulse/chat/route.ts:26` had `CLAUDE_TIMEOUT_MS = 90_000`. Research tasks with 10+ WebSearch/WebFetch/Bash tool calls consistently exceed 90s.
- **Duplicate status** — `pulse-chat-pane.tsx` rendered `Claude starting...` in the primary line AND `Status: Starting` in a secondary `ui-meta` div below it. The secondary line was purely redundant.
- **Old tool card UX** — `ToolCallBlock` rendered a full-width bordered card with tool name, input summary text, chevron, and expanding JSON panel per tool call. A single research response could show 10+ stacked cards dominating the chat.
- **DocOpBadge inconsistency** — The `+ Replace doc` / `+ Append` pills used an unrelated rounded-full text pill style (emerald dot + label text) with no interactivity, inconsistent with the new badge system.

---

## Technical Decisions

### Slash command passthrough: `claude --print` over PTY relay
- **Option A (PTY + xterm.js)** rejected — throws away the entire Next.js chat frontend for a dumb terminal window.
- **Option B (named pipe relay)** rejected — fragile, single reader/writer, breaks on s6 restarts.
- **Option C (`claude --print`)** chosen — one-shot subprocess per slash command, streams markdown output through existing `handle_sync_command` path. Stateless slash commands (`/commit`, `/review-pr`) work fine; session context not required for these.

### Tool badge grouping
Consecutive `tool_use` blocks in the `blocks` array are collapsed into a single flex-wrap row of badges. This keeps text paragraphs readable and groups tool activity visually without vertical sprawl.

### Timeout: 90s → 300s
5 minutes is a reasonable upper bound for agentic research. Suggested env-var override (`PULSE_CHAT_TIMEOUT_MS`) as a follow-up for tuning without redeploy.

---

## Files Modified

| File | Change |
|------|--------|
| `apps/web/components/pulse/pulse-chat-pane.tsx` | Removed duplicate status line; rewrote tool display system (badges + popover); rewrote DocOpBadge |
| `apps/web/app/api/pulse/chat/route.ts` | `CLAUDE_TIMEOUT_MS` 90_000 → 300_000 |

---

## Behavior Changes (Before/After)

### Redundant status text
- **Before:** Two lines during loading — `Claude starting...` (with Stop button) + `Status: Starting` below it
- **After:** Single line — `Claude starting...` with Stop button only

### Tool call display
- **Before:** Each tool call = full-width bordered card with name, truncated input text, chevron expand arrow, JSON panel on click. 10 tool calls = 10 stacked cards.
- **After:** `size-5` (20px) icon-only badge per tool call. Consecutive calls collapse into one flex-wrap row. Hover = popover with tool name, up to 4 input key/value pairs, result preview. Click = pin popover open.
- **8 categories with distinct icons/colors:**
  - `agent` (Task) → Bot icon, pink
  - `skill` (Skill) → Zap icon, violet
  - `mcp` (mcp__*) → Plug2 icon, cyan
  - `file` (Read/Write/Edit/Glob/Grep/LS) → File icon, blue
  - `bash` (Bash) → Terminal icon, amber
  - `web` (WebFetch/WebSearch) → Globe icon, teal
  - `plugin` (name contains `:`) → Package icon, orange
  - `builtin` (everything else) → Command icon, slate

### DocOpBadge
- **Before:** Rounded-full text pill `• Replace doc` in emerald green, no interactivity
- **After:** Same `size-5` icon-only badge matching tool badge system. `replace_document` → FilePen, `append_markdown` → FilePlus, `insert_section` → FileText. Hover/click shows popover with op label + "Doc op" chip.

### SIGTERM crash
- **Before:** Research tasks with 10+ tool calls (>90s) killed with `Claude CLI terminated by signal SIGTERM`
- **After:** 300s limit allows typical research tasks to complete

---

## Verification Evidence

| Check | Expected | Actual | Status |
|-------|----------|--------|--------|
| Duplicate status removed | Single loading line | `Status:` div removed from JSX | ✅ |
| Timeout change | `CLAUDE_TIMEOUT_MS = 300_000` | Confirmed at `route.ts:26` | ✅ |
| TypeScript type for DOC_OP_META Icon | No `React.ElementType` without import | Changed to inline function type | ✅ |
| Unused `ToolUseBlock` type cleaned up | Type removed | Deleted from file | ✅ |

---

## Risks and Rollback

- **Timeout increase (300s):** If a Claude subprocess hangs indefinitely, it will now hold an HTTP connection for up to 5 minutes instead of 90s. Low risk — `claude --print` doesn't hang on normal inputs; the real risk was premature kills. Rollback: revert `CLAUDE_TIMEOUT_MS` in `route.ts:26`.
- **Badge popover z-index:** Popovers use `z-50` with `absolute bottom-full` positioning. In deeply nested scroll containers, they may clip. No scroll container wrapping the badge rows currently — low risk.
- **`DocOpBadge` `useState` hooks:** Added two new `useState` + one `useRef` + one `useEffect` to a component that was previously stateless. Correct cleanup via `removeEventListener` in effect return. No memory leak risk.

---

## Decisions Not Taken

- **Env-var for timeout** (`PULSE_CHAT_TIMEOUT_MS`): Mentioned as a follow-up but not implemented — would require one `process.env` read. Skipped to keep the change minimal.
- **Portal for popover:** Could use React portals to avoid any clipping issues. Skipped — absolute positioning works for current layout and avoids a portal dependency.
- **Radix Popover:** Available via shadcn. Skipped — adds a full component dependency for what's a simple hover/click state.

---

## Open Questions

- Should `claude --print` slash command passthrough be wired into the existing WS execute bridge (`ALLOWED_MODES`) or into the Pulse chat route? Not implemented yet — only the architecture was decided this session.
- Should the timeout be exposed as `PULSE_CHAT_TIMEOUT_MS` env var for runtime tuning without redeploy?
- The `mcp__axon__axon` tool call appears in Pulse chat despite `--strict-mcp-config` being passed to `claude -p`. Worth investigating whether MCP servers reachable from the dev host are bypassing this flag.

---

## Next Steps

- Wire `claude --print` slash command detection into the Pulse chat input — detect `/` prefix, route to `claude --print "<input>"`, stream markdown back as a chat message
- Consider `PULSE_CHAT_TIMEOUT_MS` env var for operator control
- Take a screenshot after next research task to confirm badge display looks correct in production
