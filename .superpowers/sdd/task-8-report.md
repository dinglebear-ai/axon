# Task 8 Report: Uniform Source Progress Events And Metrics

## Summary

Added durable, public Source progress events for the unified web source path and bounded-label source pipeline metrics in `axon-observe`.

Source-backed web jobs now emit one ordered event per Task 8 phase on the same claimed Source job id: resolving, routing, authorizing, discovering, diffing, fetching, normalizing, preparing, embedding, upserting, publishing, cleaning, and complete. The event helper stamps monotonic sequences through the unified `JobStore`, uses public visibility so REST/CLI event readers can see the events, and records metrics with only bounded labels.

## Changes

- `crates/axon-observe/src/source_metrics.rs`
  - Added `record_source_phase_with_labels`.
  - Rejects unsupported/high-cardinality labels such as `url`.
  - Emits `axon_source_phase_total` with bounded labels only.
- `crates/axon-services/src/source/events.rs`
  - Added `SourceEventEmitter`.
  - Persists public `SourceProgressEvent` rows through `JobStore`.
  - Uses `latest_event_sequence + 1` so SQLite and fake stores both get monotonic events.
  - Records source phase metrics even when no durable job id is available.
- `crates/axon-services/src/source.rs`
  - Emits resolving/routing/authorizing/cleaning/complete events around the shared Source orchestrator.
  - Emits failed progress events for routing/auth/data-plane failures when a Source job id exists.
- `crates/axon-services/src/source/routing.rs`
  - Extracted the route/auth preflight from `source.rs` to keep the orchestrator below monolith limits.
  - Preserves the same route, credential, safety, local-source, and unsupported-kind failures while emitting the matching progress events.
- `crates/axon-services/src/source/security.rs`
  - Moved SSRF/local-source guard helpers out of `source.rs` and re-exported them from `source` to preserve existing callers/tests.
- `crates/axon-services/src/source/batch.rs`
  - Moved the source batch-planning helper and tests out of `source.rs` to satisfy repo file-size policy.
- `crates/axon-services/src/web_source.rs`
  - Emits web-source discovering/diffing/fetching/normalizing/preparing/embedding/upserting/publishing events on `WebSourceIndexInput.job_id`.
  - Threads an optional event store through `WebSourceIndexInput`.
- `crates/axon-services/src/source/dispatch.rs`
  - Supplies the unified job store to web source indexing.
- `crates/axon-services/src/source_observability_tests.rs`
  - Added ordered phase event coverage.
  - Added service/direct event-page parity coverage.
  - Added high-cardinality metric label rejection coverage.

## Notes

- REST `/v1/jobs/{id}/events` and CLI `axon jobs events` already read `axon_services::jobs::unified_job_events`, so no duplicate REST/CLI event source was added. The new parity test verifies the service event page is the same durable store page.
- Direct `index_web_source` unit tests set `event_store=None`; Source-dispatched web indexing supplies the store and gets durable events.
- The plan's `cargo test -p axon-services --test source_observability` command did not match the repository's in-crate test layout. The implemented focused test command is `cargo test -p axon-services source_observability -- --nocapture`.

## Verification

- Initial failing check: `cargo test -p axon-services source_observability -- --nocapture`
- `cargo test -p axon-services source_observability -- --nocapture`
- `cargo test -p axon-services source_web -- --nocapture`
- `cargo test -p axon-services source_security -- --nocapture`
- `cargo test -p axon-services source_batch -- --nocapture`
- `cargo test -p axon-services source_routing -- --nocapture`
- `cargo test -p axon-observe source_metrics -- --nocapture`
- `cargo fmt --check`
- `git diff --check`
- `cargo xtask check-layering`
- `python scripts/enforce_monoliths.py --file crates/axon-services/src/source.rs --file crates/axon-services/src/source/routing.rs --file crates/axon-services/src/source/security.rs --file crates/axon-services/src/source/batch.rs --file crates/axon-services/src/source/events.rs --file crates/axon-services/src/web_source.rs --file crates/axon-observe/src/source_metrics.rs --file crates/axon-services/src/source_observability_tests.rs`
