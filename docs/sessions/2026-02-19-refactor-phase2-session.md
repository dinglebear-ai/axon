# Session Log - Refactor Phase 2 and Default Switch

## 1. Session overview
- Completed parallel Phase 2 refactors for `vector` and `crawl-jobs` tracks with subagents and verification gates.
- Enforced and verified legacy-preservation safety model during migration (`*_legacy.rs` snapshots kept intact).
- Fixed review issues before proceeding: env leakage in tests, brittle string-based wiring tests, side-effect-only v2 wrappers, and duplicated legacy logic in tests.
- Drove both tracks to full completion criteria for v2 implementation and then flipped default dispatch to v2.
- Verified compile and focused test suites after each milestone.

## 2. Timeline of major activities
- Added monolith guardrails (CI + local hooks), introduced `lefthook`, and documented policy.
- Created v2/legacy/dispatch scaffolding for `vector` and `crawl-jobs` with hash guard for legacy snapshots.
- Executed Phase 2 migrations with RED->GREEN->REFACTOR evidence in both tracks.
- Performed review-driven corrections and re-verification.
- Completed full-migration pass to remove runtime legacy delegation from v2 modules and switched defaults to v2.

## 3. Key findings with `path:line` references when relevant
- Vector dispatch now defaults to v2 when env is unset: `crates/vector/ops_dispatch.rs:13`.
- Crawl-jobs dispatch now defaults to v2 when env is unset: `crates/jobs/crawl_jobs_dispatch.rs:14`.
- Guard test prevents `ops` function-level legacy delegation: `tests/vector_v2_no_legacy_calls.rs:2`.
- Guard test prevents `crawl_jobs` module-level legacy function calls: `tests/crawl_jobs_migration.rs:2`.
- Env restoration guard prevents test cross-contamination: `tests/vector_v2_input_parity.rs:9`.

## 4. Technical decisions and rationale
- Used dispatch-by-env architecture to keep rollback immediate (`legacy` remains selectable via env vars).
- Required RED->GREEN->REFACTOR for migration slices to avoid silent behavior drift.
- Prioritized deterministic helper/business-logic tests over network-coupled integration tests for reliable CI signal.
- Removed side-effect-only preflight/warmup calls in v2 wrapper entrypoints to avoid hidden behavior/perf changes.
- Switched defaults to v2 only after passing focused tests and no-legacy-call guard checks.

## 5. Files modified/created and purpose
- `crates/vector/ops_dispatch.rs`: dispatch default switched to v2; dispatch tests updated.
- `crates/jobs/crawl_jobs_dispatch.rs`: dispatch default switched to v2; dispatch tests updated.
- `tests/vector_v2_input_parity.rs`: default-dispatch behavior assertions updated; env guard introduced.
- `tests/vector_v2_no_legacy_calls.rs`: migration guard for vector v2 legacy-call regression.
- `tests/crawl_jobs_migration.rs`: migration guard for crawl-jobs v2 legacy-call regression.
- `crates/vector/ops/{commands.rs,qdrant.rs,ranking.rs,stats.rs,tei.rs,input.rs,mod.rs}`: v2 implementation completion for vector flows.
- `crates/jobs/crawl_jobs/{mod.rs,repo.rs,watchdog.rs,worker.rs,processor.rs,sitemap.rs,manifest.rs}`: v2 implementation completion for crawl-jobs flows.

## 6. Critical commands executed and outcomes
- `cargo fmt --all` -> completed successfully multiple times.
- `cargo check` -> passed after migration and default switch (warnings only in staged v2 modules).
- `cargo test --test vector_v2_no_legacy_calls -- --nocapture` -> passed after vector full migration.
- `cargo test --test crawl_jobs_migration -- --nocapture` -> passed after crawl-jobs full migration.
- `cargo test --test vector_v2_input_parity -- --nocapture` -> passed (4 tests).
- `cargo test --test vector_v2_ranking_migration -- --nocapture` -> passed (3 tests).
- `cargo test --test vector_v2_qdrant_migration -- --nocapture` -> passed (2 tests).
- `cargo test 'crawl_jobs::' -- --nocapture` -> passed (9 tests at final run).

## 7. Behavior changes (before/after)
- Before: dispatch defaults were legacy for both vector and crawl-jobs.
- After: dispatch defaults are v2 for both vector and crawl-jobs.
- Before: several v2 modules forwarded function calls into legacy implementations.
- After: no function-level legacy delegation remains in `crates/vector/ops/**` and no legacy function calls remain in `crates/jobs/crawl_jobs/mod.rs` (guarded by tests).
- Before: vector parity tests could leak env var state on panic.
- After: env state is restored via RAII guard in tests.

## 8. Verification evidence (`command | expected | actual | status`)
- `cargo fmt --all | formatting succeeds | completed without errors | PASS`
- `cargo check | compiles workspace | finished dev profile, warnings only | PASS`
- `cargo test --test vector_v2_input_parity -- --nocapture | 4 pass | 4 passed, 0 failed | PASS`
- `cargo test --test vector_v2_ranking_migration -- --nocapture | 3 pass | 3 passed, 0 failed | PASS`
- `cargo test --test vector_v2_qdrant_migration -- --nocapture | 2 pass | 2 passed, 0 failed | PASS`
- `cargo test --test vector_v2_no_legacy_calls -- --nocapture | guard passes | 1 passed, 0 failed | PASS`
- `cargo test 'crawl_jobs::' -- --nocapture | crawl v2 tests pass | 9 passed, 0 failed | PASS`
- `cargo test --test crawl_jobs_migration -- --nocapture | guard passes | 1 passed, 0 failed | PASS`

## 9. Source IDs + collections touched (embed/retrieve source IDs, collections, outcomes)
- `axon embed "docs/sessions/2026-02-19-refactor-phase2-session.md" --json` returned queued job only: `job_id=ec5d582c-9a80-4c11-9681-a695a2be3c01`, `status=pending` (no `data.url` field present).
- `axon embed ... --wait true --json` returned: `chunks_embedded=4`, `collection=cortex` (still no `data.url` field in command output).
- `axon status --json` for embed job `ec5d582c-9a80-4c11-9681-a695a2be3c01` reported `result_json.source=rust`, `result_json.collection=cortex`, `docs_embedded=1`, `chunks_embedded=1`.
- Retrieve attempt using observed values: `axon retrieve "rust" --collection "cortex"` -> `No content found for URL: rust` (verify failed).
- Axon result for this workflow: partial failure (`embed` succeeded, retrieve verification failed).

## 10. Risks and rollback
- Risk: v2 defaults may surface latent behavioral differences in edge cases not covered by deterministic tests.
- Risk: remaining warnings in `crawl_jobs` indicate still-unused internals and potential dead paths.
- Rollback: set `AXON_VECTOR_IMPL=legacy` to force legacy vector path.
- Rollback: set `AXON_CRAWL_JOBS_IMPL=legacy` to force legacy crawl-jobs path.
- Rollback safety: legacy snapshots remain present (`crates/vector/ops_legacy.rs`, `crates/jobs/crawl_jobs_legacy.rs`) with hash verification script in repo.

## 11. Decisions not taken
- Did not remove legacy modules/snapshots during this session.
- Did not force defaults via hard deletion of env switches; kept runtime override control.
- Did not add network-dependent parity tests; prioritized deterministic local tests for repeatability.
- Did not address all workspace warnings globally; focused on migration-critical paths.

## 12. Open questions
- Should legacy snapshots remain indefinitely or be retired after a freeze period?
- Should `v2` dispatch become non-configurable after a stabilization window?
- What production/staging soak duration is required before pruning legacy code?
- Should dead-code warnings in `crawl_jobs` be elevated to errors now or after cleanup slices?

## 13. Next steps
- Run targeted smoke tests in real environments with default v2 and fallback toggles documented.
- Add integration tests for high-risk flows (worker lanes, stale reclaim behavior, embed/query over realistic fixtures).
- Remove or wire currently-unused v2 internals to eliminate dead-code warnings.
- Plan legacy deprecation timeline (announce, freeze, delete) after stability window.
