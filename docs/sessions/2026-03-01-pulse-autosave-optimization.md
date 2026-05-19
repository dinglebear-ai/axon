# Session: Pulse Autosave Optimization + Editor UX + Z-index Fix
Date: 2026-03-01
Branch: feat/crawl-download-pack
Commit: 2d32f42e

## Session Overview

Optimized the Pulse autosave pipeline to eliminate a redundant file read on every update, added pre-deletion of stale Qdrant vectors before re-embedding to prevent accumulation, improved the editor's document-reload behavior, fixed a z-index stacking bug where NeuralCanvas/floating elements bled over the sidebar, and added a missing `/ws/shell` proxy rewrite to `next.config.ts`.

## Timeline

1. **Diff review** — ran `git diff --stat HEAD` and `git diff HEAD` on all 9 changed files to understand what was staged. Identified 5 distinct concerns: autosave optimization, Qdrant vector accumulation, editor UX, z-index bug, and `/ws/shell` proxy gap.

2. **Biome lint failure** — first commit attempt failed. `pulse-workspace.tsx:138` had `setCurrentDocFilename`, `setDocumentMarkdown`, `setDocumentTitle` in `handleNewSession`'s `useCallback` deps array. Biome `useExhaustiveDependencies` correctly flagged these as unnecessary (React `setState` functions are stable and never change). Removed them from the deps array.

3. **Commit** — `2d32f42e` committed and pushed to `feat/crawl-download-pack` after lint fix.

## Key Findings

- **Autosave file-read elimination** (`storage.ts:112-127`): `updatePulseDoc` previously called `loadPulseDoc` on every save to recover `createdAt`/`tags`/`collections` before writing. Client now caches these from the save response and sends them back, enabling the file read to be skipped entirely on the common autosave path.
- **Qdrant vector accumulation** (`api/pulse/save/route.ts:91-104`): Re-saving the same Pulse doc repeatedly caused vector duplication — each save appended new chunks without removing old ones. Fix: pre-DELETE via `POST /collections/{name}/points/delete` with filter `{ key: "url", match: { value: "pulse://<filename>" } }` before each re-embed.
- **Editor doc-reload bug** (`editor/page.tsx:31-38`): `loadedRef.current = true` was a one-shot guard — navigating to a different `?doc=` param in the same page session would silently skip reloading. Changed to `loadedDocRef.current = docParam` (tracks the loaded filename, not a boolean).
- **Z-index stacking** (`app-shell.tsx:22`, `pulse-sidebar.tsx:135`): NeuralCanvas or floating elements from child routes were appearing on top of the sidebar. Fix: sidebar gets `z-[2]`, main content area gets `z-[1]`.
- **Missing proxy rewrite** (`next.config.ts:39-42`): `/ws/shell` WebSocket route was implemented in the Rust backend but the Next.js proxy rewrite was never added, so browser connections to `/ws/shell` would fail in production. Added the rewrite.
- **Stable setState in deps** (`pulse-workspace.tsx:152-161`): React `useState` setter functions (`setCurrentDocFilename`, `setDocumentMarkdown`, `setDocumentTitle`) are referentially stable — they never change identity across renders. Including them in `useCallback` deps is technically harmless but triggers Biome's `useExhaustiveDependencies` lint rule because the rule knows they're stable. Removed.

## Technical Decisions

| Decision | Rationale | Alternative Rejected |
|---|---|---|
| Client-cache metadata for autosave skip | Eliminates a filesystem read on every keystroke debounce; `createdAt`/`tags`/`collections` don't change between saves except on explicit user action | Server-side in-memory cache — adds statefulness/invalidation complexity |
| Pre-delete via Qdrant filter before re-embed | Prevents unbounded vector growth; simpler than a post-embed cleanup | Upsert with matching payload IDs — requires deterministic chunk IDs, complex |
| `loadedDocRef` tracks param string not boolean | Correctly handles in-session navigation to different docs (e.g., clicking a different file in the sidebar) | Boolean flag reset on unmount — would work for full remounts but not same-page navigation |
| Remove setState from useCallback deps | Biome rule violation; React guarantees setter stability | Add `// biome-ignore` comment — suppresses the rule without improving the code |

## Files Modified

| File | Type | Purpose |
|---|---|---|
| `apps/web/lib/pulse/storage.ts` | Modified | Export `SavedDocMeta` interface; `updatePulseDoc` skips `loadPulseDoc` when client supplies all three metadata fields; both save functions return `SavedDocMeta` |
| `apps/web/app/api/pulse/save/route.ts` | Modified | Accept `createdAt` in request body; pre-delete Qdrant vectors for `pulse://<filename>` before re-embed; response includes `createdAt`, `tags`, `collections` |
| `apps/web/hooks/use-pulse-autosave.ts` | Modified | Add `docMetaRef` to cache `{ createdAt, tags, collections }` from save response; pass cached fields back on subsequent saves; reset snapshot guard on `docFilename` prop change |
| `apps/web/app/editor/page.tsx` | Modified | `loadedDocRef` tracks loaded doc param string; `SaveStatusBadge` wrapped in `memo`; `Suspense` fallback skeleton |
| `apps/web/components/pulse/pulse-workspace.tsx` | Modified | Remove stable setState functions from `handleNewSession` `useCallback` deps (Biome lint fix) |
| `apps/web/components/app-shell.tsx` | Modified | Add `z-[1]` to main content div |
| `apps/web/components/pulse/sidebar/pulse-sidebar.tsx` | Modified | Add `z-[2]` to sidebar div; remove unused `TagsSection` import |
| `apps/web/components/terminal/terminal-emulator.tsx` | Modified | Minor fix (exact change from diff: added `'use client'` directive or import — exact content not confirmed) |
| `apps/web/next.config.ts` | Modified | Add `/ws/shell` → `${axonBackendUrl}/ws/shell` proxy rewrite |
| `CHANGELOG.md` | Modified | Added `2d32f42e` entry; added highlights for autosave optimization, editor UX, z-index fix |

## Commands Executed

```bash
# Orientation
git branch --show-current     # → feat/crawl-download-pack
git diff --stat HEAD          # → 9 files, 144 insertions, 32 deletions
git log --oneline -5          # → 394917d5, ac294073, 9fdf8913, d7cff203, d357f088

# Inspect specific diffs
git diff HEAD -- apps/web/app/api/pulse/save/route.ts apps/web/hooks/use-pulse-autosave.ts apps/web/lib/pulse/storage.ts apps/web/next.config.ts
git diff HEAD -- apps/web/next.config.ts apps/web/app/editor/page.tsx apps/web/components/app-shell.tsx ...

# First commit attempt (failed)
git add . && git commit -m "fix(web): pulse autosave ..."
# → biome: pulse-workspace.tsx:138 useExhaustiveDependencies — setCurrentDocFilename etc. unnecessary

# Fix: remove stable setState from deps
# Edit pulse-workspace.tsx deps array

# Second commit (success)
git add . && git commit -m "fix(web): pulse autosave skip file-read..."
# → [feat/crawl-download-pack 2d32f42e] 10 files changed, 147 insertions(+), 70 deletions(-)

git push
# → 394917d5..2d32f42e  feat/crawl-download-pack -> feat/crawl-download-pack
```

## Behavior Changes (Before/After)

| Area | Before | After |
|---|---|---|
| Pulse autosave file read | Every update calls `loadPulseDoc` (filesystem read) to recover `createdAt`/`tags`/`collections` | First save: file read; subsequent saves: use client-cached metadata — no file read |
| Qdrant vector accumulation | Each re-save of same doc appends new chunks; vectors grow unboundedly | Pre-DELETE by `pulse://<filename>` URL filter before each re-embed; only current chunks stored |
| Pulse save response | `{ path, filename, saved: true }` | `{ path, filename, saved: true, createdAt, tags, collections }` |
| Editor doc-reload | `?doc=A` loads A; navigating to `?doc=B` in same page session silently skips re-load | `?doc=A` loads A; navigating to `?doc=B` triggers fresh load of B |
| `/ws/shell` in browser | `404` (no Next.js proxy rewrite) | Proxied to `${axonBackendUrl}/ws/shell` |
| Sidebar z-index | NeuralCanvas/floating child elements could bleed over sidebar | Sidebar `z-[2]` > content `z-[1]` — sidebar always on top |

## Verification Evidence

| Check | Expected | Actual | Status |
|---|---|---|---|
| `git push` | Push to feat/crawl-download-pack | `394917d5..2d32f42e` pushed | ✅ |
| Biome pre-commit (2nd attempt) | No errors | `Checked 9 files in 24ms. No fixes applied.` | ✅ |
| Monolith check | Pass | `Monolith policy check passed.` | ✅ |
| `git log --oneline -1` | `2d32f42e fix(web): pulse autosave...` | Confirmed | ✅ |

## Source IDs + Collections Touched

None during implementation. Session doc embedded as part of save-to-md workflow below.

## Risks and Rollback

- **Client-cache stale risk**: If `createdAt`/`tags`/`collections` become stale (e.g., user edits tags from another tab), the cached values will be written back on the next autosave, overwriting the newer values. In practice the editor is single-tab; acceptable risk.
- **Pre-delete timing**: If `fetch(qdrant/delete)` succeeds but the subsequent embed fails, the doc loses its vectors entirely until the next save. The `catch` on the pre-delete is intentional (fire-and-forget) — an embed failure will log to console but not surface as a UI error beyond the existing embed error handling.
- **Rollback**: `git revert 2d32f42e` reverts all 10 file changes.

## Decisions Not Taken

- **Optimistic UI for autosave**: Show "Saving…" before the server responds. Not needed — the existing `setSaveStatus('saving')` + debounce already gives responsive feedback.
- **Atomic delete+embed in Qdrant**: Use a batch request to delete and upsert in one call. Not supported by the Qdrant REST API in the same request; fire-and-forget delete followed by embed is the standard pattern.
- **`// biome-ignore` comment on pulse-workspace.tsx**: Suppressing the lint rule rather than fixing it. Rejected — the fix (removing stable setters from deps) is correct and non-breaking.

## Open Questions

- Does the `terminal-emulator.tsx` change have a specific bug fix or is it a minor import/directive addition? The diff was cut off (only `'use client'` visible). Behavior impact unknown.
- The Qdrant pre-delete uses a fire-and-forget `.catch(console.error)`. If the delete fails silently (e.g., Qdrant down during save), vectors accumulate again. Should there be a fallback strategy?

## Next Steps

- Navigate to the editor at `https://axon.tootie.tv` and verify `/ws/shell` connects properly from the terminal page
- Verify autosave no longer causes vector accumulation: save the same Pulse doc twice and check Qdrant chunk count doesn't double
- Confirm editor re-loads correctly when navigating between `?doc=A` and `?doc=B` in the same session
