# URL Change-Detection Watch — Design Spec

Date: 2026-05-31
Status: Approved (brainstorming, revised after `diff`-reuse + web research) — pending implementation plan update
Branch: `feat/watch-scheduler`
Related: builds on the watch scheduler (`src/jobs/workers/watch_scheduler.rs`, `src/jobs/watch.rs`) added in v4.15.0. Reuses the existing `diff` service (`src/services/diff.rs`).

## Summary

Upgrade the `watch` action from a stateless periodic scraper into a **URL change
detector that crawls only when content meaningfully changes**. A watch stays a
recurring scheduler entry; each tick it detects which of its URL(s) changed since
the last check (reusing the existing `compute_diff`), and enqueues a crawl for
the changed subtrees. No rename to `schedule` — "watch" now means "watch a URL
for changes." The stateless `refresh` task is replaced by task_type `watch`.

## Goals

- Detect content changes efficiently (cheap when unchanged), with a **rich**
  signal: a real unified diff + link/word deltas, not just "bytes differ."
- Crawl only the changed subtrees, coalesced, and never pile up crawls.
- Suppress false positives (timestamps, view counters, ads) so a watch doesn't
  crawl every tick over noise.
- Keep a browsable history of *what changed* (human + AI summary).
- Reuse existing, tested building blocks: the scheduler, `compute_diff`, the
  crawl job runtime + in-process worker, auto path-prefix scoping, and the
  Gemini `llm_backend`.

## Non-Goals (v1)

- No rename of `watch`→`schedule`.
- No multi-seed crawl-engine changes (coalescing = clustering + one
  common-ancestor seed per cluster).
- No `refresh` backward-compat alias (scheduler is unreleased).
- **No adaptive recrawl frequency** in v1 — change-rate estimation that auto-tunes
  each watch's interval (arXiv 2004.02167 / Microsoft Optimal-Freshness) is a
  v2 follow-up: it needs accumulated changed/unchanged history, which this design
  starts recording. v1 uses the fixed `every_seconds`.

## Decisions (locked)

| Question | Decision |
|---|---|
| Relationship to scheduler | One feature: a watch **is** a change-detecting scheduled job. No `schedule` rename. |
| Change signal | **Hybrid + rich**: conditional probe (ETag/Last-Modified) → 304 = unchanged; else scrape → filter → reuse `compute_diff(prior_snapshot, current)` for a unified diff + link/word deltas. A content hash of the filtered markdown is the cheap fast-equal skip. |
| Noise suppression | Optional per-watch **`ignore_patterns`** (regex lines stripped before diffing) + a **change threshold** (crawl only on a meaningful change). |
| Action on change | **Full crawl job** seeded from the changed URL(s). |
| First run (no prior state) | **Seed**: first-seen = changed → crawl once, then watch. |
| URLs per watch | **Multiple** (`urls` array). |
| Coalescing | **Group changed URLs by common path prefix**; one crawl per cluster seeded at the common ancestor. |
| In-flight | **Skip** a cluster whose previous crawl is still pending/running; keep detecting + updating state. |
| Change history | A **`axon_watch_run_artifacts`** row per *changed* URL: unified diff + AI summary + deltas. Latest snapshot for diffing lives in `axon_watch_url_state`. |
| AI summary | On change, summarize the unified diff via the existing Gemini `llm_backend` into the artifact + run `result_json`. Best-effort (raw diff retained if it fails). |
| task_type | `watch` replaces `refresh`; `SUPPORTED_TASK_TYPES = ["watch"]`. |

## Reused components & borrowed patterns

**Reused (already in the repo):**
- `src/services/diff.rs::compute_diff(markdown_a, links_a, meta_a, markdown_b, links_b, meta_b) -> DiffResult` — pure, tested (`similar` line diff + link/word deltas + `DiffStatus::{Same,Changed}`). For a watch we feed `(stored prior snapshot, fresh scrape)`. (`compute_diff` and `extract_links_from_payload` exposed `pub(crate)`.)
- `src/services/scrape.rs::scrape` — fetch + HTML→markdown extraction (this is our "content filter" step).
- `src/services/llm_backend` — `CompletionRequest::new(prompt).system_prompt(..).backend_from_config(cfg)` → `complete_text` for the AI diff summary.
- `axon_watch_run_artifacts` table — already exists for per-run artifacts.

**Borrowed from established change-detection tools (research):**
- changedetection.io / urlwatch both implement **fetch → filter/extract → diff vs previous snapshot → act**. Diffing *extracted* content (our markdown), not raw HTML, is the consensus correct approach. → validated.
- **Ignore/trigger filters** are the #1 false-positive defense → `ignore_patterns`.
- **Conditional actions / thresholds** (act only on meaningful change) → change threshold.
- **AI change summaries** ("Price dropped $89.99→$67.00") → Gemini summary of the diff.
- **Sitemap `lastmod`/ETag are hints, often missing/inaccurate** → justifies the hybrid (validators first, content diff as source of truth).
- **Adaptive recrawl frequency** (change-rate estimation) → recorded-but-deferred (v2).

## Architecture

### Per-tick flow (per URL in the watch)

1. **Conditional probe** with stored `etag`/`last_modified`:
   - `304` → **Unchanged**; update `last_checked_at`; stop (no scrape/diff).
   - `200` / no validators / first-seen → continue (capture fresh validators).
   - error / other status → record per-URL error, preserve prior state, no crawl.
2. **Scrape** the URL → `markdown`, `links` (from payload), `metadata`.
3. **Filter**: normalize the markdown (line endings, trailing ws, blank-run
   collapse) then strip lines matching the watch's `ignore_patterns` (regex).
4. **Fast-equal skip**: `content_hash(filtered)` == stored hash → **Unchanged**
   (store fresh validators, no diff). Else continue.
5. **Diff**: `compute_diff(prior_filtered_markdown, prior_links, {}, filtered, links, {})`
   → `DiffResult`. First-seen (no prior snapshot) is forced **Changed (seed)**.
6. **Threshold**: a change is *meaningful* if `DiffStatus::Changed` **and**
   (`links_added`/`links_removed` non-empty **or** `|word_count_delta|` ≥
   `change_threshold_words`). Default threshold `0` ⇒ any text change counts.
   Non-meaningful changes update the snapshot but **do not** crawl.
7. **Persist snapshot**: `last_markdown` (filtered), `last_links_json`,
   `content_hash`, `etag`, `last_modified`, `last_checked_at`,
   `last_changed_at`.

After all URLs:

8. **AI summary** (per changed URL, best-effort): summarize `DiffResult.text_diff`
   + deltas via `llm_backend` (e.g. "Added a 'Pricing' section; 3 new links").
9. **Artifact**: write one `axon_watch_run_artifacts` row per changed URL: the
   unified diff, the AI summary, link/word deltas (`kind = "url-change"`).
10. **Cluster** changed URLs by common path prefix.
11. Per cluster: **in-flight guard** — skip if a member's `last_crawl_job_id` is
    still pending/running; else enqueue one crawl at the cluster seed
    (`max_depth`, auto path-prefix scoped). Write the crawl id to members.
12. **Finalize run**: `result_json` = `{checked, changed, unchanged, skipped,
    clusters, dispatched, summaries, errors}`. Scheduler advances `next_run_at`,
    clears the lease.

### Components (each small, single-purpose, testable)

| Unit | File | Responsibility | Notes |
|---|---|---|---|
| Conditional probe | `src/core/http/conditional.rs` | `conditional_probe(url, etag, lm) -> Probe`; pure `classify`/`conditional_headers` | new |
| Content filter | `src/jobs/watch/filter.rs` | `normalize_markdown`, `apply_ignore(md, &[regex])`, `content_hash` (sha256) | new, pure |
| URL state store | `src/jobs/watch/url_state.rs` | CRUD for `axon_watch_url_state` (snapshot + validators + crawl id) | new |
| Diff adapter | inline in `change_detect` | call `services::diff::compute_diff` with stored vs fresh snapshot | reuse |
| Change detector | `src/jobs/watch/change_detect.rs` | probe → scrape → filter → fast-equal → `compute_diff` → threshold; returns `UrlOutcome { decision, diff, error }`; persists snapshot | new |
| Prefix clusterer | `src/jobs/watch/cluster.rs` | `group_by_common_prefix(&[Url]) -> Vec<Cluster>` | new, pure |
| Crawl dispatcher | `src/jobs/watch/dispatch.rs` | `crawl_job_active`, `enqueue_change_crawl` | new |
| Summary + artifact | `src/jobs/watch/report.rs` | `summarize_diff(cfg, &DiffResult) -> Option<String>` (Gemini, best-effort); `write_change_artifact(pool, run_id, url, &DiffResult, summary)` | new |
| Orchestrator | `src/jobs/watch/orchestrate.rs` | drive per-URL detection → summary/artifact → cluster → dispatch; build `result_json` | new (keeps `watch.rs` ≤500) |

`run_watch_task` in `watch.rs` stays a one-line dispatch to `orchestrate::run_url_watch`. Calling `services::{scrape,diff,llm_backend}` from the jobs layer follows the existing pattern (`run_watch_task` already calls `services::scrape::scrape`).

## Data Model

`0003_create_watch_url_state.sql` (additive):

```sql
CREATE TABLE IF NOT EXISTS axon_watch_url_state (
    watch_id          TEXT NOT NULL,
    url               TEXT NOT NULL,
    etag              TEXT,
    last_modified     TEXT,
    content_hash      TEXT,          -- sha256 of filtered markdown (fast-equal skip)
    last_markdown     TEXT,          -- filtered markdown snapshot (diff source)
    last_links_json   TEXT,          -- JSON array of LinkEntry from last snapshot
    last_checked_at   INTEGER,
    last_changed_at   INTEGER,
    last_crawl_job_id TEXT,
    PRIMARY KEY (watch_id, url),
    FOREIGN KEY (watch_id) REFERENCES axon_watch_defs(id) ON DELETE CASCADE
);
```

Change history reuses the existing `axon_watch_run_artifacts(id, watch_run_id,
kind, path, payload, created_at)` — one row per changed URL, `kind =
"url-change"`, `payload` = JSON `{url, unified_diff, summary, links_added,
links_removed, word_count_delta}`.

### `task_payload` shape

```json
{
  "urls": ["https://docs.example.com/guide/intro"],
  "max_depth": 2,
  "ignore_patterns": ["^Last updated:", "\\d+ (users|viewers) online"],
  "change_threshold_words": 0,
  "summarize": true
}
```

- `urls` (required, non-empty).
- `max_depth` (default 2) — change-crawl depth bound.
- `ignore_patterns` (optional, default `[]`) — regex; matching lines removed
  before diffing. Invalid regex → create-time validation error.
- `change_threshold_words` (optional, default 0) — min absolute word-count delta
  for a text-only change to count as meaningful (link changes always count).
- `summarize` (optional, default true) — AI diff summary on change.

## API / Surface Changes

- `validate_task_type`: `["watch"]`. `validate_task_payload` (new) additionally
  checks `urls` non-empty and `ignore_patterns` compile as regex, shared by CLI +
  both HTTP create handlers.
- CLI: `axon watch create <name> --task-type watch --every-seconds N --task-payload '{"urls":[...],"ignore_patterns":[...]}'`. No new subcommands.
- HTTP `/v1/watch*` routes unchanged; create validates payload via the shared validator.
- No new env vars; tick/lease knobs unchanged.

## Error Handling

- Probe/scrape/regex/LLM failures are **non-fatal per URL**: record an error in
  `result_json`, preserve prior state, do not crawl that URL; the run completes.
- AI summary failure → keep the raw unified diff; `summary = null`.
- Enqueue failure → per-cluster error; other clusters still dispatch.
- In-flight job missing → treat as not-in-flight.
- SSRF: `conditional_probe` and `scrape` both run `validate_url` first.
- COMPLETED finalize-write failure → existing best-effort FAILED fallback.

## Testing

Unit (no live services):
- `filter.rs`: normalize idempotence; ignore_patterns strip matching lines; hash
  stable; whitespace-only change → same hash.
- `cluster.rs`: same-dir, nested, sibling, different-host, single, root-only.
- `change_detect.rs`: `decide` matrix using injected diff results — first-seen→
  changed; equal-hash→unchanged; links-changed→meaningful; sub-threshold word
  change→not meaningful; probe 304→unchanged.
- `url_state.rs`: snapshot+links round-trip; FK cascade.
- `report.rs`: artifact payload shape; summary best-effort (LLM-failure → None).
- Reuse existing `services::diff` tests for `compute_diff` (unchanged).

Integration (SQLite temp DB; scrape/probe stubbed or `example.com`):
- First run seeds a crawl + writes snapshot + one `url-change` artifact.
- Identical second run → no crawl, no artifact (unchanged).
- Ignore-pattern: only a timestamped line changes → no crawl.
- In-flight guard: pending crawl recorded → `skipped`, no new crawl.
- End-to-end via `axon serve`: backdate `next_run_at`, observe crawl + artifact.

Regression: existing watch/scheduler tests green (`refresh`→`watch` fixtures).

## Rollout / Compatibility

- Scheduler unreleased (PR #149) → `refresh`→`watch` is not a breaking release
  change. `0003` runs on pool open. AI summary requires Gemini CLI configured
  (`AXON_HEADLESS_GEMINI_CMD`); if absent, `summarize` degrades to raw diff.

## Open Items for the Plan

- Markdown normalization rules: conservative (whitespace + line endings) to avoid
  masking real changes.
- Conditional probe: `GET` (read status+headers, ignore body) vs `HEAD`. Default
  `GET`; document.
- Crawl `config_json`: watch `cfg` with `max_depth` override, auto-scoping on.
- Summary prompt: short, factual, untrusted-input framing (mirror
  `summarize.rs`'s system prompt about not following embedded instructions).
