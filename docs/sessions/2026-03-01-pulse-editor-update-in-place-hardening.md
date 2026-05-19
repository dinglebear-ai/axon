# Session: Pulse Editor Update-in-Place Hardening

**Date:** 2026-03-01
**Branch:** `feat/crawl-download-pack`
**Commit:** `8ad11100`

---

## Session Overview

Full implementation of the "Fix Editor Page + Update-in-Place Saves" plan, followed by two rounds of adversarial code review addressing 21 issues total (8 from the first review, 13 from the second). The core problem: every autosave tick was creating a brand-new `.cache/pulse/*.md` file instead of updating the existing one. Secondary: the `/editor` route was pointing at a dead Plate demo component.

---

## Timeline

| Time | Activity |
|------|----------|
| Start | Plan already written; implementation begins from batch 1 |
| Batch 1 | `storage.ts` → `updatePulseDoc()`, `save/route.ts` → filename routing, `use-pulse-autosave.ts` → `filenameRef` + `savedFilename`, `pulse-workspace.tsx` → `currentDocFilename` state |
| Batch 2 | `workspace-persistence.ts` + `use-pulse-persistence.ts` — persist/restore `currentDocFilename` across refresh; `editor/page.tsx` full rewrite; delete `plate-editor.tsx` |
| Review 1 | 8 issues surfaced; 6 addressed immediately; issues 5 and 6 skipped |
| Review 1b | User explicitly requested all 8; issues 5 (`docMetaRef` reset) and 6 (`SaveStatusBadge` memo) fixed |
| Review 2 | 13 more issues surfaced by adversarial re-audit |
| Fix batch 1 | Issues 1–4, 7, 8, 10: filename regex, `?wait=true`, TEI logging, `updatedAt` threading, `aria-live`, remove `path` from response |
| Fix batch 2 | Issues 3, 9, 11, 12 (this session): 404 error banner, `updatePulseDoc` tests, path-segment substring fix, scroll debounce |
| Push | `git push` → `8ad11100` |

---

## Key Findings

- **Root cause (new files on every save):** `savePulseDoc` always called; no filename was tracked client-side, so `filenameRef` was always null, routing every save to the create path.
- **`docMetaRef` reset bug** (`use-pulse-autosave.ts:33-40`): When `docFilename` prop synced from `null` → the saved filename, the effect wiped `docMetaRef.current = null` and `lastSavedSnapshotRef.current = ''`, triggering a ghost re-save. Fixed with guard: only reset when `incoming !== filenameRef.current`.
- **`hasClientMeta` fast-path** (`storage.ts:135-139`): Skips `loadPulseDoc` file read on the hot autosave path when client supplies `createdAt`, `tags`, `collections`. Requires `clientUpdatedAt` to be absent (conflict-check skipped).
- **Qdrant vector accumulation** (`save/route.ts:106-116`): Pre-delete without `?wait=true` allowed race where upsert ran before delete completed, leaving stale vectors. Fixed by appending `?wait=true` to the delete URL.
- **404 orphan autosave** (`editor/page.tsx:49-53`): Navigating to `/editor?doc=missing.md` silently returned, leaving `docFilename = null`. Any subsequent typing created a new unrelated file with no feedback. Fixed: set `loadError` state, show red banner, never set `docFilename`.
- **Substring check** (`pulse-workspace.tsx:213`): `.includes('.cache/pulse/')` matched on paths like `/home/user/mycache/pulse/foo.md`. Fixed with leading slash: `/.cache/pulse/`.

---

## Technical Decisions

- **Last-write-wins for concurrent edits**: Rather than blocking saves on conflict, the server logs a warning and proceeds. Matches the "notes tool" use case where a single user edits on multiple tabs; a hard block would be more disruptive than data loss for this audience.
- **`filenameRef` instead of state** for autosave filename tracking: Avoids stale closure — the async fetch callback in the debounced `setTimeout` reads the ref directly without re-registering the effect.
- **`setTimeout(0)` for scroll restore**: `requestAnimationFrame` would also work but `setTimeout(0)` is simpler and achieves the same "after paint" goal for this case. Cleanup included to prevent firing on unmounted nodes.
- **Red error banner, not a toast**: Banner persists below the header and stays visible while the user is typing. A toast would disappear before the user noticed the implication.
- **`React.memo` on `SaveStatusBadge`**: Status changes on every autosave tick but the badge is a pure function of `status` prop. Memo prevents unnecessary re-render of the span during rapid typing.
- **Debounce scroll writes at 200 ms**: Per-event writes were firing dozens of times per scroll. 200 ms matches typical UI debounce convention and is short enough that position is never stale by more than one screen's worth of scroll.

---

## Files Modified

| File | Change |
|------|--------|
| `apps/web/lib/pulse/storage.ts` | Added `updatePulseDoc()`, `SavedDocMeta.updatedAt`, `hasClientMeta` fast-path, conflict/phantom-create logs |
| `apps/web/app/api/pulse/save/route.ts` | Filename regex validation, `?wait=true` on pre-delete, TEI failure log, `updatedAt` threading, removed `path` from response |
| `apps/web/hooks/use-pulse-autosave.ts` | New signature with `docFilename`, `filenameRef`, `docMetaRef` (with reset guard), `savedFilename` return, `updatedAt` tracking |
| `apps/web/components/pulse/pulse-workspace.tsx` | `currentDocFilename` state, sync from `savedFilename`, file-load effect wired, `handleNewSession` clears it, substring check fixed |
| `apps/web/lib/pulse/workspace-persistence.ts` | Added `currentDocFilename: string \| null` to persisted state type |
| `apps/web/hooks/use-pulse-persistence.ts` | `currentDocFilename` + `setCurrentDocFilename` threaded through hydration and persist callback |
| `apps/web/app/editor/page.tsx` | Full rewrite: `PulseEditorPane` + title input + autosave + `?doc=` load + `loadedDocRef` + 404 error banner |
| `apps/web/components/pulse/pulse-editor-pane.tsx` | Scroll restore deferred via `setTimeout(0)`, `onScroll` writes debounced 200 ms, cleanup on unmount |
| `apps/web/__tests__/pulse-storage.test.ts` | 5 new `updatePulseDoc` tests; import updated |
| `apps/web/components/editor/plate-editor.tsx` | **Deleted** (dead demo component, only used by old `/editor` route) |

---

## Commands Executed

```bash
# Test suite — all 10 pulse-storage tests passing
pnpm vitest run __tests__/pulse-storage.test.ts
# Result: 10 passed, 0 failed (104ms)

# Commit with pre-commit hooks (biome, monolith, env-guard, claude-symlinks)
git commit -m "fix(web): pulse autosave update-in-place + editor hardening"
# Result: 7 files changed, 211 insertions(+), 42 deletions(-)

git push
# Result: feat/crawl-download-pack → 8ad11100
```

---

## Behavior Changes (Before → After)

| Area | Before | After |
|------|--------|-------|
| Autosave | Each save creates new `slug-{timestamp}.md` | First save creates; all subsequent saves update the same file in-place |
| File accumulation | N edits → N files in `.cache/pulse/` | N edits → 1 file |
| `?doc=` on missing file | Blank page, no error; any typing creates orphan file | Red error banner: "Document not found — any edits will be saved as a new document." |
| `/editor` route | Rendered bare Plate demo (`plate-editor.tsx`) | Renders full `PulseEditorPane` with title, autosave, `?doc=` load |
| File load on `/editor` | Not wired | `?doc=<filename>` → fetches `/api/pulse/doc`, populates title + markdown |
| Workspace persistence | `currentDocFilename` lost on page refresh | Persisted to localStorage, restored on hydration |
| Scroll restore | `scrollTop` set immediately (race with render) | Deferred via `setTimeout(0)` |
| Scroll save to localStorage | Every scroll event writes | Debounced to one write per 200 ms |
| Qdrant re-embed | Pre-delete may race upsert | `?wait=true` ensures ordering |
| TEI failure | Silently swallowed | Logged: `[Pulse] TEI embed failed: <status> <body>` |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `pnpm vitest run __tests__/pulse-storage.test.ts` | 10 tests pass | 10 passed (104 ms) | ✅ |
| `git push` | Branch updated | `2d32f42e..8ad11100` | ✅ |
| Pre-commit hooks (biome, monolith) | No lint errors | "No fixes applied. Monolith policy check passed." | ✅ |
| Phantom-create test log | `[Pulse] updatePulseDoc: file not found, creating:` visible in stderr | Confirmed in test output | ✅ |

---

## Source IDs + Collections Touched

_Embedded after session close — see Axon embed job result below._

---

## Risks and Rollback

- **`updatePulseDoc` called on non-existent file** — phantom-create creates it silently. Now logged as a warning so it's visible, but the write still proceeds (intentional: last-write-wins).
- **`hasClientMeta` fast-path skips conflict detection** — if client omits `clientUpdatedAt`, server can't detect concurrent edits. This is by design; the trade-off is I/O savings vs. blind overwrite on the fast path.
- **Rollback**: Revert commit `8ad11100`. The previous autosave behaviour (always create) is safe — it leaks files but never loses data.

---

## Decisions Not Taken

- **Block on concurrent edit**: Rejected — would interrupt the user's typing flow for a use case (single-user, multi-tab) where last-write-wins is acceptable.
- **React Query / SWR for doc loading**: Overkill for a one-time load-on-mount pattern. The plain `useEffect` + `fetch` is sufficient and avoids an extra dependency.
- **`requestAnimationFrame` for scroll restore**: Would also work; `setTimeout(0)` chosen for simplicity.
- **Strict filename validation blocking phantom-create**: The regex validates the shape of filenames coming from the client, but `updatePulseDoc` still handles missing targets gracefully rather than throwing — keeps the server robust against race conditions.

---

## Open Questions

- The GitHub Dependabot alert flagged 2 high-severity vulnerabilities on the default branch — unrelated to this session but worth triaging.
- `pulse-editor-pane.tsx` scroll restore uses `setTimeout(0)` — should be verified in production that Plate's async render doesn't require a longer delay on slow hardware.

---

## Next Steps

- Triage the 2 Dependabot high-severity vulnerabilities on the default branch.
- Verify update-in-place behaviour manually: edit in dashboard editor → check `.cache/pulse/` — single file, `updatedAt` changes, no accumulation.
- Verify `/editor?doc=<filename>` load: create a doc, copy filename, navigate to `/editor?doc=<filename>` — content populates.
- Consider adding an integration test that exercises the full save → update cycle via the HTTP route.
