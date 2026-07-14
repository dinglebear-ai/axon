# Task 5 Report: Source-Backed Site And Docs Crawl Execution

## Summary

Implemented the crawl execution cutover so `crawl_start_with_context` is now a compatibility shim that enqueues detached `JobKind::Source` rows carrying `SourceRequest { scope: site }` instead of durable `JobKind::Crawl` rows. The SourceRunner now owns execution, web-source indexing uses the claimed Source job id, and the default runner registry intentionally leaves `Crawl` unregistered.

Also migrated watch-triggered re-crawls onto the same SourceRequest contract so the watch scheduler does not enqueue jobs that no runner claims after the CrawlRunner removal.

## Changes

- `crates/axon-services/src/crawl.rs`
  - Enqueues one detached Source job per URL via `source::enqueue::enqueue_source`.
  - Preserves crawl defaults in `SourceRequest.limits`.
  - Snapshots web adapter options into `SourceRequest.options`, including explicit empty list overrides for whitelist/blacklist/auto-dispatch skip.
- `crates/axon-services/src/source/dispatch.rs`
  - Threads `limits.max_depth` into web dispatch.
  - Uses the claimed Source job id for `WebSourceIndexInput` when SourceRunner is executing.
- `crates/axon-services/src/runtime/job_runners.rs`
  - Removed default CrawlRunner registration; Source remains registered.
- `crates/axon-jobs/src/watch/dispatch.rs`
  - Watch-triggered recrawls now enqueue `JobKind::Source` with `SourceIntent::Refresh`, `scope=site`, and source limits.
- `crates/axon-core/src/http/ssrf.rs`
  - Blocks RFC 6598 carrier-grade NAT `100.64.0.0/10`, including IPv4-mapped IPv6.
- Tests updated/added across crawl, search-crawl, watch, source runner, source-web identity/artifacts, and SSRF coverage.

## Notes

- The legacy `crawl_status/list/cancel/cleanup/clear/recover` bridge still targets historical `JobKind::Crawl` rows. New crawl submissions produce Source job ids; moving the user-facing crawl status surface to Source-aware lookup remains a follow-up.
- Hermetic multi-page site/docs crawl fixture tests are still constrained by the current Spider/loopback SSRF boundary noted in `web_source_tests.rs`; existing source-web tests cover the Source job identity, no child Crawl/Embed rows, commit fences, reuse, artifacts, and map scope.

## Verification

- `cargo test -p axon-services crawl_start_with_context -- --nocapture`
- `cargo test -p axon-services crawl_start_snapshots_effective_max_pages_at_enqueue_boundary -- --nocapture`
- `cargo test -p axon-services source::dispatch::web_options -- --nocapture`
- `cargo test -p axon-services source_runner -- --nocapture`
- `cargo test -p axon-services source_web -- --nocapture`
- `cargo test -p axon-services crawl::tests -- --nocapture`
- `cargo test -p axon-jobs watch::dispatch -- --nocapture`
- `cargo test -p axon-jobs watch::orchestrate -- --nocapture`
- `cargo test -p axon-jobs watch_first_run_seeds_source_crawl_and_writes_artifact -- --nocapture`
- `cargo test -p axon-jobs live_watch_only_recrawls_when_page_changes -- --nocapture`
- `cargo test -p axon-core validate_url -- --nocapture`
- `cargo xtask check-layering`
- `cargo fmt --check`
- `git diff --check`
- `python scripts/enforce_monoliths.py --file crates/axon-services/src/crawl.rs --file crates/axon-services/src/source/dispatch.rs --file crates/axon-services/src/source_web_artifacts_tests.rs --file crates/axon-jobs/src/watch/dispatch.rs --file crates/axon-core/src/http/ssrf.rs`
