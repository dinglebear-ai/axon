# Web UI Performance Fixes — Pulse Workspace
**Date:** 2026-03-03
**Branch:** `feat/sidebar`
**Scope:** `apps/web/` — 12 files modified, 8 performance issues fixed

## Session Overview

Implemented 8 surgical performance fixes across the Pulse workspace UI to eliminate jank during streaming, scrolling, typing, and drag-resize. Work was organized into 3 parallel tracks (hooks, components, canvas/virtualization). Two attempts to delegate via agent teams failed (agents went idle without producing changes); all work was executed directly.

## Timeline

1. **Plan review** — Read and validated the detailed performance fix plan (3 tracks, 8 fixes)
2. **Agent team attempt #1** — Spawned 3 `swe` agents → went idle, no output
3. **Agent team attempt #2** — Spawned 3 `frontend-developer` agents → same result
4. **Direct execution** — Read all 12 target files, applied all fixes sequentially
5. **Build verification** — Hit pre-existing `TagDef` type error; confirmed unrelated via `git stash` test; fixed the missing types
6. **Type fix: `useRef`** — React 19 requires initial argument for `useRef<T>()` → fixed to `useRef<T | null>(null)`
7. **Type fix: `SetStateAction`** — Tracked setters in `use-ws-messages.ts` needed to accept functional updaters, not just direct values
8. **Final verification** — `pnpm build` clean, `pnpm lint` clean (32 pre-existing warnings only)

## Key Findings

- **Root cause of streaming jank:** `persistWorkspaceState` callback had `chatHistory` as dep → every streaming token triggered `JSON.stringify(fullState)` + `localStorage.setItem()` at 10-50x/sec (`use-pulse-persistence.ts`)
- **10 unnecessary useEffect ref-sync patterns** across 4 hooks added extra render cycles per state change — replaced with atomic tracked setters
- **`transition-all`** on split pane containers transitioned every CSS property including layout-triggering `flex-basis` during drag resize (`pulse-workspace.tsx:373,479`)
- **3 duplicate NeuralCanvas rAF loops** — `logs-viewer.tsx`, `agents/page.tsx`, `terminal/page.tsx` each rendered their own canvas identical to app-shell's
- **LogsViewer rendered all 2000 DOM nodes** simultaneously + allocated new array on every SSE message via spread
- **`box-shadow` keyframe animation** on tool badges triggered paint per frame (compositor-unfriendly)
- **`serializeMd(editor)`** (full Plate AST traversal) ran on every `markdown` prop change during streaming

## Technical Decisions

| Decision | Rationale |
|----------|-----------|
| Debounce persist to 2s (not useDebounce hook) | `useDebounce` is value-based and creates extra renders; ref+setTimeout is zero-render |
| `SetStateAction`-aware tracked setters | Handlers in `ws-messages/handlers.ts` use functional updaters `(prev) => ...`; simple value-only setters broke the type contract |
| `filter: drop-shadow()` over `box-shadow` | drop-shadow is compositor-friendly (GPU-composited), box-shadow triggers paint per frame |
| Keep `persistFnRef` for pagehide flush | Pagehide/visibilitychange needs immediate flush (not debounced) — ref avoids stale closure |
| Remove NeuralCanvas from 3 pages, keep on dashboard/settings | Dashboard and settings use ref-based canvas control (`stimulate`, `setIntensity`); sharing the app-shell canvas would require context refactor |
| `push + trim` over spread for log lines | `[...prev.slice(-(N-1)), entry]` copies entire array every message; `push` on trimmed array copies only when over limit |

## Files Modified

| File | Fix | Change |
|------|-----|--------|
| `hooks/use-pulse-persistence.ts` | 1 | Debounced auto-persist to 2s via `persistFnRef` + `persistTimerRef`; pagehide uses immediate ref flush |
| `hooks/use-ws-messages.ts` | 4a | Replaced 5 useEffect ref-syncs with `SetStateAction`-aware tracked setters |
| `hooks/use-split-pane.ts` | 4b | Replaced 4 useEffect ref-syncs with tracked setters; sync refs inside `toggleChat`/`toggleEditor` updaters |
| `hooks/use-pulse-chat.ts` | 4c | Replaced 1 useEffect ref-sync with `setChatHistoryTracked` supporting `SetStateAction` |
| `components/pulse/pulse-chat-pane.tsx` | 2a | rAF-throttled scroll handler with cleanup |
| `components/pulse/pulse-editor-pane.tsx` | 3 | `lastAppliedMarkdownRef` skip guard + 300ms debounced `countWords` |
| `components/pulse/pulse-workspace.tsx` | 5 | `transition-all` → `transition-[flex-basis,width]` (2 instances) |
| `components/pulse/tool-badge.tsx` | 8 | `transition-all` → `transition-[transform]` |
| `app/globals.css` | 8 | `box-shadow` keyframe → `filter: drop-shadow()` |
| `components/logs/logs-viewer.tsx` | 6,7 | Removed duplicate NeuralCanvas; added `@tanstack/react-virtual` virtualization; optimized array allocation |
| `app/agents/page.tsx` | 6 | Removed duplicate NeuralCanvas |
| `app/terminal/page.tsx` | 6 | Removed duplicate NeuralCanvas |
| `components/pulse/sidebar/types.ts` | bonus | Added missing `TagDef` + `TaggedItem` types (pre-existing build error) |

## Commands Executed

| Command | Purpose | Result |
|---------|---------|--------|
| `pnpm build` | Type check + production build | Clean (after fixing 3 type issues) |
| `pnpm lint` | Biome check | 32 warnings (all pre-existing `any` usage) |
| `git stash` / `git stash pop` | Verify `TagDef` error was pre-existing | Confirmed — same error on base branch |

## Behavior Changes (Before/After)

| Area | Before | After |
|------|--------|-------|
| Streaming persistence | `JSON.stringify` + `localStorage.setItem` per token (10-50x/sec) | Debounced to every 2s; immediate flush on pagehide |
| Ref-sync render cycles | 10 extra render cycles per state change across 4 hooks | Zero extra renders — refs synced atomically with state |
| Chat scroll handler | 4 setState + localStorage on every scroll event (up to 120Hz) | Throttled to ~60fps via requestAnimationFrame |
| Editor sync effect | Full `serializeMd()` AST traversal per streaming token | Skipped when markdown matches last applied value |
| Split pane drag | `transition-all` animated every CSS property during drag | Only `flex-basis` and `width` transition |
| Tool badge hover | `box-shadow` keyframe triggered paint per frame | `filter: drop-shadow()` — compositor-composited |
| Logs page | 2000 `<LogLine>` DOM nodes + 2 NeuralCanvas rAF loops | Virtualized (~60 visible) + 1 canvas (app-shell only) |
| Agents/Terminal pages | 2 NeuralCanvas rAF loops each | 1 canvas (app-shell only) |
| Log line buffer | `[...prev.slice(-(N-1)), entry]` every message | `push` on pre-trimmed array |

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `pnpm build` | Clean compilation | Clean compilation | PASS |
| `pnpm lint` | No new errors | 32 pre-existing warnings, 0 errors | PASS |
| TypeScript strict mode | All types resolve | All types resolve (after 3 fixes) | PASS |

## Risks and Rollback

- **Low risk:** All changes are surgical — no architectural changes, no new dependencies (except `@tanstack/react-virtual` which was already in `package.json`)
- **Rollback:** `git checkout feat/sidebar -- <file>` for any individual file, or `git stash` the entire changeset
- **Persistence debounce risk:** 2s debounce means up to 2s of state could be lost on browser crash (not tab close — pagehide catches that). Acceptable tradeoff vs. 10-50x/sec serialization.

## Decisions Not Taken

| Alternative | Why Rejected |
|-------------|-------------|
| Share app-shell NeuralCanvas with dashboard/settings via context | Dashboard and settings use ref-based canvas control (`stimulate`, `setIntensity`) — requires context provider refactor, save for follow-up |
| Use existing `useDebounce` hook for persistence | It's value-based (creates extra renders); ref+setTimeout is zero-render overhead |
| Ring buffer for log lines | Adds complexity; push+trim is sufficient — only copies when over limit |
| Agent team delegation | Two attempts with different agent types both failed (agents went idle); direct execution was faster and more reliable |

## Open Questions

- Manual browser testing still needed: streaming smoothness, scroll performance, drag resize, persistence across tab close, logs at 2000 lines, DevTools Performance profiling
- Dashboard and settings NeuralCanvas sharing (context refactor) deferred to follow-up

## Next Steps

1. Manual browser testing per the verification checklist in the plan
2. DevTools Performance recording during streaming to measure React commit frequency reduction
3. Consider NeuralCanvas context provider for dashboard/settings canvas sharing (follow-up)
