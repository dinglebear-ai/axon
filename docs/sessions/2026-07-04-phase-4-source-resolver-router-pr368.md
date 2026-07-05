# Phase 4 Source Resolver Router PR 368

## Metadata

- Date: 2026-07-04T20:02:01-04:00
- Repo: git@github.com:jmagar/axon.git
- Worktree: `/home/jmagar/workspace/axon/.worktrees/phase-4-source-resolver-router`
- Branch: `codex/phase-4-source-resolver-router`
- Base: `origin/codex/phase-3-stores-providers-fakes` at `2d633dc29cb0092c0b4865aa6227b0d0cbc860ac`
- Head before this note: `1f0160c097be78a6b5eff8a88e975aecb2f342de`
- Pull request: https://github.com/jmagar/axon/pull/368
- Plan: `docs/pipeline-unification/plans/2026-07-04-align-phase-4.md`

## Summary

Executed Phase 4 as the route-first source resolver/router boundary. `index_source`
now resolves every `SourceRequest` through `axon-route` before data-plane
dispatch, reports unsupported route families before acquisition, carries routed
scope into web dispatch, and records route metadata in missing-data-plane and
completion results. The implementation deliberately avoids porting broad
source-family acquisition beyond the plan.

The branch was rebased onto the completed Phase 3 provider/fake/schema branch
before final push, preserving the Phase 4 route-first changes on top of the new
Phase 3 head.

## Files Changed

- `Cargo.lock`
- `crates/axon-route/src/route_tests.rs`
- `crates/axon-route/src/route_validation_tests.rs`
- `crates/axon-services/Cargo.toml`
- `crates/axon-services/src/code_search_watch_tests.rs`
- `crates/axon-services/src/feed_source_failure_tests.rs`
- `crates/axon-services/src/feed_source_tests.rs`
- `crates/axon-services/src/git_source/git_source_tests.rs`
- `crates/axon-services/src/local_source_failure_tests.rs`
- `crates/axon-services/src/local_source_tests.rs`
- `crates/axon-services/src/reddit_source_tests.rs`
- `crates/axon-services/src/registry_source_tests.rs`
- `crates/axon-services/src/sessions_source_tests.rs`
- `crates/axon-services/src/source.rs`
- `crates/axon-services/src/source/dispatch.rs`
- `crates/axon-services/src/source/routing.rs`
- `crates/axon-services/src/source_jobs_tests.rs`
- `crates/axon-services/src/source_tests.rs`
- `crates/axon-services/src/youtube_source_tests.rs`
- `docs/pipeline-unification/delivery/dependency-order-map.md`
- `docs/pipeline-unification/delivery/implementation-checklist.md`
- `docs/pipeline-unification/delivery/implementation-plan.md`
- `docs/pipeline-unification/sources/adapter-scopes.md`
- `xtask/src/checks/repo_structure_spec.rs`
- `xtask/src/checks/repo_structure_tests.rs`

## Verification

- `cargo test -p axon-services source_routing --locked` - passed, 3 tests.
- `cargo test -p axon-services index_source_ --locked` - passed, 9 tests.
- `cargo test -p axon-route --locked` - passed, 49 tests plus doc tests.
- `cargo fmt --all --check` - passed.
- `cargo xtask schemas generate --check` - passed with no generated diff.
- `cargo xtask check-layering` - passed.
- `cargo xtask check-repo-structure` - passed.
- `cargo test -p xtask repo_structure --locked` - passed, 9 tests.
- `git diff --check` - passed.
- Pre-push structural checks - passed during push.

## Review And Repair

- Independent correctness review found fresh route components were being built
  per request; repaired with cached route components via `OnceLock`.
- Simplification pass 1 kept route-first wiring narrow and confirmed no broad
  source-family acquisition was ported.
- Simplification pass 2 restored conditional xtask skeleton coverage instead of
  deleting it after `axon-prune` graduated from the skeleton list.
- Simplification pass 3 checked docs and metadata alignment; no further code
  changes were needed.
- GitHub PR comments and review threads were fetched. Copilot/Codex review tools
  were quota-limited and CodeRabbit skipped non-default-base auto review, leaving
  no actionable PR comments or threads to resolve.

## Repo Maintenance

- Plans: the Phase 4 plan remains in place for traceability; no plan files were
  moved.
- Beads: no bead activity was required or observed for this plan execution.
- Worktrees and branches: no cleanup was performed because this branch is active
  on PR 368.
- Source issue: issue 298 was not edited. A patch body was prepared at
  `/tmp/axon-298-body.md` but not applied without explicit authorization.

## Remaining Risks

- Route-time `Unsupported` remains the expected answer for memory/upload/tool
  source families until later source-family acquisition PRs wire those data
  planes.
- External bot review was unavailable because the PR is stacked on a non-default
  base and review quota was exhausted, so the final review loop relied on manual
  review plus available GitHub comment/thread fetches.
