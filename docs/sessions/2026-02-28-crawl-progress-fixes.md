# Session: Crawl Progress Visibility + MCP Cache Fixes
**Date:** 2026-02-28
**Branch:** `feat/crawl-download-pack`

---

## Session Overview

Bulk-enqueued 54 crawl jobs across cannabis industry, beverage trade, analytics, legal, and South Carolina local/government domains. Diagnosed a false "stalled job" perception caused by per-page progress throttling and MCP artifact caching for status-type commands. Implemented three targeted fixes: per-page DB progress writes, Chrome fallback progress streaming, and inline (non-cached) MCP responses for all status/list/doctor/domains/sources endpoints.

---

## Timeline

| Time (UTC) | Activity |
|---|---|
| ~01:08 | Started 54 crawl jobs in 5 parallel batches via MCP |
| ~01:09 | First status check — 1 completed, 2 running, 51 pending |
| ~01:13 | Second status check — same SHA256 artifact; appeared stalled |
| ~01:13 | Docker logs confirmed mjbizdaily.com actively fetching hundreds of pages |
| ~01:14 | Diagnosed two root causes: throttled progress writes + MCP artifact caching |
| ~01:20 | Implemented all three fixes; `cargo check` clean |

---

## Crawl Jobs Enqueued

**54 total jobs across 5 batches:**

| Batch | Category | Count | Sample URLs |
|---|---|---|---|
| 1 | Cannabis industry news | 10 | mjbizdaily.com, hempindustrydaily.com, cannabiswire.com |
| 2 | Beverage / C-store trade | 10 | bevnet.com, fooddive.com, brewbound.com |
| 3 | Analytics + advocacy | 9 | bdsa.com, headset.io, newfrontierdata.com |
| 4 | Legal / events | 9 | natlawreview.com, foxrothschild.com, mjbizconference.com |
| 5 | SC local + government | 16 | postandcourier.com, scstatehouse.gov, legiscan.com/SC |

---

## Key Findings

1. **Progress throttle** — `collector.rs` only sent `CrawlSummary` to the DB every 5 pages (`is_multiple_of(5)`), causing multi-minute gaps in DB visibility for fast-crawling sites.

2. **Chrome fallback blind spot** — `process.rs:run_primary_with_optional_chrome_fallback` passed `None` for `progress_tx` on the Chrome retry pass, so no progress was written during Chrome crawls at all.

3. **MCP artifact caching illusion** — `respond_with_mode` writes a fixed-name file (e.g. `crawl-list.json`) and returns its SHA256. When polled again, if DB content was unchanged, the SHA256 matched the previous response — making it look like a stale cache hit when it was actually a fresh DB query returning identical data.

4. **`doctor` and `stats` already inline** — `handle_doctor` and `handle_stats` in `server.rs` already used `AxonToolResponse::ok` (no artifact), so they were not affected.

5. **`list_jobs` creates a new PgPool per call** — `db.rs:207` calls `make_pool(cfg).await?` on every `list_jobs` invocation. Not the cause of staleness but worth noting as a potential optimization.

---

## Technical Decisions

- **Every page, not every N** — Changed progress from `is_multiple_of(5)` to unconditional. At 20–50 pages/sec, this means 20–50 Postgres writes/sec just for progress, but gives real-time visibility. Acceptable tradeoff for the crawl worker workload.

- **Clone `progress_tx` for HTTP pass** — `Sender<T>` is cheap to clone. Clone it for the HTTP probe, pass the original to Chrome fallback. Both passes now stream to the same DB progress task.

- **Inline for status endpoints, not "fix caching"** — The real fix isn't cache invalidation (there was no cache); it's removing the SHA256 artifact pattern entirely from status-type responses so callers see live data directly. `AxonToolResponse::ok` returns inline JSON with no file I/O.

- **Keep `response_mode` in schema** — `DomainsRequest` and `SourcesRequest` have `#[serde(deny_unknown_fields)]`, so removing `response_mode` would break existing callers sending it. Field kept, annotated `#[allow(dead_code)]`.

---

## Files Modified

| File | Change | Purpose |
|---|---|---|
| `crates/crawl/engine/collector.rs` | Removed 4x `is_multiple_of(5)` guards | Progress fires on every page |
| `crates/jobs/crawl/runtime/worker/process.rs` | `progress_tx.clone()` for HTTP pass; `Some(progress_tx)` for Chrome pass | Progress streams through both render modes |
| `crates/mcp/server.rs` | `crawl status` + `crawl list` → `AxonToolResponse::ok`; `_response_mode` prefix; `domains`/`sources` → inline | No artifact caching on status commands |
| `crates/mcp/schema.rs` | `#[allow(dead_code)]` on `DomainsRequest.response_mode` + `SourcesRequest.response_mode` | Suppress warnings without breaking API compat |

---

## Behavior Changes (Before / After)

| Endpoint | Before | After |
|---|---|---|
| `crawl status <id>` | Writes `.cache/axon-mcp/crawl-status-{id}.json`, returns SHA256 | Returns live inline JSON directly |
| `crawl list` | Writes `.cache/axon-mcp/crawl-list.json`, returns SHA256 | Returns live inline JSON directly |
| `domains` | Writes `.cache/axon-mcp/domains.json`, returns SHA256 | Returns live inline JSON directly |
| `sources` | Writes `.cache/axon-mcp/sources.json`, returns SHA256 | Returns live inline JSON directly |
| Per-page DB progress | Written every 5 pages | Written every page |
| Chrome fallback progress | No progress written during Chrome pass | Progress streams through Chrome pass |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---|---|---|---|
| `cargo check --bin axon --bin axon-mcp` | Zero warnings, zero errors | 0 warnings, 0 errors, `Finished` | ✅ PASS |
| `crawl list` MCP call | Inline JSON, no SHA256 field | `AxonToolResponse::ok` with live data | ✅ PASS (code review) |
| Docker logs during crawl | Active fetch lines | Hundreds of `fetch https://mjbizdaily.com/...` per second | ✅ CONFIRMED |

---

## Code Locations

- Progress send sites: `collector.rs:86`, `collector.rs:95`, `collector.rs:144`, `collector.rs:177`
- Chrome fallback: `process.rs:323-367` (`run_primary_with_optional_chrome_fallback`)
- MCP status handler: `server.rs:460-472` (`CrawlSubaction::Status`)
- MCP list handler: `server.rs:484-503` (`CrawlSubaction::List`)
- MCP domains handler: `server.rs:1456-1469` (`handle_domains`)
- MCP sources handler: `server.rs:1471-1484` (`handle_sources`)

---

## Risks and Rollback

- **Higher Postgres write rate** — Every-page progress at high crawl concurrency means more DB writes. If Postgres becomes a bottleneck, restore `is_multiple_of(N)` with a larger N (e.g. 25) or switch to time-gated updates.
- **Large inline responses** — `crawl list` with 200 jobs returns ~80KB inline. MCP clients with small context windows may struggle. If this becomes an issue, cap the default list limit or re-introduce artifact mode behind an explicit `response_mode=path` opt-in.
- **Rollback**: `git revert` the 4 changed files. No schema migrations, no infrastructure changes.

---

## Decisions Not Taken

| Option | Rejected Because |
|---|---|
| Time-gated progress (write if >Ns since last) | More complex, harder to reason about under load |
| Re-introduce caching with TTL for list/status | Defeats the purpose; staleness window always wrong for status-type data |
| Remove `response_mode` from `DomainsRequest`/`SourcesRequest` schemas | Would break `deny_unknown_fields` for existing callers |
| Increase `is_multiple_of` to 25 instead of 1 | User explicitly asked for per-page; can tune back if needed |

---

## Open Questions

- Does per-page Postgres write rate cause contention under `extreme`/`max` perf profiles at full concurrency?
- Should `extract status`, `embed status`, `ingest status` also be changed to inline? Currently they still use `respond_with_mode`.
- The 54 crawl jobs are still running — no completion counts available yet.

---

## Next Steps

1. `docker compose up -d --build axon-workers` — rebuild workers to pick up progress + Chrome fixes
2. `cargo build --release --bin axon-mcp` — rebuild MCP binary for inline status responses
3. Monitor crawl completions: `axon crawl list --limit 60` should now show live page counts
4. Consider applying the same inline treatment to `extract`, `embed`, and `ingest` status/list handlers
