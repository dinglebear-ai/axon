---
date: 2026-06-04 11:40:22 EST
repo: git@github.com:jmagar/axon.git
branch: main
head: 1e8b407a
plan: /home/jmagar/workspace/axon_rust/docs/plans/2026-05-27-android-phase2-stubbed-modes.md
session id: 8f94339b-2256-424d-b6df-d0e1a0b19aa2
transcript: /home/jmagar/.claude/projects/-home-jmagar-workspace-axon/8f94339b-2256-424d-b6df-d0e1a0b19aa2.jsonl
working directory: /home/jmagar/workspace/axon
worktree: /home/jmagar/workspace/axon
beads: axon_rust-c7ue
---

# Comprehensive review remediation

## User Request

The session began with repo hygiene and review questions: "are we all synced with main? any other branches / worktrees", then `$comprehensive-full-review`, "continue", "how many total issues are there", and finally "address all of those". The last request was `$vibin:save-to-md`, which required saving this session artifact and committing only the generated file.

## Session Overview

The Axon repo was confirmed on `main`, tracking `origin/main`, with a single registered worktree. A comprehensive review surfaced 12 issues, and the remediation pass addressed the review findings around scrape fallback SSRF safety, release workflow sparse checkout, stale CI test filters, watch REST route drift, MCP scope metadata, stale docs, and monolith-report actionability.

The session also ran the required save-session maintenance pass. The implementation changes remain dirty and uncommitted by design; this save workflow commits only this session log.

## Sequence of Events

1. Checked repository state, worktree registration, branch tracking, and sync status.
2. Ran the comprehensive review workflow and produced `.full-review/00-scope.md` through `.full-review/05-final-report.md`.
3. Counted 12 total review findings: 4 high, 5 medium, 3 low, 0 critical.
4. Added red tests for fallback header leakage, legacy watch create routing, active-operation MCP scopes, release workflow shape, and stale CI cargo filters.
5. Fixed scrape fallback HTTP client construction, route/scope drift, release and CI workflows, docs, and monolith artifact reporting.
6. Ran targeted and broad verification, then restored an accidental tracked plugin binary artifact produced by local build state.
7. Ran the save-to-md maintenance pass, wrote this artifact, and prepared to stage, commit, and push only this file.

## Key Findings

- `src/crawl/scrape.rs` had a raw fallback `reqwest::Client::builder()` path and a process-global `SCRAPE_FALLBACK_CLIENT`; a red test proved the second fallback request reused the first config's `x-axon-test` header.
- `.github/workflows/release.yml` had `tests`, `scripts`, `config`, `vendor`, and `.cargo` indented under `sparse-checkout-cone-mode: false`, so those paths were parsed as part of the wrong YAML key.
- `.github/workflows/ci.yml` ran `cargo test --locked server_mode_post_bodies_match_canonical_rest_contract_fields --lib`, which matched zero tests.
- `src/web/server/handlers/rest.rs` retained the old test-facing `POST /v1/watch/create` route while production uses `POST /v1/watch`.
- `src/mcp/server/authz.rs` labeled `ask`, `evaluate`, `suggest`, `research`, and `screenshot` as read-scoped even though REST/action surfaces treat active LLM/browser/network work as write-scoped.
- Existing dirty OpenAPI route work in `src/web/server/handlers/exploration.rs`, `src/web/server/openapi.rs`, and `src/web/server/routing.rs` required missing `utoipa::ToSchema` derives on response structs before verification could compile.

## Technical Decisions

- The scrape fallback now builds a fresh client per call through `build_ssrf_guarded_client_builder`, preserving the SSRF resolver while avoiding config leakage for headers, proxy, TLS, and timeout settings.
- CI now uses `scripts/cargo_test_filter_guard.py` before running named cargo test filters so zero-match filters fail explicitly.
- Workflow YAML shape is guarded by `tests/workflow_shapes.rs` instead of relying on visual indentation review.
- The retained REST test router was aligned with production route shape by merging guarded `GET` and `POST` handlers on `/v1/watch`.
- The whole-repo monolith report remains informational but now uploads `monolith-report.txt` as an artifact, making the finding actionable without making pre-existing debt block unrelated changes.

## Files Changed

| status | path | previous path | purpose | evidence |
|---|---|---|---|---|
| modified | `.github/workflows/ci.yml` | - | Replaced stale zero-test step with guarded REST contract filter and added workflow-shape test; uploads whole-repo monolith report artifact. | `git diff --name-status`; `cargo_test_filter_guard.py` verified 1 matched test. |
| modified | `.github/workflows/release.yml` | - | Moved required sparse-checkout paths inside the literal block before `sparse-checkout-cone-mode`. | `tests/workflow_shapes.rs` passed. |
| modified | `docs/operations/security.md` | - | Documented fallback use of the SSRF-guarded builder and per-call isolation. | `git diff --name-status`. |
| modified | `src/core/http.rs` | - | Re-exported the SSRF-guarded client builder for internal fallback construction. | `cargo check --workspace --all-targets --locked` passed. |
| modified | `src/core/http/client.rs` | - | Extracted `base_client_builder` and exposed `build_ssrf_guarded_client_builder`. | `cargo test -p axon scrape -- --nocapture` passed. |
| modified | `src/crawl/scrape.rs` | - | Removed `SCRAPE_FALLBACK_CLIENT`; fallback now builds per-call via the guarded builder. | Red/green test `direct_fetch_fallback_keeps_custom_headers_per_config`. |
| modified | `src/crawl/scrape_tests.rs` | - | Added fallback config-isolation test using `httpmock`. | Test passed after fix. |
| modified | `src/jobs/watch.rs` | - | Updated stale route comment from `/v1/watch/create` to `/v1/watch`. | `rg /v1/watch/create` showed no active code/comment refs outside regression/old sessions. |
| modified | `src/mcp/server/authz.rs` | - | Marked active LLM/browser actions as `ActionScope::Write`. | `cargo test --locked --test mcp_contract_parity -- --nocapture` passed. |
| modified | `src/services/types/client_server.rs` | - | Added `utoipa::ToSchema` derive for `ArtifactHandle`; existing dirty route work also had supported-route additions. | Compile blocker resolved. |
| modified | `src/services/types/service.rs` | - | Added `utoipa::ToSchema` derives for screenshot, diff, and brand response types. | Compile blocker resolved. |
| modified | `src/web/server/handlers/rest.rs` | - | Consolidated retained watch create route onto production `POST /v1/watch`. | Route regression passed. |
| modified | `src/web/server/handlers/rest_tests.rs` | - | Updated watch create tests to production path and asserted legacy path no longer reaches create handler. | `watch_create_uses_production_path_not_legacy_create_path` passed. |
| modified | `tests/mcp_contract_parity.rs` | - | Added active-operation write-scope contract. | Full target passed. |
| created | `scripts/cargo_test_filter_guard.py` | - | Fails CI when a named cargo test filter matches zero tests. | Real filter passed; fake filter returned `matched 0 tests`. |
| created | `tests/workflow_shapes.rs` | - | Guards release sparse-checkout shape and CI named-filter policy. | `cargo test --locked --test workflow_shapes -- --nocapture` passed. |
| modified | `.full-review/00-scope.md` through `.full-review/05-final-report.md` | - | Ignored comprehensive review artifacts produced during the review workflow. | `ls -la .full-review` showed updated artifacts. |
| modified | `apps/palette-tauri/**` | - | Pre-existing unrelated dirty palette-tauri work observed and preserved. | `git diff --name-status -- apps/palette-tauri`. |
| modified | `src/web/server/handlers/exploration.rs`, `src/web/server/openapi.rs`, `src/web/server/routing.rs` | - | Pre-existing dirty OpenAPI/REST additions observed; not reverted. The session added compatible schema derives elsewhere. | `git diff` showed brand/diff/screenshot route additions already dirty. |
| created | `docs/sessions/2026-06-04-comprehensive-review-remediation.md` | - | This session artifact. | Created by `vibin:save-to-md`. |

## Beads Activity

| bead | title | action(s) | final status | why it mattered |
|---|---|---|---|---|
| `axon_rust-c7ue` | Comprehensive full review and green-repo fixes | Observed via `bd show`; recent interactions show status changed from `in_progress` to `closed` at 2026-06-04T07:38:34Z. | closed | Directly tracks the comprehensive review and remediation work. Close reason states phases 1-5 completed, surfaced issues remediated, and verification passed. |

No new bead was created during the save-to-md pass. No bead was closed during the save-to-md pass because `axon_rust-c7ue` was already closed when inspected.

## Repository Maintenance

### Plans

- Checked `docs/plans` with `find docs/plans -maxdepth 2 -type f`.
- No plan was moved. Several non-complete plan files remain under `docs/plans/`, and the injected active plan points to `/home/jmagar/workspace/axon_rust/docs/plans/2026-05-27-android-phase2-stubbed-modes.md`, outside this repo's current plan tree. That made completion status ambiguous for this session.

### Beads

- Ran `bd list --all --sort updated --reverse --limit 100 --json`, `tail -200 .beads/interactions.jsonl`, and `bd show axon_rust-c7ue --json`.
- No tracker mutation was needed during the save pass; relevant bead `axon_rust-c7ue` was already closed with a close reason matching the review remediation.

### Worktrees and branches

- Ran `git worktree list --porcelain`, `git branch -vv`, `git branch -r -vv`, and `git rev-list --left-right --count origin/main...HEAD`.
- Evidence showed one worktree at `/home/jmagar/workspace/axon`, branch `main`, tracking `origin/main`, and ahead/behind `0 0` before the session artifact commit. No stale worktrees or branches were present, so nothing was removed.

### Stale docs

- Updated `docs/operations/security.md` because the scrape fallback SSRF/client behavior changed.
- No broad stale-doc sweep was attempted beyond docs directly contradicted by the review remediation; wider docs cleanup was treated as out of scope for this save pass.

### Transparency

- `plugins/axon/bin/axon` changed during local build state but was restored with `git restore -- plugins/axon/bin/axon`; the final status no longer listed it.
- Existing dirty palette-tauri files were observed and left untouched.
- Existing dirty REST/OpenAPI additions were observed and preserved; only compatible schema derives and review remediation changes were added.

## Tools and Skills Used

- **Skills.** `$comprehensive-full-review` drove the multi-phase review, `superpowers:test-driven-development` guided red/green fixes, and `vibin:save-to-md` generated this closeout artifact.
- **Shell commands.** Used `git`, `rg`, `find`, `cargo`, `python3`, `bd`, `gh`, `wc`, and `ls` for evidence gathering, edits verification, and repository maintenance.
- **File tools.** Used patch-based edits for source, workflow, docs, tests, and this session artifact.
- **Beads CLI.** Used read-only bead commands for recent issue and interaction context; no bead write was performed in the save pass.
- **GitHub CLI.** Used `gh pr view` to confirm no active PR for the current branch.
- **MCP/browser/subagents.** No browser automation, MCP gateway calls, or subagents were used in the remediation turn captured here.
- **Issues encountered.** Cargo runs contended on file locks when parallelized; sccache repeatedly warned that the server shut down unexpectedly and builds continued locally.

## Commands Executed

| command | result |
|---|---|
| `git status --short` | Showed implementation dirty files plus unrelated palette-tauri changes. |
| `git worktree list --porcelain` | Showed only `/home/jmagar/workspace/axon` on `refs/heads/main`. |
| `git branch -vv` / `git branch -r -vv` | Showed `main` tracking `origin/main`; remote `origin/main` at the same commit. |
| `git rev-list --left-right --count origin/main...HEAD` | Returned `0 0` before the session artifact commit. |
| `bd show axon_rust-c7ue --json` | Confirmed the comprehensive review bead was closed with remediation verification in the close reason. |
| `cargo test -p axon direct_fetch_fallback_keeps_custom_headers_per_config -- --nocapture` | Failed red before the fallback fix, passed after the fallback fix. |
| `cargo test -p axon watch_create_uses_production_path_not_legacy_create_path -- --nocapture` | Failed red before route consolidation, passed after test invariant adjustment and router fix. |
| `cargo test --locked --test workflow_shapes -- --nocapture` | Failed before release/CI workflow fixes, passed after fixes. |
| `cargo check --workspace --all-targets --locked` | Passed after adding missing OpenAPI schema derives. |
| `cargo test --locked --test mcp_contract_parity -- --nocapture` | Passed all 31 tests after MCP scope alignment. |
| `cargo test -p axon scrape -- --nocapture` | Passed 97 scrape-related tests. |
| `cargo test -p axon rest_ -- --nocapture` | Passed 10 REST-related filtered tests. |
| `python3 scripts/cargo_test_filter_guard.py -- cargo test --locked --test http_api_parity_inventory rest_route_contracts_match_openapi_request_schemas -- --nocapture` | Matched and ran 1 test successfully. |
| `python3 scripts/cargo_test_filter_guard.py -- cargo test --locked --test http_api_parity_inventory definitely_not_a_test_filter -- --nocapture` | Intentionally failed with `matched 0 tests`, proving the guard. |
| `python3 scripts/enforce_monoliths.py --file <path>` | Passed on changed Rust files; `src/web/server/handlers/rest.rs` retained a 91-line warning below the 120-line limit. |

## Errors Encountered

- The fallback header-isolation test failed red because the second request sent `x-axon-test=first` instead of `second`; removing the global client fixed it.
- Initial test compilation failed because existing dirty OpenAPI additions referenced response types without `utoipa::ToSchema`; schema derives were added.
- The first route regression expected `404` for legacy `POST /v1/watch/create`, but Axum returned `405` because the path matched the `{id}` read route with the wrong method. The assertion was corrected to the real invariant: the legacy path must not reach the create handler and return `400`.
- An `rg` command attempted a newline regex without multiline mode and failed; the scan was rerun with simpler patterns.
- Parallel cargo commands contended on the package/artifact locks; later commands were run more carefully.
- `plugins/axon/bin/axon` was modified by local build state and restored to avoid committing a generated binary artifact.
- The injected transcript path exists but appears to be a prior Claude session transcript, not the current Codex API turn stream. This note relies on the current conversation context and command evidence for the review remediation details.

## Behavior Changes (Before/After)

| area | before | after |
|---|---|---|
| Scrape fallback HTTP client | Raw client builder without the shared SSRF resolver path; process-global client could leak config-specific headers/options. | Per-call client from the SSRF-guarded builder with no process-global config leakage. |
| Release sparse checkout | Required directories were indented under `sparse-checkout-cone-mode` and ignored by `actions/checkout`. | Required directories are inside `sparse-checkout`; workflow shape test guards this. |
| CI REST contract | Stale cargo filter matched zero tests. | Guarded real filter fails if zero tests match and runs the intended REST contract. |
| REST watch create tests | Retained router encoded `POST /v1/watch/create`. | Retained router and tests use production `POST /v1/watch`. |
| MCP auth metadata | Active LLM/browser actions were labeled read. | `ask`, `evaluate`, `suggest`, `research`, and `screenshot` require write scope. |
| Monolith report | Whole-repo report was informational console output only. | CI uploads `monolith-report.txt` as an artifact. |

## Verification Evidence

| command | expected | actual | status |
|---|---|---|---|
| `cargo check --workspace --all-targets --locked` | Workspace all-target compile succeeds. | Finished successfully. | pass |
| `cargo fmt --all -- --check` | Formatting check succeeds. | No output; exit 0. | pass |
| `cargo test -p axon direct_fetch_fallback_keeps_custom_headers_per_config -- --nocapture` | Fallback uses per-config headers. | Test passed. | pass |
| `cargo test -p axon watch_create_uses_production_path_not_legacy_create_path -- --nocapture` | Legacy route does not reach create handler. | Test passed. | pass |
| `cargo test --locked --test workflow_shapes -- --nocapture` | Release and CI workflow shape contracts pass. | 2 passed. | pass |
| `cargo test --locked --test mcp_contract_parity -- --nocapture` | MCP contract target passes. | 31 passed. | pass |
| `cargo test -p axon scrape -- --nocapture` | Scrape-related tests pass. | 97 passed. | pass |
| `cargo test -p axon rest_ -- --nocapture` | REST-filtered tests pass. | 10 passed. | pass |
| `python3 scripts/cargo_test_filter_guard.py -- cargo test --locked --test http_api_parity_inventory rest_route_contracts_match_openapi_request_schemas -- --nocapture` | Guard finds and runs real filter. | Matched 1 test and passed. | pass |
| `python3 scripts/cargo_test_filter_guard.py -- cargo test --locked --test http_api_parity_inventory definitely_not_a_test_filter -- --nocapture` | Guard rejects zero-match filter. | Returned `matched 0 tests`. | pass |
| `python3 scripts/enforce_monoliths.py --file <changed-rust-file>` | Changed Rust files pass monolith hard limits. | Passed; one retained REST warning below hard limit. | pass |

## Risks and Rollback

- The scrape fallback no longer reuses a process-global client. This improves isolation but may reduce connection reuse for fallback-only fetches. Roll back by reverting `src/crawl/scrape.rs`, `src/core/http.rs`, `src/core/http/client.rs`, and the fallback test.
- MCP write-scope alignment may require clients with read-only tokens to request write scope for `ask`, `evaluate`, `suggest`, `research`, and `screenshot`. Roll back by reverting the `MCP_ACTION_SPECS` scope changes and the parity test.
- CI workflow changes affect GitHub Actions behavior. Roll back by reverting `.github/workflows/ci.yml`, `.github/workflows/release.yml`, `scripts/cargo_test_filter_guard.py`, and `tests/workflow_shapes.rs`.

## Decisions Not Taken

- Did not move any files under `docs/plans/` because completion status for remaining non-complete plans was not proven by this session.
- Did not delete branches or worktrees because only one active worktree and branch were registered.
- Did not revert unrelated palette-tauri dirty files.
- Did not commit the review remediation code in this save-to-md step because the skill contract requires committing only the generated session artifact.

## References

- `.full-review/05-final-report.md`
- `docs/operations/security.md`
- `src/core/http/client.rs`
- `src/crawl/scrape.rs`
- `src/web/server/handlers/rest.rs`
- `.github/workflows/ci.yml`
- `.github/workflows/release.yml`
- Bead `axon_rust-c7ue`

## Open Questions

- The active plan injection points to `/home/jmagar/workspace/axon_rust/docs/plans/2026-05-27-android-phase2-stubbed-modes.md`, outside the current Axon repo; no plan move was made.
- The implementation remediation files remain dirty and uncommitted after this save artifact commit.
- Existing dirty palette-tauri work remains untouched and should be reviewed separately by its owner.

## Next Steps

1. Review the implementation diff excluding `apps/palette-tauri/**` and decide whether to commit the review remediation as a separate code commit.
2. If committing remediation, stage only the intended review-fix files and keep palette-tauri work out of that commit.
3. Run the same verification set before the implementation commit if more edits are made.
4. Push only after the implementation commit is reviewed and the dirty-worktree split is clear.
