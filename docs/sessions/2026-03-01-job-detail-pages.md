# Session: Job Detail Pages `/jobs/[id]`
Date: 2026-03-01
Branch: feat/crawl-download-pack
Commit: 394917d5

## Session Overview

Implemented clickable job detail pages at `/jobs/[id]`. Previously, the `/jobs` dashboard listed jobs in a table but had no way to drill into a specific job. Now every job row is a link; clicking navigates to a full detail page showing status, crawl statistics, timing, configuration, and raw result JSON, with live 3-second polling for running jobs.

Also completed a prior-session pending task: recreated the `axon-web` container to pick up the new `/axon-output:ro` volume mount added in the docs knowledge base session, confirming the `/docs` page now has access to scraped content.

## Timeline

1. **Screenshot review** — checked latest screenshot (`Screenshot From 2026-03-01 03-04-54.png`); showed Pulse workspace with gofastmcp.com crawl in progress, not the `/docs` page. User asked: "Can we setup pages for individual jobs? So like it kicked back the jobid to us — could we make it so that I could click that jobID in the doc and it take us to a page for that specific job?"

2. **Container recreation** — `axon-web` had not been recreated after the docs session's docker-compose volume mount addition. Ran stop→rm→create→start. Confirmed `/axon-output` now visible with manifests and readable as `node` user.

3. **Database schema inspection** — queried `axon_crawl_jobs` for `result_json` and `config_json` shapes. Found rich result data: `pages_crawled`, `pages_discovered`, `md_created`, `thin_md`, `elapsed_ms`, `output_dir`, `cache_hit`, `audit_diff`. Embed jobs have `docs_embedded`, `chunks_embedded`.

4. **API route** — created `/api/jobs/[id]/route.ts`: queries all 4 tables sequentially (crawl → embed → extract → ingest), first match wins. Returns `JobDetail` interface with flattened fields from both `result_json` and `config_json`.

5. **Detail page** — created `/app/jobs/[id]/page.tsx`: stat grid for crawl/embed, timing section, config section, raw result JSON pre block, error display. Running jobs auto-poll via `setInterval(fetchJob, 3000)` cleared on unmount/status-change.

6. **Dashboard link** — updated `jobs-dashboard.tsx`: target cell changed from `<span>` to `<Link href="/jobs/${job.id}">` with `ExternalLink` icon that fades in on hover.

7. **Push** — committed `394917d5`, pushed to `feat/crawl-download-pack`.

## Key Findings

- `result_json` for crawl jobs contains: `phase`, `pages_crawled`, `pages_discovered`, `md_created`, `thin_md`, `filtered_urls`, `elapsed_ms`, `output_dir`, `cache_hit`, `audit_diff` (`apps/web/app/api/jobs/[id]/route.ts:55-85`)
- `config_json` for crawl jobs contains: `collection`, `render_mode`, `max_depth`, `max_pages`, `embed`, `delay_ms`, `fetch_retries`, etc. (`apps/web/app/api/jobs/[id]/route.ts:58-68`)
- Embed job `result_json` contains: `collection`, `docs_embedded`, `chunks_embedded`, `input`, `source` (`apps/web/app/api/jobs/[id]/route.ts:96-103`)
- Extract jobs store URLs as `urls_json` JSONB array; `result_json` varied by implementation
- `/axon-output` manifests confirmed readable by `node` user — files written by `axon` user are world-readable (mode 644 or group-readable via docker group_add)
- API response for `21643f31` (completed gofastmcp.com crawl): `pagesCrawled: 105`, `pagesDiscovered: 390`, `mdCreated: 390`, `elapsedMs: 2643`

## Technical Decisions

| Decision | Rationale | Alternative Rejected |
|---|---|---|
| Sequential table search (crawl → embed → extract → ingest) | Simple, correct; UUIDs are globally unique across tables | Parallel queries with `Promise.all` — unnecessary round trips when first table usually matches |
| `JobDetail` interface flattens result_json + config_json | Consumer (page component) doesn't need to parse nested JSON | Pass raw JSON to page — would require parsing logic in UI |
| Poll interval 3s for running jobs | Matches axon worker heartbeat cadence; responsive without hammering DB | SSE/WebSocket — overkill for a single-page status viewer |
| `use(params)` for dynamic route params | Next.js 15 App Router requires `params` to be awaited as a Promise | Destructuring params directly — deprecated pattern, causes warnings |
| ExternalLink icon opacity-0 → hover:opacity-100 | Signals clickability without cluttering the table | Always-visible icon — adds visual noise to every row |

## Files Modified

| File | Type | Purpose |
|---|---|---|
| `apps/web/app/api/jobs/[id]/route.ts` | Created | REST endpoint: finds job by UUID across all 4 tables, returns `JobDetail` |
| `apps/web/app/jobs/[id]/page.tsx` | Created | Job detail page: stat grid, timing, config, result JSON, live polling |
| `apps/web/components/jobs/jobs-dashboard.tsx` | Modified | Target cell → `<Link>` to `/jobs/[id]`; added `ExternalLink` hover icon |
| `CHANGELOG.md` | Modified | Added `394917d5` entry; added `ac294073` (docs page, previously undocumented) |

## Commands Executed

```bash
# Container recreation for volume mount
docker stop axon-web && docker rm axon-web && \
  docker compose -f docker-compose.yaml create axon-web && docker start axon-web

# Verify /axon-output is mounted and readable
docker exec axon-web ls /axon-output
# → domains  jobs

docker exec axon-web find /axon-output -name "manifest.jsonl" | head -5
# → /axon-output/domains/whitneyeconomics.com/latest/manifest.jsonl ...

docker exec axon-web sh -c "cat /axon-output/domains/gofastmcp.com/latest/manifest.jsonl | head -2"
# → {"url":"https://gofastmcp.com/","relative_path":"markdown/0001-...","markdown_chars":5204,...}

# Database schema probe
docker exec axon-postgres psql -U axon -d axon -c \
  "SELECT id, url, status, result_json, config_json FROM axon_crawl_jobs ORDER BY created_at DESC LIMIT 2;"

# API smoke test
curl -s "http://localhost:49010/api/jobs/21643f31-b712-47ad-8e6c-95e401cb0d66" | python3 -m json.tool | head -30
# → {"id":"21643f31...","type":"crawl","status":"completed","pagesCrawled":105,...}

# Push
git add apps/web/app/api/jobs/\[id\]/ apps/web/app/jobs/\[id\]/ \
  apps/web/components/jobs/jobs-dashboard.tsx CHANGELOG.md
git commit -m "feat(web): /jobs/[id] detail page ..."
git push
# → ac294073..394917d5  feat/crawl-download-pack -> feat/crawl-download-pack
```

## Behavior Changes (Before/After)

| Area | Before | After |
|---|---|---|
| `/jobs` table target cell | Plain text span, not interactive | `<Link>` to `/jobs/[id]`; ExternalLink icon on hover |
| Job detail page | Did not exist — no way to see full job stats | `/jobs/[id]` shows type, status, target, stats grid, timing, config, raw JSON |
| Running job view | No way to monitor progress | Detail page auto-polls every 3s, shows "Auto-refreshing…" indicator |
| `/axon-output` mount | Container recreated with new volume | `/axon-output` readable; `/docs` page can now list scraped content |
| `ac294073` in CHANGELOG | Undocumented commit | Added to commit table |

## Verification Evidence

| Check | Expected | Actual | Status |
|---|---|---|---|
| `docker exec axon-web ls /axon-output` | `domains jobs` | `domains jobs` | ✅ |
| `curl /api/jobs/21643f31...` | 200 with `type: crawl` | 200, `pagesCrawled: 105, pagesDiscovered: 390` | ✅ |
| `docker logs axon-web \| tail -3` | Compiled, no errors | `✓ Compiled in 175ms`, then `GET /api/jobs/21643f31... 200 in 859ms` | ✅ |
| `git push` | Push to feat/crawl-download-pack | `ac294073..394917d5 feat/crawl-download-pack` | ✅ |
| Biome pre-commit hook | Pass | `Checked 3 files in 19ms. No fixes applied.` | ✅ |

## Source IDs + Collections Touched

None — this session did not perform any Axon embed/query/scrape operations. Session doc will be embedded as part of the save-to-md workflow.

## Risks and Rollback

- **Sequential table search**: If a UUID collides across tables (effectively impossible with UUID v4), the first match wins silently. Not a practical risk.
- **Live polling open on navigate-away**: `setInterval` is cleared in the `useEffect` cleanup when `job.status !== 'running'` or on unmount. No leak risk.
- **Rollback**: `git revert 394917d5` removes the detail page and reverts the dashboard link to a plain span.

## Decisions Not Taken

- **Server-Side Rendering for job detail**: Would require passing params to a server component with DB access. Chose client-side fetch for simplicity and to enable live polling without additional server infrastructure.
- **Inline job expansion in `/jobs` table**: Accordion-style expand within the row. Rejected — complex layout, harder to share/bookmark a specific job.
- **Cancel button on detail page**: DB has cancel infrastructure (`axon_crawl_jobs` + Redis key). Not wired to UI yet — left for a future session.
- **Progress bar on crawl jobs**: `pages_crawled / pages_discovered` ratio available but `pages_discovered` grows during a crawl (not a fixed total), so a progress bar would be misleading. Left as plain counts.

## Open Questions

- Does the `/docs` page fully populate now that the container was recreated? Not verified via screenshot — user should navigate to `https://axon.tootie.tv/docs` to confirm.
- Are there embed jobs that need to be linked to their parent crawl job? Currently the detail pages are independent — no cross-linking between a crawl job and the embed job spawned from it.
- The `elapsedMs` field comes from `result_json.elapsed_ms` which is only set on completion. For running crawl jobs, duration is computed from `startedAt` to `Date.now()`. This approximation may diverge if the worker pauses.

## Next Steps

- Navigate to `https://axon.tootie.tv/docs` to verify `/docs` page shows indexed domains
- Navigate to `https://axon.tootie.tv/jobs` and click a job row to verify detail page loads
- Consider adding a "Cancel" button to the detail page for running jobs (Redis key `axon:crawl:cancel:{id}`)
- Consider linking crawl job → embed job on the detail page (join via `output_dir` matching embed `input`)
