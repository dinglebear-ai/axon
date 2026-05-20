---
date: 2026-05-19 21:28:42 EDT
repo: git@github.com:jmagar/axon.git
branch: codex/server-mode-rest-cutover
head: ad5f714c
plan: docs/superpowers/plans/2026-05-19-server-mode-rest-cutover.md
working directory: /home/jmagar/workspace/axon_rust/.worktrees/server-mode-rest-cutover
worktree: /home/jmagar/workspace/axon_rust/.worktrees/server-mode-rest-cutover ad5f714c [codex/server-mode-rest-cutover]
---

# Server Mode REST Cutover

## User Request

Execute `docs/superpowers/plans/2026-05-19-server-mode-rest-cutover.md` end to end in an isolated worktree from `origin/main` at `ad5f714c`, without modifying the root checkout.

## Session Overview

- Created and worked in `/home/jmagar/workspace/axon_rust/.worktrees/server-mode-rest-cutover` on branch `codex/server-mode-rest-cutover`.
- Implemented canonical client contracts, route metadata, artifact handles, endpoint resolution, direct REST server-mode routing, capability-aware doctor output, auth scope parity helpers, sync command plumbing, and MCP thin-client routing.
- Removed the mounted `/v1/actions` action router and added a cutover guard route so POST `/v1/actions` no longer succeeds.
- Updated API/MCP/contract/spec docs and the client/server smoke script.
- Ran focused tests, full tests, live smoke, docker compose config validation, `just verify`, pushed PR #115, and followed GitHub/AI-review checks.

## Sequence of Events

- Read the plan and required workflow skills, then created the isolated worktree from `origin/main`.
- Added the plan's foundational service modules and sidecar `_tests.rs` files.
- Rewired REST async lifecycle routes and CLI server mode to direct `/v1/*` REST routes.
- Deleted the legacy web action router and adjusted tests to assert `/v1/actions` is removed.
- Added doctor mode/capability/effective-endpoint output and parser support for `doctor diagnose`.
- Added MCP thin-client routing for server-configured stdio MCP requests.
- Fixed env migration matrix drift for `AXON_USER_AGENT` surfaced by the full suite.
- Ran verification gates and live client/server smoke.
- Created PR #115 and pushed follow-up CI fixes for CI-colored help snapshots and MCP `summarize` action discovery drift.

## Key Findings

- `cargo test -q` caught missing env migration metadata for `AXON_USER_AGENT`; the fix lives in `docs/config/env-migration-matrix.toml` and `scripts/check-env-config-boundary.py`.
- `just verify` caught clippy issues in `src/mcp/thin_client.rs` and `src/web/server/routing.rs`; both were fixed.
- `cargo-nextest` is installed, so `just verify` used nextest rather than the fallback test runner.
- GitHub CI caught a help snapshot environment issue caused by `CARGO_TERM_COLOR=always` without `NO_COLOR`; the help snapshot test harness now forces `NO_COLOR=1`.
- GitHub `mcp-smoke` caught drift where the tool description advertised `summarize` but `action:help`, generated schema docs, and mcporter expected lists omitted it.

## Technical Decisions

- CLI server mode now builds direct REST plans instead of MCP action envelopes.
- MCP stdio thin-client mode forwards supported actions to direct REST when `AXON_SERVER_URL` is set and `--local` is not active.
- `ServerRestPlan.path` is an owned `String` so dynamic job-status paths do not leak boxed strings.
- POST `/v1/actions` is explicitly routed to a removed-action response so static fallback cannot mask the cutover.

## Files Modified

- `src/services/client_contract.rs`, `src/services/route_meta.rs`, `src/services/artifacts.rs`, `src/services/sync.rs`: new service-level contracts and helpers.
- `src/core/endpoints.rs`, `src/core/health/doctor.rs`, `src/core/health/doctor/sqlite.rs`: endpoint resolution and doctor metadata.
- `src/cli/server_mode.rs`, `src/cli/server_mode/plan.rs`, `src/cli/client.rs`, `src/cli/commands/sync.rs`: direct REST client/server mode and sync command work.
- `src/mcp/thin_client.rs`, `src/mcp/server.rs`, `src/mcp/auth.rs`: MCP thin-client and shared scope behavior.
- `src/web/server/handlers/rest*.rs`, `src/web/server/routing.rs`, `src/web.rs`: REST parity/lifecycle routes and `/v1/actions` removal.
- `docs/API.md`, `docs/MCP.md`, `docs/MCP-TOOL-SCHEMA.md`, `docs/contracts/server-mode-routing-contract.md`, `docs/specs/server-mode-capability-tiers.md`, `scripts/test-client-server-mode.sh`, `scripts/test-mcp-tools-mcporter.sh`: docs and smoke coverage.
- Matching sidecar tests and fixtures were added or updated for each changed surface.

## Commands Executed

- `cargo test -q client_contract_tests`, `route_meta_tests`, `artifacts_tests`, `endpoints_tests`, `sync_tests`, `thin_client_tests`, `route_tests`, `rest_client_tests`: focused module coverage passed.
- `cargo test -q server_mode`, `cargo test -q rest`, `cargo test -q client`: focused integration groups passed.
- `cargo test -q --test http_api_parity_inventory`: REST inventory passed.
- `cargo test -q --test cli_help_contract`: help contract passed.
- `env -u NO_COLOR CARGO_TERM_COLOR=always cargo test -q --test cli_help_contract setup_split_help_snapshots_match`: reproduced the CI color environment and passed after the test harness fix.
- `cargo test -q mcp`: MCP-focused tests passed.
- `python3 scripts/generate_mcp_schema_doc.py --check`: generated MCP schema doc is in sync.
- `bash -n scripts/test-mcp-tools-mcporter.sh`: syntax passed.
- `cargo test -q`: full suite passed.
- `cargo fmt --check`: passed.
- `cargo check --bin axon`: passed.
- `bash -n scripts/axon` and `bash -n scripts/test-client-server-mode.sh`: passed.
- `docker compose -f docker-compose.yaml -f docker-compose.dev.yaml config --services`: passed and listed `axon-chrome`, `axon-qdrant`, `axon-tei`, `axon`.
- `AXON_SERVER_URL=http://127.0.0.1:8001 scripts/test-client-server-mode.sh`: live smoke passed.
- `just verify`: passed clippy, check, and 2,276 nextest tests with 6 skipped.

## Errors Encountered

- Initial full suite failed because help fixtures and command section coverage had not yet accounted for `sync`; updated help wiring and fixtures.
- Full suite then failed on env boundary drift for `AXON_USER_AGENT`; added the env matrix entry and ignored the Rust-only `AXON_API_UA` constant token.
- `just verify` failed on clippy warnings; collapsed nested `if` statements in `src/mcp/thin_client.rs` and returned the router expression directly in `src/web/server/routing.rs`.
- GitHub `test` initially failed on ANSI-colored help snapshot output; fixed by forcing `NO_COLOR=1` in the help contract subprocess harness.
- GitHub `mcp-smoke` initially failed on MCP action discovery parity; fixed by adding `summarize` to `action:help`, generated MCP schema action mapping, docs, and mcporter smoke expectations.

## Behavior Changes

- Before: CLI server mode used the MCP action-envelope HTTP path and `/v1/actions` remained mounted.
- After: CLI server mode uses direct REST endpoints, and `/v1/actions` returns the cutover guard response instead of accepting action envelopes.
- Before: stdio MCP always handled requests locally.
- After: stdio MCP forwards supported actions to direct REST when server mode is configured, with local fallback for unsupported actions.
- Before: doctor output did not report the new server-mode capability/endpoints metadata.
- After: doctor JSON includes mode, capabilities, effective endpoints, and remedies.

## Verification Evidence

| command | expected | actual | status |
| --- | --- | --- | --- |
| `cargo test -q` | full suite passes | 2002 lib tests passed plus integration/doc test groups passed; 6 ignored | pass |
| `cargo fmt --check` | formatted | no output | pass |
| `cargo check --bin axon` | builds | finished dev profile | pass |
| `docker compose -f docker-compose.yaml -f docker-compose.dev.yaml config --services` | compose parses | listed `axon-chrome`, `axon-qdrant`, `axon-tei`, `axon` | pass |
| `AXON_SERVER_URL=http://127.0.0.1:8001 scripts/test-client-server-mode.sh` | smoke passes | `client-server smoke: ok` | pass |
| `just verify` | repo gate passes | clippy/check passed; nextest ran 2,276 tests, 2,276 passed, 6 skipped | pass |
| `env -u NO_COLOR CARGO_TERM_COLOR=always cargo test -q --test cli_help_contract setup_split_help_snapshots_match` | CI color snapshot path passes | 1 passed | pass |
| `cargo test -q mcp` | MCP tests pass | 113 lib tests plus filtered integration groups passed | pass |
| `python3 scripts/generate_mcp_schema_doc.py --check` | generated schema doc in sync | `OK: docs/MCP-TOOL-SCHEMA.md is up to date` | pass |

## PR and Review

- PR: <https://github.com/jmagar/axon/pull/115>
- Commits pushed:
  - `05db4926 feat(server-mode): cut over clients to REST`
  - `71ebf5a7 test: stabilize help snapshots in CI`
- Review/tool substitutions: the Lab `Agent` tool was discovered but no usable agent type was available; local manual review, strict repo gates, PR check polling, CodeRabbit, Cubic, and GitHub checks were used instead.
- CodeRabbit completed with a pass and no actionable findings after an initial rate-limit/usage notice.
- Cubic AI reviewer completed with a pass on the rerun.

## Risks and Rollback

- The REST cutover changes a broad API path. Rollback is to revert this branch/commit and restore the previous `/v1/actions` router.
- Some local artifact sync functionality is represented as conservative service/CLI plumbing rather than a full artifact upload pipeline.
- MCP thin-client routing covers supported REST-equivalent actions and falls back locally for unsupported MCP-only actions.

## Decisions Not Taken

- Did not introduce `mod.rs`; all new modules use Rust 2018 sidecar file layout.
- Did not rewrite every service result struct to embed route metadata directly; route metadata helpers and REST/CLI surfaces were added without forcing a large response-contract migration.
- Did not replace the existing artifact subsystem with a new storage index; added stable handle helpers and sync decisions first.

## References

- `docs/superpowers/plans/2026-05-19-server-mode-rest-cutover.md`
- `docs/specs/server-mode-capability-tiers.md`
- `docs/contracts/server-mode-routing-contract.md`

## Open Questions

- Whether the partial artifact reconciliation scaffolding should become a full upload/register pipeline in a follow-up PR.
- Whether all service result structs should be migrated to carry route metadata fields directly, beyond the helper/envelope layer added here.

## Next Steps

- Resolve any additional PR review comments if new comments appear after final GitHub check completion.
