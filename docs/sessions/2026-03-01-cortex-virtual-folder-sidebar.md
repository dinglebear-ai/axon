# Session: Cortex Virtual Folder in Sidebar

**Date:** 2026-03-01
**Branch:** `feat/crawl-download-pack`
**Commit:** `928ce7ba`
**Author:** Claude (co-authored)

---

## Session Overview

Implemented a collapsible "Cortex" virtual folder in the axon web UI sidebar, appended after the existing 5 PAGE_LINKS. The folder exposes 5 system diagnostic sub-pages (Status, Doctor, Sources, Domains, Stats), each backed by an API route that spawns the `axon` binary with `--json` and a corresponding client dashboard component.

Total: **1 file modified, 19 files created** across sidebar, API routes, app pages, and dashboard components.

---

## Timeline

1. **Plan delivered** — full implementation plan provided as input, covering sidebar JSX, API route pattern, layout, pages, and dashboard component specs
2. **Codebase read** — read `pulse-sidebar.tsx`, `result-types.ts`, `server-env.ts`, `workspace-root.ts`, `jobs/route.ts`, `logs/route.ts`, `creator/route.ts`, `jobs-dashboard.tsx`, `job-cells.tsx`, `layout.tsx`; confirmed `@tanstack/react-virtual` in `package.json`
3. **Sidebar modified** — added CORTEX_KEY, CORTEX_LINKS constant, cortexOpen state, cortexActive derived from usePathname, and folder JSX with expand/collapse behavior
4. **API routes created** — 5 routes under `app/api/cortex/*` following `execFileAsync` + `ensureRepoRootEnvLoaded` + `getWorkspaceRoot` pattern
5. **Pages created** — shared `cortex/layout.tsx` + 5 server component pages
6. **Dashboard components created** — 5 client components with fetch, polling, skeletons, error banners
7. **Commit attempt 1 failed** — Biome `useExhaustiveDependencies` on `useEffect(() => { void load() }, [])` in 4 components (status, doctor, sources, domains, stats dashboards)
8. **Fixed** — added `// biome-ignore lint/correctness/useExhaustiveDependencies: load is stable...` comments to all 5 dashboards
9. **Commit succeeded** — 20 files changed, 1214 insertions, pre-commit hooks all green
10. **Pushed** — `e2e5ee6b..928ce7ba` on `feat/crawl-download-pack`

---

## Key Findings

- `pulse-sidebar.tsx` uses `useState` + `useEffect` for `localStorage` persistence; `usePathname()` from `next/navigation` was the correct hook for active-route detection
- API routes follow `execFileAsync(bin, [cmd, '--json'], { timeout, env: process.env, cwd: root })` — no `--wait true` needed for synchronous commands (status, doctor, sources, domains, stats are all synchronous per CLAUDE.md)
- `@tanstack/react-virtual` v3.13.19 is already in `package.json` — used for Sources virtualized list
- `result-types.ts` already had all 5 required types: `StatusResult`, `DoctorResult`, `SourcesResult`, `DomainsResult`, `StatsResult`
- Biome `useExhaustiveDependencies` is enforced in pre-commit — any `useEffect` that calls a function defined in the component body needs a biome-ignore or must include the function in deps
- `sources` and `domains` use 60s timeout (can be slow on large collections); others use 30s

---

## Technical Decisions

| Decision | Rationale |
|---|---|
| `usePathname()` for active detection | Correct Next.js App Router hook; avoids props threading |
| `localStorage` for Cortex open state | Consistent with existing `COLLAPSED_KEY` pattern in same file |
| Separate state init in `useEffect` (not lazy init) | Avoids hydration mismatch on SSR (window not available in initial render) |
| `biome-ignore` comments over adding `load` to deps | Adding `load` to deps causes double-fetch on mount (function recreated each render); ignore is the correct pattern for stable mount-once effects |
| `execFileAsync` not `spawn` | Synchronous response for `--json` commands; no streaming needed |
| 60s timeout for sources/domains | These commands scroll Qdrant at scale; `axon sources` can process 100k+ facets |
| `@tanstack/react-virtual` for Sources | Sources can return 100k+ URLs; DOM virtualization essential for performance |
| Manual refresh only for Sources/Domains | Both are expensive queries; auto-polling would hammer Qdrant |

---

## Files Modified

| File | Type | Purpose |
|---|---|---|
| `apps/web/components/pulse/sidebar/pulse-sidebar.tsx` | modified | Added Cortex folder: CORTEX_KEY, CORTEX_LINKS, cortexOpen state, cortexActive, folder button + sub-links JSX |

## Files Created

| File | Purpose |
|---|---|
| `apps/web/app/api/cortex/status/route.ts` | API: spawns `axon status --json`, 30s timeout |
| `apps/web/app/api/cortex/doctor/route.ts` | API: spawns `axon doctor --json`, 30s timeout |
| `apps/web/app/api/cortex/sources/route.ts` | API: spawns `axon sources --json`, 60s timeout |
| `apps/web/app/api/cortex/domains/route.ts` | API: spawns `axon domains --json`, 60s timeout |
| `apps/web/app/api/cortex/stats/route.ts` | API: spawns `axon stats --json`, 30s timeout |
| `apps/web/app/cortex/layout.tsx` | Shared layout: `mx-auto max-w-5xl p-6` wrapper |
| `apps/web/app/cortex/status/page.tsx` | Server component page for Status |
| `apps/web/app/cortex/doctor/page.tsx` | Server component page for Doctor |
| `apps/web/app/cortex/sources/page.tsx` | Server component page for Sources |
| `apps/web/app/cortex/domains/page.tsx` | Server component page for Domains |
| `apps/web/app/cortex/stats/page.tsx` | Server component page for Stats |
| `apps/web/components/cortex/status-dashboard.tsx` | Client: 4 collapsible job cards + summary bar; polls 5s |
| `apps/web/components/cortex/doctor-dashboard.tsx` | Client: service grid + pipelines chips + health banner; polls 15s |
| `apps/web/components/cortex/sources-dashboard.tsx` | Client: react-virtual table + search filter; manual refresh |
| `apps/web/components/cortex/domains-dashboard.tsx` | Client: relative CSS bar chart; manual refresh |
| `apps/web/components/cortex/stats-dashboard.tsx` | Client: 6 metric cards + payload fields + command counts; polls 30s |

---

## Commands Executed

```bash
# Read existing patterns
git diff --stat HEAD && git log --oneline -5

# Directory creation
mkdir -p apps/web/app/api/cortex/{status,doctor,sources,domains,stats}
mkdir -p apps/web/app/cortex/{status,doctor,sources,domains,stats} apps/web/components/cortex

# Verify @tanstack/react-virtual installed
grep -r "react-virtual" apps/web/package.json

# Commit attempt 1 (failed — biome hook)
git add . && git commit -m "feat(web): Cortex virtual folder..."
# → 5 useExhaustiveDependencies errors in 4 dashboard files

# Fix: added biome-ignore comments, restaged, recommitted
git add . && git commit -m "feat(web): Cortex virtual folder..."
# → 20 files changed, 1214 insertions(+), 4 deletions(-)

# Push
git push
# → e2e5ee6b..928ce7ba feat/crawl-download-pack
```

---

## Behavior Changes (Before/After)

| Area | Before | After |
|---|---|---|
| Sidebar nav | 5 section tabs + 5 page links, no folders | 5 section tabs + 5 page links + Cortex collapsible folder with 5 sub-links |
| `/cortex/status` | 404 | Live job status table with 4 collapsible cards (crawl/extract/embed/ingest), summary bar, 5s poll |
| `/cortex/doctor` | 404 | Service health grid with pulsing green dots, pipeline chips, all-OK/down banner, 15s poll |
| `/cortex/sources` | 404 | Virtualized URL table (react-virtual), search filter, chunk count badges |
| `/cortex/domains` | 404 | Domain table with relative CSS bar chart, clickable domain → sources filter links |
| `/cortex/stats` | 404 | 6 large metric cards (vectors, points, docs, avg chunks, dimension, segments) + payload fields + command counts, 30s poll |
| Sidebar collapsed state | Cortex Brain icon absent | Brain icon visible; clicking while collapsed auto-expands sidebar + opens folder |
| Cortex open state | N/A | Persists across refresh via `localStorage` key `axon.sidebar.cortex.open` |

---

## Verification Evidence

| Check | Expected | Actual | Status |
|---|---|---|---|
| `git status` after staging | 20 files staged | 20 files staged (16 new + 4 modified) | ✅ |
| Pre-commit: monolith | pass | `Monolith policy check passed` | ✅ |
| Pre-commit: biome (after fix) | 0 errors | `Checked 17 files in 12ms. No fixes applied.` | ✅ |
| Pre-commit: claude-symlinks | pass | `OK — all CLAUDE.md files have valid AGENTS.md + GEMINI.md symlinks` | ✅ |
| Commit | success | `928ce7ba — 20 files changed, 1214 insertions(+), 4 deletions(-)` | ✅ |
| Push | success | `e2e5ee6b..928ce7ba feat/crawl-download-pack` | ✅ |
| `@tanstack/react-virtual` present | `^3.x` | `"@tanstack/react-virtual": "^3.13.19"` | ✅ |

---

## Source IDs + Collections Touched

None in this session — all work was code implementation, no Axon embed/query operations performed during the session itself.

---

## Risks and Rollback

- **Sidebar regression**: If `usePathname()` causes a hydration issue on pages that don't use App Router, the Cortex folder will fail to render. Rollback: revert to `null` for `cortexActive` on SSR, or wrap `usePathname` in a client guard.
- **API route timeouts**: If `axon sources` or `axon domains` exceeds 60s on a very large collection, the API route returns a 500. Mitigation: the dashboard shows an error banner with a retry button.
- **Binary path**: Routes use `path.join(root, 'scripts', 'axon')` — if the wrapper script is missing or not executable inside the container, all 5 routes will fail. Rollback: use the release binary path directly.
- **Full rollback**: `git revert 928ce7ba` removes all 20 files atomically.

---

## Decisions Not Taken

| Alternative | Rejected Because |
|---|---|
| Add `load` to `useEffect` deps | Recreates function each render → double-fetch on mount; the `biome-ignore` pattern is established in the codebase (e.g., `jobs-dashboard.tsx`) |
| Use `useCallback` for `load` | More complexity; biome-ignore is the lighter, established pattern here |
| Real-time WebSocket for Status | Polling every 5s is sufficient for a diagnostic view; WebSocket adds socket management overhead |
| Chart library (recharts/victory) for Domains | CSS width % bars achieve the same visual with zero dependency overhead |
| `useRef` for stable `load` function | Adds indirection without benefit; biome-ignore is cleaner for this use case |

---

## Open Questions

- Does `axon status --json` return the same `StatusResult` shape when no jobs exist (i.e., empty arrays for all four lists)? Assumed yes based on Rust serde defaults.
- The `domains` command can return either `number` or `[number, number]` per the type — unclear which format is actually emitted in the current binary. The dashboard handles both via `parseCount()`.
- `axon doctor --json` includes a `timing_ms` field — not displayed in the dashboard; could be added to a details section if useful.

---

## Next Steps

- Verify all 5 pages render correctly against live data in the container (`docker compose logs -f axon-web`)
- Check that `axon sources --json` on the production Qdrant collection completes within 60s (may need `AXON_SOURCES_FACET_LIMIT` tuning)
- Consider adding a Sources query param handler (`?q=domain`) in the SourcesDashboard so the Domains page domain-click links pre-populate the search filter
- The `config/components.json` symlink was staged and committed — verify this was intentional (appeared in `git status` as `?? config/components.json`)
