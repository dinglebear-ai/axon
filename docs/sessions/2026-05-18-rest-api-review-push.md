---
date: 2026-05-18 01:19:26 EDT
repo: git@github.com:jmagar/axon.git
branch: work/axon_rust-2qva
head: bc00837f
working directory: /home/jmagar/workspace/axon_rust/.worktrees/axon_rust-2qva
worktree: /home/jmagar/workspace/axon_rust/.worktrees/axon_rust-2qva bc00837f [work/axon_rust-2qva]
pr: "#106 Add dedicated REST API routes https://github.com/jmagar/axon/pull/106"
---

# REST API Review And Push Session

## User Request

Address all GitHub PR comments in the worktree, run Lavra and CodeRabbit review passes, fix all surfaced issues in the worktree, then quick-push.

## Session Overview

PR #106 had open review feedback on the new dedicated REST API routes. The session resolved all open PR threads, implemented additional review hardening, bumped the package to 3.0.1, verified the changes, and pushed `work/axon_rust-2qva` to origin.

## Sequence of Events

1. Fetched PR #106 review comments with the repo-local `gh-pr` scripts.
2. Fixed the first set of REST review comments, committed as `01402636`.
3. Refetched PR state, found 22 additional open review threads, fixed them, and committed as `433d0f9d`.
4. Ran a Lavra-style local review after subagent outputs proved stale for this worktree.
5. Hardened upstream 5xx response messages and committed as `bc00837f`.
6. Verified PR review state and pushed the branch.

## Key Findings

- `scrape_batch` validated concurrently but fetched serially; fixed with bounded concurrent scraping while preserving input order in `src/services/scrape.rs`.
- REST taxonomy heuristics were placed in the service taxonomy layer; fixed by keeping `taxonomy_from_error` typed-only and moving string HTTP classification to `src/web/server/error.rs`.
- `/v1/actions` needed to preserve its legacy JSON auth-error envelope instead of being wrapped by the shared REST auth layer; fixed in `src/web/actions.rs` and `src/web/server/routing.rs`.
- Loopback development mode could expose destructive REST routes without configured auth; fixed with a destructive route guard in `src/web/server/routing.rs`.
- Raw upstream/internal error text could leak through REST responses; fixed with generic 500/502/504 response messages in `src/web/server/error.rs`.

## Technical Decisions

- Kept MCP/service taxonomy semantics explicit: typed taxonomy errors downcast only, while REST-only fallback classification remains at the HTTP boundary.
- Preserved `/v1/actions` compatibility by leaving its authorization/error handling inside `web/actions.rs`.
- Treated `/v1/query` and `/v1/retrieve` as read-scoped routes; kept cost-bearing or mutating endpoints write-scoped.
- Shared ingest source parsing through `services::ingest::source_from_mcp_request` so MCP action and REST ingest start use the same contract.

## Files Modified

- `src/web/server/routing.rs`: REST route grouping, scope enforcement, loopback destructive guard.
- `src/web/actions.rs` and `src/web/actions/tests.rs`: action router split, static-token fallback, JSON auth envelope preservation.
- `src/web/server/error.rs`: taxonomy mapping, fallback status classification, generic 5xx response messages.
- `src/services/scrape.rs` and tests: concurrent scrape batch handling with input-order preservation.
- `src/services/query.rs`, `src/vector/ops/commands/evaluate*.rs`, `src/cli/commands/evaluate.rs`, `src/web/server/handlers/rag.rs`: direct async evaluate route and Send-safe error flow.
- `src/services/ingest.rs`, `src/services/action_api/commands/helpers.rs`, `src/web/server/handlers/async_jobs.rs`: shared ingest request parsing.
- `CHANGELOG.md`, `Cargo.toml`, `Cargo.lock`, `docs/API.md`: 3.0.1 release entry/version and REST docs.

## Commands Executed

- `python3 /home/jmagar/.agents/skills/gh-pr/scripts/fetch_comments.py --pr 106 ...`
- `python3 /home/jmagar/.agents/skills/gh-pr/scripts/post_reply.py --all ... --commit`
- `python3 /home/jmagar/.agents/skills/gh-pr/scripts/mark_resolved.py --all ...`
- `RUSTC="$(rustup which rustc)" RUSTC_WRAPPER= cargo check --lib`
- `RUSTC="$(rustup which rustc)" RUSTC_WRAPPER= cargo test --lib ...`
- `git push`

## Errors Encountered

- Initial direct `evaluate` route await failed axum `Handler` bounds because non-Send boxed errors crossed await points. Fixed by collapsing evaluate errors to Send-safe strings/boxed errors on the REST-facing path.
- Parallel cargo test invocations contended on cargo package/artifact locks. Subsequent checks were run sequentially where useful.
- First attempt to commit the second review batch failed clippy on `let_and_return` in `src/web/actions.rs`. Fixed and recommitted.
- CodeRabbit CLI review could not run locally: installed CLI was `0.3.5` and unauthenticated, while the requested `--agent` workflow requires authenticated `0.4.0+`.

## Behavior Changes

- Batched scrape requests now fetch concurrently but return results in request order.
- REST read endpoints use read scope; cost-bearing/write/destructive endpoints use write scope.
- Loopback dev keeps convenient local access for non-destructive writes but rejects destructive REST endpoints without configured auth.
- 5xx REST error bodies now return generic messages instead of raw internal/upstream details.
- `/v1/actions` still returns its deprecated JSON response envelope for auth errors.

## Verification Evidence

| command | expected | actual | status |
| --- | --- | --- | --- |
| `cargo check --lib` | library compiles | finished successfully | pass |
| `cargo test --lib taxonomy_` | taxonomy tests pass | 6 passed | pass |
| `cargo test --lib classify_` | HTTP/error classification tests pass | 34 passed | pass |
| `cargo test --lib loopback_dev_` | loopback auth tests pass | 3 passed | pass |
| `cargo test --lib all_v1_rest_routes_reject_missing_auth_when_auth_is_configured` | auth route inventory passes | 1 passed | pass |
| pre-commit hook on `01402636`, `433d0f9d`, `bc00837f` | rustfmt, clippy, tests pass | hook passed each commit | pass |
| `pr_summary.py --open-only` after fixes | no open review threads | 0 open, 27 resolved | pass |
| `git push` | branch pushed | `497aaa99..bc00837f` pushed | pass |

## Risks and Rollback

- REST auth behavior changed for route grouping; rollback by reverting `433d0f9d` and `bc00837f` if compatibility issues appear.
- Version is now `3.0.1`; rollback requires reverting the version/changelog commit or preparing a follow-up patch.

## Decisions Not Taken

- Did not run a fresh CodeRabbit local review because the local CLI was unauthenticated and below the required version.
- Did not use stale Lavra subagent findings about desktop palette files because they did not match the REST API branch diff.

## Open Questions

- Whether to upgrade/authenticate CodeRabbit CLI on this machine for future `$coderabbit:code-review` runs.

## Next Steps

- Monitor PR #106 for any new review comments or CI failures after the push.
- Merge PR #106 once CI and reviewers are satisfied.
