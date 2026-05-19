---
date: 2026-05-19 14:01:27 EST
repo: git@github.com:jmagar/axon.git
branch: main
head: 161001d9
working directory: /home/jmagar/workspace/axon_rust
worktree: /home/jmagar/workspace/axon_rust                                         161001d9 [main]
---

# Setup Split, Lite Cleanup, and Help Snapshots

## User Request

The session started with branch/PR cleanup around PR #105 and then shifted into Axon runtime cleanup: remove remaining `lite` language, remove obsolete setup repair/migration behavior, split setup into explicit preflight/smoke/stack/init surfaces, update docs, and lock CLI help output with snapshots.

## Session Overview

- Investigated `watch` versus `watch_lite` and confirmed the public command is `axon watch` while the internal implementation retained stale `watch_lite` naming.
- Continued the cleanup theme by removing `axon setup repair` / `--migrate-env` behavior and moving setup toward explicit commands.
- Implemented top-level `axon preflight`, `axon smoke`, `axon stack`, and `axon setup init`, with `axon setup` acting as a wrapper.
- Updated active docs for the new setup model and added CLI help snapshot coverage.
- Fixed nested help rendering so `axon setup init --help` no longer shows unrelated global crawl/vector flags.

## Sequence of Events

1. Reviewed open Beads after PR #105 and selected PR aftermath plus `axon_rust-3qrm` pool reuse as the near-term implementation path.
2. Investigated why `cargo test watch --lib` matched unrelated `watchdog` and YouTube `watch_urls` tests; concluded Cargo's filter is substring-based.
3. Confirmed `watchdog` is job stale/reclaim logic, while `watch_urls` is YouTube URL parsing, not scheduled watch.
4. Identified stale `lite` runtime naming and started removing the `src/jobs/lite/` nesting and `watch_lite` module naming.
5. Removed setup repair/migrate-env compatibility surfaces because the project has no external users requiring migration.
6. Split setup into `preflight`, `smoke`, `stack up/down/restart/rebuild`, `setup init`, and `setup` wrapper behavior.
7. Dispatched a docs worker to refresh active docs and then tightened remaining docs manually.
8. Added CLI help contract assertions and literal help snapshot fixtures.
9. Found and fixed the nested-help bug where `setup init --help` fell through to raw Clap output and inherited all global flags.

## Key Findings

- Cargo test filters are simple substring matches, so broad filters like `cargo test watch --lib` can match unrelated `watchdog_*` and `watch_urls` tests.
- `setup init --help` was still polluted because the custom help renderer only intercepted one-level command help paths.
- The installed `/home/jmagar/.local/bin/axon` was verified after the nested-help fix and produced focused `setup init` help.
- `scripts/check-env-config-boundary.py` treated auth scope constants (`AXON_READ_SCOPE`, `AXON_WRITE_SCOPE`, `AXON_FULL_ACCESS_SCOPE`) as env vars; they are Rust constants, not runtime env keys.
- The worktree is very dirty with many unrelated changes already in progress; no staging or commit was performed in this save step.

## Technical Decisions

- `setup init` remains file/setup oriented; auth completeness is caught by `preflight` in the full `axon setup` wrapper instead of making init itself a hard validator.
- `stack up` starts services with `docker compose up -d` and then follows logs so the user can watch startup and exit with Ctrl-C while containers continue running.
- Help snapshots were added as literal fixture files instead of only assertion-based tests, so accidental help drift is reviewed explicitly.
- Nested CLI help now routes through the same custom renderer as top-level command help.
- Removed migration/repair surfaces rather than preserving compatibility shims, matching the single-user/no-backcompat requirement.

## Files Modified

Primary setup and CLI changes:

- `src/core/config/cli.rs`
- `src/core/config/cli/setup_args.rs`
- `src/core/config/help.rs`
- `src/core/config/parse/build_config.rs`
- `src/core/config/parse/build_config/command_dispatch.rs`
- `src/core/config/parse_tests.rs`
- `src/core/config/types/enums.rs`
- `src/cli/commands/setup.rs`
- `src/lib.rs`

Setup service changes:

- `src/services/setup.rs`
- `src/services/setup/local.rs`
- `src/services/setup/local/model.rs`
- `src/services/setup/local/preflight.rs`
- `src/services/setup/local/env.rs`
- `src/services/setup/local/runtime.rs`
- `src/services/setup/local/env_tests.rs`

Tests and fixtures:

- `tests/cli_help_contract.rs`
- `tests/setup_check_cli.rs`
- `tests/compose_env_contract.rs`
- `tests/fixtures/cli-help/preflight.help`
- `tests/fixtures/cli-help/smoke.help`
- `tests/fixtures/cli-help/stack.help`
- `tests/fixtures/cli-help/setup-init.help`

Docs and guard scripts:

- `.env.example`
- `CLAUDE.md`
- `README.md`
- `docs/CONFIG.md`
- `docs/MCP.md`
- `docs/SETUP.md`
- `docs/commands/mcp.md`
- `docs/commands/serve.md`
- `docs/commands/setup.md`
- `docs/production-readiness-sprint-report-2026-05-12.md`
- `scripts/check-env-config-boundary.py`

Other dirty files exist in the worktree and were not all reviewed as part of this save note.

## Commands Executed

- `cargo fmt --all`
- `cargo fmt --all --check`
- `cargo check --bin axon`
- `cargo test -q --locked --test cli_help_contract`
- `cargo test -q --lib --locked parse_setup_init_preflight_smoke_and_stack_modes`
- `cargo test -q --locked --test setup_check_cli`
- `cargo test -q --locked --test compose_env_contract`
- `./scripts/check_legacy_runtime_terms.sh`
- `python3 scripts/check-env-config-boundary.py`
- `rg -n -e "--no-repair|no-repair|setup repair|setup check|--repair" ...`
- `./target/debug/axon setup init --help`
- `cargo run --quiet --bin axon -- setup init --help`
- `which -a axon && axon setup init --help`

## Errors Encountered

- `cargo test -q --lib --locked parse_setup_init_preflight_smoke_and_stack_modes` initially failed to compile because `src/services/action_api_tests.rs` still assigned removed `AskRequest { graph: None }` fields. The stale fixture fields were removed and the targeted lib test passed.
- `python3 scripts/check-env-config-boundary.py` initially failed because Rust auth scope constants looked like env var names. The script now ignores `AXON_READ_SCOPE`, `AXON_WRITE_SCOPE`, and `AXON_FULL_ACCESS_SCOPE`.
- `setup init --help` initially showed raw Clap output with inherited globals like `--max-depth`, `--render-mode`, `--tei-url`, and `--qdrant-url`. Nested help paths now use the custom help renderer and snapshot tests lock the focused output.

## Behavior Changes (Before/After)

| Area | Before | After |
|---|---|---|
| Setup repair | `axon setup repair` / env migration behavior existed | Public repair/migrate-env surface removed |
| Setup wrapper | Setup mixed setup/repair/preflight concerns | `axon setup` wraps init + stack up + preflight |
| Preflight | Setup-adjacent checks | Top-level `axon preflight` |
| Smoke checks | Not a first-class top-level setup surface | Top-level `axon smoke` |
| Stack control | Docker compose mostly documented/manual | `axon stack up/down/restart/rebuild` |
| Stack logs | Start command did not follow logs | `stack up` starts detached and follows compose logs |
| Nested help | `setup init --help` inherited unrelated globals | Focused setup-init help only |
| Help drift | Assertion-only coverage | Literal snapshot fixtures for setup split help |

## Verification Evidence

| Command | Expected | Actual | Status |
|---|---|---|---|
| `cargo check --bin axon` | Binary typechecks | Finished successfully | Pass |
| `cargo test -q --locked --test cli_help_contract` | Help contract and snapshots pass | 11 passed | Pass |
| `cargo test -q --lib --locked parse_setup_init_preflight_smoke_and_stack_modes` | Setup parse test passes | 1 passed | Pass |
| `cargo test -q --locked --test setup_check_cli` | Setup CLI tests pass | 7 passed | Pass |
| `cargo test -q --locked --test compose_env_contract` | Compose/env contract tests pass | 12 passed | Pass |
| `./scripts/check_legacy_runtime_terms.sh` | No forbidden legacy runtime terms | Exit 0 | Pass |
| `python3 scripts/check-env-config-boundary.py` | Env/config inventory clean | `env/config boundary ok: 197 classified keys` | Pass |
| Active docs stale repair scan | No active stale setup repair/no-repair hits | No matches | Pass |

## Risks and Rollback

- The setup split changes public CLI behavior by removing repair/migration commands. Rollback is a normal git revert of the setup/config/docs/test changes.
- `stack up` following logs intentionally blocks until Ctrl-C after starting detached services. If this proves irritating, add `stack logs` and make `stack up` print a pointer instead.
- The worktree contains broad unrelated changes, including job/runtime renames and docs imports. Review staging carefully before committing.
- Help snapshots will fail on intentional wording changes until fixture files are updated after review.

## Decisions Not Taken

- Did not make `setup init --auth-mode oauth` fail on missing OAuth fields; `init` is meant to write files, while `preflight` validates operational completeness.
- Did not preserve `migrate-env` or `setup repair` as hidden aliases; the user explicitly rejected compatibility cleanup for nonexistent external users.
- Did not stage, commit, or push during the save step.

## Open Questions

- Whether `preflight` should hard-fail or warn on GPU/TEI-specific checks for CPU-only development.
- Whether `stack up` should use `docker compose logs --tail N -f` instead of unbounded `logs -f`.
- Whether the env migration matrix should be renamed to an env/config inventory now that migration behavior was removed.
- Whether all large unrelated dirty changes in the worktree are intended for the same commit.

## Next Steps

Started but not fully completed:

- Finish reviewing the broad dirty worktree before staging or committing.
- Decide whether to rename the env migration matrix and associated script language.

Recommended follow-up:

- Add `axon stack logs` and `axon stack status`.
- Add temp-home integration tests for `setup init --auth-mode bearer` and OAuth field writing.
- Continue `axon_rust-3qrm` pool reuse work after the setup/runtime cleanup is committed.
