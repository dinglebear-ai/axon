# Recent Sessions Landing Page
**Date:** 2026-02-27 07:34 EST
**Branch:** feat/crawl-download-pack
**Session type:** Feature implementation

---

## Session Overview

Implemented a "Recent Sessions" panel on the Axon web UI landing page. The panel displays the most recently modified Claude Code conversation files (`~/.claude/projects/**/*.jsonl`) below the omnibox when no results are showing. Clicking a session opens the full conversation in the Pulse workspace for interactive continuation.

Zero Rust changes — entirely contained within `apps/web`.

---

## Timeline

1. **Feature scoping** — clarified that "sessions" = Claude Code `.jsonl` logs from `~/.claude/projects`, not crawl/embed job history
2. **Codebase exploration** — two parallel agents mapped the UI (landing page, omnibox, Pulse workspace, WS protocol) and the session data sources (`recentRuns`, localStorage, DB job tables)
3. **Architecture design** — selected Next.js API routes + client hook over Rust WS handler (no redeployment needed, established pattern)
4. **Implementation** — 7 files created/modified
5. **Code review** — 4 real bugs found and fixed (race condition, silent errors, empty session guard, project name truncation)
6. **Build verification** — TypeScript clean, `pnpm build` passes

---

## Key Findings

- `recentRuns` in `use-ws-messages.ts:27-35` is in-memory only (max 20, wiped on reload) — not the right source for persistent session history
- `axon sessions` command (`crates/ingest/sessions/claude.rs`) reads the same `.jsonl` files but embeds them into Qdrant — unrelated to this UI feature
- `submitWorkspacePrompt()` in `use-ws-messages.ts:693` is the existing entry point for loading content into Pulse; used as-is
- `isPulseWorkspaceActive` at `page.tsx:104` gates the workspace flip; `!hasResults` is the correct condition for showing the sessions panel
- `withFileTypes: true` on `fs.readdir` without `encoding: 'utf-8'` produces `Dirent<NonSharedBuffer>` in TS strict mode — fixed with plain `readdir` + `fs.stat`
- The `[id]` route must scan with a higher limit (200) than the list route (20) to avoid a 404 race when a session is written between list and detail calls

---

## Technical Decisions

- **Next.js API routes over Rust WS handler**: No `cargo build`, no worker redeployment, established pattern (`omnibox/files/route.ts`), trivial filesystem access
- **SHA-256 path hash as session ID**: Stable across rescans, never exposes absolute paths to the browser, collision-resistant for local use
- **Cap at 50 messages in handoff prompt**: Prevents enormous prompts for long sessions; last 50 captures the most recent context
- **Last 2 path segments as project name**: Single last segment was too ambiguous (`app`, `src`); 2 segments provides meaningful context (`axon-rust`, `my-project`)
- **`loadSession` returns `boolean`**: Enables `SessionCard` to surface errors inline without a toast system or global error state

---

## Files Modified

| File | Status | Purpose |
|------|--------|---------|
| `apps/web/lib/sessions/claude-jsonl-parser.ts` | Created | Pure TS port of Rust JSONL parser — handles string/array content blocks |
| `apps/web/lib/sessions/session-scanner.ts` | Created | Scans `~/.claude/projects`, hashes paths to stable IDs, sorts by mtime |
| `apps/web/app/api/sessions/list/route.ts` | Created | GET handler returning top-20 session metadata |
| `apps/web/app/api/sessions/[id]/route.ts` | Created | GET handler reading session content by ID (scans 200 to avoid race) |
| `apps/web/hooks/use-recent-sessions.ts` | Created | Fetches list on mount, `loadSession()` → `submitWorkspacePrompt()` |
| `apps/web/components/recent-sessions.tsx` | Created | Session card list with loading/error states |
| `apps/web/app/page.tsx` | Modified | Added `{!hasResults && <RecentSessions />}` import + render |

---

## Commands Executed

```bash
# TypeScript type check (passed clean)
pnpm tsc --noEmit

# Production build (passed — both new routes registered)
pnpm build
```

Build output confirmed:
```
├ ƒ /api/sessions/[id]
├ ƒ /api/sessions/list
```

---

## Behavior Changes (Before / After)

**Before:** Landing page showed only the omnibox + NeuralCanvas background; no history visible.

**After:** On first visit (when `!hasResults`), a "Recent Sessions" panel appears below the omnibox, showing up to 20 most-recently-modified Claude Code conversation files across all projects. Each card shows project name, filename, relative age, and file size. Clicking opens the conversation in the Pulse workspace.

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `pnpm tsc --noEmit` | No errors | No output | ✅ PASS |
| `pnpm build` | Both routes registered | `/api/sessions/list` + `/api/sessions/[id]` in route table | ✅ PASS |
| TS type fix (Dirent) | Clean types | Resolved by using plain `readdir` + `stat` | ✅ PASS |
| `onLoad` prop type | `boolean` return propagates | `SessionCard` prop updated to `Promise<boolean>` | ✅ PASS |

---

## Source IDs + Collections Touched

_(Axon embed/retrieve to follow below)_

---

## Risks and Rollback

- **Risk:** Session files can be large (100+ turns); capped at 50 messages to limit prompt size, but large individual messages (tool use, code blocks) may still produce very long prompts.
- **Risk:** `cleanProjectName` still relies on heuristics — unusual directory naming may produce confusing labels.
- **Rollback:** Remove `{!hasResults && <RecentSessions />}` from `page.tsx` and delete the 6 new files. No DB migrations, no Rust changes.

---

## Decisions Not Taken

| Alternative | Reason Rejected |
|------------|----------------|
| Rust WebSocket handler (`sessions.list` / `sessions.read` WS messages) | Requires `cargo build`, worker redeployment, whitelist updates — disproportionate for read-only listing |
| localStorage persistence of `recentRuns` | Only captures current-page-load runs; doesn't surface historical sessions from other projects |
| Pull history from PostgreSQL job tables | Job tables track crawl/embed/extract jobs, not Claude Code conversations |
| Show only current project sessions | User explicitly asked for cross-project "most recent" view |
| Single last segment as project name | Too ambiguous — `app`, `src`, `rust` with no context |

---

## Open Questions

- Should sessions panel show a preview of the first user message (truncated)? Currently shows only filename + age.
- Should the session panel auto-refresh (polling) to pick up new sessions created during the page session?
- What happens for very new `.jsonl` files that contain only tool-use blocks (no text)? Currently returns `false` from `loadSession` with "Failed to load" displayed — is this the right UX or should it say "Empty session"?

---

## Next Steps

- Consider adding first-message preview to each session card (would require reading file content at list time — trade-off: more I/O)
- Add unit tests for `parseClaudeJsonl` (port from `crates/ingest/sessions/claude.rs:234-349`)
- Add unit tests for `cleanProjectName` (port from same file)
- Consider pagination if users have >20 sessions they want to access
