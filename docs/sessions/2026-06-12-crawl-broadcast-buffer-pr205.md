---
date: 2026-06-12 15:25:14 EST
repo: git@github.com:jmagar/axon.git
branch: detached HEAD at origin/main
head: 96cac500
working directory: /home/jmagar/workspace/axon/.worktrees/save-session-pr205
worktree: /home/jmagar/workspace/axon/.worktrees/save-session-pr205
pr: "#205 fix(crawl): honor broadcast buffer config https://github.com/jmagar/axon/pull/205"
beads: axon_rust-1jmm
---

# Crawl broadcast buffer PR 205 session

## User Request

Check Axon's crawl implementation against Spider's `examples/subscribe.rs`, create a worktree, wire the missing crawl subscription buffer behavior completely, dispatch PR review toolkit agents, address every issue found in the worktree, merge the PR, and save the session notes.

## Session Overview

The session confirmed Axon already used Spider's `Website::subscribe` crawl broadcast flow, found that Axon's configured crawl broadcast buffer min/max values were not actually wired into crawl subscription calls, implemented the missing wiring, reviewed the PR with toolkit agents, fixed all review and CI issues, merged PR #205, and documented the session.

## Sequence of Events

1. Checked Spider subscription parity and found Axon already called `website.subscribe(...)`, collected pages through a broadcast receiver, and called `website.unsubscribe()`.
2. Created the `codex/wire-crawl-broadcast-buffer` worktree and bead `axon_rust-1jmm` for the implementation.
3. Replaced hard-coded crawl subscription buffer sizing with `crawl_subscribe_buffer_size(cfg)` and added focused tests for default, profile, clamp, inverted-bound, and legacy-cap behavior.
4. Dispatched PR review toolkit agents; code and type reviews found no issues, while silent-failure and test-analysis agents found buffer regression and profile-parsing coverage gaps.
5. Fixed the review findings, then fixed CI-only drift in compose contracts, worktree memory metadata, MCP smoke Qdrant port isolation, and env registry/matrix coverage.
6. Rebasing, pushing, and CI watching continued until every GitHub check passed.
7. Merged PR #205 with squash merge commit `96cac500a3143573b254cca519724af064decfa1`, deleted the remote PR branch, and then removed the stale local PR worktree and branch during session-note maintenance.

## Key Findings

- Axon's Spider crawl path already used the broadcast subscription API: `src/crawl/engine.rs:273`, `src/crawl/engine.rs:274`, `src/crawl/engine.rs:330`, `src/crawl/engine.rs:409`, `src/crawl/engine.rs:410`, and `src/crawl/engine.rs:440`.
- The config fields existed, but crawl subscription calls previously used hard-coded bounds. PR #205 now routes both normal crawl and sitemap-only crawl through `crawl_subscribe_buffer_size(cfg)` at `src/crawl/engine.rs:53`.
- Review found a possible silent regression where a balanced profile max of `8192` could shrink below Axon's previous hard-coded safe cap. The fix preserved the legacy `16_384` cap at `src/crawl/engine.rs:51`.
- CI revealed unrelated but merge-blocking drift: compose tests needed the `local-qdrant` profile expectation at `tests/compose_env_contract.rs:266`, and MCP smoke needed isolated Qdrant ports in `.github/workflows/ci.yml:927`.
- Worktree-sensitive memory metadata previously derived project names from checkout directory names. It now prefers the remote repo slug at `src/services/memory/runtime_metadata.rs:27`.

## Technical Decisions

- Kept Spider's existing broadcast subscription architecture instead of replacing it, because the missing piece was config propagation rather than a missing feature.
- Used a helper function for buffer sizing so both crawl call sites share identical behavior and tests can exercise the sizing logic directly.
- Preserved the old `16_384` effective cap as a lower floor for max sizing to avoid silently reducing capacity for balanced profiles.
- Treated CI failures as part of the PR acceptance bar after the review agents passed, since each failure was reproducible or directly evidenced by GitHub job output.
- Saved this session artifact from a clean detached worktree at `origin/main` because the normal `main` checkout had unrelated dirty palette work.

## Files Changed

| status | path | previous path | purpose | evidence |
|---|---|---|---|---|
| modified | `.github/workflows/ci.yml` | - | Isolated MCP smoke Qdrant ports to avoid host port collisions. | `QDRANT_HTTP_PORT=55333` and `QDRANT_GRPC_PORT=55334` at `.github/workflows/ci.yml:927` |
| modified | `docker-compose.prod.yaml` | - | Made Qdrant host ports configurable while preserving defaults. | `${QDRANT_HTTP_PORT:-53333}` and `${QDRANT_GRPC_PORT:-53334}` at `docker-compose.prod.yaml:115` |
| modified | `docs/reference/env-matrix.toml` | - | Registered compose-only Qdrant port env keys. | `QDRANT_HTTP_PORT` at `docs/reference/env-matrix.toml:26` and `QDRANT_GRPC_PORT` at `docs/reference/env-matrix.toml:38` |
| modified | `src/core/config/parse/env_registry/runtime.rs` | - | Added env registry entries for compose interpolation keys. | `QDRANT_HTTP_PORT` at `src/core/config/parse/env_registry/runtime.rs:17` |
| modified | `src/core/config/parse_tests.rs` | - | Added profile parsing coverage proving max profile flows into crawl subscribe buffer sizing. | `parse_max_profile_flows_to_crawl_subscribe_buffer` uses helper at `src/core/config/parse_tests.rs:4` |
| modified | `src/crawl/engine.rs` | - | Wired configured subscription buffer sizing into Spider subscribe calls. | `crawl_subscribe_buffer_size` at `src/crawl/engine.rs:53` |
| modified | `src/crawl/engine_tests.rs` | - | Added regression tests for buffer size defaults, profiles, clamp behavior, and legacy cap floor. | Assertions at `src/crawl/engine_tests.rs:31` through `src/crawl/engine_tests.rs:76` |
| modified | `src/services/memory/runtime_metadata.rs` | - | Made project metadata stable in git worktrees by deriving it from the remote repo slug first. | remote lookup at `src/services/memory/runtime_metadata.rs:27` |
| modified | `tests/compose_env_contract.rs` | - | Aligned compose contract tests with opt-in local Qdrant profile and `AXON_QDRANT_URL`. | local profile checks at `tests/compose_env_contract.rs:266` and env allowlist at `tests/compose_env_contract.rs:409` |
| created | `docs/sessions/2026-06-12-crawl-broadcast-buffer-pr205.md` | - | Captured this session per `vibin:save-to-md`. | This generated artifact |

## Beads Activity

| bead | title | actions | final status | why it mattered |
|---|---|---|---|---|
| `axon_rust-1jmm` | Wire crawl broadcast buffer profile config | Created, worked, and closed during the implementation. | closed | Tracked the non-trivial code change to use configured `crawl_broadcast_buffer_min/max` instead of hard-coded subscription bounds. |

## Repository Maintenance

### Plans

Checked `docs/plans/` in the clean save-session worktree. No plan file was clearly tied to this completed PR, so no completed plans were moved. Existing top-level plans such as `docs/plans/2026-03-05-watch-top-level-scheduler.md` and `docs/plans/env-var-fatigue-reduction.md` were left untouched because their completion state was not proven by this session.

### Beads

Read `bd show axon_rust-1jmm --json`; it was already closed with close reason `Implemented crawl subscribe buffer sizing from configured profile min/max; added focused tests and verified.` No additional bead state changes were needed while saving the note.

### Worktrees and branches

Inspected `git worktree list --porcelain`, `git branch -vv`, `git branch -r -vv`, PR state, and remote branch state. The PR worktree `/home/jmagar/workspace/axon/.worktrees/wire-crawl-broadcast-buffer` was clean, PR #205 was `MERGED`, and `git ls-remote --heads origin codex/wire-crawl-broadcast-buffer` returned no remote branch. Removed that stale worktree and deleted local branch `codex/wire-crawl-broadcast-buffer`. Left `/home/jmagar/workspace/axon` untouched because it had unrelated dirty palette files and local `main` was behind `origin/main`.

### Stale docs

The implementation updated the env matrix and compose contract where the session proved docs or config contracts were stale. No broader docs sweep was attempted because unrelated palette work was present in the main checkout and the PR scope was crawl buffer wiring plus CI contract fixes.

### Transparency

This session note was written in a clean detached worktree at `origin/main` to avoid staging unrelated dirty files from the normal `main` checkout. The note commit is intentionally path-limited to this artifact.

## Tools and Skills Used

- **Skill.** Used `vibin:save-to-md` to perform the maintenance pass, write the session artifact, and commit/push only the generated file.
- **Shell and Git CLI.** Used `git`, `gh`, `rg`, `find`, and `bd` for repo inspection, PR state, CI checks, cleanup, and evidence gathering.
- **GitHub CLI.** Created and merged PR #205, watched checks, fetched CI state, and confirmed merged state.
- **PR review toolkit agents.** Dispatched code review, type design analysis, silent failure hunting, and PR test analysis agents; two reported no issues and two reported actionable findings that were fixed and re-reviewed.
- **Build and test tools.** Used Cargo, npm OpenAPI check, pre-push hooks, clippy, nextest, and GitHub Actions to verify behavior.
- **File tools.** Used patch-based editing for code and docs. No browser automation was used.

## Commands Executed

| command | result |
|---|---|
| `git worktree add ... codex/wire-crawl-broadcast-buffer` | Created isolated implementation worktree. |
| `bd create ...` | Created bead `axon_rust-1jmm`. |
| `cargo fmt --check` | Passed during implementation and follow-up fixes. |
| `cargo test crawl_subscribe_buffer --lib` | Passed focused crawl buffer tests. |
| `cargo test -p axon --test compose_env_contract -- --nocapture` | Initially failed due compose contract drift, then passed after updates. |
| `cargo test -p axon --test env_config_boundary -- --nocapture` | Initially exposed missing Qdrant port env registry entries, then passed after updates. |
| `npm run openapi:check` | Passed after rebase verification. |
| `cargo build --bin axon` | Passed after implementation and fixes. |
| `gh pr checks 205` | Confirmed all PR checks passed, including `mcp-smoke`, `release`, `release-smoke`, `test`, and `windows-build`. |
| `gh pr merge 205 --squash --delete-branch` | Remote merge succeeded, but local checkout cleanup failed because `main` was already checked out in another worktree. |
| `git push origin --delete codex/wire-crawl-broadcast-buffer` | Deleted the remote feature branch after confirming the PR was merged. |
| `git worktree remove ... && git branch -D codex/wire-crawl-broadcast-buffer` | Removed stale local PR worktree and local branch after merge evidence was collected. |

## Errors Encountered

- `gh pr merge 205 --squash --delete-branch` returned `fatal: 'main' is already used by worktree at '/home/jmagar/workspace/axon'` during local cleanup. The PR still merged remotely; confirmed with `gh pr view 205 --json state,mergedAt,mergeCommit,url`, then deleted the remote branch manually.
- GitHub `test` initially failed because compose contract tests did not match the current local-Qdrant profile and env shape. Updated `tests/compose_env_contract.rs` and verified the test.
- GitHub `mcp-smoke` initially failed from Qdrant port collision on host port `53334`. Made Qdrant ports configurable and used isolated CI ports.
- GitHub env boundary tests initially failed because `QDRANT_HTTP_PORT` and `QDRANT_GRPC_PORT` were missing from the env matrix and registry. Added both entries and verified.
- A worktree-sensitive memory test failed because project metadata used the checkout directory basename. Updated metadata derivation to prefer the remote repo slug.

## Behavior Changes (Before/After)

| area | before | after |
|---|---|---|
| Crawl subscription buffer | Spider subscription calls used hard-coded buffer bounds. | Both crawl subscription paths use config/profile-aware buffer sizing. |
| Balanced profile safety | A naive config wiring could reduce the effective max below the previous `16_384` safe cap. | The helper floors max sizing at `16_384` unless a higher configured max applies. |
| Max profile parsing | No direct test proved profile parsing flowed into subscription sizing. | `parse_max_profile_flows_to_crawl_subscribe_buffer` covers that path. |
| MCP smoke Qdrant ports | CI could collide with existing host Qdrant ports. | MCP smoke uses isolated Qdrant host ports. |
| Worktree memory project name | Project name could become the worktree folder name. | Project name prefers the git remote repo slug. |

## Verification Evidence

| command | expected | actual | status |
|---|---|---|---|
| `cargo fmt --check` | Rust formatting clean. | Passed. | pass |
| `cargo test crawl_subscribe_buffer --lib` | Focused crawl buffer tests pass. | Passed. | pass |
| `cargo test -p axon --test compose_env_contract -- --nocapture` | Compose contract matches current stack. | Passed after contract updates. | pass |
| `cargo test -p axon --test env_config_boundary -- --nocapture` | Env matrix and registry agree. | Passed after adding Qdrant port keys. | pass |
| `npm run openapi:check` | OpenAPI artifacts remain synced. | Passed. | pass |
| `cargo build --bin axon` | Axon binary builds. | Passed. | pass |
| pre-push clippy and nextest | Full pushed branch test gate passes. | Passed with `2809 passed / 6 skipped` before final PR push. | pass |
| `gh pr checks 205` | All required PR checks pass. | All checks passed, including `mcp-smoke`, `release-smoke`, `test`, and `windows-build`. | pass |
| `gh pr view 205 --json state,mergedAt,mergeCommit,url` | PR is merged. | `state=MERGED`, merge commit `96cac500a3143573b254cca519724af064decfa1`. | pass |

## Risks and Rollback

The crawl behavior change is low risk because it keeps the existing Spider subscription design and only changes buffer sizing. The primary risk is memory pressure if very large profiles are selected, but the value now follows existing config/profile knobs and remains bounded. Roll back by reverting merge commit `96cac500a3143573b254cca519724af064decfa1` or by restoring hard-coded subscription buffer sizing in `src/crawl/engine.rs`.

## Decisions Not Taken

- Did not replace Spider's broadcast subscription flow; Axon already used the official subscribe/unsubscribe pattern.
- Did not clean or update the dirty main checkout, because its palette files were unrelated user work.
- Did not move old top-level plan files to `docs/plans/complete/`, because their completion state was not proven by this session.

## References

- Spider subscribe example: https://github.com/spider-rs/spider/blob/main/examples/subscribe.rs
- PR #205: https://github.com/jmagar/axon/pull/205
- Merge commit: `96cac500a3143573b254cca519724af064decfa1`
- Bead: `axon_rust-1jmm`

## Open Questions

- The main checkout at `/home/jmagar/workspace/axon` still has unrelated dirty palette files and is behind `origin/main`; this session left those untouched by design.

## Next Steps

- Pull or rebase the normal `main` worktree when its unrelated palette work is ready to reconcile with `origin/main`.
- No follow-up is required for PR #205 itself; it is merged, CI-passing, and its stale local/remote branches were cleaned up.
