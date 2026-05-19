# Session: Pulse Module Splits + Ask Gates + Omnibox Polish
**Date:** 2026-02-26
**Branch:** feat/crawl-download-pack
**Commits pushed:** 7be0ba0, 3f0d8bb, 3d5b68c

---

## Session Overview

Implemented a pre-written refactor plan to split three over-limit Pulse files into 13 focused modules, staying under the 450-line monolith policy hard limit. No behavioral changes. Simultaneously added `ask_strict_procedural` / `ask_strict_config_schema` config fields (both default `true`) to the Rust `ask` command.

All verification passed: TSC clean, Biome clean, 110/110 tests green.

---

## Timeline

1. **Plan loaded** — `happy-sauteeing-gizmo.md` plan read; execution order confirmed: Agent B (workspace) first to create shared `workspace-persistence.ts`, then Agents A (route.ts) and C (chat-pane.tsx) in parallel.
2. **Foundation created** — `lib/pulse/workspace-persistence.ts` written manually (pure TS, zero React) as the shared leaf module.
3. **Parallel agents spawned** — 3 Task agents executed concurrently, each owned a distinct file group.
4. **Agent results integrated** — All 3 agents returned with green output; minor post-hoc fixes applied.
5. **`.pnpm-store` in git** — `git add .` staged the pnpm cache. Added `.pnpm-store` to `.gitignore`, unstaged with `git rm -r --cached`.
6. **Verification** — `pnpm tsc --noEmit`, `pnpm biome check`, `pnpm test` all green.
7. **Pushed** — 3 commits pushed to remote.

---

## Key Findings

- `lib/pulse/workspace-persistence.ts` must be created first — it owns `ChatMessage` which is imported by both `message-content.tsx` and `pulse-chat-pane.tsx`.
- `use-pulse-chat.ts` came in at 377L (vs 250L target) — still under 450L hard limit; `runChatPrompt`/`runSourcePrompt` extracted to `lib/pulse/chat-api.ts` to stay manageable.
- `handleCopyError` must remain in `PulseChatPane` body — it captures `setCopyStatus` state from the component.
- WS scrape/crawl subscription moved into `use-pulse-chat.ts`, takes `subscribe` as a param to avoid owning `useAxonWs` state.

---

## Technical Decisions

- **No backwards compat / re-exports** — Sole user policy; all import paths updated in-place. Zero shim files created.
- **`workspace-persistence.ts` as leaf** — Pure TS, no React imports, making it safe to import from both hooks and components without circular deps.
- **`lib/pulse/chat-api.ts` overspill** — Agent B correctly identified `use-pulse-chat.ts` would exceed 450L and split async API helpers into a new pure-function module.
- **Agent execution order** — Sequential for Agent B (creates shared foundation), parallel for Agents A and C after.
- **`extractToolResultText` invariant** — Silently returns empty string on malformed input, never throws; enforced in `stream-parser.ts`.

---

## Files Modified

### New files created (13)

| File | Lines | Purpose |
|------|-------|---------|
| `apps/web/app/api/pulse/chat/replay-cache.ts` | 35 | `ReplayCacheEntry`, `replayCache` singleton Map, `pruneReplayCache()`, `computeReplayKey()` |
| `apps/web/app/api/pulse/chat/claude-stream-types.ts` | 108 | Constants, `ClaudeStreamAssistantContent`/`ClaudeStreamEvent` interfaces, `buildClaudeArgs()`, `computeContextCharsTotal()` |
| `apps/web/app/api/pulse/chat/stream-parser.ts` | 161 | `StreamParserState`, `ParsedLineResult` discriminated union, `parseClaudeStreamLine()`, `extractToolResultText()` |
| `apps/web/components/pulse/tool-badge.tsx` | 217 | `ToolCategory`, `classifyTool()`, `ToolCallBadge` component |
| `apps/web/components/pulse/doc-op-badge.tsx` | 73 | `DOC_OP_META`, `DocOpBadge` component |
| `apps/web/components/pulse/chat-utils.ts` | 61 | Storage key constants, `computeMessageVirtualWindow()`, `formatMessageTime()`, `formatStreamPhaseLabel()` |
| `apps/web/components/pulse/message-content.tsx` | 208 | `ThinkingBlock`, `groupBlocksForRender()`, `MessageContent`, `MessageBubble` |
| `apps/web/hooks/use-pulse-autosave.ts` | 71 | 1500ms debounced autosave to `/api/pulse/save` |
| `apps/web/hooks/use-pulse-chat.ts` | 377 | All chat state + streaming logic, WS subscription |
| `apps/web/hooks/use-pulse-persistence.ts` | 188 | localStorage hydration + `persistWorkspaceState` |
| `apps/web/hooks/use-split-pane.ts` | 147 | All split/layout state + drag effects + media query |
| `apps/web/lib/pulse/workspace-persistence.ts` | 126 | `ChatMessage` interface, persisted state types, pure helpers |
| `apps/web/lib/pulse/chat-api.ts` | 183 | `runChatPrompt()`, `runSourcePrompt()` pure async helpers |

### Files trimmed (3)

| File | Before | After |
|------|--------|-------|
| `apps/web/app/api/pulse/chat/route.ts` | 562L | 388L |
| `apps/web/components/pulse/pulse-workspace.tsx` | 1,093L | 342L |
| `apps/web/components/pulse/pulse-chat-pane.tsx` | 952L | 450L |

### Other modifications

- `apps/web/__tests__/pulse-chat-pane-layout.test.ts` — updated `computeMessageVirtualWindow` import → `@/components/pulse/chat-utils`
- `apps/web/__tests__/pulse-ui-smoke.test.ts` — updated `ChatMessage` import → `@/lib/pulse/workspace-persistence`
- `apps/web/__tests__/pulse-chat-route-streaming.test.ts` — updated imports for extracted modules
- `apps/web/.gitignore` — added `.pnpm-store`
- `crates/core/config/types.rs` — added `ask_strict_procedural: bool`, `ask_strict_config_schema: bool` (both default `true`)
- `crates/core/config/parse.rs` / `crates/core/config/parse/performance.rs` — parse new config fields
- `crates/vector/ops/commands/ask.rs` — gate logic for strict procedural + config schema checks
- `CHANGELOG.md` — added highlights entry for pulse module splits

---

## Commands Executed

```bash
# TypeScript check
cd apps/web && pnpm tsc --noEmit   # CLEAN

# Linter
pnpm biome check app/ components/ hooks/ lib/ __tests__/  # CLEAN

# Tests
pnpm test   # 110/110 PASSING

# Fix .pnpm-store staged accidentally
echo '.pnpm-store/' >> apps/web/.gitignore
git rm -r --cached apps/web/.pnpm-store/
git add apps/web/.gitignore

# Push
git push   # 3 commits to feat/crawl-download-pack
```

---

## Behavior Changes (Before/After)

| Aspect | Before | After |
|--------|--------|-------|
| `route.ts` size | 562L | 388L — POST handler only |
| `pulse-workspace.tsx` size | 1,093L | 342L — orchestrator shell |
| `pulse-chat-pane.tsx` size | 952L | 450L — component core only |
| `ChatMessage` import path | `@/components/pulse/pulse-workspace` | `@/lib/pulse/workspace-persistence` |
| `computeMessageVirtualWindow` import | `@/components/pulse/pulse-chat-pane` | `@/components/pulse/chat-utils` |
| Ask strict gates | Always enforced | Configurable via env vars `AXON_ASK_STRICT_PROCEDURAL` / `AXON_ASK_STRICT_CONFIG_SCHEMA` |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `pnpm tsc --noEmit` | 0 errors | 0 errors | ✅ |
| `pnpm biome check ...` | 0 violations | 0 violations | ✅ |
| `pnpm test` | 110 pass | 110 pass | ✅ |
| `wc -l` all target files | All ≤450L | All ≤450L | ✅ |
| `git push` | 3 commits pushed | 3 commits on remote | ✅ |

---

## Errors and Fixes

- **`.pnpm-store` accidentally staged**: `git add .` captured pnpm cache. Fixed by adding to `.gitignore` + `git rm --cached`.
- **`sed` too greedy**: `sed -i 's/\`TBD\`/\`7be0ba0\`/g' CHANGELOG.md` replaced a TBD inside an older description sentence. Fixed with targeted Edit tool call + fixup commit `3d5b68c`.
- **`use-pulse-chat.ts` at 377L**: Above 250L estimate but under 450L hard limit. Agent B correctly extracted API helpers to `lib/pulse/chat-api.ts` to keep under the limit.

---

## Decisions Not Taken

- **Re-export shims**: Rejected per sole-user policy — all import paths updated in-place.
- **Single large hooks file**: Rejected to comply with monolith policy.
- **Circular dep via workspace.tsx re-export**: Rejected — `ChatMessage` now has a single canonical location.

---

## Open Questions

- None — all tasks complete, all verification passed.

---

## Next Steps

- Monitor `use-pulse-chat.ts` (377L) — if it grows, candidate for further extraction.
- Consider extracting `applyOperations` logic from `pulse-workspace.tsx` if it grows.
- The `ask_strict_procedural` / `ask_strict_config_schema` fields should be documented in `crates/vector/CLAUDE.md`.
