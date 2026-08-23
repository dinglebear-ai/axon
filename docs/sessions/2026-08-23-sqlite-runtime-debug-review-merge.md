---
date: 2026-08-23 08:40:50 EDT
repo: git@github.com:dinglebear-ai/axon.git
branch: main
head: ad842bd396423e632d0f6281e836dea0d5bcd280
working directory: /home/jmagar/workspace/axon
worktree: /home/jmagar/workspace/axon
pr: "#576 fix(sqlite): prevent scheduler write starvation (https://github.com/dinglebear-ai/axon/pull/576)"
---

# SQLite runtime debugging, review, and merge

## User Request

Debug and resolve unified job `2138dfdd-c6dd-4187-91cc-fb0d68ba1e8f`, address the related issues, commit and push the fixes, create or update a PR, review it, fix every surfaced issue, and merge it into `main`.

## Session Overview

The session diagnosed an unsupported historical `jobs/7` migration receipt, hardened the SQLite scheduler and embedding cache paths, restored legacy job error decoding, added migration/reset/regression coverage, and updated generated contracts. The changes were reviewed repeatedly, pushed to PR #576, passed the exact-head CI matrix, and were squash-merged to `main` as `ad842bd39`.

## Sequence of Events

1. Inspected the failed job and traced startup failure to the unsupported `jobs/7` receipt.
2. Implemented scheduler write-gating, source batching, persistent embedding caching, migration compatibility, and reset fixes.
3. Ran focused and crate-level tests, formatting, Clippy, generated-contract checks, and repository hooks.
4. Reviewed PR #576 with code, test, and silent-failure reviewers; fixed every actionable finding.
5. Fixed exact-head CI's workspace-only constant-assertion lint failure with a compile-time assertion.
6. Waited for the full hosted matrix, including live RAG, then merged PR #576 into `main`.

## Key Findings

- `embedding_vector_cache_state` was bookkeeping but reset inventory treated it as job content, making a newly migrated database appear non-empty.
- Cache replacement retained stale `created_at`, so expired or corrupt entries could remain unreadable or be deleted by delayed retirement.
- Millisecond timestamps could collide during replacement; material replacements now advance the retirement fence monotonically.
- Root job rows decoded historical compact errors strictly even though attempt and stage rows already used compatibility decoding.
- Detached cache mutation tasks could saturate or fail without sufficient lifecycle observation.

## Technical Decisions

- Preserve fixed TTL for identical warm cache writes, but advance the generation token for expired or materially changed entries.
- Fence retirement by the observed cache generation and guarantee replacement uses `MAX(new_time, old_time + 1)`.
- Classify cache-state singleton rows as reset bookkeeping rather than user data.
- Decode historical job errors through the compatibility path at every job retrieval boundary.
- Keep cache touches detached from request latency while observing terminal task failures.

## Files Changed

PR #576's squash commit changed 98 files: 13 created and 85 modified. The authoritative complete inventory is `git show --name-status --format='' ad842bd396423e632d0f6281e836dea0d5bcd280`. The affected surfaces were:

| status | paths | purpose | evidence |
|---|---|---|---|
| created | `crates/axon-document/src/markdown/{semantics,windowing}.rs` and related tests | Structured markdown semantics and windowing | merge commit |
| created | `crates/axon-embedding/src/cache.rs`, `cache_tests.rs` | Runtime embedding-cache orchestration | merge commit |
| created | `crates/axon-jobs/src/embedding_cache_store.rs`, `embedding_cache_store_tests.rs` | Durable cache storage and generation-aware retirement | merge commit |
| created | `crates/axon-jobs/src/migrations/0007_embedding_vector_cache.sql`, `0008_embedding_vector_cache_expiry.sql` | Cache schema, state, expiry, and triggers | merge commit |
| created | `crates/axon-services/src/source/executor/created_generation/{batches,batches_tests}.rs` | Bounded source batching | merge commit |
| created | `crates/axon-services/src/source/executor/vectorize/{batching,pipeline,pipeline_tests}.rs` | Pipelined vectorization | merge commit |
| modified | `.github/workflows/ci.yml`, `Cargo.lock`, `config.example.toml` | CI and configuration contracts | merge commit |
| modified | `crates/axon-adapters/**`, `crates/axon-core/**`, `crates/axon-document/**` | Adapter scope, cache tuning, and chunking behavior | merge commit |
| modified | `crates/axon-embedding/**`, `crates/axon-jobs/**` | Cache lifecycle, scheduler, migrations, and legacy codec | merge commit |
| modified | `crates/axon-services/**`, `src/main.rs`, `src/main_tests.rs` | Runtime composition, reset inventory, pipeline execution, and stack invariant | merge commit |
| modified | `docs/pipeline-unification/**`, `docs/reference/**` | Generated and authored runtime/config/schema documentation | merge commit |
| modified | `scripts/check-env-config-boundary.py`, `xtask/**` | Contract validation and snapshots | merge commit |

## Beads Activity

No bead activity was observed for this session. `bd list --all --sort updated --reverse --limit 30 --json` returned older, unrelated closed issues; no bead was created, edited, or closed.

## Repository Maintenance

- Plans: inspected `docs/plans/`; none was clearly tied to and completed by this session, so no files were moved.
- Beads: inspected current tracker output; no session bead existed and the merged work had no remaining tracked defect requiring a new bead.
- Worktrees: preserved `/home/jmagar/workspace/axon/.claude/worktrees/connected-tools-not-exposed-0e199c` because it is a separately registered worktree with unclear active ownership.
- Branches: preserved unrelated local branches even where their upstream was gone; no destructive cleanup was needed to save the artifact.
- Stale docs: PR #576 refreshed generated configuration, database, source, and dependency contracts; exact-head contract CI passed.

## Tools and Skills Used

- Shell and file tools: inspected Git state, source, migrations, test output, workflow logs, and applied scoped patches.
- Git and GitHub CLI: committed, pushed, updated and inspected PR #576, watched checks, and verified the merge.
- Skills: `superpowers:systematic-debugging`, `vibin:gh-pr`, `vibin:review-pr`, `vibin:merge-status`, and `vibin:save-to-md` guided diagnosis, review, merge gating, and session capture.
- Review agents: code review, test review, and silent-failure review surfaced cache races, reset inventory drift, missing compatibility decoding, observability gaps, and test gaps.
- No browser tools or external MCP mutations were used for the repository changes.

## Commands Executed

| command | result |
|---|---|
| `cargo test -p axon-jobs --lib --locked` | 184 passed |
| `cargo test -p axon-embedding --lib --locked` | 59 passed |
| `cargo clippy --workspace --all-targets --locked --features test-helpers -- -D warnings` | passed after compile-time assertion fix |
| `cargo xtask generated-contracts check` | generated contracts current |
| `gh pr checks 576 --watch --interval 10` | exact-head CI and live RAG passed |
| `gh pr merge 576 --squash --delete-branch` | PR already merged; synchronized local `main` |

## Errors Encountered

- Startup failed with `startup.incompatible_store` for unknown receipt `jobs/7`; migration compatibility and reset behavior were corrected.
- Cache replacement tests exposed stale timestamp and delayed-retirement races; conditional replacement and a monotonic generation fence resolved them.
- Hosted workspace Clippy rejected a runtime assertion over a constant; `const _: () = assert!(...)` moved enforcement to compile time.

## Behavior Changes (Before/After)

| area | before | after |
|---|---|---|
| historical stores | `jobs/7` could block runtime startup | supported migration path upgrades cache state safely |
| cache replacement | stale values could remain expired or be retired after recomputation | material replacements advance a fenced generation |
| reset inventory | cache bookkeeping made empty stores appear non-empty | singleton state is excluded from user content |
| legacy errors | compact root job errors could fail `jobs get` decoding | compatibility decoding covers root jobs, attempts, and stages |
| cache maintenance | saturation and detached failures could be silent | admission and terminal failures are observed |

## Verification Evidence

| command | expected | actual | status |
|---|---|---|---|
| `cargo test -p axon-jobs --lib --locked` | job/cache regressions pass | 184 passed | pass |
| `cargo test -p axon-embedding --lib --locked` | cache provider tests pass | 59 passed | pass |
| workspace Clippy | no warnings | passed | pass |
| PR #576 CI | all required gates green | test, Clippy, security, binary smoke, CodeQL, contracts, and live RAG passed | pass |
| `gh pr view 576` | merged into `main` | merge commit `ad842bd39` | pass |

## Risks and Rollback

The main residual risk is behavior under production-scale cache churn and legacy databases not represented by fixtures. Rollback is a revert of merge commit `ad842bd396423e632d0f6281e836dea0d5bcd280`; database rollback should not delete migration receipts or cache tables manually without a reviewed reset/migration plan.

## Decisions Not Taken

- Did not use an unfenced timestamp-only retirement delete because delayed tasks could delete fresh replacements.
- Did not refresh TTL on identical warm writes because that would turn a fixed TTL into sliding expiration.
- Did not remove unrelated worktrees or branches during maintenance because ownership was not established.

## References

- PR #576: https://github.com/dinglebear-ai/axon/pull/576
- Failed unified job: `2138dfdd-c6dd-4187-91cc-fb0d68ba1e8f`
- Merge commit: `ad842bd396423e632d0f6281e836dea0d5bcd280`

## Next Steps

- Deploy or update the Axon runtime from `main` before retrying the original historical-store workload.
- On the affected runtime, run `axon reset --dry-run` only if startup still reports an unsupported receipt; review the plan before any destructive reset.
- Re-run `axon jobs get 2138dfdd-c6dd-4187-91cc-fb0d68ba1e8f` after deployment to verify the live store boundary.
