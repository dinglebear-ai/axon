# Remove Starred, Recents, Creator, Skills, Tasks
**Date:** 2026-03-02
**Branch:** feat/sidebar

---

## Session Overview

Removed five dead/half-baked features from the Axon web UI: Starred, Recents, Creator (skill/agent/command file manager), Skills (sidebar panel), and Tasks (scheduled job scheduler). Deleted 11 source files, cleaned 3 component files, trimmed the type system, and fixed a pre-existing unused import warning. The sidebar now has exactly two section tabs (Extracted, Workspace) and four page links (Jobs, Logs, Terminal, Cortex).

---

## Timeline

1. Read `pulse-sidebar.tsx`, `workspace-section.tsx`, `types.ts` to understand current state
2. Created task list and executed plan in order
3. `rm -f` of all 11 target files (initial attempt via relative path appeared to succeed but files persisted — root cause unclear, possibly a transient file-sync race with `axon-web` Docker container)
4. Re-deleted with absolute paths — confirmed gone immediately
5. Rewrote 3 modified files via `cat >` heredoc (Edit tool changes also weren't persisting on first pass)
6. Verified via `git status` (11 `D`, 4 `M`) and `pnpm lint` (393 files, 16 warnings)
7. Fixed pre-existing `WorkspaceContextState` unused import in `handlers.ts`

---

## Key Findings

- **Edit tool writes not persisting**: Multiple `Edit` tool calls returned success but file content was unchanged when re-read. Workaround: `cat > file << 'ENDOFFILE'` heredoc writes worked reliably.
- **`rm` result ambiguity**: First `rm -v` (relative path, chained with `cd`) showed "removed" for all 11 files, but subsequent `ls` showed files still present. Second attempt with absolute paths and immediate `ls` confirmed deletion. Exact cause unknown — possibly a Docker bind-mount sync race.
- **17 → 16 warnings**: `WorkspaceContextState` unused import in `hooks/ws-messages/handlers.ts:21` was the only fixable warning. Removed with `sed -i`.
- **Remaining 16 warnings**: All in Plate.js editor glue (`noExplicitAny` ×5, `noStaticElementInteractions` ×4, `useMediaCaption` ×2, `noBannedTypes`, `useExhaustiveDependencies`, `noUnusedVariables`, `noImgElement`). Pre-existing, not touched.

---

## Technical Decisions

- **Deleted entire files** rather than stubbing — these features had no callers outside their own files after the sidebar cleanup; no reason to keep skeletons.
- **Kept empty dirs** (`app/creator/`, `app/tasks/`, `components/creator/`, `components/tasks/`) — git will drop them on commit since they're now empty. Did not `rmdir` to avoid confusion.
- **`types.ts` rewritten from scratch** — cleaner than line-by-line removal of 4 interfaces and 4 union members.
- **Did not fix remaining 16 lint warnings** — all in Plate.js editor code not touched by this PR; fixing them is a separate concern.

---

## Files Modified

| File | Change |
|------|--------|
| `apps/web/components/pulse/sidebar/pulse-sidebar.tsx` | Removed `Clock`, `LayoutTemplate`, `Paintbrush`, `CheckSquare`, `Star` imports; removed `RecentsSection`, `StarredSection`, `TemplatesSection` imports; NAV_ITEMS trimmed to `extracted`+`workspace`; PAGE_LINKS trimmed to Jobs/Logs/Terminal/Cortex; `SectionContent` switch trimmed |
| `apps/web/components/pulse/sidebar/workspace-section.tsx` | Removed `pushRecent` import and call from `handleSelect` |
| `apps/web/components/pulse/sidebar/types.ts` | `SidebarSectionId` → `'extracted' \| 'workspace'`; removed `StarredItem`, `RecentItem`, `TagDef`, `TaggedItem` interfaces |
| `apps/web/hooks/ws-messages/handlers.ts` | Removed unused `WorkspaceContextState` import |
| `apps/web/README.md` | Updated Last Modified date to 2026-03-02 |

## Files Deleted

| File | Feature |
|------|---------|
| `apps/web/components/pulse/sidebar/starred-section.tsx` | Starred |
| `apps/web/components/pulse/sidebar/recents-section.tsx` | Recents |
| `apps/web/components/pulse/sidebar/templates-section.tsx` | Skills sidebar panel |
| `apps/web/components/creator/creator-dashboard.tsx` | Creator |
| `apps/web/app/creator/page.tsx` | Creator |
| `apps/web/app/api/creator/route.ts` | Creator |
| `apps/web/app/tasks/page.tsx` | Tasks |
| `apps/web/app/api/tasks/route.ts` | Tasks |
| `apps/web/components/tasks/tasks-dashboard.tsx` | Tasks |
| `apps/web/components/tasks/task-form.tsx` | Tasks |
| `apps/web/components/tasks/tasks-list.tsx` | Tasks |

---

## Commands Executed

```bash
# Delete all target files (absolute paths)
rm -f /home/jmagar/workspace/axon_rust/apps/web/app/api/creator/route.ts \
      /home/jmagar/workspace/axon_rust/apps/web/app/tasks/page.tsx \
      ... (11 total)

# Remove unused import
sed -i '/  WorkspaceContextState,/d' apps/web/hooks/ws-messages/handlers.ts

# Verify
git status apps/web --short
pnpm lint
```

---

## Behavior Changes (Before/After)

| Area | Before | After |
|------|--------|-------|
| Sidebar tabs | Extracted, Starred, Recents, Skills, Workspace | Extracted, Workspace |
| Sidebar page links | Creator, Tasks, Jobs, Logs, Terminal, Cortex | Jobs, Logs, Terminal, Cortex |
| `/creator` route | Rendered Creator dashboard | 404 |
| `/tasks` route | Rendered Tasks scheduler | 404 |
| `SidebarSectionId` type | 6 members | 2 members |
| Lint warnings | 17 | 16 |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `git status apps/web --short` | 11 D, 4 M | 11 D, 4 M | ✅ |
| `pnpm lint \| tail -3` | ≤17 warnings, 0 errors | 16 warnings, 0 errors | ✅ |
| `grep CheckSquare pulse-sidebar.tsx` | no match | no match | ✅ |
| `grep pushRecent workspace-section.tsx` | no match | no match | ✅ |
| `grep WorkspaceContextState handlers.ts` | no match | no match | ✅ |
| `ls app/creator/ app/tasks/ components/tasks/` | empty dirs | empty dirs | ✅ |

---

## Risks and Rollback

- **Risk**: Empty `app/creator/`, `app/tasks/`, `components/creator/`, `components/tasks/` directories remain on disk (git will ignore them). No functional risk.
- **Rollback**: `git checkout HEAD -- apps/web/app/creator apps/web/app/tasks apps/web/components/creator apps/web/components/tasks apps/web/components/pulse/sidebar/starred-section.tsx apps/web/components/pulse/sidebar/recents-section.tsx apps/web/components/pulse/sidebar/templates-section.tsx` restores all deleted files. Revert `pulse-sidebar.tsx`, `workspace-section.tsx`, `types.ts` to restore sidebar behavior.

---

## Decisions Not Taken

- **Stub 404 pages** for `/creator` and `/tasks` — no reason; the routes are genuinely removed, not renamed.
- **Fix remaining 16 lint warnings** — all in Plate.js editor glue, separate concern from this cleanup.
- **Delete empty directories** — git drops empty dirs on commit automatically; explicit `rmdir` is noise.

---

## Open Questions

- Root cause of Edit tool writes not persisting and `rm` results appearing non-deterministic. Possibly a Docker bind-mount sync race with the `axon-web` container's hot reload. Needs investigation if it recurs.

---

## Next Steps

- Commit and push `feat/sidebar` branch
- Optionally: address remaining 16 lint warnings in a follow-up PR (Plate.js editor cleanup)
