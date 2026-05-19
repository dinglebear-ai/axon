# Multi-Agent Sessions Sidebar — Claude + Codex + Gemini

**Date**: 2026-03-09
**Branch**: `refactor/acp-performance-modern-rust`
**Commit**: `2cf2a067` — `feat(web): multi-agent sessions sidebar — Claude + Codex + Gemini (v0.13.0)`

---

## Session Overview

Implemented multi-agent session scanning in the `/reboot` sidebar. Previously, only Claude sessions (~/.claude/projects/**/*.jsonl) were shown. After this session, the sidebar scans Claude, Codex, and Gemini session stores, shows sessions from all three with colored agent badge pills (CX=green, G=blue), and auto-switches the agent selector when a non-Claude session is selected.

---

## Timeline

1. **Initial implementation**: Created `session-utils.ts`, `codex-scanner.ts`, `gemini-scanner.ts`, `codex-jsonl-parser.ts`, `gemini-json-parser.ts`
2. **Circular dependency discovered**: Codex/Gemini scanners imported from `session-scanner.ts`, which imported them back — Turbopack silently broke initialization, scanners returned empty arrays
3. **Fix**: Extracted shared helpers into `session-utils.ts`; sub-scanners import only from there
4. **Turbopack negative module cache**: Server started before `gemini-scanner.ts` existed — Turbopack cached "Module not found" and continued serving stale compiled output even after files were created
5. **Workaround**: Moved multi-agent merge logic into `list/route.ts` which directly imports scanners (bypasses stale session-scanner module cache)
6. **Per-agent guarantee bug**: After global dedup+sort, 20 most-recent sessions were all Claude (from today). Codex/Gemini sessions from yesterday fell past position 20
7. **Fix**: Reserve minimum 3 slots per agent before filling remaining slots with global recency order
8. **Tests fixed**: Added `vi.mock` for codex-scanner and gemini-scanner in sessions-routes.test.ts
9. **Verification**: Chrome DevTools confirmed `{'claude': 14, 'codex': 3, 'gemini': 3}` in API response
10. **Commit and push**: Version bumped 0.12.0 → 0.13.0

---

## Key Findings

- **Turbopack negative module cache**: If a module doesn't exist when the dev server starts, Turbopack caches `"Module not found"` and does NOT invalidate this even when the file is subsequently created. Confirmed via `.next/dev/logs/next-development.log` showing repeated "Can't resolve './gemini-scanner'" errors with uptime `00:02:34` while files were created at 15:34 (11 min after server start).

- **Circular dependency with dynamic scanner loading**: `session-scanner.ts` importing `codex-scanner.ts` which imports `session-scanner.ts` caused Turbopack/webpack module initialization to break silently — scanners returned `[]` arrays.

- **Dynamic imports bypass stale cache**: Using `await import('./codex-scanner')` inside `scanSessions` function body (rather than static `import` at module top) causes Turbopack to re-resolve the module on each call, bypassing the cached negative resolution.

- **Per-agent guarantee requires special slicing logic**: Simply doing `allSorted.slice(0, limit)` after sorting 90 sessions by mtime will always drop minority agents when the majority agent (Claude) has 20+ sessions from today. Must first reserve `minPerAgent` (3) slots per agent, then fill remaining `limit - guaranteed.length` slots from most-recent.

- **Codex session format**: `~/.codex/sessions/{year}/{month}/{day}/*.jsonl` — first line is `session_meta` with `payload.cwd` for project, subsequent lines include `event_msg` with `payload.type:'user_message'`

- **Gemini session format**: `~/.gemini/tmp/{SHA256(path)}/chats/session-*.json` — JSON object with `{ sessionId, projectHash, lastUpdated, messages[] }`

---

## Technical Decisions

- **`session-utils.ts` extraction**: Rather than duplicating `SKIP_PATTERNS`, `cleanProjectName`, `sessionId`, `mapWithConcurrency` in each scanner, extracted to a shared utility file. This also broke the circular dependency.

- **Direct scanner imports in `list/route.ts`**: Rather than routing through `session-scanner.ts`'s `scanSessions()` for all three agents, the route directly calls `scanCodexSessions()` and `scanGeminiSessions()`. This was necessary because of Turbopack's negative module cache, but has the side-effect of duplicating the dedup/guarantee logic between `session-scanner.ts` and `list/route.ts`.

- **Dynamic imports in `session-scanner.ts`**: Used `await import('./codex-scanner')` and `await import('./gemini-scanner')` inside the function body to break the circular dependency at runtime. The `list/route.ts` workaround handles the Turbopack cache issue separately.

- **Badge design**: Only non-Claude agents get a badge (Claude is the default/expected). Badge is small colored text (`text-xs font-mono`), right-aligned after the session title. Colors: CX (Codex) = `#7dda7d` (green), G (Gemini) = `#7db8f7` (blue).

- **Agent auto-switch**: When a Codex or Gemini session is selected, `handleSelectSession` checks `session.agent` and calls `setPulseAgent()`. Uses `rawSessions` (the full unprocessed list from the API) for lookup, not the displayed subset.

---

## Files Modified

| File | Status | Purpose |
|------|--------|---------|
| `apps/web/lib/sessions/session-utils.ts` | **Created** | Shared helpers: `SKIP_PATTERNS`, `cleanProjectName`, `sessionId`, `mapWithConcurrency` |
| `apps/web/lib/sessions/codex-scanner.ts` | **Created** | Scan `~/.codex/sessions/{year}/{month}/{day}/*.jsonl` |
| `apps/web/lib/sessions/gemini-scanner.ts` | **Created** | Scan `~/.gemini/tmp/{hash}/chats/session-*.json` |
| `apps/web/lib/sessions/codex-jsonl-parser.ts` | **Created** | Parse Codex JSONL for history display |
| `apps/web/lib/sessions/gemini-json-parser.ts` | **Created** | Parse Gemini JSON for history display |
| `apps/web/lib/sessions/session-scanner.ts` | **Modified** | Added `AgentKind`, `agent` field on `SessionFile`; dynamic imports for codex/gemini scanners; per-agent guarantee logic |
| `apps/web/app/api/sessions/list/route.ts` | **Modified** | Direct scanner imports workaround; full per-agent guarantee logic (LIMIT=20, PER_AGENT_LIMIT=30, MIN_PER_AGENT=3) |
| `apps/web/app/api/sessions/[id]/route.ts` | **Modified** | Branch on `session.agent` to select parser |
| `apps/web/hooks/use-recent-sessions.ts` | **Modified** | Added `agent: AgentKind` to `SessionSummary` interface |
| `apps/web/components/reboot/axon-sidebar.tsx` | **Modified** | `AGENT_BADGE` map + badge pill rendered for non-claude sessions |
| `apps/web/components/reboot/axon-shell.tsx` | **Modified** | `handleSelectSession` auto-switches agent on non-Claude session |
| `apps/web/__tests__/api/sessions-routes.test.ts` | **Modified** | Added `vi.mock` for codex/gemini scanners; updated call assertion to `(30, 30)` |
| `apps/web/__tests__/sessions/codex-parser.test.ts` | **Created** | Tests for Codex JSONL parser |

---

## Commands Executed

```bash
# Verification via debug endpoint
GET /api/debug/scanners → {"codex":811,"gemini":139}

# Test run (before fix)
pnpm test → some failures (missing vi.mock for codex/gemini scanners)

# Test run (after fix)
pnpm test → 761 tests passing

# Final state of API
GET /api/sessions/list → 20 sessions: {'claude': 14, 'codex': 3, 'gemini': 3}
```

---

## Behavior Changes (Before/After)

| Aspect | Before | After |
|--------|--------|-------|
| Sessions shown | Claude only (`.claude/projects/**/*.jsonl`) | Claude + Codex + Gemini |
| Agent badge | None | CX (green) for Codex, G (blue) for Gemini |
| Agent switch on session click | No (always stayed on Claude) | Auto-switches to correct agent |
| History display | Claude JSONL parser only | Routes to correct parser per agent |
| Per-agent representation | Not guaranteed | Minimum 3 sessions per agent type |
| Version | 0.12.0 | 0.13.0 |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `GET /api/sessions/list` | 3 agent types present | `{'claude': 14, 'codex': 3, 'gemini': 3}` | ✅ PASS |
| `pnpm test` | All tests pass | 761 passing, 0 failing | ✅ PASS |
| `pnpm lint` | Biome clean | Clean | ✅ PASS |
| Click Gemini session | Agent switches to Gemini | "Gemini is ready" shown in composer | ✅ PASS |
| Click Codex session | History loads | Messages displayed | ✅ PASS |
| CX badge visible | Green badge on Codex sessions | ✅ visible | ✅ PASS |

---

## Source IDs + Collections Touched

None — no Axon embed/retrieve operations in this session (code changes only).

---

## Risks and Rollback

- **Rollback**: `git revert 2cf2a067` — reverts all 5 new files and 8 modified files
- **Risk — Turbopack cache**: If dev server was started before these files existed in a downstream environment, the Turbopack negative module cache may cause issues. Workaround: restart the dev server (`pnpm dev`).
- **Risk — Circular dep**: The `session-utils.ts` extraction is required. If any scanner is modified to re-import from `session-scanner.ts`, the circular dep will silently break scanner output.
- **Risk — Duplicate logic**: Per-agent guarantee logic exists in both `session-scanner.ts` (for `scanSessions()`) and `list/route.ts`. These need to be kept in sync if either changes.

---

## Decisions Not Taken

- **Keeping all logic in `session-scanner.ts`**: Original plan had the API route calling only `scanSessions()`. Rejected because Turbopack's stale negative module cache made the codex/gemini scanners invisible through that path.
- **Server restart as fix**: Restarting the dev server would have cleared Turbopack's cache. Rejected because the route-level direct import is a more reliable, self-contained fix that doesn't depend on server restart order.
- **Using `require()` / CommonJS dynamic imports**: Rejected — project is ESM-only.
- **Showing all 3 badges (including Claude "C")**: Rejected — Claude is the default and expected; badge is only informative for non-default agents.

---

## Open Questions

- The `list/route.ts` now duplicates the per-agent guarantee logic from `session-scanner.ts`. This is intentional (workaround), but ideally these should be unified once the Turbopack cache issue is understood/resolved.
- Gemini `projects.json` reverse map may not contain all project hashes (if the user never used Gemini on a given project path). Fallback to first 8 chars of hash is acceptable but project name will be opaque.
- Codex scanner assumes `~/.codex/sessions/{year}/{month}/{day}/` depth structure. If Codex changes this layout in future versions, the scanner will need updating.

---

## Next Steps

- Consider adding Gemini/Codex scanner tests to `__tests__/sessions/` (gemini-scanner.test.ts, codex-scanner.test.ts) per the original plan
- Unify per-agent guarantee logic between `session-scanner.ts` and `list/route.ts` once Turbopack cache behavior is better understood
- Consider showing the Claude "C" badge when mixed with other agents (currently hidden) for clarity
