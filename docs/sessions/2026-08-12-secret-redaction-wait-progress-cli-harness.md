---
date: 2026-08-12 21:03:15 EST
repo: git@github.com:dinglebear-ai/axon.git
branch: detached HEAD
head: 49c0a635bff9790c44cf829b9a144eafbc226064
working directory: /home/jmagar/workspace/axon
worktree: /home/jmagar/workspace/axon
pr: "#552 fix: narrow secret redaction and improve wait progress (https://github.com/dinglebear-ai/axon/pull/552)"
beads: axon_rust-craqb, axon_rust-craqb.1, axon_rust-craqb.2, axon_rust-craqb.3, axon_rust-craqb.4, axon_rust-craqb.5, axon_rust-craqb.6
---

# Secret redaction, interactive wait progress, and CLI harness hardening

## User Request

Investigate Axon's overly aggressive `secret-redaction-forbidden` behavior in a new worktree, then make synchronous `--wait` output quieter, more useful, animated, and visually consistent with Aurora. Run and improve the complete CLI harness, fix its environment handling, review the resulting PR with Vibin and Lavra, address every finding, and merge it.

An earlier request to download and deploy the latest release to the host path and Incus container was superseded before deployment work began.

## Session Overview

The session produced and merged PR #552. It narrowed context-free secret detection without weakening contextual credential protection, added a cohesive Aurora-styled foreground progress system, polished synchronous CLI output, expanded regression coverage, and made the all-commands live harness safer, faster, and collision-resistant. Two review rounds exposed security, correctness, architecture, generated-contract, release, and harness issues; all were fixed before the PR was squash-merged as `b3df570806d19ebcd4ed7ba3d7522a10e0a0789d`.

## Sequence of Events

1. Created `.worktrees/secret-redaction-investigation` and traced the redaction warnings through detector, boundary, vector payload, and source indexing code.
2. Established that compatibility vocabulary was being treated too broadly, then implemented contextual detection and regression coverage for headers, assignments, cookies, Bearer tokens, and local-path behavior.
3. Designed and mocked a compact Aurora CLI wait view, then implemented quiet interactive progress for source, batch, session, and extract foreground work.
4. Polished synchronous command output with stronger contrast, minimal important notices, terminal sanitization, animation, and deterministic non-TTY behavior.
5. Ran the all-commands harness, fixed failures, reviewed its coverage and speed, added isolated fixtures, bounded timeouts, parser parallelism, transient web retries, and `.env` boundary classifications.
6. Pushed PR #552 and used Vibin review plus targeted review/simplification passes to fix cookie-tail leakage, false success states, batch outcome accounting, layering, generated docs, versioning, frontmatter, and monolith issues.
7. Used Lavra architecture, security, and simplicity review passes; tracked all findings in Beads and addressed the six primary findings plus lower-severity cleanup.
8. Merged current `main` twice as it advanced, resolved conflicts, reran local and hosted verification, and squash-merged the clean PR after all required checks passed.
9. Ran the save-session maintenance pass, removed only the merged PR #552 worktree and branch, and left unrelated worktrees and branches untouched.

## Key Findings

- Context-free forbidden fragments were the main false-positive source; contextual recognition now lives in `crates/axon-core/src/redact/detectors.rs:323-430`, while `passwd` remains assignment vocabulary at `crates/axon-core/src/redact/detectors.rs:513`.
- Redaction must fail closed after span replacement. Residual checks are centralized in `requires_full_redaction` at `crates/axon-core/src/redact/boundary.rs:407` and applied after span handling at lines 272 and 340.
- Cookie and quoted values could leak trailing secret material if only the first regex span was accepted. Regression fixes consume the relevant value and revalidate the transformed output.
- Foreground terminal state must derive from actual lifecycle outcomes. Terminal modeling is isolated in `crates/axon-cli/src/commands/wait_progress/model/terminal.rs:6-58`, and aggregate skipped-chunk counts are parsed in `model.rs:453-475`.
- Every progress hop needs backpressure and hostile terminal text must be sanitized. Batch forwarding uses a bounded channel at `render/batch.rs:52`, with sanitization in `format.rs:248`.
- Concurrent live harness runs previously shared state and ports. The final harness reserves isolated port blocks in `scripts/live-test-all-commands.sh:59-81`, classifies its environment-only keys in `scripts/check-env-config-boundary.py:57-68`, and has collision/cleanup coverage in `tests/live_command_harness.rs:485-599`.

## Technical Decisions

- Kept high-confidence contextual credential detection and fail-closed public-write boundaries, while removing context-free substring rejection that treated ordinary documentation as secret material.
- Used one transport-neutral foreground snapshot in `axon-services`; CLI renderers consume that facade rather than reaching into `axon-jobs` storage internals.
- Rendered motion only for interactive terminals. JSON/stdout contracts remain machine-readable, while progress and notices stay on stderr and non-TTY output is stable.
- Used Aurora's product CLI palette and console helpers rather than the separate Claude Code theme token values.
- Made live retries narrow: only safe web commands retry explicitly classified transient failures, with the first failure retained in logs; state-changing commands do not retry.
- Integrated current `main` rather than duplicating concurrent harness hardening, then reran the head-specific suite and hosted checks after each merge.

## Files Changed

PR #552 changed 158 files: 17 created and 141 modified. This is the complete path inventory from the GitHub pull-request files API; no files were renamed or deleted.

```text
M CHANGELOG.md
M Cargo.lock
M Cargo.toml
M README.md
M apps/web/openapi/axon.json
M apps/web/package-lock.json
M apps/web/package.json
M crates/axon-adapters/src/cli_tool/redact.rs
M crates/axon-adapters/src/cli_tool/redact_tests.rs
M crates/axon-adapters/src/mcp_tool/redact.rs
M crates/axon-adapters/src/mcp_tool/redact_tests.rs
M crates/axon-adapters/src/web/site_discovery.rs
M crates/axon-adapters/src/web/site_discovery_tests.rs
M crates/axon-cli/Cargo.toml
M crates/axon-cli/src/commands.rs
M crates/axon-cli/src/commands/ask.rs
M crates/axon-cli/src/commands/brand.rs
M crates/axon-cli/src/commands/common_jobs.rs
M crates/axon-cli/src/commands/debug.rs
M crates/axon-cli/src/commands/diff.rs
M crates/axon-cli/src/commands/doctor.rs
M crates/axon-cli/src/commands/domains.rs
M crates/axon-cli/src/commands/endpoints.rs
M crates/axon-cli/src/commands/evaluate.rs
M crates/axon-cli/src/commands/extract.rs
M crates/axon-cli/src/commands/extract_tests.rs
M crates/axon-cli/src/commands/job_progress.rs
M crates/axon-cli/src/commands/jobs.rs
M crates/axon-cli/src/commands/map.rs
M crates/axon-cli/src/commands/memory/import_export.rs
M crates/axon-cli/src/commands/prune.rs
M crates/axon-cli/src/commands/research.rs
M crates/axon-cli/src/commands/reset.rs
M crates/axon-cli/src/commands/resources.rs
M crates/axon-cli/src/commands/retrieve.rs
M crates/axon-cli/src/commands/screenshot.rs
M crates/axon-cli/src/commands/screenshot/screenshot_migration_tests.rs
M crates/axon-cli/src/commands/search.rs
M crates/axon-cli/src/commands/sessions.rs
A crates/axon-cli/src/commands/sessions_tests.rs
M crates/axon-cli/src/commands/setup.rs
M crates/axon-cli/src/commands/source.rs
A crates/axon-cli/src/commands/source/batch.rs
M crates/axon-cli/src/commands/source_tests.rs
M crates/axon-cli/src/commands/sources.rs
M crates/axon-cli/src/commands/stats.rs
M crates/axon-cli/src/commands/status.rs
M crates/axon-cli/src/commands/suggest.rs
M crates/axon-cli/src/commands/summarize.rs
M crates/axon-cli/src/commands/sync.rs
M crates/axon-cli/src/commands/train.rs
M crates/axon-cli/src/commands/update.rs
A crates/axon-cli/src/commands/wait_progress.rs
A crates/axon-cli/src/commands/wait_progress/format.rs
A crates/axon-cli/src/commands/wait_progress/format_tests.rs
A crates/axon-cli/src/commands/wait_progress/model.rs
A crates/axon-cli/src/commands/wait_progress/model/batch.rs
A crates/axon-cli/src/commands/wait_progress/model/terminal.rs
A crates/axon-cli/src/commands/wait_progress/model_tests.rs
A crates/axon-cli/src/commands/wait_progress/render.rs
A crates/axon-cli/src/commands/wait_progress/render/batch.rs
A crates/axon-cli/src/commands/wait_progress/render/batch_tests.rs
A crates/axon-cli/src/commands/wait_progress/render/extract.rs
A crates/axon-cli/src/commands/wait_progress/render/session.rs
A crates/axon-cli/src/commands/wait_progress/render/session_tests.rs
A crates/axon-cli/src/commands/wait_progress/render_tests.rs
A crates/axon-cli/src/commands/wait_progress/timing.rs
A crates/axon-cli/src/commands/wait_progress/timing_tests.rs
M crates/axon-cli/src/commands/watch.rs
M crates/axon-cli/src/json.rs
M crates/axon-cli/src/json_tests.rs
M crates/axon-cli/src/lib.rs
M crates/axon-core/src/config.rs
M crates/axon-core/src/config/cli/global_args.rs
M crates/axon-core/src/config/help.rs
M crates/axon-core/src/config/parse/build_config/config_literal.rs
M crates/axon-core/src/config/parse_tests.rs
M crates/axon-core/src/config/types.rs
M crates/axon-core/src/config/types/config.rs
M crates/axon-core/src/config/types/config_debug.rs
M crates/axon-core/src/config/types/config_impls.rs
M crates/axon-core/src/config/types/enums.rs
M crates/axon-core/src/logging.rs
M crates/axon-core/src/logging/aurora.rs
M crates/axon-core/src/logging_tests.rs
M crates/axon-core/src/redact.rs
M crates/axon-core/src/redact/boundary.rs
M crates/axon-core/src/redact/boundary_tests.rs
M crates/axon-core/src/redact/detectors.rs
M crates/axon-core/src/redact/detectors_tests.rs
M crates/axon-core/src/redact_tests.rs
M crates/axon-core/src/ui.rs
A crates/axon-core/src/ui/console.rs
A crates/axon-core/src/ui/console_tests.rs
M crates/axon-core/src/ui_color_tests.rs
M crates/axon-document/src/chunk_router_tests.rs
M crates/axon-document/src/markdown.rs
M crates/axon-document/src/markdown_tests.rs
M crates/axon-document/src/preparer.rs
M crates/axon-document/src/preparer/chunk_build.rs
M crates/axon-document/src/preparer_tests.rs
M crates/axon-document/src/profile.rs
M crates/axon-jobs/src/unified.rs
M crates/axon-jobs/src/unified/control.rs
M crates/axon-jobs/src/unified/ops.rs
A crates/axon-jobs/src/unified/ops_helpers.rs
A crates/axon-jobs/src/unified/terminal_warnings.rs
M crates/axon-jobs/src/unified_tests.rs
M crates/axon-llm/src/runtime/headless/common_tests.rs
M crates/axon-observe/src/sink/sqlite_tests.rs
M crates/axon-services/src/context.rs
M crates/axon-services/src/extract.rs
M crates/axon-services/src/extract/sync.rs
M crates/axon-services/src/extract/sync_tests.rs
M crates/axon-services/src/lib.rs
M crates/axon-services/src/source.rs
M crates/axon-services/src/source/dispatch/tool_tests.rs
M crates/axon-services/src/source/events.rs
M crates/axon-services/src/source/events_tests.rs
M crates/axon-services/src/source/execution.rs
M crates/axon-services/src/source/executor.rs
M crates/axon-services/src/source/executor/progress.rs
M crates/axon-services/src/source/executor/progress_tests.rs
M crates/axon-services/src/source/executor/vector_points.rs
M crates/axon-services/src/source/executor/vectorize.rs
M crates/axon-services/src/source/executor/vectorize_tests.rs
A crates/axon-services/src/source/foreground_progress.rs
A crates/axon-services/src/source/foreground_progress_tests.rs
M crates/axon-vectors/src/payload.rs
M crates/axon-vectors/src/payload_redaction.rs
M crates/axon-vectors/src/payload_tests.rs
M crates/axon-vectors/src/point.rs
M crates/axon-vectors/src/point_tests.rs
M docs/development/repo/scripts.md
M docs/guides/configuration.md
A docs/investigations/2026-08-09-secret-redaction-false-positives.md
M docs/reference/source-input-manifest.json
M docs/reference/sources/vector-payload.md
M docs/reference/sources/vector-payload.schema.json
A docs/superpowers/plans/2026-08-11-interactive-wait-progress.md
A docs/superpowers/specs/2026-08-11-interactive-wait-progress-design.md
M scripts/check-env-config-boundary.py
A scripts/lib/live-cli-fixtures.sh
M scripts/lib/live-cli-parser.sh
M scripts/lib/live-cli-reporting.sh
M scripts/lib/live-cli-runtime.sh
M scripts/live-test-all-commands.sh
M tests/cli_polish_regression.rs
M tests/fixtures/cli-help/compose.help
M tests/fixtures/cli-help/preflight.help
M tests/fixtures/cli-help/setup-init.help
M tests/fixtures/cli-help/smoke.help
M tests/fixtures/cli-json/status.json
M tests/live_command_harness.rs
M xtask/src/schemas/generated_contract_tests.rs
M xtask/src/schemas/vector_payload.rs
M xtask/src/schemas/vector_payload_markdown.rs
M xtask/tests/fixtures/schemas/vector-payload/snapshots/vector-payload.schema.json
```

This session log itself is the only file changed by the save-session publication commit.

## Beads Activity

| ID | Title | Actions | Final status | Why it mattered |
|---|---|---|---|---|
| `axon_rust-craqb` | Lavra review PR #552 | Created, tracked review, received two summary comments, closed | closed | Parent record for resolving every introduced Lavra finding |
| `axon_rust-craqb.1` | Isolate concurrent live harness runs | Created, documented cause/fix/prevention, closed | closed | Prevented shared output, collection, Compose, and port collisions |
| `axon_rust-craqb.2` | Prevent vector redaction diagnostics from logging raw paths | Created, documented security boundary, closed | closed | Removed attacker-controlled metadata paths from warnings |
| `axon_rust-craqb.3` | Preserve aggregate redaction skip counts in wait output | Created, added aggregate-count regression, closed | closed | Kept operator policy counts accurate |
| `axon_rust-craqb.4` | Unify foreground progress snapshot state | Created, documented cohesive snapshot design, closed | closed | Prevented routed source-kind/status state divergence |
| `axon_rust-craqb.5` | Bound batch progress forwarding | Created, documented backpressure rule, closed | closed | Prevented unbounded progress memory growth |
| `axon_rust-craqb.6` | Restore contextual standalone bearer and passwd detection | Created, documented credential regressions, closed | closed | Preserved real secret detection after narrowing false positives |

All seven records were closed only after implementation and verification were observed. No remaining PR #552 work required a follow-up bead.

## Repository Maintenance

### Plans

- Inspected `docs/plans/` with `find docs/plans -maxdepth 2 -type f`. No session-related completed plan lived there, so nothing was moved.
- The delivered design and execution records already live at `docs/superpowers/specs/2026-08-11-interactive-wait-progress-design.md` and `docs/superpowers/plans/2026-08-11-interactive-wait-progress.md`; they were left in place because the maintenance contract only targets clearly completed files under `docs/plans/` and this publication must contain only the generated session artifact.

### Beads

- Read the parent and all six child records with `bd show ... --json`. Every record was already closed with implementation/verification evidence; no tracker mutation was needed during save-session closeout.

### Worktrees and branches

- Inspected `git worktree list --porcelain`, `git branch -vv`, remote branches, PR state, and the merge commit on `origin/main`.
- Removed only `/home/jmagar/workspace/axon/.worktrees/secret-redaction-investigation` after confirming it was clean and PR #552 was merged; pruned registrations and deleted local branch `codex/secret-redaction-investigation`.
- GitHub had already removed the remote feature branch. The explicit delete returned `remote ref does not exist`; `git fetch --prune origin` removed the stale tracking ref.
- Left all other worktrees, detached checkouts, divergent local branches, and remote branches untouched because their ownership or active status was unrelated or ambiguous.

### Stale documentation

- Reviewed the investigation, interactive-progress plan/spec, generated vector-payload reference, configuration guide, and harness documentation represented in the merged PR.
- Generated-contract and repository-contract checks were green on the merged head. No additional stale-doc correction was identified, so the publication commit intentionally changes only this session note.

## Tools and Skills Used

- **Shell and file tools.** Used `rg`, Git, Cargo, repository scripts, `apply_patch`, and focused file reads for diagnosis, implementation, maintenance, and verification. A `gh pr diff --name-status` probe failed because that flag is unsupported; the GitHub files API supplied the authoritative inventory instead.
- **GitHub CLI.** Created, inspected, pushed, monitored, and merged PR #552. A merge-commit attempt was rejected by repository policy; squash merge succeeded.
- **Superpowers skills.** Used systematic debugging for redaction causality, executing plans for inline implementation, and finishing-a-development-branch for final integration. Subagent-driven development was explicitly not used.
- **Vibin plugin.** Used `review-pr` for the first full review and `save-to-md` for this artifact and its path-limited landing workflow.
- **Lavra plugin/reviewers.** Architecture, security, and simplicity reviewers performed read-only review passes; implementation remained in the primary session. Additional read-only code-review and simplification passes surfaced correctness and monolith issues.
- **Image generation.** Produced a visual CLI mock used to refine the interactive wait layout; the user-provided/generated artifact was `/home/jmagar/.codex/generated_images/019fe881-c24d-7432-9285-a74b65033a2c/exec-5c275c59-a637-4e5d-ad6c-b30afafe4e89.png`.
- **MCP/Labby.** The session-start Labby health probe reported `http://localhost:8765/health` unreachable. No MCP gateway mutation or external browser automation was used for the implementation.

## Commands Executed

| Command | Result |
|---|---|
| `git worktree add ... codex/secret-redaction-investigation` | Created the isolated implementation checkout |
| `cargo test -p axon-core redact --quiet` | 86 tests passed |
| focused adapter/vector/CLI/service test commands | 5, 26, 29, and 4 tests passed respectively |
| `scripts/live-test-all-commands.sh` | 1,590 passed, 0 failed; 310/310 behavioral contracts present |
| `cargo test --locked --features test-helpers --test live_command_harness -- --nocapture` | Final synchronized harness: 18 passed |
| `python3 scripts/check-env-config-boundary.py` | 338 classified keys; boundary passed |
| `cargo fmt --all -- --check`, Clippy, layering, generated-contract, monolith checks | All passed |
| `gh pr checks 552 --watch --interval 30` | All hosted checks and `ci-gate` passed |
| `gh pr merge 552 --squash` | Merged PR #552 as `b3df570806d19ebcd4ed7ba3d7522a10e0a0789d` |
| `git worktree remove ...secret-redaction-investigation` | Removed the clean merged worktree |

## Errors Encountered

- Multi-cookie and quoted-value review cases showed residual secret tails could survive the first span replacement. The boundary now revalidates transformed output and tests cover these cases.
- Failed foreground jobs could render a synthetic green terminal, and batch `Ok(SourceResult { status: Failed })` outcomes could be counted as success. Terminal and batch outcome derivation now use real lifecycle/result status.
- Hosted contract jobs initially failed because the CLI version was already tagged and the investigation document lacked required frontmatter. The component version was bumped to 7.2.19, artifacts regenerated, and frontmatter added.
- `main` advanced twice during review/CI. Both conflicts were resolved by merging current `origin/main`, then rerunning head-specific tests and CI.
- A transient live web fetch timeout exposed harness flakiness. A single retry was added only for explicitly safe web commands and transient error codes.
- `gh pr merge 552 --merge` returned `Merge commits are not allowed on this repository`; `gh pr merge 552 --squash` was accepted.
- During maintenance, remote branch deletion returned `remote ref does not exist` because GitHub had already removed it; `git fetch --prune origin` cleared the stale tracking ref.
- The first session-log commit attempt hit the pre-commit hook's 60-second `xtask-check` budget while compiling a cold temporary worktree. No commit was created; after the cache warmed, the same path-limited commit was retried without bypassing hooks.

## Behavior Changes (Before/After)

| Area | Before | After |
|---|---|---|
| Documentation ingestion | Ordinary secret-related prose could cause whole chunks to be rejected | Contextual markers drive detection; ordinary prose is retained |
| Credential safety | Early span return could leak later cookie/quoted material | Transformed content is revalidated and residual credentials fail closed |
| `--wait` output | Repetitive logs with limited operational context | Compact Aurora milestones, animated live phase, current item, progress, and important notices |
| Failure rendering | Some failed jobs could briefly show green completion | Terminal color/status derives from actual failure, cancellation, expiry, skip, or degradation |
| Batch progress | Status and forwarding could miscount or grow without bound | Explicit outcome accounting and bounded forwarding preserve correctness/backpressure |
| Synchronous noise | Routine informational lines competed with results | Quiet-by-default operator output gates routine noise while preserving warnings/errors |
| Live harness | Shared state/ports and broad retries made parallel runs flaky | Per-run identity, isolated state, leased ports, bounded cleanup, and narrow retries |
| Environment contract | Harness-only keys were not fully classified | All live harness environment keys are explicitly classified without becoming tuning knobs |

## Verification Evidence

| Command | Expected | Actual | Status |
|---|---|---|---|
| `cargo test -p axon-core redact --quiet` | Redaction suite green | 86 passed | pass |
| Adapter redaction tests | Contextual adapter behavior green | 5 passed | pass |
| Vector payload tests | Boundary and logging regressions green | 26 passed | pass |
| CLI wait-progress tests | Rendering/model regressions green | 29 passed | pass |
| Service foreground-progress tests | Snapshot facade green | 4 passed | pass |
| Full live command matrix | No command or behavioral failures | 1,590 passed, 0 failed; 310/310 behavioral | pass |
| Final `live_command_harness` after `main` sync | Harness remains green | 18 passed | pass |
| `check-env-config-boundary.py` | Every env key classified | 338 classified keys | pass |
| Formatting, Clippy, layering, generated contracts, monolith | All policy gates green | All passed | pass |
| GitHub required checks | Required checks and aggregate gate green | Repository Contract, codeql-gate, compose-smoke-gate, and ci-gate passed | pass |
| Final PR audit | Current head, clean state, mergeable | `CLEAN`, `MERGEABLE`, worktree clean before merge | pass |
| Merge ancestry | Squash commit present on remote main | `origin/main` at `b3df570806d19ebcd4ed7ba3d7522a10e0a0789d` | pass |

## Risks and Rollback

- Redaction changes affect a security boundary. Roll back the squash commit to restore the previous behavior, but doing so also restores the documented false positives and residual-span bugs; prefer reverting only the specific detector change with its tests if a regression appears.
- Interactive rendering depends on TTY detection and stderr/stdout separation. Machine consumers should continue using `--json`; rollback is the wait-progress renderer/facade portion of PR #552.
- Harness isolation adds temporary directories, port leases, and fixture cleanup. Failures are bounded and reported; stale temporary state can be inspected before removal rather than broadly deleting Axon state.

## Decisions Not Taken

- Did not disable secret redaction or weaken fail-closed vector publication globally; the fix narrows matching while preserving contextual credentials.
- Did not log matched values or metadata paths to explain rejections; diagnostics use stable detector IDs and chunk IDs to avoid leaking attacker-controlled or secret content.
- Did not use subagent-driven development. Review agents were read-only, and fixes were integrated inline as explicitly requested.
- Did not delete unrelated worktrees or divergent branches during maintenance because their ownership and merge state were outside this session.
- Did not perform the original release deployment request after the user redirected the session to investigation and implementation.

## References

- PR #552: https://github.com/dinglebear-ai/axon/pull/552
- Investigation: `docs/investigations/2026-08-09-secret-redaction-false-positives.md`
- Interactive wait design: `docs/superpowers/specs/2026-08-11-interactive-wait-progress-design.md`
- Interactive wait implementation plan: `docs/superpowers/plans/2026-08-11-interactive-wait-progress.md`
- Vector payload contract: `docs/reference/sources/vector-payload.md`
- CLI harness guide: `docs/development/repo/scripts.md`

## Open Questions

- The initial request to deploy the latest Axon release to the host path and Incus container was superseded and remains unexecuted; it should be treated as a separate operational task if still desired.
- Several unrelated worktrees and divergent local branches remain registered. Their owners and active PR state were not established, so they were intentionally left untouched.

## Next Steps

- No unfinished PR #552 implementation or review work remains; the PR is merged and all related Beads are closed.
- Monitor real documentation-heavy crawls for redaction skip-rate changes and use stable detector IDs rather than payload content for diagnostics.
- If deployment is still desired, begin a separate task by verifying the current release, host binary, systemd unit, Incus container, and runtime endpoints before changing either installation.
- Reconcile unrelated worktrees only in their owning sessions after checking dirt, PR state, ancestry, and remote heads.
