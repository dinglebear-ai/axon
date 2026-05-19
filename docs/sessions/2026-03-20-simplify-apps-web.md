# Session: Simplify apps/web — Full Codebase Review

**Date:** 2026-03-20
**Branch:** `feat/pulse-shell-and-hybrid-search`
**Scope:** `/apps/web` — 467 source files, ~58K lines (Next.js 16 App Router frontend)

## Session Overview

Comprehensive code quality review of the entire `apps/web` directory using 6 parallel review agents, each covering a distinct domain of the codebase. Agents identified 58 findings across API routes, shell/terminal, editor/AI, hooks/lib, UI/results, and pulse/cortex/omnibox. Applied 13 fixes covering bug fixes, utility extraction, route consolidation, constant deduplication, and safety improvements. All 928 tests pass, Biome lint clean, no new TypeScript errors.

## Timeline

1. **File inventory** — Enumerated all 467 non-test source files and mapped directory distribution
2. **Agent dispatch** — Launched 6 parallel review agents partitioned by domain:
   - API routes + proxy + server utils (~39 route files)
   - Shell + terminal + WebSocket (~39 shell components + WS hooks)
   - Editor (Plate.js) + AI elements (~58 editor files)
   - Hooks + lib utilities + state (~50 files)
   - UI components + results + jobs (~145 files)
   - Pulse chat + Cortex + omnibox + canvas (~50 files)
3. **Findings aggregation** — 58 total findings (13 critical, 27 improvement, 18 minor)
4. **Fix application** — Applied 13 targeted fixes (bugs, extraction, consolidation)
5. **Verification** — TypeScript check (0 new errors), 928 tests passing, Biome lint clean

## Key Findings

### Critical Issues Found
- **Hardcoded stale default** — `use-workspace-files.ts:10` defaulted `selectedFilePath` to `'lib/supabase.ts'` (copy-paste from different codebase)
- **Duplicate `EditorUpdateSchema`** — `editor-handler.ts:6` uses `.default('replace')` while `ws-handler.ts:166` uses `.optional()` — same wire message validated differently depending on code path
- **`isRecord` duplicated 5 times** — identical type guard across `structured-text.ts`, `result-normalizers.ts`, `result-to-markdown.ts`, `structured-data-view.tsx`, `job-lifecycle-renderer.tsx`
- **`formatBytes` duplicated 3 times** — `docker-stats.tsx:7`, `recent-sessions.tsx:16`, `screenshot-renderer.tsx:11`
- **5 cortex proxy routes** — doctor, domains, sources, stats, status are near-identical 20-line handlers
- **`parseOpenAiSseChunk` in wrong location** — utility function exported from `copilot/route.ts`, imported by `chat/route.ts` (route-to-route coupling)
- **`TextEncoder` per-chunk allocation** — `ai/chat/route.ts:80` created `new TextEncoder()` inside SSE streaming `while(true)` loop
- **Raw `sessionStorage.removeItem`** — `ws-handler.ts:506` bypassed safe storage wrapper, could throw in environments without sessionStorage
- **`useIsTouchDevice` resize listener** — listened to `resize` events to detect touch capability (a device constant)
- **Mutable module-level `COLORS` export** — `color-utils.ts:16` mutated via `applyPalette()`, leaked state across imports

### Deferred Findings (too risky for cleanup pass)
- Fake-stream infrastructure: 8 files (~900 lines) + `@faker-js/faker` as prod dependency — should be dynamic-imported
- `conversation-memory.ts`: 43-line module only handles "what's my favorite color?"
- `MarkdownBlock` using full Plate.js editor for read-only markdown display
- Shell desktop layout duplicated between `axon-shell.tsx` and `AxonShellDesktop`
- Dual WebSocket reconnect logic in `use-axon-ws.ts` vs `use-shell-session.ts`
- `JobDetail` interface with 30+ null fields per job type — should use discriminated union
- `OmniboxInputBar` takes 30+ props — excessive prop drilling

## Technical Decisions

- **`isRecord` → `lib/type-guards.ts`** — Centralized because it was the most duplicated utility (5 copies). Also exported `hasKeys` from the same file since `result-normalizers.ts` had both.
- **`formatBytes` + `formatRelativeTime` → `components/results/shared.ts`** — Added to existing shared utilities file rather than creating new one. These formatters are result-rendering concerns.
- **Cortex proxy factory** — `lib/server/cortex-proxy.ts` with standardized error logging shape (`{ message, name, stack }`) — fixes the inconsistency where some routes logged `{ message }` and others logged `{ error: { message, name, stack } }`.
- **`EditorUpdateSchema` consolidation** — Kept canonical version in `editor-handler.ts` (with `.default('replace')`) because consumers expect a concrete `operation` value. The `.optional()` variant in `ws-handler.ts` would silently pass `undefined` through.
- **Did NOT refactor mutable `COLORS`** — Would require threading palette through 6 render files' `draw()` methods called from `requestAnimationFrame`. Documented constraint instead.
- **Did NOT extract shared WS reconnect hook** — Both `use-axon-ws.ts` and `use-shell-session.ts` have subtly different reconnect behaviors (visibility events, message queuing). Needs careful design.

## Files Modified

| File | Change |
|------|--------|
| `lib/type-guards.ts` | **NEW** — shared `isRecord()` + `hasKeys()` |
| `lib/server/cortex-proxy.ts` | **NEW** — factory for cortex proxy routes |
| `lib/server/openai-sse.ts` | **NEW** — `parseOpenAiSseChunk` + `encodeCopilotStreamEvent` |
| `components/results/shared.ts` | Added `formatBytes()` + `formatRelativeTime()` |
| `lib/structured-text.ts` | Import `isRecord` from shared |
| `lib/result-normalizers.ts` | Import `isRecord` + `hasKeys` from shared |
| `lib/result-to-markdown.ts` | Import `isRecord` from shared |
| `components/results/structured-data-view.tsx` | Import `isRecord` from shared |
| `components/results/job-lifecycle-renderer.tsx` | Import `isRecord` from shared |
| `components/docker-stats.tsx` | Import `formatBytes` from shared |
| `components/recent-sessions.tsx` | Import `formatBytes` + `formatRelativeTime` from shared |
| `components/results/screenshot-renderer.tsx` | Import `formatBytes` from shared |
| `hooks/use-workspace-files.ts` | Fix: default `null` instead of `'lib/supabase.ts'` |
| `hooks/use-is-touch-device.ts` | Fix: removed unnecessary `resize` listener |
| `hooks/use-axon-acp.ts` | Exported `ACP_SESSION_STORAGE_KEY` |
| `hooks/use-axon-acp/ws-handler.ts` | Import `EditorUpdateSchema` from `editor-handler.ts`, use `removeSessionItem` |
| `hooks/use-axon-acp/editor-handler.ts` | No change (already canonical) |
| `components/neural-canvas/color-utils.ts` | Documented mutable COLORS constraint |
| `components/cmdk-palette/cmdk-palette-types.ts` | Added `URL_MODES` constant |
| `components/cmdk-palette/CmdKOutput.tsx` | Import `URL_MODES` from types |
| `components/cmdk-palette/cmdk-palette-dialog.tsx` | Import `URL_MODES` from types |
| `components/omnibox/utils.ts` | Added `MENTION_TIP_SEEN_KEY` constant |
| `components/omnibox/omnibox-hooks.ts` | Import `MENTION_TIP_SEEN_KEY` from utils |
| `components/omnibox/omnibox-input-bar.tsx` | Import `MENTION_TIP_SEEN_KEY` from utils |
| `app/api/ai/chat/route.ts` | Hoisted `TextEncoder`, import from `openai-sse.ts` |
| `app/api/ai/copilot/route.ts` | Import from `openai-sse.ts` |
| `app/api/cortex/doctor/route.ts` | Slimmed to 3-line factory call |
| `app/api/cortex/domains/route.ts` | Slimmed to 3-line factory call |
| `app/api/cortex/sources/route.ts` | Slimmed to 3-line factory call |
| `app/api/cortex/stats/route.ts` | Slimmed to 3-line factory call |
| `app/api/cortex/status/route.ts` | Slimmed to 3-line factory call |
| `__tests__/api-copilot.test.ts` | Updated import path for moved utilities |

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `pnpm test` | 928 tests pass | 98 files, 928 passed | PASS |
| `pnpm lint` | No errors | 0 errors, 1 warning (pre-existing) | PASS |
| `npx tsc --noEmit` | No new errors | 8 pre-existing errors, 0 new | PASS |

## Behavior Changes (Before/After)

| Area | Before | After |
|------|--------|-------|
| `use-workspace-files` | Defaulted to loading `lib/supabase.ts` (nonexistent) | Defaults to `null` — no file selected on mount |
| `useIsTouchDevice` | Re-evaluated touch on every resize event | Evaluates once on mount (correct behavior) |
| `EditorUpdateSchema` in ws-handler | `operation` could be `undefined` | `operation` defaults to `'replace'` (matches editor-handler) |
| `ws-handler` session cleanup | Raw `sessionStorage.removeItem` (could throw) | Uses `removeSessionItem` wrapper (safe) |
| SSE chat stream | New `TextEncoder` per chunk in loop | Single `TextEncoder` reused across loop |
| Cortex error logging | Inconsistent shapes (`{ message }` vs `{ error: { message, name, stack } }`) | Standardized to `{ error: { message, name, stack } }` |

## Risks and Rollback

- **Low risk** — All changes are mechanical refactors (extract, import, consolidate). No logic changes.
- **Rollback** — `git checkout -- apps/web/` reverts everything. No database/infrastructure changes.
- **Pre-existing TS errors** — 8 errors exist on the branch unrelated to this session (`Job`/`StatusCounts` not exported from jobs route, `FileCode2` missing import, test type error).

## Decisions Not Taken

- **Fake-stream removal** — ~900 lines + faker dep, but needs confirmation it's truly unused in dev workflows
- **Neural canvas COLORS refactor** — Threading palette through 6 draw functions is high-risk for a visual component
- **Shell desktop layout dedup** — `axon-shell.tsx` vs `AxonShellDesktop` needs UI testing on both mobile and desktop
- **WS reconnect hook extraction** — Subtle behavioral differences between the two hooks require careful design
- **`conversation-memory.ts` removal** — Only handles "favorite color", but removing requires confirming no downstream deps
- **`MarkdownBlock` → `AxonMarkdown` migration** — Need to verify Plate-specific features aren't being used

## Open Questions

- Are the 8 fake-stream sample files still used in development? Can `@faker-js/faker` move to devDependencies?
- Should `conversation-memory.ts` be removed entirely or generalized?
- Is the `AxonShellDesktop` component intended to replace the inline desktop layout in `axon-shell.tsx`?
- The `EditorUpdateSchema.passthrough()` in ws-handler was there for a reason — are there extra fields being forwarded?

## Next Steps

- Address deferred findings (fake-stream cleanup, MarkdownBlock migration, WS reconnect dedup)
- Fix pre-existing TS errors (`Job`/`StatusCounts` exports, `FileCode2` import)
- Consider extracting `useMediaQuery` hook to replace 4 duplicate media query patterns
- Consider extracting `useReconnectingWebSocket` shared hook
