# Database Schema
Last Modified: 2026-03-03

Version: 1.0.0
Last Updated: 01:26:53 | 02/25/2026 EST

Tables are auto-created on first worker/command start via `CREATE TABLE IF NOT EXISTS` in each `*_jobs.rs` file's `ensure_schema()` function.

## axon_crawl_jobs

| Column | Type | Nullable | Default | Description |
|--------|------|----------|---------|-------------|
| `id` | UUID | NOT NULL | — | Primary key, job identifier |
| `url` | TEXT | NOT NULL | — | Target URL for the crawl |
| `status` | TEXT | NOT NULL | — | `pending` / `running` / `completed` / `failed` / `canceled` |
| `created_at` | TIMESTAMPTZ | NOT NULL | `NOW()` | Job creation timestamp |
| `updated_at` | TIMESTAMPTZ | NOT NULL | `NOW()` | Last status change |
| `started_at` | TIMESTAMPTZ | NULL | — | When worker began processing |
| `finished_at` | TIMESTAMPTZ | NULL | — | When job completed/failed/canceled |
| `error_text` | TEXT | NULL | — | Error message on failure |
| `result_json` | JSONB | NULL | — | Crawl results (pages found, stats) |
| `config_json` | JSONB | NOT NULL | — | Serialized job configuration |

**Indexes:**
- `idx_axon_crawl_jobs_status` on `status`
- `idx_axon_crawl_jobs_status_created_desc` on `(status, created_at DESC)`
- `idx_axon_crawl_jobs_pending` — partial index on `created_at ASC WHERE status = 'pending'`
- `idx_axon_crawl_jobs_running_updated` — partial index on `updated_at ASC WHERE status = 'running'`

## axon_extract_jobs

| Column | Type | Nullable | Default | Description |
|--------|------|----------|---------|-------------|
| `id` | UUID | NOT NULL | — | Primary key |
| `status` | TEXT | NOT NULL | — | `pending` / `running` / `completed` / `failed` / `canceled` |
| `created_at` | TIMESTAMPTZ | NOT NULL | `NOW()` | Job creation timestamp |
| `updated_at` | TIMESTAMPTZ | NOT NULL | `NOW()` | Last status change |
| `started_at` | TIMESTAMPTZ | NULL | — | When worker began processing |
| `finished_at` | TIMESTAMPTZ | NULL | — | When job completed/failed/canceled |
| `error_text` | TEXT | NULL | — | Error message on failure |
| `urls_json` | JSONB | NOT NULL | — | Array of URLs for LLM extraction |
| `result_json` | JSONB | NULL | — | Extracted structured data |
| `config_json` | JSONB | NOT NULL | — | Serialized job configuration |

**Indexes:**
- `idx_axon_extract_jobs_pending` — partial index on `created_at ASC WHERE status = 'pending'`
- `idx_axon_extract_jobs_status_created_desc` on `(status, created_at DESC)`
- `idx_axon_extract_jobs_running_updated` — partial index on `updated_at ASC WHERE status = 'running'`

## axon_embed_jobs

| Column | Type | Nullable | Default | Description |
|--------|------|----------|---------|-------------|
| `id` | UUID | NOT NULL | — | Primary key |
| `status` | TEXT | NOT NULL | — | `pending` / `running` / `completed` / `failed` / `canceled` |
| `created_at` | TIMESTAMPTZ | NOT NULL | `NOW()` | Job creation timestamp |
| `updated_at` | TIMESTAMPTZ | NOT NULL | `NOW()` | Last status change |
| `started_at` | TIMESTAMPTZ | NULL | — | When worker began processing |
| `finished_at` | TIMESTAMPTZ | NULL | — | When job completed/failed/canceled |
| `error_text` | TEXT | NULL | — | Error message on failure |
| `input_text` | TEXT | NOT NULL | — | Input path, URL, or text to embed |
| `result_json` | JSONB | NULL | — | Embedding results (chunk count, point IDs) |
| `config_json` | JSONB | NOT NULL | — | Serialized job configuration |

**Indexes:**
- `idx_axon_embed_jobs_status_created_desc` on `(status, created_at DESC)`
- `idx_axon_embed_jobs_pending` — partial index on `created_at ASC WHERE status = 'pending'`
- `idx_axon_embed_jobs_running_updated` — partial index on `updated_at ASC WHERE status = 'running'`

## axon_graph_jobs

Tracks graph extraction jobs that read indexed chunks from Qdrant and write graph entities/relationships into Neo4j.

| Column | Type | Nullable | Default | Description |
|--------|------|----------|---------|-------------|
| `id` | UUID | NOT NULL | — | Primary key, job identifier |
| `url` | TEXT | NOT NULL | — | Source document URL used to retrieve chunks from Qdrant |
| `status` | TEXT | NOT NULL | `'pending'` | `pending` / `running` / `completed` / `failed` / `canceled` (CHECK constraint) |
| `chunk_count` | INTEGER | NULL | `0` | Number of chunks processed for the URL |
| `entity_count` | INTEGER | NULL | `0` | Number of entities written/updated |
| `relation_count` | INTEGER | NULL | `0` | Number of relationships written/updated |
| `result_json` | JSONB | NULL | — | Graph extraction summary payload |
| `config_json` | JSONB | NULL | — | Job config payload (e.g., source type) |
| `error_text` | TEXT | NULL | — | Error message on failure |
| `created_at` | TIMESTAMPTZ | NULL | `NOW()` | Job creation timestamp |
| `updated_at` | TIMESTAMPTZ | NULL | `NOW()` | Last status change or heartbeat touch |
| `started_at` | TIMESTAMPTZ | NULL | — | When worker began processing |
| `finished_at` | TIMESTAMPTZ | NULL | — | When job completed/failed/canceled |

**Indexes:**
- `idx_graph_jobs_status` on `status`
- `idx_graph_jobs_status_created_desc` on `(status, created_at DESC)`
- `idx_graph_jobs_url` on `url`

## axon_ingest_jobs

This table differs structurally from the other four: it uses `source_type` and `target` to identify the ingest target rather than a `url` or `urls_json` column. The `source_type` discriminator routes processing to the correct ingest backend (GitHub API, Reddit OAuth2, yt-dlp).

| Column | Type | Nullable | Default | Description |
|--------|------|----------|---------|-------------|
| `id` | UUID | NOT NULL | — | Primary key, job identifier |
| `status` | TEXT | NOT NULL | — | `pending` / `running` / `completed` / `failed` / `canceled` |
| `source_type` | TEXT | NOT NULL | — | Ingest backend discriminator: `github`, `reddit`, `youtube`, or `sessions` |
| `target` | TEXT | NOT NULL | — | Ingest target label: repo/subreddit/url for source-driven ingests, or provider selection label for sessions |
| `created_at` | TIMESTAMPTZ | NOT NULL | `NOW()` | Job creation timestamp |
| `updated_at` | TIMESTAMPTZ | NOT NULL | `NOW()` | Last status change |
| `started_at` | TIMESTAMPTZ | NULL | — | When worker began processing |
| `finished_at` | TIMESTAMPTZ | NULL | — | When job completed/failed/canceled |
| `error_text` | TEXT | NULL | — | Error message on failure |
| `result_json` | JSONB | NULL | — | Ingest results: `{"chunks_embedded": N}` |
| `config_json` | JSONB | NOT NULL | — | Serialized `IngestJobConfig` (source variant + collection name) |

**Indexes:**
- `idx_axon_ingest_jobs_pending` — partial index on `created_at ASC WHERE status = 'pending'` for efficient FIFO queue polling
- `idx_axon_ingest_jobs_status_created_desc` on `(status, created_at DESC)`
- `idx_axon_ingest_jobs_running_updated` — partial index on `updated_at ASC WHERE status = 'running'`

### source_type values

| Value | Target format | Command |
|-------|--------------|---------|
| `github` | `owner/repo` (e.g. `rust-lang/rust`) | `axon github <owner/repo>` |
| `reddit` | subreddit name (e.g. `rust`) or thread URL | `axon reddit <target>` |
| `youtube` | video, playlist, or channel URL | `axon youtube <url>` |
| `sessions` | `all` or provider list label (for example `claude,codex[:project]`) | `axon sessions [--claude|--codex|--gemini] [--project <name>]` |

### Structural differences vs other job tables

| Table | Target column(s) | Notes |
|-------|-----------------|-------|
| `axon_crawl_jobs` | `url TEXT NOT NULL` | Single crawl seed URL |
| `axon_extract_jobs` | `urls_json JSONB NOT NULL` | Array of URLs |
| `axon_embed_jobs` | `input_text TEXT NOT NULL` | File path, URL, or raw text |
| `axon_graph_jobs` | `url TEXT NOT NULL` + counters | Graph extraction stats per URL |
| `axon_ingest_jobs` | `source_type TEXT` + `target TEXT` | Discriminated source type + typed target |

## axon_refresh_jobs

Tracks refresh jobs that re-fetch previously crawled URLs to detect content changes via ETag/If-Modified-Since headers and SHA-256 hash comparison.

| Column | Type | Nullable | Default | Description |
|--------|------|----------|---------|-------------|
| `id` | UUID | NOT NULL | — | Primary key, job identifier |
| `status` | TEXT | NOT NULL | — | `pending` / `running` / `completed` / `failed` / `canceled` (CHECK constraint) |
| `created_at` | TIMESTAMPTZ | NOT NULL | `NOW()` | Job creation timestamp |
| `updated_at` | TIMESTAMPTZ | NOT NULL | `NOW()` | Last status change or heartbeat touch |
| `started_at` | TIMESTAMPTZ | NULL | — | When worker began processing |
| `finished_at` | TIMESTAMPTZ | NULL | — | When job completed/failed/canceled |
| `error_text` | TEXT | NULL | — | Error message on failure |
| `urls_json` | JSONB | NOT NULL | — | Array of URLs to refresh |
| `result_json` | JSONB | NULL | — | Progress/final result: `checked`, `changed`, `unchanged`, `not_modified`, `failed`, `embedded_chunks` |
| `config_json` | JSONB | NOT NULL | — | Serialized `RefreshJobConfig` (urls, embed flag, output_dir) |

**Indexes:**
- `idx_axon_refresh_jobs_pending` — partial index on `created_at ASC WHERE status = 'pending'` for efficient FIFO queue polling
- `idx_axon_refresh_jobs_status_created_desc` on `(status, created_at DESC)`
- `idx_axon_refresh_jobs_running_updated` — partial index on `updated_at ASC WHERE status = 'running'`

**Notes:**
- `result_json` is updated periodically during processing (every URL) with a `"phase": "refreshing"` progress snapshot, then finalized with `"phase": "completed"` on success.
- Uses `CHECK (status IN ('pending', 'running', 'completed', 'failed', 'canceled'))`. Other `axon_*_jobs` tables also enforce status CHECK constraints in current schema.

## axon_refresh_targets

Per-URL state table for conditional HTTP requests. Stores the last-known ETag, Last-Modified header, and content hash for each URL that has been refreshed. Used to send `If-None-Match` / `If-Modified-Since` headers on subsequent refreshes.

| Column | Type | Nullable | Default | Description |
|--------|------|----------|---------|-------------|
| `url` | TEXT | NOT NULL | — | Primary key, the target URL |
| `etag` | TEXT | NULL | — | Last `ETag` response header value |
| `last_modified` | TEXT | NULL | — | Last `Last-Modified` response header value |
| `content_hash` | TEXT | NULL | — | SHA-256 hex digest of the trimmed markdown content |
| `markdown_chars` | INTEGER | NULL | — | Character count of the last successful markdown extraction |
| `last_status` | INTEGER | NULL | — | HTTP status code from the last check |
| `last_checked_at` | TIMESTAMPTZ | NOT NULL | `NOW()` | When this URL was last checked |
| `last_changed_at` | TIMESTAMPTZ | NULL | — | When this URL's content last changed (set only when `changed = true`) |
| `error_text` | TEXT | NULL | — | Error message from last check (e.g., `"HTTP 404"`, network error) |

**Notes:**
- Uses `ON CONFLICT (url) DO UPDATE` upsert semantics — `COALESCE` preserves existing ETag/Last-Modified/content_hash when the new value is NULL (e.g., 304 responses carry no new headers).
- No foreign key to `axon_refresh_jobs` — targets persist across jobs and accumulate state over time.

## axon_refresh_schedules

Scheduled refresh configurations. Each schedule defines a set of URLs (or a seed URL) to refresh on a recurring interval. Due schedules are claimed atomically via `FOR UPDATE SKIP LOCKED`.

| Column | Type | Nullable | Default | Description |
|--------|------|----------|---------|-------------|
| `id` | UUID | NOT NULL | — | Primary key, schedule identifier |
| `name` | TEXT | NOT NULL | — | Unique human-readable name for the schedule |
| `seed_url` | TEXT | NULL | — | Optional seed URL (for crawl-and-refresh workflows) |
| `urls_json` | JSONB | NULL | — | Array of specific URLs to refresh |
| `every_seconds` | BIGINT | NOT NULL | — | Interval between runs in seconds |
| `enabled` | BOOLEAN | NOT NULL | `TRUE` | Whether the schedule is active |
| `next_run_at` | TIMESTAMPTZ | NOT NULL | — | When the next run is due |
| `last_run_at` | TIMESTAMPTZ | NULL | — | When the schedule last ran |
| `source_type` | TEXT | NULL | — | Non-URL schedule type: `"github"` for repo re-ingest |
| `target` | TEXT | NULL | — | Target identifier for non-URL schedules (e.g., `"owner/repo"` for GitHub) |
| `created_at` | TIMESTAMPTZ | NOT NULL | `NOW()` | Schedule creation timestamp |
| `updated_at` | TIMESTAMPTZ | NOT NULL | `NOW()` | Last modification timestamp |

**Unique constraint:** `name` column is UNIQUE.

**Index:** `idx_axon_refresh_schedules_due` — partial index on `next_run_at ASC WHERE enabled = TRUE` for efficient due-schedule polling.

**Notes:**
- Claim uses a lease mechanism: `next_run_at` is advanced by `SCHEDULE_CLAIM_LEASE_SECS` (300s) during claim to prevent duplicate claims. After the job completes, `mark_refresh_schedule_ran` sets `next_run_at` to the actual next interval.
- For URL schedules: either `seed_url` or `urls_json` (or both) should be provided.
- For GitHub repo schedules: `source_type = 'github'` and `target = 'owner/repo'`. The scheduler checks the repo's `pushed_at` timestamp via the GitHub API and skips re-ingestion if nothing has been pushed since `last_run_at`.

## axon_watch_defs

Top-level scheduler definitions used by `axon watch` and by the refresh schedule compatibility bridge (`task_type = 'refresh'`).

| Column | Type | Nullable | Default | Description |
|--------|------|----------|---------|-------------|
| `id` | UUID | NOT NULL | — | Primary key, watch definition identifier |
| `name` | TEXT | NOT NULL | — | Unique human-readable watch name |
| `task_type` | TEXT | NOT NULL | — | Dispatched task family (current worker support: `refresh`) |
| `task_payload` | JSONB | NOT NULL | — | Task payload for dispatcher (for refresh: `{\"urls\":[...]}`) |
| `every_seconds` | BIGINT | NOT NULL | — | Recurrence interval in seconds (`CHECK > 0`) |
| `enabled` | BOOLEAN | NOT NULL | `TRUE` | Whether watch is active |
| `next_run_at` | TIMESTAMPTZ | NOT NULL | — | Next due timestamp |
| `lease_expires_at` | TIMESTAMPTZ | NULL | — | Short claim lease to avoid duplicate dispatch |
| `last_run_at` | TIMESTAMPTZ | NULL | — | Last successful/attempted run timestamp |
| `created_at` | TIMESTAMPTZ | NOT NULL | `NOW()` | Creation timestamp |
| `updated_at` | TIMESTAMPTZ | NOT NULL | `NOW()` | Last update timestamp |

**Unique constraint:** `name` column is UNIQUE.

**Index:** `idx_axon_watch_defs_due` — partial index on `next_run_at ASC WHERE enabled = TRUE`.

## axon_watch_runs

Run history for each watch execution attempt.

| Column | Type | Nullable | Default | Description |
|--------|------|----------|---------|-------------|
| `id` | UUID | NOT NULL | — | Primary key, watch run identifier |
| `watch_id` | UUID | NOT NULL | — | FK to `axon_watch_defs(id)` with `ON DELETE CASCADE` |
| `status` | TEXT | NOT NULL | — | `pending` / `running` / `completed` / `failed` / `canceled` |
| `dispatched_job_id` | UUID | NULL | — | Back-reference to downstream async job id (when any) |
| `error_text` | TEXT | NULL | — | Dispatcher/runtime error details |
| `result_json` | JSONB | NULL | — | Execution result metadata |
| `started_at` | TIMESTAMPTZ | NULL | — | Run start timestamp |
| `finished_at` | TIMESTAMPTZ | NULL | — | Run finish timestamp |
| `created_at` | TIMESTAMPTZ | NOT NULL | `NOW()` | Creation timestamp |
| `updated_at` | TIMESTAMPTZ | NOT NULL | `NOW()` | Last update timestamp |

**Index:** `idx_axon_watch_runs_watch_id` on `(watch_id, created_at DESC)`.

## axon_watch_run_artifacts

Optional artifact pointers associated with watch runs.

| Column | Type | Nullable | Default | Description |
|--------|------|----------|---------|-------------|
| `id` | BIGSERIAL | NOT NULL | — | Primary key |
| `watch_run_id` | UUID | NOT NULL | — | FK to `axon_watch_runs(id)` with `ON DELETE CASCADE` |
| `kind` | TEXT | NOT NULL | — | Artifact kind discriminator |
| `path` | TEXT | NULL | — | Filesystem path pointer |
| `payload` | JSONB | NULL | — | Structured artifact payload |
| `created_at` | TIMESTAMPTZ | NOT NULL | `NOW()` | Creation timestamp |
