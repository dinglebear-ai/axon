# Session: Crawl Download & Repomix Pack for Web UI

**Date:** 2026-02-24
**Branch:** main
**Duration:** ~30 min

## Session Overview

Implemented HTTP download routes on the axum web server and corresponding frontend UI for downloading crawl results. Three download formats: Repomix-style packed Markdown, packed XML, and ZIP archive. Also added per-file download buttons in the crawl file explorer. This unblocks the primary use case of feeding crawled docs to LLMs.

## Timeline

1. Read all relevant backend files (`web.rs`, `execute/mod.rs`, `execute/files.rs`, `execute/polling.rs`) and frontend files (`next.config.ts`, `ws-protocol.ts`, `use-ws-messages.ts`, `crawl-file-explorer.tsx`, `results-panel.tsx`)
2. Added `dashmap = "6"` and `zip = "2"` deps to `Cargo.toml`
3. Created `crates/web/pack.rs` — pure pack format generators with 5 unit tests
4. Created `crates/web/download.rs` — 4 HTTP route handlers with security validation and 3 unit tests
5. Updated `crates/web.rs` — DashMap in AppState, 4 download routes, mod declarations, job_dirs tracking in WS forward task
6. Updated `crates/web/execute/files.rs` — `send_crawl_manifest()` now includes `job_id` in `crawl_files` WS message
7. Updated `crates/web/execute/polling.rs` — passes `Some(job_id)` to `send_crawl_manifest()`
8. Frontend: rewrite rule, ws-protocol type, download-urls helper, currentJobId state, toolbar component, explorer+results-panel updates
9. Verified: `cargo check`, `cargo clippy` (0 warnings), `cargo fmt --check` (clean), `cargo test --lib` (363 passed, 8 new), `biome check` (clean on new files)

## Key Findings

- `execute/` is a module directory (not a single file) with `mod.rs`, `files.rs`, `polling.rs`
- The `url` crate is available transitively through spider/reqwest — used `reqwest::Url` for domain extraction in `download.rs`
- Axum's `Router::merge()` allows download routes to have separate state (`Arc<DashMap>`) from the main app state (`Arc<AppState>`)
- The `crawl_files` WS message already contained `output_dir` — adding `job_id` was straightforward
- Pre-existing biome errors in `results-panel.tsx:LogViewer` (exhaustive deps lint on `lines`) — not introduced by this session

## Technical Decisions

1. **DashMap over RwLock<HashMap>**: DashMap provides lock-free concurrent reads which is ideal for the download routes (many readers, rare writers). No contention between download requests and job registration.
2. **Separate router state for downloads**: Download routes only need the `DashMap`, not the full `AppState`. Using `Router::merge()` with `with_state(job_dirs)` avoids coupling.
3. **ZIP in spawn_blocking**: ZIP compression is CPU-bound and must not block the tokio runtime. `spawn_blocking` provides panic safety and proper isolation.
4. **UUID validation before DashMap lookup**: Rejects traversal attempts (`../`, `%2F`, null bytes) at the format level before any filesystem interaction.
5. **job_id as Optional in crawl_files**: `send_crawl_manifest()` accepts `Option<&str>` for backward compatibility — the WS message only includes `job_id` when provided.
6. **Pack format escaping**: Markdown uses quad-backtick fences with breakout prevention (```````` → `\` \` \` \``). XML uses standard entity escaping for attrs and text.

## Files Modified

| File | Action | Purpose |
|------|--------|---------|
| `Cargo.toml` | modified | +`dashmap = "6"`, +`zip = "2"` |
| `crates/web/pack.rs` | **created** | Pure pack format generators (MD + XML) |
| `crates/web/download.rs` | **created** | 4 HTTP download route handlers |
| `crates/web.rs` | modified | DashMap in AppState, route registration, job_dirs tracking |
| `crates/web/execute/files.rs` | modified | `send_crawl_manifest()` now includes `job_id` |
| `crates/web/execute/polling.rs` | modified | Passes `Some(job_id)` to manifest sender |
| `apps/web/next.config.ts` | modified | `/download/:path*` rewrite to axon backend |
| `apps/web/lib/ws-protocol.ts` | modified | `job_id?: string` on `crawl_files` type |
| `apps/web/lib/download-urls.ts` | **created** | Pure URL constructors for 4 download routes |
| `apps/web/hooks/use-ws-messages.ts` | modified | `currentJobId` state from crawl_files/crawl_progress |
| `apps/web/components/crawl-download-toolbar.tsx` | **created** | Download buttons (Pack MD, Pack XML, ZIP) |
| `apps/web/components/crawl-file-explorer.tsx` | modified | `jobId` prop, per-file download icon |
| `apps/web/components/results-panel.tsx` | modified | Toolbar render, jobId prop pass |

## Commands Executed

| Command | Result |
|---------|--------|
| `cargo check` | Clean compilation |
| `cargo clippy` | 0 warnings (after fixing `//!` module doc) |
| `cargo fmt --check` | Clean |
| `cargo test --lib` | 363 passed, 0 failed (8 new tests) |
| `cargo test pack` | 5/5 passed |
| `cargo test download` | 3/3 passed |
| `biome check` (new files) | Clean |

## Behavior Changes (Before/After)

| Aspect | Before | After |
|--------|--------|-------|
| Crawl file download | Not possible — view only via WS file explorer | 4 download routes: pack.md, pack.xml, archive.zip, individual files |
| `crawl_files` WS message | `{type, files, output_dir}` | `{type, files, output_dir, job_id?}` |
| File explorer | View-only file list | Per-file download icon when jobId available |
| Results panel | No download UI | Download toolbar (Pack MD/XML/ZIP) after crawl completes |
| AppState | `{stats_tx}` | `{stats_tx, job_dirs}` |

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check` | Clean | Clean | PASS |
| `cargo clippy` | 0 warnings | 0 warnings | PASS |
| `cargo fmt --check` | Clean | Clean | PASS |
| `cargo test --lib` | All pass | 363 passed, 0 failed | PASS |
| `cargo test pack` | 5 tests pass | 5 passed | PASS |
| `cargo test download` | 3 tests pass | 3 passed | PASS |
| `biome check` (new TS) | Clean | Clean | PASS |

## Risks and Rollback

- **Risk**: Job directory registry (`DashMap`) is in-memory only — server restart loses all registered jobs. Downloads won't work until a new crawl completes.
- **Risk**: No size limit on pack/zip response body — a crawl with thousands of large pages could produce multi-GB responses. Mitigated by `AXON_DOWNLOAD_MAX_FILES` (default 2000 files).
- **Rollback**: Revert the commit. No database migrations, no config file changes, no infrastructure changes.

## Decisions Not Taken

- **Persistent job directory registry** (e.g., in Postgres or on-disk JSON): Rejected as over-engineering for the current use case. The DashMap is populated naturally when crawls complete through the WS flow.
- **Streaming ZIP response**: Rejected in favor of in-memory ZIP + `spawn_blocking`. Streaming would reduce memory but adds complexity. Current approach is simpler and sufficient for typical crawl sizes (<2000 files).
- **Server-sent events for download progress**: Not needed — downloads are synchronous HTTP GETs. The browser handles progress natively.

## Open Questions

- Should the job_dirs registry have a TTL/eviction policy to prevent unbounded growth on long-running servers?
- Should the download routes be behind authentication if the serve command is exposed on a network interface?
- Manual verification pending: run `axon serve`, crawl a site, test all 4 download links in browser, verify path traversal returns 400/403.

## Next Steps

- Manual end-to-end testing with `axon serve` + a real crawl
- Consider adding `AXON_DOWNLOAD_MAX_SIZE_MB` env var for total response size cap
- Consider persisting job_dirs to disk (simple JSON sidecar) for server restart resilience
