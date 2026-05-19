# Session: Pulse 3-Panel Collapsible Layout
Last Modified: 2026-03-01
Branch: feat/crawl-download-pack

## Session Overview

Implemented a 3-panel collapsible layout for the Pulse workspace, replacing a `DesktopViewMode = 'chat' | 'editor' | 'both'` enum with `showChat: boolean` / `showEditor: boolean` boolean panel visibility state. Each panel can collapse to a 28px chevron strip. A dual-behavior drag handle (≥4px drag = resize split; <4px click = toggle editor) sits between panels. Execution followed the subagent-driven-development workflow: 5 tasks, each with spec review + code quality review + fix loops.

Also committed two unrelated stashed changes: `config/mcporter.json` plate MCP entry and `job_context.rs` crawl worker output_dir path fix.

## Timeline

1. **Plan review** — read `docs/plans/2026-03-01-3-panel-collapsible-layout.md` and dispatched 3 parallel accuracy agents; found 3 plan errors
2. **Plan fixes** — corrected drag-handle highlight class mismatch, added `group/chat`/`group/editor` to JSX snippets, fixed flexBasis note
3. **Task 1** — `workspace-persistence.ts`: remove old types, add booleans, migration, widen clamp (commit `1925a5bb`)
4. **Tasks 2+4 parallel** — `use-split-pane.ts` rewrite + `pulse-toolbar.tsx` prop removal (commits `60cd01ed`, `f5c13206`)
5. **Task 2 quality fix** — removed dead `showChatRef` (commit `cf1323ce`)
6. **Task 3** — `use-pulse-persistence.ts` interface + hydration update (commit `50dd9473`)
7. **Task 5** — `pulse-workspace.tsx` 3-panel JSX (commits `3359e863`, `61a1696e`)
8. **Task 5 quality fixes** — both-panels guard, editor flex-1 when chat collapsed, `aria-valuetext`, dead vertical drag removed (commit `4e4633d9`)
9. **Final review fixes** — dual-hydration race, both-collapsed restore guard (commit `5dee20a7`)
10. **Push** — all implementation commits + plate MCP + crawl output_dir fix (commit `e2e5ee6b`)

## Key Findings

- **Plan accuracy issues (3 found, all fixed)**:
  1. `stopDrag` removed `'bg-[rgba(175,215,255,0.3)]'` but `onPointerDown` added `'bg-[rgba(175,215,255,0.15)]'` (class mismatch)
  2. `group/chat` and `group/editor` were mentioned in a note but missing from the JSX snippet's className
  3. Note on line 650 incorrectly stated "chat panel gets `flexBasis: desktopSplitPercent%`" — chat actually uses `flex-1`

- **Quality review caught: dead `showChatRef`** — was declared + synced but never read in Task 2's initial implementation; removed in fix commit

- **Quality review caught: editor flex-1 gap** — when chat is collapsed, editor had `lg:flex-none` + `flexBasis: (100-split)%`, leaving a visual gap instead of expanding to fill. Fixed by switching to `lg:flex-1` when only editor is open.

- **Quality review caught: both-panels-collapsed dead state** — no guard prevented collapsing both panels, leaving two 7px strips with no drag handle and no easy recovery. Added guards in `toggleChat`/`toggleEditor` and the drag-handle click path in `stopDrag`.

- **Final review caught: dual-hydration race** — `useSplitPane` and `usePulsePersistence` both called `setShowChat`/`setShowEditor` on mount from separate localStorage keys; whichever React effect ran last would win non-deterministically. Fixed by removing the read from `useSplitPane`'s mount effect (write paths kept for fast-path persistence).

- **Final review caught: stale blob both-collapsed** — `parsePersistedWorkspaceState` could restore a pre-guard blob with both panels collapsed. Added: `if (!showChat && !showEditor) showChat = true`.

- **Tailwind v4.2.1 named group variants confirmed compatible** — `group/chat`, `group/editor`, `group-hover/chat:opacity-100` all valid.

- **Crawl worker output_dir bug** — `job_context.rs` was using the serialized `parsed.output_dir` (from the job submitter, e.g. MCP on host) instead of `cfg.output_dir` (worker's own base dir). Caused path mismatches when MCP runs on host and worker runs inside Docker.

## Technical Decisions

- **Single source of truth for `showChat`/`showEditor` on mount**: `usePulsePersistence` owns hydration from the monolithic `axon.web.pulse.workspace-state.v2` blob. `useSplitPane` only writes to dedicated keys (`SHOW_CHAT_STORAGE_KEY`, `SHOW_EDITOR_STORAGE_KEY`) for fast-path persistence on toggle, never reads them on startup.

- **Both-panels-collapsed guard: silent block (not expand-other)**: When collapsing a panel that would leave both collapsed, the collapse is silently blocked. Simpler than expanding the other panel, and the state transition reads naturally (the button click appears to do nothing — expected when already at the limit).

- **`showEditorRef` + `showChatRef` pattern in stopDrag**: The `stopDrag` handler is registered in a `useEffect` with empty deps (`[]`), so it has a stale closure over state. Refs sync'd via paired `useEffect` calls provide fresh values inside the handler without needing to re-register it.

- **Editor flex layout three-way conditional**:
  - Both open → `lg:flex-none` + inline `flexBasis: (100-split)%` (resize split behavior)
  - Editor open, chat collapsed → `lg:flex-1` (fill remaining space)
  - Editor collapsed → `lg:w-7 lg:flex-none` (28px strip)

- **Backward-compatible migration**: `parsePersistedWorkspaceState` derives `showChat`/`showEditor` from old `desktopViewMode` field when new fields are absent (safe upgrade path for existing localStorage blobs).

## Files Modified

| File | Purpose | Commits |
|------|---------|---------|
| `apps/web/lib/pulse/workspace-persistence.ts` | Remove `DesktopViewMode`/`DesktopPaneOrder`, add `showChat`/`showEditor`, migration, clamp 20-80, both-collapsed restore guard | `1925a5bb`, `5dee20a7` |
| `apps/web/hooks/use-split-pane.ts` | Full rewrite: 3-panel state, toggleChat/toggleEditor, dual-behavior drag handle, collapse guards, remove dead vertical drag effect | `60cd01ed`, `cf1323ce`, `4e4633d9`, `5dee20a7` |
| `apps/web/hooks/use-pulse-persistence.ts` | Remove old type imports, update interface + hydration for `showChat`/`showEditor` | `50dd9473` |
| `apps/web/components/pulse/pulse-toolbar.tsx` | Remove view-mode props, type aliases, and button group (-84 lines net) | `f5c13206` |
| `apps/web/components/pulse/pulse-workspace.tsx` | 3-panel JSX with `ChevronLeft`/`ChevronRight` collapse strips, drag handle, flex layout | `3359e863`, `61a1696e`, `4e4633d9` |
| `config/mcporter.json` | Add plate shadcn MCP server entry | `e2e5ee6b` |
| `crates/jobs/crawl/runtime/worker/job_context.rs` | Fix `output_dir` to use worker's `cfg.output_dir` not serialized job path | `e2e5ee6b` |
| `CHANGELOG.md` | Update with session commits | `e2e5ee6b` |
| `docs/plans/2026-03-01-3-panel-collapsible-layout.md` | Fix 3 plan accuracy errors before execution | (pre-commit edit) |

## Commands Executed

```bash
# TypeScript verification (passes throughout)
cd apps/web && pnpm tsc --noEmit

# Pre-commit hook (480 tests passing)
git commit  # triggers lefthook: biome, monolith, rustfmt, cargo check, cargo test

# Push
git push  # feat/crawl-download-pack → a941173c..e2e5ee6b
```

## Behavior Changes (Before/After)

| Behavior | Before | After |
|----------|--------|-------|
| Panel visibility state | `DesktopViewMode = 'chat' \| 'editor' \| 'both'` | `showChat: boolean`, `showEditor: boolean` |
| Panel collapse | No collapse — mode switch only | Each panel collapses to 28px chevron strip independently |
| Drag handle | Always visible on desktop | Hidden when either panel is collapsed |
| Drag handle click | No special behavior | Click (< 4px movement) toggles editor panel |
| Editor size when chat collapsed | N/A | `flex-1` — fills all available space |
| Both panels collapsed | Possible (no guard) | Blocked — second collapse silently no-ops |
| localStorage hydration | `useSplitPane` and `usePulsePersistence` both restored `showChat`/`showEditor` | Only `usePulsePersistence` reads on mount (no race) |
| Stale blob restore | Could restore both panels collapsed | `parsePersistedWorkspaceState` forces `showChat=true` if both would be false |
| Crawl worker output path | Used serialized job path (submitter's filesystem root) | Uses worker's `cfg.output_dir` (worker's filesystem root) |

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `pnpm tsc --noEmit` (apps/web) | Exit 0 | Exit 0, no output | ✅ |
| `cargo test --lib` (pre-commit) | 480 tests pass | 480 passing | ✅ |
| `cargo check` (pre-commit) | Clean | Clean | ✅ |
| Spec review Task 1 | All 6 requirements met | ✅ all met | ✅ |
| Spec review Task 2 | All 11 requirements met | ✅ all met | ✅ |
| Spec review Task 3 | All 7 requirements met | ✅ all met | ✅ |
| Spec review Task 4 | All requirements met | ✅ all met | ✅ |
| Spec review Task 5 | All 14 requirements met | ✅ all met | ✅ |
| Quality review Task 1 | Approved | ✅ Approved | ✅ |
| Quality review Task 2 | Approved | NEEDS WORK → fix → ✅ Approved | ✅ |
| Quality review Task 4 | Approved | ✅ Approved | ✅ |
| Quality review Task 3 | Approved | ✅ Approved | ✅ |
| Quality review Task 5 | Approved | NEEDS WORK → fix → ✅ Approved | ✅ |
| Final code review | Approved | NEEDS WORK → fix → ✅ Approved | ✅ |

## Commit Chain

```
e2e5ee6b  chore: plate MCP entry + crawl worker output_dir fix + changelog
5dee20a7  fix(web): pulse dual-hydration race + both-collapsed restore guard
a941173c  feat(web): jobs dashboard (prior session, part of same push)
4e4633d9  fix(web): pulse workspace quality fixes — collapse guard, editor flex, aria
61a1696e  fix(web): remove unused verticalDragStartRef
3359e863  feat(web): 3-panel collapsible layout — chat left, editor right, chevron strips
cf1323ce  fix(web): remove unused showChatRef from use-split-pane
50dd9473  feat(web): update use-pulse-persistence for showChat/showEditor
f5c13206  feat(web): remove view-mode toggle buttons from PulseToolbar
60cd01ed  feat(web): rewrite use-split-pane for 3-panel chevron layout
1925a5bb  feat(web): replace DesktopViewMode/DesktopPaneOrder with showChat/showEditor booleans
```

## Risks and Rollback

- **localStorage migration**: Any user with an old `v1` key is unaffected (key changed to `v2` in an earlier commit). Users with the old `desktopViewMode` field in `v2` blobs get migration logic in `parsePersistedWorkspaceState`. Low risk.
- **Both-panels-collapsed pre-guard blobs**: Handled by the restore guard (`if (!showChat && !showEditor) showChat = true`). Chat (primary panel) is restored as the recovery default.
- **Rollback**: Revert the implementation commits (`1925a5bb`–`5dee20a7`). The `v2` localStorage key change means users would start with a clean state — acceptable.
- **Crawl worker output_dir**: Low risk change — uses `cfg.output_dir` (already the correct worker root) instead of the serialized submitter path. Only affects cross-environment crawl deployments (MCP on host + worker in Docker).

## Decisions Not Taken

- **Expand-other on last-panel collapse** — when collapsing would leave both panels closed, an alternative was to expand the other panel instead of blocking. Rejected: silent block is simpler and the UX expectation (button click = no-op at the limit) is clear enough.
- **Persist `showChat`/`showEditor` only in the monolithic blob** — could have removed the dedicated `SHOW_CHAT_STORAGE_KEY`/`SHOW_EDITOR_STORAGE_KEY` entirely. Rejected: the dedicated keys provide immediate fast-path persistence on toggle without waiting for the monolithic blob write cycle.
- **Remove `verticalDragStartRef` return from `useSplitPane` entirely** — the ref itself still exists in the hook; only the return export and the dead vertical drag effect were removed. Left open for future mobile vertical resize implementation.

## Open Questions

- The `isDirty` indicator in `PulseToolbar` is never reset to `false` after an autosave — the dirty dot persists forever after first edit. Pre-existing issue, not introduced this session.
- The `SHOW_CHAT_STORAGE_KEY`/`SHOW_EDITOR_STORAGE_KEY` keys are written but never read on mount — they are effectively dead writes if the monolithic blob is always restored first. Consider removing the write calls too for a cleaner single-source-of-truth model in a future cleanup.

## Next Steps

- Move `docs/plans/2026-03-01-3-panel-collapsible-layout.md` to `docs/plans/complete/` (plan is fully executed)
- Address the `isDirty` indicator reset in `PulseToolbar` (separate ticket)
- Consider a PR from `feat/crawl-download-pack` → `main` once the branch is deemed stable
