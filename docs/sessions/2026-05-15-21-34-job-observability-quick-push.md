# Job Observability Quick Push

Date: 2026-05-15 21:34 -0400
Branch: `feat/crawl-status-error-diagnostics`
Pushed commit: `fc2cf975 feat: improve job observability`

## Summary

Implemented and pushed the job/crawl observability bundle requested through `lavra-work-many`:

- `axon_rust-ehh7`: attempt metadata for watchdog retries.
- `axon_rust-uw2f`: local cancellation-token cleanup during periodic reclaim.
- `axon_rust-k9jv`: structured crawl diagnostics.
- `axon_rust-13zj`: lifecycle documentation and crawl renderer coverage.

The quick-push also included pre-existing dirty-tree work that was already present before the final push, including API parity docs/tests, environment migration docs/tests, ask follow-up changes, benchmark docs/scripts, and query/config help updates.

## Changed Areas

- Added SQLite migration `0005_add_attempt_metadata.sql` for attempt count, active attempt IDs, and reclaim metadata on job tables.
- Made worker claim, heartbeat, progress, result, and terminal writes attempt-aware so stale attempts cannot overwrite a reclaimed retry.
- Changed reclaim to return exact reclaimed job IDs and cancel matching in-process tokens before waking retry workers.
- Added bounded structured crawl diagnostics to crawl summaries, final job result JSON, and `axon crawl errors`.
- Updated crawl/job lifecycle docs and command docs.
- Added/updated focused tests for lifecycle attempts, watchdog token cancellation, crawl diagnostics, status/service metadata, API parity, and env migration behavior.

## Verification

Before commit:

- `cargo fmt --check`
- `cargo check`
- `git diff --check`

Focused tests from the implementation pass:

- `cargo test jobs::lite::ops::tests`
- `cargo test jobs::lite::workers::tests`
- `cargo test jobs::lite::workers::runners::crawl::tests`
- `cargo test status::tests`

Pre-commit hook on `fc2cf975` also passed:

- compose ports
- monolith policy
- rustfmt
- MCP HTTP check
- env guard
- unwrap warning check
- CLAUDE/AGENTS/GEMINI symlink check
- no `mod.rs`
- clippy
- test compile

The unwrap check reported warn-only new unwrap/expect calls in staged files:

- `src/cli/commands/ask/followup.rs`
- `src/jobs/lite/store.rs`
- `src/jobs/lite/workers.rs`

## Beads

Closed during the implementation session:

- `axon_rust-ehh7`
- `axon_rust-uw2f`
- `axon_rust-k9jv`
- `axon_rust-13zj`

`bd` emitted an auto-export `git add` warning during Bead closure while the repo was still dirty. The later quick-push staged, committed, rebased, and pushed the full tree successfully.

## Repository State

After the quick-push:

- Branch `feat/crawl-status-error-diagnostics` was synced with `origin/feat/crawl-status-error-diagnostics`.
- Working tree was clean.
- Latest commit was `fc2cf975 feat: improve job observability`.

## Open Questions

- The quick-push intentionally included the whole dirty tree. Some included files were not part of the four crawl/job observability Beads, especially ask/config/API parity work. Reviewers should treat the commit as a bundled local-state push rather than a narrowly scoped single-feature patch.
- The warn-only unwrap/expect additions were not remediated before push because the configured hook allows them. They may still be worth reducing in a follow-up cleanup.
