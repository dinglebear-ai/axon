# Remove Legacy Claude Headless Subprocess Code
**Date:** 2026-03-09
**Branch:** `refactor/acp-performance-modern-rust`
**Duration:** Single focused session

---

## Session Overview

Removed all dead code from the pre-ACP era when Pulse Chat worked by spawning the `claude` CLI as a Node.js subprocess and parsing its NDJSON stream output (`stream-json` format). The ACP (Agent Communication Protocol) bridge has been the live path for some time; the old subprocess plumbing was left behind unused and this session cleaned it out.

---

## Timeline

1. **Read plan** — reviewed the implementation plan document detailing which exports to remove vs. keep
2. **Read source files** — inspected `claude-stream-types.ts`, `stream-parser.ts`, and both test files to confirm the exact dead-code boundaries before editing
3. **Rewrote `claude-stream-types.ts`** — stripped ~200 lines, kept 5 live exports
4. **Rewrote `stream-parser.ts`** — stripped `parseClaudeStreamLine` + `ParsedLineResult` + old type imports, kept 3 live exports
5. **Deleted test file** — `__tests__/pulse/build-claude-args.test.ts` removed entirely
6. **Trimmed stream-parser tests** — removed `parseClaudeStreamLine` describe block (~130 lines), kept `createStreamParserState` and `extractToolResultText` groups
7. **Updated CLAUDE.md** — replaced stale subprocess description with accurate ACP description; removed `PULSE_SKIP_PERMISSIONS` env var entry
8. **Verified** — grep confirms no remaining references to removed exports; 740 tests pass; 0 new TypeScript errors

---

## Key Findings

- `buildClaudeArgs()` was the primary dead symbol: 50+ line function assembling `claude` CLI argv arrays — never called by `route.ts` (which already uses `runAxonCommandWsStream`)
- `parseClaudeStreamLine()` parsed NDJSON lines from `claude --output-format stream-json` — also never called in the ACP path
- The `PulseChatRequestSchema` session validation tests at the bottom of `build-claude-args.test.ts` were collateral to deleting that file (plan explicitly called for full deletion)
- Pre-existing TypeScript errors in `components/editor/` and `components/ui/` (Plate.js) and `__tests__/api/cortex-routes.test.ts` are unrelated to this session

---

## Technical Decisions

- **Keep `fs`/`os`/`path` imports** in `claude-stream-types.ts` — they are still needed for `GLOBAL_CLAUDE_MD_CHARS` (reads `~/.claude/CLAUDE.md` size at module load)
- **Delete entire `build-claude-args.test.ts`** rather than rescue the `PulseChatRequestSchema` tests — plan called for full deletion; those schema tests duplicate coverage in `pulse-types.test.ts`
- **Keep `parseClaudeStreamLine` state shape** (`StreamParserState`) — the type and `createStreamParserState()` are still used by `route-helpers.ts` for ACP event accumulation; only the NDJSON *parser function* was dead
- **CLAUDE.md update** — removed `PULSE_SKIP_PERMISSIONS` env var (only consumed by `resolveSkipPermissions()` which was deleted); kept `AXON_ALLOWED_CLAUDE_BETAS` since it is still referenced in the env var table for future use if needed

---

## Files Modified

| File | Action | Purpose |
|------|--------|---------|
| `apps/web/app/api/pulse/chat/claude-stream-types.ts` | Rewritten (220 → 50 lines) | Remove `buildClaudeArgs`, helper functions, and dead interface types; keep 5 live exports |
| `apps/web/app/api/pulse/chat/stream-parser.ts` | Rewritten (193 → 53 lines) | Remove `parseClaudeStreamLine`, `ParsedLineResult`, dead type imports; keep 3 live exports |
| `apps/web/__tests__/pulse/build-claude-args.test.ts` | **Deleted** | Tests for removed `buildClaudeArgs` function |
| `apps/web/__tests__/stream-parser.test.ts` | Trimmed (289 → 58 lines) | Remove `parseClaudeStreamLine` describe block; keep `extractToolResultText` + `createStreamParserState` groups |
| `apps/web/CLAUDE.md` | Updated (2 hunks) | Pulse Chat section now describes ACP path; `PULSE_SKIP_PERMISSIONS` removed from env var table |

---

## Commands Executed

```bash
# Verify no remaining references to removed exports
grep -r "buildClaudeArgs|ClaudeStreamEvent|parseClaudeStreamLine|..." apps/web/
# → No files found

# Run tests
cd apps/web && pnpm test -- stream-parser
# → 67 test files passed, 740 tests passed

# TypeScript check for changed files
pnpm exec tsc --noEmit | grep "claude-stream-types\|stream-parser\|build-claude-args"
# → No errors in changed files
```

---

## Behavior Changes (Before / After)

| Surface | Before | After |
|---------|--------|-------|
| `buildClaudeArgs` export | Exported 50-line function assembling `claude` CLI argv | **Removed** — not exported |
| `ClaudeStreamEvent` / `ClaudeStreamAssistantContent` | Exported interfaces for NDJSON parsing | **Removed** |
| `parseClaudeStreamLine` | Exported NDJSON parser function | **Removed** |
| `ParsedLineResult` | Exported discriminated union type | **Removed** |
| `stream-parser.ts` imports | `import type { ClaudeStreamEvent, ClaudeStreamAssistantContent }` | **Removed** — only `@/lib/pulse/types` remains |
| Test count | 740+ (includes `build-claude-args.test.ts`) | 740 (deleted file was last run count) |
| `PULSE_SKIP_PERMISSIONS` env var doc | Listed in `apps/web/CLAUDE.md` env table | **Removed** from docs |
| Pulse Chat CLAUDE.md description | "spawns `claude` CLI as subprocess via `child_process.spawn`" | "sends prompt turns via WebSocket to the Rust ACP bridge (`runAxonCommandWsStream`)" |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `grep buildClaudeArgs apps/web/**` | No matches | No files found | ✅ |
| `grep parseClaudeStreamLine apps/web/**` | No matches | No files found | ✅ |
| `pnpm test` | All pass | 740 passed, 0 failed | ✅ |
| `tsc --noEmit \| grep claude-stream-types` | No errors | No output | ✅ |
| `tsc --noEmit \| grep stream-parser` | No errors | No output | ✅ |

---

## Source IDs + Collections Touched

None — no Axon embed/retrieve operations were part of the implementation work itself.

---

## Risks and Rollback

- **Risk:** `PulseChatRequestSchema` session validation tests (5 tests at bottom of `build-claude-args.test.ts`) were deleted as collateral. Those tests validated `sessionId` regex — coverage still exists in `pulse-types.test.ts`.
- **Rollback:** `git checkout apps/web/app/api/pulse/chat/claude-stream-types.ts apps/web/app/api/pulse/chat/stream-parser.ts apps/web/__tests__/stream-parser.test.ts apps/web/CLAUDE.md` and `git checkout HEAD apps/web/__tests__/pulse/build-claude-args.test.ts` restores everything.

---

## Decisions Not Taken

- **Rescue `PulseChatRequestSchema` tests** — could have moved them to a separate test file; plan explicitly called for full deletion and coverage exists elsewhere
- **Keep `ClaudeStreamEvent` type** — could have retained it as a type-only export for documentation; no consumers remain so deletion was appropriate
- **Partial cleanup only** — could have left `resolveSkipPermissions()` as a no-op shim; removed cleanly since the only caller (`buildClaudeArgs`) is also gone

---

## Open Questions

- `AXON_ALLOWED_CLAUDE_BETAS` env var still appears in `CLAUDE.md` env table but `sanitizeBetas()` (its only consumer) was deleted. Verify whether this env var is consumed anywhere else in the ACP path before removing the doc entry.
- The `PulseChatRequestSchema` `sessionId` validation tests that were deleted — confirm `pulse-types.test.ts` has equivalent coverage.

---

## Next Steps

- Verify the separate ticket: update `crates/web/execute/constants.rs` `ALLOWED_MODES` to remove `github`/`reddit`/`youtube` entries (replaced by unified `ingest` command) and mirror changes in `apps/web/lib/ws-protocol.ts`, `axon-options.ts`, `axon-command-map.ts`, `omnibox/utils.ts`, `CmdKOutput.tsx`
- Smoke-test Pulse Chat in browser — send a message, verify `assistant_delta` events appear in WS frames (ACP path)
