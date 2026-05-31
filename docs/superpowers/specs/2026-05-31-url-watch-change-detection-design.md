# URL Change-Detection Watch — Design Spec

Date: 2026-05-31
Status: Approved (brainstorming) — pending implementation plan
Branch: `feat/watch-scheduler`
Related: builds directly on the watch scheduler (`src/jobs/workers/watch_scheduler.rs`, `src/jobs/watch.rs`) added in v4.15.0.

## Summary

Upgrade the `watch` action from a stateless periodic scraper into a **URL change
detector that crawls only when content changes**. A watch remains a recurring
scheduler entry; on each tick it determines which of its URL(s) changed since the
last check and enqueues a crawl for the changed ones. There is **no rename** to
`schedule` — "watch" now carries its literal meaning (watch a URL for changes).

This replaces the current stateless `refresh` task (which always reports
`changed: 0`) with a stateful, change-aware `watch` task.

## Goals

- Detect content changes on watched URLs efficiently (cheap when unchanged).
- Crawl/re-index only the changed subtrees, not everything, every tick.
- Avoid redundant re-crawling of the same pages across ticks and across
  multiple changed URLs in one watch.
- Reuse the existing scheduler, crawl job runtime, in-process crawl worker, and
  auto path-prefix scoping. No new long-running subsystems.

## Non-Goals

- No rename of the `watch` command/routes/tables to `schedule`.
- No multi-seed crawl-engine changes (coalescing is done by clustering + a
  single common-ancestor seed per cluster, not by teaching one crawl job to take
  many seeds).
- No diff rendering / change notifications in v1 (only crawl-on-change). A
  `diff`/notify layer can come later.
- No `refresh` backward-compat alias (the scheduler is unreleased; `refresh`
  watches are not a deployed surface worth preserving).

## Decisions (locked during brainstorming)

| Question | Decision |
|---|---|
| Relationship to scheduler | One feature: a watch **is** a change-detecting scheduled job. No `schedule` rename. |
| Change signal | **Hybrid**: conditional request first (ETag/Last-Modified); on 200/first-seen, fall back to content hash of normalized markdown. |
| Action on change | **Full crawl job** seeded from the changed URL(s). |
| First run (no prior state) | **Seed**: treat first-seen as changed → crawl once, then watch. |
| URLs per watch | **Multiple** (`urls` array retained). |
| Coalescing changed URLs | **Group by common path prefix**; one crawl per cluster seeded at the common ancestor. |
| In-flight behavior | **Skip new crawl** for a cluster whose previous crawl is still pending/running; keep probing/hashing/updating state. |
| task_type | `watch` replaces `refresh`; `SUPPORTED_TASK_TYPES = ["watch"]`. |

## Architecture

### Per-tick flow (executed by the scheduler for each leased due watch)

For each `url` in the watch's `task_payload.urls`:

1. **Conditional probe** (`conditional_probe`): issue an HTTP GET carrying
   `If-None-Match` (stored `etag`) and/or `If-Modified-Since` (stored
   `last_modified`).
   - `304 Not Modified` → **Unchanged**. Skip scrape/hash/crawl entirely. Update
     `last_checked_at`. (Cheap path — no Chrome, no embed.)
   - `200` / no stored validators / first-seen → proceed to hashing. Capture any
     fresh `ETag`/`Last-Modified` from the response for storage.
   - Network error / non-2xx-non-304 → record per-URL error, leave prior state
     intact, no crawl.
2. **Content hash**: run the existing scrape pipeline for the URL, normalize the
   markdown (trim, collapse intra-line whitespace, normalize line endings),
   compute SHA-256.
   - No prior `content_hash` (first-seen) → **Changed (seed)**.
   - Hash differs from stored → **Changed**.
   - Hash equal → **Unchanged** (store the rotated validators so future probes
     can 304).
3. **Persist** the new per-URL state row: `etag`, `last_modified`,
   `content_hash`, `last_checked_at`, and `last_changed_at` (only bumped when
   changed).

After all URLs are classified:

4. **Cluster** the changed URLs by common path prefix (`group_by_common_prefix`).
5. For each cluster, apply the **in-flight guard**: if the cluster's members'
   `last_crawl_job_id` references a crawl job still in `pending`/`running`, skip
   (record `skipped — crawl in flight`). Otherwise **enqueue one crawl job**
   seeded at the cluster's common-ancestor URL, with `max_depth` from the watch
   payload (default 2). Write the new crawl job id to `last_crawl_job_id` for
   every member URL.
6. **Finalize the run**: `result_json` records per-URL outcomes
   (`changed`/`unchanged`/`skipped`/`error`), the clusters, and the dispatched
   crawl job ids. The run's `dispatched_job_id` is set to the first/primary
   crawl job id (or null if none). The scheduler's existing reschedule logic
   advances `next_run_at` and clears the lease.

### Components (each small, single-purpose, unit-testable)

| Unit | File | Responsibility | Depends on |
|---|---|---|---|
| Conditional probe | `src/core/http/conditional.rs` | `conditional_probe(url, etag, last_mod) -> Probe` where `Probe = NotModified | Modified { etag, last_modified } | Failed(String)`. Pure HTTP; no scrape. | core HTTP client, `validate_url` (SSRF guard) |
| URL state store | `src/jobs/watch/url_state.rs` | CRUD over `axon_watch_url_state` (get/upsert per `(watch_id, url)`; read `last_crawl_job_id`). | sqlx pool |
| Change detector | `src/jobs/watch/change_detect.rs` | Orchestrate probe → scrape+hash → decision for one URL; return `UrlOutcome { state, changed }`. | conditional probe, `services::scrape`, url_state |
| Prefix clusterer | `src/jobs/watch/cluster.rs` | `group_by_common_prefix(&[Url]) -> Vec<Cluster { seed, members }>`. Pure function. | url parsing only |
| Crawl dispatcher | inline in watch task | Per cluster: in-flight check via `job_status`, else `enqueue_job(JobPayload::Crawl)`; record job id. | `jobs::ops::enqueue`, job status query |
| Task driver | `src/jobs/watch.rs::run_watch_task` (rewritten) | Drive the above for task_type `watch`; build `result_json`. | all of the above |

`run_watch_now_with_pool` keeps its current shape (create run → run task → finalize once, with the COMPLETED-write guard). Only `run_watch_task` changes: the `"refresh"` arm becomes the `"watch"` arm implementing the flow above. The crawl enqueue needs access to the job runtime; the scheduler already holds the shared `pool`, so dispatch uses the low-level `jobs::ops::enqueue_job(pool, JobPayload::Crawl { url, config_json })`, which the in-process crawl worker (already spawned under `serve`/`mcp`) drains.

### Clustering algorithm (`group_by_common_prefix`)

Deterministic, pure, no I/O:

1. Partition changed URLs by `(scheme, host)`.
2. Within a host partition, sort by path and greedily cluster URLs that share a
   directory ancestor: two URLs join the same cluster if one's directory path is
   a prefix of the other's, or they share a common directory ancestor of depth
   ≥ 1 (i.e. they live under a common `…/segment/` subtree, not just `/`).
3. Each cluster's **seed** = the longest common directory prefix of its members
   (falling back to the single URL when a cluster has one member). The seed is a
   real URL ending in `/` at the common ancestor.
4. Root-only commonality (only `/` shared) does **not** merge — those URLs form
   separate single-member clusters, so we never seed a whole-site crawl from a
   coincidental host match.

Heavily unit-tested with fixtures: same dir, nested dirs, sibling subtrees,
different hosts, single URL, root-only.

## Data Model

New migration `src/jobs/migrations/0003_create_watch_url_state.sql` (additive —
no changes to `0002`, which is already applied in production):

```sql
CREATE TABLE IF NOT EXISTS axon_watch_url_state (
    watch_id          TEXT NOT NULL,
    url               TEXT NOT NULL,
    etag              TEXT,
    last_modified     TEXT,
    content_hash      TEXT,
    last_checked_at   INTEGER,
    last_changed_at   INTEGER,
    last_crawl_job_id TEXT,
    PRIMARY KEY (watch_id, url),
    FOREIGN KEY (watch_id) REFERENCES axon_watch_defs(id) ON DELETE CASCADE
);
```

`axon_watch_defs` / `axon_watch_runs` / `axon_watch_run_artifacts` are unchanged.
`dispatched_job_id` on `axon_watch_runs` carries the primary crawl job id;
full per-cluster job ids live in `result_json`.

### `task_payload` shape for a `watch`

```json
{
  "urls": ["https://docs.example.com/guide/intro"],
  "max_depth": 2
}
```

- `urls` (required, non-empty array of valid http(s) URLs).
- `max_depth` (optional, default 2) — depth bound for change-triggered crawls.

## API / Surface Changes

- `validate_task_type`: supported set becomes `["watch"]`. Clear error for
  `refresh` and anything else, shared by CLI + both HTTP create handlers (already
  centralized).
- CLI: `axon watch create <name> --task-type watch --every-seconds N --task-payload '{"urls":[...]}'`.
  No new subcommands. `watch list/history/run-now` unchanged.
- HTTP `/v1/watch*`: unchanged routes; the create validators reject non-`watch`
  task types via the shared validator. REST tests that assert
  `unsupported task_type` keep passing (value updated to `watch`).
- Env: existing `AXON_WATCH_TICK_SECS` / `AXON_WATCH_LEASE_SECS` unchanged. No
  new env vars (lease TTL must still exceed a single tick's probe+scrape time —
  documented).

## Error Handling

- **Probe failure / network error**: record per-URL `error` in `result_json`,
  leave prior state intact (no false "changed"), do not crawl that URL. The run
  as a whole still completes.
- **Scrape failure on a 200**: same as probe failure — per-URL error, state
  unchanged, no crawl.
- **Enqueue failure**: record per-cluster error in `result_json`; other clusters
  still dispatch. `last_crawl_job_id` only written on successful enqueue.
- **COMPLETED finalize write failure**: existing best-effort FAILED fallback in
  `run_watch_now_with_pool` still applies.
- **In-flight crawl job not found** (e.g. cleaned up): treat as not-in-flight and
  allow a new crawl.
- **SSRF**: `conditional_probe` runs URLs through the existing `validate_url`
  guard before fetching, same as scrape/crawl.

## Testing

Unit (no live services):
- `cluster.rs`: same-dir, nested, sibling-subtree, different-host, single-URL,
  root-only-no-merge fixtures.
- `change_detect.rs`: decision matrix — 304→unchanged; 200+equal-hash→unchanged
  +validators stored; 200+diff-hash→changed; first-seen→changed(seed); error→
  state preserved. Probe + scrape mocked/injected.
- `url_state.rs`: upsert/get round-trip; FK cascade on watch delete (SQLite temp).
- Markdown normalization + hash stability (whitespace-only diff → same hash).

Integration (SQLite temp DB; scrape/probe stubbed or `example.com`):
- First run on a fresh watch enqueues a seed crawl and writes baseline state.
- Second run with identical content enqueues nothing (unchanged).
- In-flight guard: with a `pending` crawl job recorded, a changed URL records
  `skipped — crawl in flight` and enqueues nothing.
- End-to-end via `axon serve` (manual/CI-optional): create watch, backdate
  `next_run_at`, observe a crawl job appear in `axon_crawl_jobs`.

Regression: all existing watch/scheduler tests stay green (task_type string
updated `refresh`→`watch` in fixtures).

## Rollout / Compatibility Notes

- The scheduler is unreleased (open PR #149), so changing `refresh`→`watch` is
  not a breaking change to any shipped release.
- On deploy, the `0003` migration runs automatically via `sqlx::migrate!` on pool
  open. Existing `axon_watch_defs` rows with `task_type = "refresh"` would become
  unsupported at run time (they fail validation on `run-now`); acceptable given
  no released `refresh` users. None exist in production beyond test pollution.

## File-by-File Impact (anticipated)

- New: `src/jobs/migrations/0003_create_watch_url_state.sql`
- New: `src/core/http/conditional.rs` (+ sidecar tests)
- New: `src/jobs/watch/url_state.rs`, `src/jobs/watch/change_detect.rs`,
  `src/jobs/watch/cluster.rs` (+ sidecar tests). Note: this introduces a
  `src/jobs/watch/` submodule dir; `src/jobs/watch.rs` stays the module root and
  declares `mod url_state; mod change_detect; mod cluster;`.
- Modified: `src/jobs/watch.rs` (`run_watch_task` rewrite, `validate_task_type`
  supported set, new SQL helpers wiring). Watch the 500-line cap — likely split
  helpers into the new `watch/` submodules to stay under.
- Modified: `src/core/http.rs` (declare `mod conditional;`).
- Modified: tests/fixtures referencing `task_type: "refresh"` → `"watch"`
  (`src/jobs/watch_tests.rs`, `src/web/server/handlers/rest_tests.rs`, any
  parse/help fixtures).
- Docs: `docs/commands/watch.md`, `CLAUDE.md` watch section, CHANGELOG.

## Open Items for the Implementation Plan

- Exact markdown normalization rules (which whitespace/boilerplate to strip) —
  start conservative (whitespace + line endings only) to avoid masking real
  changes; revisit if false-positives appear.
- Whether the conditional probe uses `GET` (read status+headers, discard body)
  or `HEAD` (cheaper but more often unsupported). Default `GET`; document.
- Crawl `config_json` snapshot: reuse the watch's `cfg` with `max_depth` override
  and auto path-prefix scoping left on.
