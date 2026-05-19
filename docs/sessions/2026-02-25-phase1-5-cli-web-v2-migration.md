# Session Log — Phase 1-5 CLI/Web v2 Migration

## 1. Session overview
- Completed a 5-phase migration from mixed legacy websocket/event handling to structured v2 contracts across Rust web execute + Next.js frontend.
- Executed with subagent-driven development: implementation batch per phase, then independent review, then targeted fix batch.
- Preserved compatibility where required in Phases 2-4, then removed planned legacy paths in Phase 5.
- Performed an additional two-agent review pass after completion and fixed newly surfaced drift.

## 2. Timeline of major activities
- Planned phase decomposition (5 phases, 8 bite-sized tasks each) and enforced TDD checkpoints.
- Phase 1: introduced v2 event schema types and contract tests (Rust + TS) and brought schema tests green.
- Phase 2: dual-emitted v2 and legacy runtime events; added artifact/job status/progress v2 channels.
- Phase 3: unified CLI job JSON contracts across crawl/extract/ingest using shared contract structs.
- Phase 4: migrated frontend runtime handling to v2 lifecycle/artifact/cancel flows with compatibility paths.
- Phase 5: removed planned legacy branches/fallbacks, expanded regressions, and ran full verification gates.
- Final extra 2-agent pass: fixed duplicate sync structured-output emission and frontend drift in lifecycle actions/fallback rendering.

## 3. Key findings with path:line references
- Sync command execution could emit duplicate `command.output.json` when both line parsing and full-stdout recovery parsed the same single-line JSON; fixed by gating recovery parse (`crates/web/execute/mod.rs:505`, `crates/web/execute/mod.rs:534`).
- Lifecycle entries could pollute normalized structured-result rendering; fixed by filtering lifecycle-shaped entries before normalization (`apps/web/components/results-panel.tsx:24`).
- v2 lifecycle correlation could fail if first job ID arrived via `job.status.metrics`; fixed by persisting job ID from lifecycle projection in provider/reducer flow (`apps/web/hooks/use-ws-messages.ts:170`).
- Shared CLI contract refactor initially risked dropping list-metadata richness (`result_json/urls_json/config_json`); compatibility aliases were restored in shared contract structs (`crates/cli/commands/job_contracts.rs:1`).
- Final clippy blockers were outside phase files and required cleanup to get global green (`crates/mcp/schema.rs:255`, `crates/mcp/server.rs:858`).

## 4. Technical decisions and rationale
- Adopted a typed v2 envelope (`command.*`, `job.*`, `artifact.*`) to remove heuristic parsing and make frontend behavior deterministic.
- Kept dual-emit during migration phases to avoid a flag day and reduce rollout risk.
- Consolidated CLI job responses into shared contract structs to prevent drift between crawl/extract/ingest JSON shapes.
- Deferred aggressive deletion of compatibility paths until dedicated cleanup phase to preserve intermediate stability.
- Used independent review agents between implementation batches to detect drift early and fix before proceeding.

## 5. Files modified/created and purpose
- `crates/web/execute/events.rs` (created): v2 websocket event schema definitions.
- `crates/web/execute/mod.rs`: v2 command event emission, cancel response handling, duplicate-output fix.
- `crates/web/execute/files.rs`: v2 artifact list/content emission paths.
- `crates/web/execute/polling.rs`: mode-agnostic `job.status`/`job.progress` emissions.
- `crates/web/execute/tests/ws_event_v2_tests.rs`: contract + regression tests for v2 runtime semantics.
- `crates/cli/commands/job_contracts.rs` (created): shared CLI job response contracts + adapters.
- `crates/cli/commands/{crawl,extract,ingest_common}.rs`: switched status/cancel/errors/list JSON construction to shared contracts.
- `apps/web/lib/ws-protocol.ts`: frontend v2 protocol unions/types/helpers.
- `apps/web/hooks/use-ws-messages.ts`: v2 lifecycle/artifact/cancel runtime handling.
- `apps/web/components/{omnibox.tsx,results-panel.tsx,results/job-lifecycle-renderer.tsx,results/raw-renderer.tsx}`: v2 consume paths, cancel payload/action updates, fallback removal/filtering.
- `apps/web/__tests__/{ws-protocol-v2,use-ws-messages,results-panel,result-normalizers}.test.ts` (new/updated): phase regressions and compatibility assertions.
- `crates/mcp/{schema.rs,server.rs}`: clippy cleanup required for global `-D warnings` green.

## 6. Critical commands executed and outcomes
- `cargo check` → passed repeatedly through phases; final pass green.
- `cargo test` → final full run passed (`384` tests + `1` doctest).
- `cargo clippy --all-targets --all-features -- -D warnings` → initially failed on two warnings, then passed after fixes.
- `pnpm --dir apps/web lint` → passed after frontend review fixes.
- `pnpm --dir apps/web exec tsc --noEmit` → passed.
- `pnpm --dir apps/web test` → passed (`13` files, `74` tests).

## 7. Behavior changes (before/after)
- Before: mixed legacy events and ad hoc parsing (`command_start/stdout_*`, crawl-specific progress, fallback JSON scraping).
- After: v2 typed runtime channels for command lifecycle, job lifecycle, artifact delivery, and cancel responses as primary behavior.
- Before: CLI job responses varied by command family and risked schema drift.
- After: shared CLI contracts produce consistent status/cancel/errors/list payloads with compatibility aliases retained.
- Before: possible duplicate structured output in sync path.
- After: single structured output event for single-line JSON sync output (covered by regression test).

## 8. Verification evidence (`command | expected | actual | status`)
- `cargo check | compile clean | Finished dev profile | PASS`
- `cargo test | all tests pass | 384 passed, 0 failed; doctest 1 passed | PASS`
- `cargo clippy --all-targets --all-features -- -D warnings | zero warnings/errors | initially failed (2 findings), then finished dev profile | PASS after fix`
- `pnpm --dir apps/web lint | no lint violations | Biome checked files, no fixes applied | PASS`
- `pnpm --dir apps/web exec tsc --noEmit | no TS errors | no output, exit 0 | PASS`
- `pnpm --dir apps/web test | all tests pass | 13 files, 74 tests passed | PASS`

## 9. Source IDs + collections touched (embed/retrieve source IDs, collections, outcomes)
- `axon status --json` attempted; failed at compile stage (`crates/mcp/server.rs` rmcp signature mismatch), no runtime status payload returned.
- `axon embed "/home/jmagar/workspace/axon_rust/docs/sessions/2026-02-25-phase1-5-cli-web-v2-migration.md" --json` attempted; failed at same compile stage before producing `data.url` / `data.collection`.
- `axon retrieve "UNKNOWN_SOURCE_ID_EMBED_FAILED" --collection "UNKNOWN_COLLECTION_EMBED_FAILED"` attempted per mandatory flow; failed at same compile stage.
- Axon outcome: **failure** (embed failed, verify failed). Source ID unavailable due embed failure.

## 10. Risks and rollback
- Remaining compatibility aliases in CLI contracts can mask downstream consumers still depending on old field names.
- Some v2/legacy coexistence paths may still exist outside audited scope; future cleanup should remove dead branches explicitly.
- Rollback: revert v2 runtime/frontend files together (web execute + ws protocol + message hook + renderer) to avoid protocol mismatch.
- Rollback: revert `job_contracts` adoption across all three command families atomically to avoid mixed schemas.

## 11. Decisions not taken
- Did not force a one-shot protocol cutover before compatibility period.
- Did not remove all crawl-specific legacy channels in early phases; deferred to avoid unstable migration windows.
- Did not commit phase-by-phase; worked in a single accumulating tree per user direction.

## 12. Open questions
- Should remaining compatibility aliases (`result_json`, `urls_json`, etc.) be formally deprecated with a removal date?
- Is there any external automation parsing legacy CLI not-found payloads that still needs migration guidance?
- Should lifecycle classification in frontend be moved from shape heuristics to strict discriminator-only input everywhere?

## 13. Next steps
- Create a single commit including all current workspace changes (including unrelated edits per user instruction).
- Optionally run one manual UI smoke flow: crawl → observe lifecycle → cancel → artifact retrieval.
- Add release notes documenting v2 protocol primacy and any compatibility fields retained.
