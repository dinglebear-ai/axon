# Session: Pulse Chat Display Bug Fixes
Date: 2026-02-27
Branch: feat/crawl-download-pack
Commits: `bc62851`, `8e1f4e1`

---

## Session Overview

Investigated and fixed two distinct rendering bugs in the Pulse streaming chat UI:
1. **Duplicate tool call badges** — tool uses appeared twice during and after streaming
2. **Raw JSON / wrong text** — Claude's JSON-formatted response text was shown verbatim instead of the parsed, human-readable text when thinking blocks were present

Both bugs traced to the interaction between `--include-partial-messages` streaming mode and how blocks accumulate across multiple `assistant` events.

---

## Timeline

| Time | Activity |
|------|----------|
| Start | Received bug report: duplicate tool badges + reasoning display confusion |
| +5m | Read `message-content.tsx`, `pulse-chat-pane.tsx`, `stream-parser.ts`, `use-pulse-chat.ts` |
| +15m | Read `route.ts`, `claude-stream-types.ts`, `chat-api.ts`, `workspace-persistence.ts` |
| +10m | Traced full data flow: streaming events → parserState.blocks → data.blocks → MessageContent |
| +5m | Identified both root causes; confirmed with test review |
| +10m | Implemented 4 targeted fixes across 4 files |
| +5m | Ran full test suite: 243/243 pass |
| +5m | Committed (`bc62851`), hit Biome lint warning on unused prop |
| +5m | Fixed unused prop + updated changelog SHA (`8e1f4e1`), pushed both commits |

---

## Key Findings

### Bug 1: Duplicate Tool Call Badges — Two Root Causes

**Root cause A (client-side, during streaming)**
- `pulse-chat-pane.tsx:410-415`: `liveToolUses` rendered as `ToolCallBadge` rows inside the loading indicator
- `use-pulse-chat.ts:267-277`: simultaneously, the draft assistant message in `chatHistory` had `partialBlocks` (including `tool_use` entries) rendered via `MessageContent` → `groupBlocksForRender`
- Result: same tool appeared as badge in the loading indicator AND in the message bubble

**Root cause B (server-side, final state)**
- `stream-parser.ts:105-118`: every `assistant` event containing a tool pushed a **new** `tool_use` block to `state.blocks`, even when that tool already existed (same `block.id`)
- With `--include-partial-messages`, the same tool arrives across many events as its JSON input grows → `parserState.blocks` ends up with N copies of each tool
- `data.blocks` (sent to client in `done` event) therefore has N duplicate `tool_use` entries → N badge rows in the final message

**Same issue for thinking blocks**: growing thinking content arrived as many `assistant` events, each producing a new `ThinkingBlock` component ("Reasoning" box), stacking them visually.

### Bug 2: Raw JSON Shown as Response Text

**Root cause**
- `route.ts:81-92`: Claude is prompted to respond as JSON — `{"text":"...","operations":[]}`
- `stream-parser.ts:97-103`: text deltas from streaming accumulate into `state.blocks` text entries as raw characters (the JSON string itself, e.g., `{"text":"Hello",...}`)
- `route.ts:334`: `parseClaudeAssistantPayload(result)` extracts the clean `text` field → stored in `data.text`
- `use-pulse-chat.ts:309-316`: final `updateChatMessage` sets `content: data.text` (clean) but `blocks: data.blocks` (raw JSON in text blocks)
- `message-content.tsx:106-107`: `hasStructuredBlocks` is `true` when thinking or tool_use present → component renders from `msg.blocks`, showing raw JSON instead of clean text
- Non-structured responses (no thinking/tool_use) were not affected because `hasStructuredBlocks = false` → fell through to `<PulseMarkdown content={msg.content} />`

---

## Technical Decisions

### Fix A: Remove liveToolUses from loading indicator (not from props)
- Kept `liveToolUses` in the component interface to preserve API compatibility with `pulse-workspace.tsx`
- Renamed to `_liveToolUses` in destructuring to silence Biome `noUnusedFunctionParameters`
- Draft message in `chatHistory` renders all tool badges; loading indicator only needs the status text and cancel button

### Fix B: Update-in-place instead of append for tool_use deduplication
- `stream-parser.ts`: checks `state.toolUseIdToIdx.get(block.id)` before pushing — if already seen, mutates the existing block's `input` field
- Does NOT emit a new `tool_use` event to the client for partial updates (input refinements are silent)
- Same logic applied to thinking blocks: update `lastBlock.content` if previous block is also `thinking`

### Fix C: Use `msg.content` for text group rendering
- `message-content.tsx`: `displayContent = (msg.role === 'assistant' && msg.content) ? msg.content : group.content`
- After stream completion, `msg.content = data.text` (parsed clean text); before completion, `msg.content = partialText` (same raw JSON as blocks, so no regression)
- User sees raw JSON accumulating during streaming (acceptable — streaming is transient and fast), then clean text after completion
- Avoids fragile JSON-parse-while-streaming approach

### Fix D: Client-side thinking deduplication
- `use-pulse-chat.ts`: mirrors server-side logic — checks if `lastBlock.type === 'thinking'` before pushing a new block
- Prevents multiple "Reasoning" collapsible boxes from stacking during streaming

---

## Files Modified

| File | Change |
|------|--------|
| `apps/web/app/api/pulse/chat/stream-parser.ts` | Deduplicate tool_use blocks by ID (update-in-place); deduplicate growing thinking blocks |
| `apps/web/components/pulse/message-content.tsx` | Use `msg.content` over `group.content` for text groups in structured-blocks mode |
| `apps/web/components/pulse/pulse-chat-pane.tsx` | Remove `liveToolUses` badge section from loading indicator; remove unused `ToolCallBadge` import; rename prop to `_liveToolUses` |
| `apps/web/hooks/use-pulse-chat.ts` | Update last thinking block in-place during streaming instead of pushing new block |
| `apps/web/__tests__/__snapshots__/pulse-chat-pane-layout.test.ts.snap` | Update snapshot: remove `space-y-1.5` from loading indicator div (no longer needed without badge row) |
| `CHANGELOG.md` | Add entries for `d9823b2`, `b20a7a3`, `bc62851` |

---

## Commands Executed

```bash
# Test suite after fixes
cd apps/web && pnpm test --run
# Result: 243 passed, 0 failed

# Final push
git push
# Result: b20a7a3..8e1f4e1 feat/crawl-download-pack -> feat/crawl-download-pack
```

---

## Behavior Changes (Before / After)

| Scenario | Before | After |
|----------|--------|-------|
| Claude uses a tool, response has no thinking | Tool badge shown once in message bubble ✓ | Unchanged ✓ |
| Claude uses a tool with `--include-partial-messages` | N duplicate tool badges in final message | One badge per unique tool |
| Claude uses a tool during streaming | Tool badge in loading indicator + message bubble | Badge only in message bubble |
| Response with thinking block present | Response text shows raw JSON `{"text":"...","operations":[]}` | Response text shows parsed clean text |
| Incremental thinking events | N stacked "Reasoning" collapsible boxes | Single "Reasoning" box, updated in-place |
| Response with no thinking/tool_use | Clean text rendered ✓ | Unchanged ✓ |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `pnpm test --run` (243 tests) | All pass | 243 passed, 0 failed | ✅ |
| Snapshot test `pulse-chat-pane-layout` | Updated snapshot accepted | 4 tests pass | ✅ |
| Route streaming test (tool_use + tool_result) | 1 tool_use block in `done.blocks` | Confirmed by test line 221-230 | ✅ |
| Biome lint after prop rename | 0 warnings | Clean (`Checked 1 file. No fixes applied.`) | ✅ |
| git push | 2 commits pushed | `b20a7a3..8e1f4e1` | ✅ |

---

## Source IDs + Collections Touched

None — this session was pure code investigation and fix; no Axon embed/query/search operations were performed.

---

## Risks and Rollback

**Risk**: `update-in-place` for tool_use blocks assumes that each tool_use ID appears only once per "logical tool call". If Claude re-uses the same ID for a different tool call (should not happen per spec), the input of the second call would silently overwrite the first.
- Mitigation: Claude Code assigns unique IDs per tool invocation; this is a spec guarantee.

**Rollback**: `git revert bc62851 8e1f4e1` restores both the duplicate display and the raw JSON behavior. The snap file would also need reverting.

---

## Decisions Not Taken

| Alternative | Reason Rejected |
|-------------|-----------------|
| Parse JSON incrementally during streaming to show clean text in real time | Fragile — JSON is invalid until the closing `}` arrives; would require try/catch on every delta |
| Remove `liveToolUses` prop entirely from `PulseChatPaneProps` | Would break `pulse-workspace.tsx` interface; prop kept for future use if design changes |
| Normalize `data.blocks` text entries in `use-pulse-chat.ts` after completion | More invasive; the `msg.content` fallback in the renderer achieves the same result with less code |
| Deduplicate thinking blocks by content prefix comparison | Not needed; sequential-block heuristic (update last block if same type) is sufficient and simpler |

---

## Open Questions

- Does `--include-partial-messages` cause text content to arrive as many small delta events or as one complete block? The current fix handles both cases correctly, but the exact Claude CLI streaming format was not verified empirically.
- The `liveToolUses` prop is now a no-op. Should it be removed entirely in a follow-up cleanup, or kept for potential future use (e.g., showing tool activity in a sidebar)?
- Multiple thinking events with `non-sequential` thinking blocks (e.g., thinking → tool → thinking) would create two separate "Reasoning" boxes. Is this the correct UX, or should all thinking be merged into one?

---

## Next Steps

- Verify live in browser: trigger a Pulse chat response that uses tools with thinking enabled and confirm single badge + clean text
- Consider adding a test case for the thinking-deduplication path in `stream-parser.ts`
- Follow-up: remove `liveToolUses` prop from `PulseChatPaneProps` if it remains unused after further UI review
