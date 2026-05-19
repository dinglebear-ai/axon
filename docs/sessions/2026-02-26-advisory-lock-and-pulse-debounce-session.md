# Session Log — Advisory Lock Hardening + Pulse Debounce

## 1. Session overview
- Investigated Postgres notice `you don't own a lock of type ExclusiveLock` observed during `axon crawl` enqueue.
- Audited advisory-lock usage and migrated schema init paths from session lock/unlock patterns to transaction-scoped locking helper.
- Added CI policy to reject `pg_advisory_lock(...)` and `pg_advisory_unlock(...)` in `crates/`.
- Investigated Pulse multi-send behavior and added debounce + one-shot guards in workspace prompt path; added autosave backpressure controls.

## 2. Timeline of major activities
- Reproduced and traced advisory-lock pattern via code search and file inspection.
- Patched `crawl`, `extract`, and `embed` schema init to use transaction-scoped advisory locks.
- Added shared schema helper and CI guardrail; added concurrency tests for schema init in crawl/extract/embed test modules.
- Investigated UI screenshot symptom (`PULSE error: exit code 1`) and traced repeated prompt submissions in Pulse workspace effect.
- Implemented frontend debounce/guard fixes and verified via web test suite.

## 3. Key findings with path:line references
- Session-scoped advisory lock/unlock was previously used in job schema init paths and can produce ownership notices with pooled connections.
- Shared schema lock helper now uses `pg_advisory_xact_lock` in a transaction: `crates/jobs/common/schema.rs:13`, `crates/jobs/common/schema.rs:26`.
- Crawl schema init now uses shared helper: `crates/jobs/crawl/runtime.rs:3`, `crates/jobs/crawl/runtime.rs:142`.
- Extract schema init now uses shared helper: `crates/jobs/extract.rs:3`, `crates/jobs/extract.rs:44`.
- Embed schema init now uses shared helper: `crates/jobs/embed.rs:6`, `crates/jobs/embed.rs:48`.
- CI now enforces advisory lock policy: `.github/workflows/ci.yml:13`, `.github/workflows/ci.yml:22`.
- Crawl URL local-file confusion warning is non-blocking and heuristic-only: `crates/cli/commands/crawl.rs:37`, `crates/cli/commands/crawl.rs:64`.
- Pulse workspace had repeat-trigger risk from effect dependencies; one-shot version guard is now present: `apps/web/components/pulse/pulse-workspace.tsx:34`, `apps/web/components/pulse/pulse-workspace.tsx:70`.
- Workspace prompt submit now debounced (`250ms`): `apps/web/hooks/use-ws-messages.ts:119`, `apps/web/hooks/use-ws-messages.ts:502`.
- Autosave now aborts in-flight requests and dedupes unchanged snapshots: `apps/web/components/pulse/pulse-workspace.tsx:32`, `apps/web/components/pulse/pulse-workspace.tsx:162`.

## 4. Technical decisions and rationale
- Chose transaction-scoped advisory locks (`pg_advisory_xact_lock`) to guarantee auto-release on commit/rollback and avoid session mismatch with pooled DB connections.
- Centralized schema-lock bootstrapping in a common helper to reduce drift and make lock policy enforceable.
- Added CI static guard to prevent regression to session lock/unlock APIs.
- Debounced workspace prompt submission at provider boundary to collapse rapid submits into one event.
- Added one-shot `workspacePromptVersion` processing guard in Pulse workspace to prevent duplicate fetches from rerenders/state updates.

## 5. Files modified/created and purpose
- Created `crates/jobs/common/schema.rs` to encapsulate migration transaction + advisory xact lock.
- Updated `crates/jobs/common/mod.rs` to export schema helper.
- Updated `crates/jobs/crawl/runtime.rs`, `crates/jobs/extract.rs`, `crates/jobs/embed.rs` to use shared migration helper.
- Updated `.github/workflows/ci.yml` to add advisory lock policy check.
- Updated `crates/jobs/crawl/runtime/tests.rs`, `crates/jobs/extract/tests.rs`, `crates/jobs/embed/tests.rs` with concurrency-safe schema init tests.
- Updated `crates/cli/commands/crawl.rs` with non-blocking local-file-like URL warning heuristic.
- Updated `apps/web/hooks/use-ws-messages.ts` with workspace prompt debounce.
- Updated `apps/web/components/pulse/pulse-workspace.tsx` with one-shot prompt processing and autosave abort/dedupe.

## 6. Critical commands executed and outcomes
- `rg -n "pg_advisory_lock|pg_advisory_unlock|pg_advisory_xact_lock|..."` found advisory lock usage in crawl/extract/embed paths.
- `cargo fmt && cargo check` succeeded after lock refactor (observed compile success).
- `cargo test` previously completed with `405` tests passing during lock-hardening phase.
- `cargo check -q` later passed with warnings in `crates/mcp/server.rs` (unused variable/function warnings).
- Targeted Rust tests passed: `extract_ensure_schema_is_concurrency_safe`, `embed_ensure_schema_is_concurrency_safe`, `crawl_ensure_schema_is_concurrency_safe`.
- Frontend tests passed: `npm --prefix apps/web test` -> `13` test files, `74` tests, all passed.
- `axon status` failed with DB syntax error (`42601`) in embed status lookup.
- `axon embed "...session.md" --json` failed with DB syntax error (`42601`), no job ID emitted.
- `axon retrieve "...session.md"` returned `No content found for URL`.

## 7. Behavior changes (before/after)
- Before: schema init used session lock/unlock patterns vulnerable to lock ownership notices with pooled sessions.
- After: schema init in crawl/extract/embed runs under transaction-scoped advisory lock helper with DB-local timeouts.
- Before: no CI policy blocking session advisory lock API usage.
- After: CI fails if `pg_advisory_lock`/`pg_advisory_unlock` appears in `crates/`.
- Before: Pulse workspace could re-trigger chat request for same prompt version under effect churn.
- After: each prompt version processes once; submit path is debounced; autosave drops stale in-flight requests.

## 8. Verification evidence (`command | expected | actual | status`)
- `cargo check -q | compiles | compile succeeded with warnings only | PASS`
- `cargo test -q extract_ensure_schema_is_concurrency_safe | test passes | 1 passed | PASS`
- `cargo test -q embed_ensure_schema_is_concurrency_safe | test passes | 1 passed | PASS`
- `cargo test -q crawl_ensure_schema_is_concurrency_safe | test passes | 1 passed | PASS`
- `npm --prefix apps/web test | web tests green | 13 files / 74 tests passed | PASS`
- `axon status | preflight status available | failed: embed status lookup DB syntax error 42601 | FAIL`
- `axon embed "<session-path>" --json | queued job with data.job_id | failed: DB syntax error 42601, no job id | FAIL`
- `axon retrieve "<session-path>" | indexed content returned | no content found for URL | FAIL`

## 9. Source IDs + collections touched (embed/retrieve source IDs, collections, outcomes)
- Embed outcome: FAILED (`42601` syntax error at/near "(") during DB operation; no `data.job_id` returned.
- Source ID used for retrieve attempt: `/home/jmagar/workspace/axon_rust/docs/sessions/2026-02-26-advisory-lock-and-pulse-debounce-session.md`.
- Collection used: not explicitly provided (default CLI resolution).
- Retrieve outcome: failed to verify (`No content found for URL`).

## 10. Risks and rollback
- Risk: repository already contains many unrelated modified files; commit scoping must be explicit.
- Risk: ingest schema path lock hardening status is uncertain from latest inspection and should be re-validated separately.
- Rollback (targeted): revert only touched files listed in section 5, leaving unrelated workspace changes intact.
- Rollback for frontend debounce: revert `apps/web/hooks/use-ws-messages.ts` and `apps/web/components/pulse/pulse-workspace.tsx` only.

## 11. Decisions not taken
- Did not block `https://*.md` crawls; warning remains informational only when matching local filename exists.
- Did not introduce broad global input debounce in omnibox typing itself; implemented backpressure at workspace submission and save paths.
- Did not run a full monorepo test matrix in this turn; ran targeted Rust tests and full web tests.

## 12. Open questions
- Is ingest schema init intended to be migrated to shared schema lock helper in the final state? Latest file inspection did not confirm helper usage.
- Should the workspace prompt debounce interval (`250ms`) be configurable via UI setting/env?
- Should autosave embed be optional for large docs to reduce TEI/Qdrant load?

## 13. Next steps
- Validate ingest schema init path and align with shared helper if still divergent.
- Add a focused test for workspace prompt dedupe behavior (same version processed once).
- Add telemetry counter for debounced/drop events in Pulse submit and autosave paths.
- If preparing PR, stage only files from section 5 to avoid unrelated workspace drift.
