# Task 7 Report: Watch Refresh Search Research Auto-Index Cutover

## Summary

Cut search/research auto-indexing off the crawl shim and onto direct Source job enqueue. Search and research result URLs now create bounded, page-scoped `JobKind::Source` rows through a new `enqueue_web_source_auto_index` helper, with `max_pages=1`, `max_depth=0`, sitemap discovery disabled, and caller headers stripped by construction.

Refresh already used Source jobs for ledger-registered origins. This task removed the remaining pre-ledger web fallback that replayed `crawl_start_with_context`; unregistered web origins now fail closed with a migration-required message instead of bypassing the Source pipeline. Ingest legacy fallback remains in place for pre-ledger ingest origins.

## Changes

- `crates/axon-services/src/search_source_index.rs`
  - Added a Source-backed auto-index helper for search/research result URLs.
  - Validates URLs before enqueue, uses trusted system auth, stamps `auto_index_reason` and `headers_policy=stripped`, and notifies unified workers.
- `crates/axon-services/src/search_crawl.rs`
  - Replaced `crawl_start_with_context` calls with `enqueue_web_source_auto_index`.
  - Search/research auto-index jobs are `SourceScope::Page`, `max_pages=1`, `max_depth=0`.
  - Preserved existing `crawl_jobs` / `auto_crawl_status` response field names for CLI/MCP/REST compatibility.
- `crates/axon-services/src/refresh.rs`
  - Removed legacy web crawl replay for origins without ledger registration.
  - Added fail-closed migration-required errors for web/source refresh when no Source ledger row or unified job store is available.
- `crates/axon-services/src/source_auto_index_cutover_tests.rs`
  - Added cutover regression coverage for Source job enqueue, zero Crawl jobs, sanitized headers, and Tailscale/private target rejection.
- CLI/MCP/Web text/comments
  - Updated user-facing and authorization comments from crawl jobs to source auto-index/source refresh wording while keeping compatibility JSON keys.

## Notes

- The public search/research payload still uses `crawl_jobs`, `crawl_jobs_rejected`, and `auto_crawl_status`. These are now compatibility aliases over Source jobs; renaming them should be a separate API migration.
- Broad `crawl_start_with_context` grep still finds crawl command compatibility, first-run setup, freshness scheduling, and action-dispatch paths. It no longer appears in search/research auto-index or refresh fallback code.

## Verification

- `cargo test -p axon-services source_auto_index_cutover -- --nocapture`
- `cargo test -p axon-services search_crawl -- --nocapture`
- `cargo test -p axon-services refresh -- --nocapture`
- `cargo test -p axon-jobs watch::dispatch -- --nocapture`
- `cargo test -p axon-jobs watch::orchestrate -- --nocapture`
- `cargo test -p axon-cli search -- --nocapture`
- `cargo test -p axon-mcp authz -- --nocapture`
- `cargo test -p axon-web exploration -- --nocapture`
- Changed-surface grep: `rg "crawl_start_with_context|JobKind::Crawl|UnifiedJobKind::Crawl" ...`
- Broad grep: `rg "crawl_start_with_context" crates/axon-services crates/axon-jobs crates/axon-cli crates/axon-mcp crates/axon-web -g '*.rs'`
- `cargo fmt --check`
- `git diff --check`
- `python scripts/enforce_monoliths.py --file <changed-file>`
- `cargo xtask check-layering`
