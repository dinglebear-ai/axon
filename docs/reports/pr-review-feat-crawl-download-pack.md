# PR Review: `feat/crawl-download-pack`
**Branch:** `feat/crawl-download-pack` vs `main`
**Date:** 2026-02-26
**Scope:** 308 files, 24,211 insertions — refresh job pipeline, crawl download pack, pulse workspace overhaul, axon-web Docker service
**Reviewers:** 7 specialized agents (code, tests, errors, types, MCP/config, TypeScript, Docker/CI)

---

## CRITICAL — Must fix before merge (19 issues)

### Rust — Monolith Policy Violations

| # | File | Issue | Fix |
|---|------|-------|-----|
| C1 | `crates/jobs/refresh.rs` (1131L) | Exceeds 500-line hard limit | Split into `refresh/mod.rs`, `refresh/processor.rs`, `refresh/schedule.rs`, `refresh/worker.rs` |
| C2 | `crates/cli/commands/refresh.rs` (881L) | Exceeds 500-line hard limit | Extract schedule handlers to `refresh/schedule.rs`, URL resolution to `refresh/resolve.rs` |
| C3 | `crates/jobs/refresh.rs:646-907` | `process_refresh_job` is 261L, exceeds 120-line function hard limit | Extract `setup_refresh_job_context()`, `process_single_refresh_url()`, `finalize_refresh_job()` |

### Rust — Error Handling (Panics in Production)

| # | File | Issue | Fix |
|---|------|-------|-----|
| C4 | `crates/web/download.rs:152,185,218,304` | `.unwrap()` on `Content-Disposition` header built from user-data domain — panics on IDN hostnames, IPv6 literals, or any domain with non-ASCII chars | Replace with `.unwrap_or_else(\|_\| HeaderValue::from_static("attachment"))` or sanitize domain before use |
| C5 | `crates/web/execute/polling.rs:206` | `resolve_exe()` `Err` path `break`s silently — no terminal WS message sent, frontend hangs permanently in "running" state | Add `send_error_dual(tx, ctx, format!("poll aborted: {e}"), ...)` before `break` |

### Rust — Correctness

| # | File | Issue | Fix |
|---|------|-------|-----|
| C6 | `crates/cli/commands/refresh.rs:434` | `let _ = mark_refresh_schedule_ran(cfg, ...).await?` — `?` aborts the entire sweep on any single schedule's DB update failure; already-dispatched jobs fire again immediately on next sweep (duplicate refreshes) | Use `if let Err(err) = ...` + `log_warn`, don't propagate |
| C7 | `crates/cli/commands/refresh.rs:444` | `start_refresh_job` `Err` silently increments `failed` counter with no logged cause or schedule name | `log_warn(&format!("refresh schedule {} dispatch failed: {err}", schedule.name))` |

### MCP

| # | File | Issue | Fix |
|---|------|-------|-----|
| C8 | `crates/mcp/schema.rs` | `AxonRequest` enum has no `Refresh` variant — MCP clients cannot manage refresh jobs at all despite full CLI+job infrastructure existing | Add `Refresh(RefreshRequest)` variant + `RefreshSubaction` enum (start/status/cancel/list/cleanup/clear/recover/schedule); wire in server.rs; update `docs/MCP-TOOL-SCHEMA.md` |
| C9 | `crates/mcp/config.rs:42-53` | `load_mcp_config()` doesn't load `AXON_REFRESH_QUEUE` or any `AXON_ASK_AUTHORITATIVE_*` env vars — authoritative domain filtering and refresh queue overrides silently ignored from MCP | Mirror all new env vars from `parse.rs:into_config()` into `load_mcp_config()` |
| C10 | `crates/web.rs:77` + `crates/web/download.rs` | Download routes (`/download/{job_id}/*`) have no auth, no rate limiting, bound to `0.0.0.0` — ZIP endpoint buffers entire crawl into memory with no byte-size cap | Add token-based auth middleware OR restrict to `127.0.0.1`; add `max_total_bytes` guard to `serve_zip` |

### TypeScript — Security

| # | File | Issue | Fix |
|---|------|-------|-----|
| C11 | `apps/web/lib/download-urls.ts:14` | `fileDownloadUrl(jobId, relPath)` passes `relPath` raw into URL — no `encodeURIComponent`, no `..` rejection | `relPath.split('/').map(encodeURIComponent).join('/')` before interpolation |
| C12 | `apps/web/lib/pulse/copilot-validation.ts:3-4` | `CopilotRequestSchema.prompt` has no `.max()` — unbounded LLM payload, OOM vector | `z.string().min(1).max(8000)` on `prompt`; `.max(4000)` on `system`; `.max(100)` on `model` |

### CI

| # | File | Issue | Fix |
|---|------|-------|-----|
| C13 | `.github/workflows/ci.yml:206` | `mcp-smoke` writes `AXON_REDIS_URL=redis://axon-redis:6379` (no password) but Redis requires `axonredis` password — all container-internal Redis connections fail auth | Change to `redis://:axonredis@axon-redis:6379` |

### Docker / Docs

| # | File | Issue | Fix |
|---|------|-------|-----|
| C14 | `docker/README.md:14` | Still references `Dockerfile.chrome` — file was moved to `chrome/Dockerfile` | Update to `chrome/Dockerfile` |
| C15 | `docker/README.md:19-24` | New `web-server` s6 service not listed in Worker Supervision section | Add entry for `s6-rc.d/web-server/run` |

### Test Coverage — Security-Relevant

| # | File | Issue | Fix |
|---|------|-------|-----|
| C16 | `crates/web/download.rs` (serve_file) | Path traversal guard (`canonicalize` + `starts_with`) has zero tests — a regression removing `canonicalize` would bypass the string `..` check via symlinks | Add test using `tempfile::TempDir`: assert `../sibling/secret.txt` → 403, valid path → 200 |
| C17 | `crates/jobs/refresh.rs` (refresh_one_url) | HTTP conditional request logic (304, ETag, content-hash comparison, `changed` flag) has zero tests — silent regression in any of these silently burns embed quota or suppresses re-embeds | Use `httpmock` to test: 304 early-return, hash-match → `changed=false`, new hash → `changed=true`, 404 → not changed |

---

## IMPORTANT — Should fix before merge (26 issues)

### Rust — Error Handling (Silent Failures)

| # | File | Issue | Fix |
|---|------|-------|-----|
| I1 | `crates/jobs/refresh.rs:803` | `manifest.write_all().await` result discarded with `let _ =` — manifest has a gap when disk is full; DB state and file state diverge silently | `log_warn(...)` on error; consider returning `Err` to mark job failed |
| I2 | `crates/jobs/refresh.rs:764,771,784,826,840` | All `upsert_target_state()` calls use `let _ =` — DB failures in the hot loop are completely silent; next run treats all URLs as first-seen and re-embeds everything | Add `log_warn` at each call site |
| I3 | `crates/jobs/refresh.rs:844-861` | In-loop progress heartbeat `UPDATE` discards its result — failed update causes watchdog to reclaim a healthy running job as stale | Add `log_warn` on error |
| I4 | `crates/cli/commands/refresh.rs:712` | `serde_json::from_value::<Vec<String>>(...).unwrap_or_default()` on corrupt `urls_json` → silent empty URL list → `skipped=1` in log with no explanation | Add `log_warn` with schedule name and error on `Err` |
| I5 | `crates/cli/commands/refresh.rs:425` | `resolve_schedule_urls(...).await?` — URL validation error on one schedule aborts the entire sweep; all subsequent schedules in the batch are skipped | Wrap per-schedule in error handler; log and `continue` |
| I6 | `apps/web/lib/pulse/rag.ts:44,80,90` | `retrieveFromCollections` swallows all TEI and Qdrant failures — returns empty citations silently; user gets LLM answers grounded in nothing | Log errors; consider returning partial results with an error flag |
| I7 | `apps/web/lib/pulse/server-env.ts:49` | `.env` read failures swallowed with empty catch — service starts with missing env vars, all RAG silently returns no results; no log | Add `console.error('[Pulse] Failed to load repo root .env:', err)` |
| I8 | `apps/web/lib/pulse/storage.ts:134` | `listPulseDocs` outer catch returns `[]` — permission errors on `.cache/pulse` are indistinguishable from empty workspace | Add `console.error('[Pulse] listPulseDocs failed:', err)` |
| I9 | `apps/web/lib/pulse/storage.ts:110` | `loadPulseDoc` returns `null` on any error including permission errors — user's docs silently disappear from list | Log operational errors (not ENOENT which is expected) |
| I10 | `crates/jobs/refresh.rs:658,664,668,679,693,731,742,904` | 8x `let _ = mark_job_failed(...).await` — if marking a job failed itself fails, job is stuck permanently with no log | `if let Err(err) = mark_job_failed(...).await { log_warn(...) }` at each site |

### Rust — Performance

| # | File | Issue | Fix |
|---|------|-------|-----|
| I11 | `crates/jobs/refresh.rs:499-522` | N+1 query in `load_target_states` — one `SELECT` per URL; 100-URL job = 100 round-trips | Use `WHERE url = ANY($1::text[])` to batch-load all states in one query |
| I12 | `crates/jobs/refresh.rs` (17 sites) | `make_pool()` called once per public function — creates fresh `PgPool` each time; CLAUDE.md explicitly prohibits this | Create one pool at entry point (`run_refresh`, `run_refresh_worker`) and pass `&PgPool` down |

### Rust — Async Safety

| # | File | Issue | Fix |
|---|------|-------|-----|
| I13 | `crates/cli/commands/refresh.rs:694` | `path.exists()` (blocking `std::fs::metadata`) in async fn `urls_from_manifest_seed` | Use `tokio::fs::try_exists(&path).await.unwrap_or(false)` |
| I14 | `crates/web/download.rs:48` | `dir.is_dir()` (blocking) in `validate_job_dir`, called from async handlers | Make `validate_job_dir` async; use `tokio::fs::metadata` |

### Rust — Schema / Architecture

| # | File | Issue | Fix |
|---|------|-------|-----|
| I15 | `crates/jobs/refresh.rs:116-186` | `ensure_schema()` runs DDL without `begin_schema_migration_tx` advisory lock — race condition when multiple workers start simultaneously; crawl and embed modules in this same PR use the correct pattern | Wrap in `begin_schema_migration_tx(pool, REFRESH_SCHEMA_LOCK_KEY).await?` |
| I16 | `crates/web/execute/mod.rs:42` + `files.rs:14` + `polling.rs` | `serialize_v2_event()` defined identically in 3 files — DRY violation | Define once in `events.rs`, import everywhere |

### TypeScript — Validation / React

| # | File | Issue | Fix |
|---|------|-------|-----|
| I17 | `apps/web/app/api/pulse/doc/route.ts:9` | `filename` query param used without Zod validation — `loadPulseDoc` uses `path.basename` internally but route boundary is unguarded | Validate: `z.string().min(1).max(255).regex(/^[a-z0-9_-]+\.md$/i)` |
| I18 | `apps/web/hooks/use-axon-ws.ts:96` | `connectRef.current = connect` runs during render (outside `useEffect`) — unsafe in React 19 concurrent mode, stale closure risk | Move inside `useEffect(() => { connectRef.current = connect }, [connect])` |
| I19 | `apps/web/app/api/pulse/source/route.ts:23` | URLs spread into `spawn` args without `--` end-of-flags separator — URL beginning with `--` could be misinterpreted by CLI argument parser | Restructure: `['scrape', '--json', '--', ...urls]` |

### CI / Docker

| # | File | Issue | Fix |
|---|------|-------|-----|
| I20 | `.github/workflows/ci.yml:17-24` | `advisory-lock-policy` job uses `rg` — not installed on `ubuntu-latest` runners | Replace with `grep -rPn 'pg_advisory_lock\s*\(|pg_advisory_unlock\s*\(' crates/` |
| I21 | `docker/web/Dockerfile:4` | `corepack prepare pnpm@latest` — non-reproducible, breaks on pnpm breaking releases | Pin specific version: `pnpm@10.5.0` (or read from `package.json` `packageManager` field) |
| I22 | `docker-compose.yaml` (axon-web service) | `axon-web` is the only service without a `healthcheck` — `docker compose ps` always shows "running", future `condition: service_healthy` impossible | Add `wget -qO- http://127.0.0.1:49010/ \|\| exit 1` healthcheck |
| I23 | `docker/README.md` | `web/Dockerfile` not listed in Layout section | Add `web/Dockerfile: Next.js dev image for hot-reload development` |

### Test Coverage

| # | File | Issue | Fix |
|---|------|-------|-----|
| I24 | `crates/web/download.rs` (read_manifest) | Malformed JSONL skip behavior and `max_files` over-limit early return untested | Test: corrupt line → silently skipped; >2000 entries → `PAYLOAD_TOO_LARGE` |
| I25 | `crates/cli/commands/refresh.rs:734-739` | `looks_like_domain_seed()` has no tests — gates manifest vs literal refresh dispatch | Test: `https://example.com` → true; `/docs` path → false; `?q=1` query → false |
| I26 | `apps/web/lib/download-urls.ts` | Zero tests for `packMdUrl`, `packXmlUrl`, `archiveZipUrl`, `fileDownloadUrl` — route contract has no regression net | Add tests asserting exact URL shapes (catches route renames) |

---

## SUGGESTIONS — Nice to have (10 issues)

### Type Design

| # | File | Issue | Recommendation |
|---|------|-------|----------------|
| S1 | `crates/jobs/refresh.rs` (all job structs) | `status: String` on read side — `JobStatus` enum exists but only used for writes; raw string comparisons compile but silently fail | Add `#[sqlx(type_name = "text", rename_all = "lowercase")]` to `JobStatus` and use it on the `RefreshJob.status` field (one-type change, highest ROI) |
| S2 | `crates/jobs/refresh.rs:46-55` | `RefreshPageResult` has `not_modified: bool` + `changed: bool` simultaneously representable → illegal state | Replace with `enum RefreshOutcome { NotModified, Unchanged, Changed(String), Failed(u16) }` |
| S3 | `crates/cli/commands/job_contracts.rs` | `JobStatusResponse` and `JobSummaryEntry` are structurally identical 14-field structs; impossible field combinations per job type | Extract `JobBase` + discriminated `enum JobStatusResponse { Crawl, Extract, Ingest }` |
| S4 | `crates/jobs/refresh.rs` (`RefreshSchedule`) | `every_seconds: i64` accepts negatives and zero | Change to `u64` or `PositiveSeconds` newtype with `new(secs: u64) -> Option<Self>` |
| S5 | `apps/web/lib/pulse/types.ts` | `scrapedContext.url: z.string()` — inconsistent, other URL fields use `.url()` | Add `.url()` to `scrapedContext.url` |

### Minor Code Issues

| # | File | Issue | Recommendation |
|---|------|-------|----------------|
| S6 | `crates/vector/ops/commands/ask.rs:69` | `_query: &str` parameter name — underscore prefix means "unused" but it IS used on line 86 | Rename to `query` |
| S7 | `crates/web/execute/mod.rs:449` | `stdout_capture.await.ok().flatten()` converts task panic to `None` silently | Log task panics: `.unwrap_or_else(\|e\| { log_warn(&format!("stdout capture task panicked: {e}")); None })` |
| S8 | `crates/web/download.rs:114` | `Err(_) => continue` in `load_all_files` skips unreadable files silently — download pack is truncated with no log | Add `log_warn` with file path and error |
| S9 | `apps/web/app/api/pulse/save/route.ts:94-95` | Embed failure → `console.error` but response returns `{ saved: true }` — caller can't tell if document is searchable | Include `embedded: false` in response body when embed fails |
| S10 | `crates/vector/ops/commands/ask.rs` | `extract_cited_source_ids` loop guard `i + 3 < bytes.len()` — off-by-one, last 3 bytes never scanned | Change to `i + 2 < bytes.len()` |

---

## Issue Counts by Area

| Area | Critical | Important | Suggestion | Total |
|------|----------|-----------|------------|-------|
| Rust error handling / silent failures | 3 | 10 | 3 | 16 |
| Rust monolith / architecture | 3 | 4 | 1 | 8 |
| TypeScript security / validation | 2 | 3 | 1 | 6 |
| MCP schema / config | 3 | 0 | 1 | 4 |
| Test coverage | 2 | 3 | 0 | 5 |
| CI / Docker | 2 | 4 | 0 | 6 |
| Type design | 1 | 0 | 4 | 5 |
| Performance | 0 | 2 | 0 | 2 |
| **Total** | **16** | **26** | **10** | **52** |

> Note: C13 (CI Redis auth) + C14/C15 (stale README) are quick wins — fix in minutes.
> Highest-leverage fixes: C1–C3 (monolith splits) unblock everything downstream; C6 (sweep abort bug) and C4 (production panic) are the correctness-critical ones.
