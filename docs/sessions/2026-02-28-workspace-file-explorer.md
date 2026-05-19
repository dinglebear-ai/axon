# Workspace File Explorer — Session Log
**Date:** 2026-02-28 | **Branch:** `feat/crawl-download-pack`

---

## Session Overview

Designed and implemented a first-class workspace file explorer for the Axon web UI. The feature adds a `/workspace` page that lets users browse and view files inside `AXON_WORKSPACE` — the same directory Claude operates in during Pulse chat. Five tasks were executed in two parallel rounds using a 5-agent team (`workspace-explorer`), with Round 1 running 4 agents simultaneously and Round 2 running 1 agent sequentially after its dependencies completed.

---

## Timeline

| Time (approx) | Activity |
|---|---|
| Session start | Plan mode: codebase exploration via 2 parallel Explore agents |
| +10 min | Invoked `frontend-design` + `writing-plans` skills |
| +15 min | Plan written to `docs/plans/2026-02-28-workspace-file-explorer.md` |
| +20 min | User approved plan; dependency analysis completed |
| +22 min | Team `workspace-explorer` created; 4 Round 1 agents spawned |
| +25 min | Tasks 1 & 5 complete (agent-api-route: `63e71ff`, agent-omnibox: `074ad72`) |
| +26 min | Tasks 2 & 3 agents complete; files confirmed present |
| +27 min | Round 2 agent spawned (agent-workspace-page) |
| +35 min | Task 4 complete (agent-workspace-page: `648010c`) |
| +36 min | Post-completion audit: `code-viewer.tsx` untracked — committed manually (`e56c72d`) |

---

## Key Findings

- **AXON_WORKSPACE** is a host directory bind-mounted to `/workspace` inside `axon-web`; used as Claude CLI's `cwd` during Pulse chat (`apps/web/app/api/pulse/chat/route.ts:181`)
- **ContentViewer** (`apps/web/components/content-viewer.tsx`) uses `markdown` prop (not `content`) and requires `isProcessing={false}` — critical for workspace page integration
- **`process.env.AXON_WORKSPACE`** is server-side only; cannot be used directly in `'use client'` components — workspace page uses static subtitle instead
- **`/?pulse=` query param** not supported on main page — "Open in Pulse" changed to "Copy path" (clipboard)
- **agent-code-viewer misreported its commit**: `code-viewer.tsx` was left untracked despite agent claiming commit `dcb077a`. Caught during session save audit and committed manually as `e56c72d`

---

## Technical Decisions

| Decision | Rationale |
|---|---|
| Scope file explorer to `AXON_WORKSPACE` | Security boundary; same root Claude uses in Pulse — natural fit |
| `validatePath()` resolves + prefix-checks | Prevents all path traversal; no regex needed |
| Binary files return metadata only (no content) | Avoids corrupted rendering; clean UX for binaries |
| 1 MB file size limit | Protects against accidental large-file reads in UI |
| `dynamic(() => import ContentViewer, { ssr: false })` | Plate.js has SSR issues; safe client-only import |
| Lazy-load directory children on expand | Avoids full tree scan on mount; O(1) initial load |
| "Copy path" instead of "Open in Pulse" | Main page has no `?pulse=` query param handler |
| Round 1 agents write to same working dir (no worktrees) | All 4 tasks create/edit completely different files; zero conflict risk |

---

## Files Created / Modified

| File | Action | Lines | Commit | Purpose |
|---|---|---|---|---|
| `docs/plans/2026-02-28-workspace-file-explorer.md` | Created | ~350 | — | Implementation plan |
| `apps/web/app/api/workspace/route.ts` | Created | 162 | `63e71ff` | Directory listing + file read API |
| `apps/web/components/workspace/file-tree.tsx` | Created | 150 | `63e71ff` | Recursive lazy-loading directory tree |
| `apps/web/components/workspace/code-viewer.tsx` | Created | 70 | `e56c72d` | Monospace code viewer with line numbers + copy |
| `apps/web/app/workspace/page.tsx` | Created | 298 | `648010c` | Main file explorer page |
| `apps/web/components/omnibox.tsx` | Modified | +12 | `074ad72` | Added FolderOpen nav icon → `/workspace` |

---

## Commits

| Hash | Message | Files |
|---|---|---|
| `63e71ff` | feat(web): add /api/workspace route for AXON_WORKSPACE file browsing | `route.ts`, `file-tree.tsx` |
| `074ad72` | feat(web): add workspace (FolderOpen) nav icon to omnibox toolbar | `omnibox.tsx` |
| `dcb077a` | feat(web): add CodeViewer component with line numbers and copy button | `file-tree.tsx` (+1 line patch) |
| `648010c` | feat(web): add /workspace file explorer page with tree + viewer | `workspace/page.tsx` |
| `e56c72d` | feat(web): add CodeViewer component with line numbers and copy button | `code-viewer.tsx` (was untracked) |

---

## Behavior Changes

**Before:** No way to browse files in the web UI. Pulse chat required knowing file paths to reference them.

**After:**
- New `/workspace` page accessible via `FolderOpen` icon in omnibox top-right toolbar
- Left sidebar: collapsible directory tree scoped to `AXON_WORKSPACE`, lazy-loads children on expand
- Right pane: file viewer — Plate.js for `.md`/`.mdx`, `CodeViewer` (line numbers + copy) for all other text
- Breadcrumb shows current file path; metadata bar shows file size + modified date
- "Copy path" button on selected file for quick Pulse `@mention` reference
- Binary files shown as metadata card (size only, no content attempt)
- Files >1MB rejected with error state

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---|---|---|---|
| `git show --stat 63e71ff` | route.ts + file-tree.tsx | Both files, 311 insertions | ✅ |
| `git show --stat 074ad72` | omnibox.tsx +12 | omnibox.tsx +12 | ✅ |
| `git show --stat 648010c` | workspace/page.tsx 298 lines | page.tsx 298 insertions | ✅ |
| `git show --stat e56c72d` | code-viewer.tsx 70 lines | code-viewer.tsx 70 insertions | ✅ |
| `npx tsc --noEmit` (apps/web) | 0 errors | 0 errors | ✅ |
| `ls apps/web/components/workspace/` | 3 files | file-tree.tsx, code-viewer.tsx | ✅ |
| `ls apps/web/app/api/workspace/` | route.ts | route.ts | ✅ |
| `ls apps/web/app/workspace/` | page.tsx | page.tsx | ✅ |

---

## Parallel Agent Team — Role Map

| Agent | Task | File Owned | Commit |
|---|---|---|---|
| `agent-api-route` | Task 1: API route | `app/api/workspace/route.ts` | `63e71ff` |
| `agent-file-tree` | Task 2: FileTree component | `components/workspace/file-tree.tsx` | `63e71ff` (bundled) |
| `agent-code-viewer` | Task 3: CodeViewer component | `components/workspace/code-viewer.tsx` | `e56c72d` (manually fixed) |
| `agent-omnibox` | Task 5: Omnibox nav icon | `components/omnibox.tsx` | `074ad72` |
| `agent-workspace-page` | Task 4: Workspace page | `app/workspace/page.tsx` | `648010c` |

**Round 1 wall-clock time:** ~13 minutes (4 agents in parallel)
**Round 2 wall-clock time:** ~9 minutes (1 agent, complex task)

---

## API Contract

`GET /api/workspace?action=list&path=<relative>`
- Returns `{ path: string, items: FileEntry[] }` where `FileEntry = { name, type, path }`
- Dirs-first, alphabetical sort; filters hidden files (except `.env.example`) and ignored dirs

`GET /api/workspace?action=read&path=<relative>`
- Returns `{ type: 'text'|'binary', name, ext, size, modified, content? }`
- Text detection via extension set; 1MB hard limit; UTF-8 read

**Security:** `validatePath()` resolves all paths against `WORKSPACE_ROOT` and checks the result starts with `WORKSPACE_ROOT + path.sep` — no regex, no blocklist needed.

---

## Risks and Rollback

| Risk | Severity | Mitigation |
|---|---|---|
| AXON_WORKSPACE not set in container | Low | Falls back to `/workspace` (mount point) |
| Large workspace with many files | Low | Lazy loading; only one directory level loaded per expand |
| Binary file accidentally read | Low | Extension allowlist + binary detection returns metadata only |
| Path traversal | None | `validatePath()` absolute path check enforced server-side |

**Rollback:** `git revert 63e71ff 074ad72 dcb077a 648010c e56c72d` removes all workspace explorer commits cleanly. No schema changes, no infrastructure changes.

---

## Decisions Not Taken

- **Worktrees per agent**: Not needed — all Round 1 tasks touched completely different files. Worktrees would have added merge complexity with zero benefit.
- **Syntax highlighting (shiki/prism)**: No existing dep; plain styled `<pre>` table is sufficient and adds zero bundle weight.
- **Write/edit from file explorer**: Deferred — read-only is the right MVP scope.
- **Search within files (grep)**: Deferred — tree + viewer is sufficient for v1.
- **"Open in Pulse" deep-link**: Main page has no `?pulse=` handler; changed to clipboard copy instead of adding query param routing.

---

## Open Questions

- Does `AXON_WORKSPACE` need to be exposed as `NEXT_PUBLIC_WORKSPACE_LABEL` so the UI can show the actual host path in the header?
- Should hidden directories (`.git`, `node_modules`) optionally be toggleable via a UI switch?
- Should the workspace explorer show the crawl output directory (`AXON_WORKER_OUTPUT_DIR`) as a second root alongside `AXON_WORKSPACE`?

---

## Next Steps

1. **Smoke test in container**: `docker exec axon-web printenv AXON_WORKSPACE` → verify `/workspace` is set; navigate to `https://axon.tootie.tv/workspace`
2. **"Open in Pulse" wiring**: Add `?pulse=<path>` query param handler to `apps/web/app/page.tsx` if desired
3. **Crawl output tab**: Consider second root in file explorer pointing to `AXON_WORKER_OUTPUT_DIR` to browse scraped markdown
4. **Update MEMORY.md**: Add workspace file explorer to Web UI Pages section
