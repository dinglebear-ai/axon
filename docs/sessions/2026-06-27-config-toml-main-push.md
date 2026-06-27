---
date: 2026-06-27 08:07:47 EST
repo: git@github.com:jmagar/axon.git
branch: main
head: e07e5be5
session id: 34fb82ca-bbd6-4c0c-9a6a-2a467ee97e15
transcript: /home/jmagar/.claude/projects/-home-jmagar-workspace-axon/34fb82ca-bbd6-4c0c-9a6a-2a467ee97e15.jsonl
working directory: /home/jmagar/workspace/axon
worktree: /home/jmagar/workspace/axon e07e5be5 [main]
beads: axon_rust-pvl6e.23
---

# Config TOML migration and protected-main push

## User Request

The session started in the broader Lab/Labby issue flow, then narrowed to Axon configuration work. The final request was to push the Axon work to `main`, then save the session to markdown.

## Session Overview

The Axon work moved OpenAI-compatible embedding and tuning settings out of `.env` and into durable TOML config surfaces, resolved a rebase-introduced TOML parser duplicate, landed the changes through protected-branch PR #282, verified main and release workflows, and then captured this session log.

The live repository state observed during this save pass had already advanced past PR #282: local `main` and `origin/main` were both at `e07e5be5`, a later merge from PR #284. PR #282 remains the merge that landed the config work, at merge commit `2f4d56bb`, with release tag `v6.1.0` published from that commit.

## Sequence of Events

1. Reviewed the user correction that `.env` should not become a kitchen sink and that OpenAI-compatible embedding tuning belongs in `config.toml`.
2. Implemented and reviewed TOML-backed tuning/config surfaces, including example config and docs alignment.
3. Attempted to push local `main` directly and hit GitHub protected-branch rejection.
4. Fixed a pre-push compile failure caused by duplicate TOML parser structs/fields after rebase.
5. Pushed the same head to `codex/axon-config-toml-main`, opened PR #282, waited for required checks, merged it, and pulled local `main`.
6. Observed `auto-tag` publish `v6.1.0`; the first release attempt failed on a transient crates.io HTTP2 download, then a rerun succeeded.
7. During save-session closeout, confirmed current `main` had advanced to `e07e5be5` from PR #284 and documented that live state rather than rewriting it.

## Key Findings

- `crates/axon-core/src/config/parse/toml_config.rs` had duplicate TOML section definitions and duplicate root fields after rebase; removing the duplicates and aligning `watch.lease_secs` fixed `cargo check -p axon-core --locked`.
- Direct `git push origin main` is blocked by GitHub branch protection: required status checks must run before `main` updates.
- PR #282 checks passed before merge: `ci-gate`, `codeql-gate`, `compose-smoke-gate`, `GitGuardian`, and reviewer/status contexts were green or intentionally skipped.
- The first `v6.1.0` release failure was not a code issue. The Linux release job failed downloading `spider_scraper` from crates.io with curl `[16] Error in the HTTP2 framing layer`; rerunning the release workflow succeeded.
- The discovered Claude transcript path was not the active Codex conversation. It contained 15 lines from an older cut-off prompt, so this note relies on current conversation context plus live git/GitHub/beads evidence.

## Technical Decisions

- TOML is the durable home for non-secret OpenAI-compatible embedding and throughput tuning because the user explicitly wanted `.env` kept to secrets/bootstrap/runtime values rather than general configuration.
- A PR was used instead of another direct push because `main` is protected and GitHub rejected the direct update even after local pre-push validation passed.
- The branch push used `--no-verify` only after the exact same head had already passed the local pre-push hook; this avoided repeating the 10-minute local gate before opening the PR.
- The local `main` pointer was realigned to `origin/main` only after confirming the local-only session-log commit was preserved on `codex/save-session-log-20260627` and `origin/codex/save-session-log-20260627`.
- No stale worktrees or branches were removed in the maintenance pass because several worktrees are active or unmerged and ownership was not fully clear.

## Files Changed

| status | path | previous path | purpose | evidence |
|---|---|---|---|---|
| modified | `.env.example` | - | Remove non-secret/tuning OpenAI-compatible config from env examples | PR #282 files list |
| modified | `config.example.toml` | - | Add TOML config examples for tuning surfaces | PR #282 files list |
| created | `config.toml.example` | - | Add compatibility/example TOML config path | PR #282 files list |
| modified | `crates/axon-code-index/src/config.rs` | - | Consume tuning through config instead of env-only surfaces | PR #282 files list |
| modified | `crates/axon-code-index/src/indexer.rs` | - | Apply configured code-index tuning | PR #282 files list |
| modified | `crates/axon-core/src/config/parse.rs` | - | Wire config parse changes | PR #282 files list |
| modified | `crates/axon-core/src/config/parse/build_config/config_literal.rs` | - | Align generated config literal handling | PR #282 files list |
| modified | `crates/axon-core/src/config/parse/env_registry/advanced.rs` | - | Remove advanced env keys superseded by TOML tuning | PR #282 files list |
| modified | `crates/axon-core/src/config/parse/env_registry_tests.rs` | - | Cover env/TOML precedence and migration behavior | PR #282 files list |
| modified | `crates/axon-core/src/config/parse/toml_config.rs` | - | Add TOML sections and resolve duplicate parser definitions | PR #282 files list plus pre-push fix |
| modified | `crates/axon-core/src/config/parse/toml_config_tests.rs` | - | Add TOML parsing tests | PR #282 files list |
| modified | `crates/axon-core/src/config/parse/tuning.rs` | - | Add tuning application model | PR #282 files list |
| modified | `crates/axon-jobs/src/watch.rs` | - | Apply watch tuning | PR #282 files list |
| modified | `crates/axon-jobs/src/watch/validation.rs` | - | Validate watch tuning | PR #282 files list |
| modified | `crates/axon-jobs/src/workers/watch_scheduler.rs` | - | Apply scheduler tuning | PR #282 files list |
| modified | `crates/axon-mcp/src/server/tasks.rs` | - | Use configured MCP/embed task limits | PR #282 files list |
| modified | `crates/axon-services/src/endpoints.rs` | - | Apply endpoint tuning | PR #282 files list |
| modified | `crates/axon-services/src/endpoints/probe.rs` | - | Apply probe tuning | PR #282 files list |
| modified | `crates/axon-services/src/endpoints/verify.rs` | - | Apply verification tuning | PR #282 files list |
| modified | `crates/axon-vector/src/ops/input.rs` | - | Apply input batching/tuning limits | PR #282 files list |
| modified | `crates/axon-vector/src/ops/tei/qdrant_store.rs` | - | Apply Qdrant tuning | PR #282 files list |
| modified | `crates/axon-vector/src/ops/tei/qdrant_store/payload_indexes.rs` | - | Apply payload index tuning | PR #282 files list |
| modified | `crates/axon-vector/src/ops/tei/qdrant_store/upsert.rs` | - | Apply upsert tuning | PR #282 files list |
| modified | `docs/guides/configuration.md` | - | Document current config behavior | PR #282 files list |
| created | `docs/sessions/2026-06-26-env-config-drift-alignment.md` | - | Save prior session log included in PR #282 | PR #282 files list |
| created | `docs/superpowers/plans/2026-06-26-axon-env-config-drift-alignment.md` | - | Save implementation plan for the config drift work | PR #282 files list |
| created | `docs/sessions/2026-06-27-config-toml-main-push.md` | - | Save this session artifact | This file |

## Beads Activity

| bead | title | action(s) | final status | why it mattered |
|---|---|---|---|---|
| `axon_rust-pvl6e.23` | Resolve ignored TOML migration-section drift | Observed as already closed during save pass | closed | This directly tracks the TOML migration/config drift fixed by the session; close reason says embed/OpenAI-compatible tuning keys are accepted in TOML, applied to `Config` with env precedence, documented in `config.example.toml`, and covered by parse/build tests. |
| `axon_rust-pvl6e` | Freshness schedules for embedding-producing operations | Observed parent epic state | open | Parent epic remains open with follow-up freshness surface work such as REST/MCP freshness controls, web/palette UI, lifecycle controls, scrape job family, and content-hash skip-before-embed. |

No new bead was created or closed during the save-session pass. The directly relevant bead was already closed before this request.

## Repository Maintenance

### Plans

Checked `docs/plans` and `docs/plans/complete`. Many older plans remain in `docs/plans/`, including `docs/plans/env-var-fatigue-reduction.md`, but the save pass did not prove they are complete. No plan files were moved. This was left as a no-op because the skill requires moving only clearly completed plans.

### Beads

Ran `bd show axon_rust-pvl6e.23 --json` and listed related `axon_rust-pvl6e` beads. The directly relevant TOML drift bead was already closed with an explicit verification reason. Parent and follow-up freshness beads remain open where work is still outstanding.

### Worktrees and branches

Inspected `git worktree list --porcelain`, local branches, and remote branches. No cleanup was performed because active worktrees remain under `.claude/worktrees/` and `.worktrees/`, and several branches are behind or unmerged but not proven obsolete. The temporary PR branch `codex/axon-config-toml-main` was removed remotely by the PR merge; `git ls-remote --heads origin codex/axon-config-toml-main` returned no output during the earlier push closeout.

### Stale docs

The session's implementation already updated `docs/guides/configuration.md`, `.env.example`, `config.example.toml`, and `config.toml.example` in PR #282. The save pass did not identify another stale doc that could be updated safely without broader review.

### Transparency

The optional Codex transcript scan over `~/.codex` was interrupted because it was slow and the required Claude transcript path had already been found. The found Claude transcript was not useful for this session because it was an old 15-line cut-off prompt.

## Tools and Skills Used

- **Skill: `vibin:save-to-md`.** Used for the session-documentation workflow, maintenance pass, and path-limited commit/push contract.
- **MCP: `mcp__lumen.semantic_search`.** Used first for code/repo discovery per repo instruction; it found existing session-note examples.
- **Shell and git.** Used to inspect branch state, worktrees, commits, tags, PR branches, and to push/merge/sync.
- **GitHub CLI.** Used to create and merge PR #282, inspect checks, watch workflows, rerun the release workflow, and inspect the published release.
- **Cargo and local hooks.** Used through pre-push validation and targeted `cargo check -p axon-core --locked`.
- **Beads CLI.** Used to inspect `axon_rust-pvl6e.23` and related parent/follow-up beads.
- **File editing.** Used to create the session artifact only; no code edits were made during the save pass.

## Commands Executed

| command | result |
|---|---|
| `cargo check -p axon-core --locked` | Passed after removing duplicate TOML parser definitions. |
| `git diff --check` | Passed before committing the duplicate-parser fix. |
| `git push origin main` | Local pre-push validation passed, then GitHub rejected the direct push because `main` is protected. |
| `git push --no-verify origin HEAD:refs/heads/codex/axon-config-toml-main` | Pushed the PR branch after the same head had already passed local pre-push. |
| `gh pr create --base main --head codex/axon-config-toml-main ...` | Created PR #282. |
| `gh pr checks 282 --watch --interval 30` | PR checks completed successfully. |
| `gh pr merge 282 --merge --delete-branch` | Merged PR #282 and deleted the temporary branch. |
| `git pull --ff-only origin main` | Fast-forwarded local `main` to PR #282 merge commit at that time. |
| `gh run watch 28280975992 --exit-status --interval 30` | First release attempt failed, rerun attempt succeeded. |
| `gh api /repos/jmagar/axon/actions/jobs/83796549199/logs` | Retrieved the Linux release failure log before the whole failed run completed. |
| `git branch -f main origin/main` | Realigned local `main` after confirming the local-only session-log commit was preserved on its own branch. |
| `bd show axon_rust-pvl6e.23 --json` | Confirmed the relevant TOML drift bead was closed with verification details. |

## Errors Encountered

- **Pre-push compile failure.** The first direct push attempt failed locally because `toml_config.rs` had duplicate section structs/root fields and a `watch.lease_secs` type mismatch. Fixed by removing duplicates and changing `lease_secs` to the expected signed integer type.
- **Protected branch rejection.** GitHub rejected direct `main` push with required status checks expected. Resolved by creating PR #282 and merging after checks passed.
- **Accidental staging risk.** The first duplicate-parser fix commit initially picked up staged `.full-review` deletions. The commit was amended so it contained only `crates/axon-core/src/config/parse/toml_config.rs`.
- **Release workflow transient failure.** The first `v6.1.0` Linux release build failed while downloading `spider_scraper` from crates.io with curl `[16] Error in the HTTP2 framing layer`. The run was canceled and rerun; attempt 2 succeeded.
- **Optional transcript scan interruption.** A broad `~/.codex` transcript search was interrupted after hanging. The found Claude transcript was old and not materially useful.

## Behavior Changes (Before/After)

| area | before | after |
|---|---|---|
| OpenAI-compatible embedding tuning | Some non-secret tuning lived in `.env`/env-oriented examples and was at risk of being treated as kitchen-sink runtime state. | Tuning is represented in TOML config examples and parse/apply code, with env precedence where applicable. |
| TOML migration sections | Accepted sections could drift from runtime application, and a rebase introduced duplicate parser definitions. | TOML sections are applied/tested, and duplicate parser definitions were removed. |
| Release state | `main` merge triggered `v6.1.0`; first release attempt failed on transient crates.io transport. | Release attempt 2 succeeded and published `v6.1.0`. |
| Local checkout | Local `main` briefly carried a preserved side-branch session-log commit. | Local `main` was realigned to `origin/main`; the session-log commit remains on `codex/save-session-log-20260627`. |

## Verification Evidence

| command | expected | actual | status |
|---|---|---|---|
| `cargo check -p axon-core --locked` | TOML parser compiles after duplicate cleanup | Passed | pass |
| `git diff --check` | No whitespace errors | Passed | pass |
| `gh pr checks 282 --watch --interval 30` | Required PR checks pass | `ci-gate`, `codeql-gate`, `compose-smoke-gate`, GitGuardian and review/status contexts passed or were intentionally skipped | pass |
| `gh pr view 282 --json state,mergedAt,mergeCommit,url` | PR merged | State `MERGED`, merge commit `2f4d56bb25793ee2fc2987ac776a8691981a8927` | pass |
| `gh run view 28280975992 --json status,conclusion,attempt` | Release rerun successful | Attempt 2, status `completed`, conclusion `success` | pass |
| `gh release view v6.1.0 --json url,tagName,publishedAt` | Release published | `v6.1.0` published at `2026-06-27T07:04:26Z` | pass |
| `git status --short --branch` | Local main clean and tracking origin | `## main...origin/main` | pass |

## Risks and Rollback

- Config behavior changed across multiple crates; rollback is to revert PR #282 or the specific merge commit `2f4d56bb` if the TOML tuning path causes operator regressions.
- The `v6.1.0` tag and release were published. Rolling back release artifacts would require deleting or superseding the GitHub release/tag and publishing a corrected version.
- Several open freshness follow-up beads remain; this session did not complete the parent epic.

## Decisions Not Taken

- Did not force-push or bypass branch protection. Protected `main` was handled through PR #282.
- Did not delete stale-looking worktrees or branches because they were not proven safe to remove.
- Did not move old plan files to `docs/plans/complete/` without direct evidence that each plan was completed.
- Did not create a new bead for the transient release failure because rerunning the release resolved it and no code/config change was needed.

## References

- PR #282: https://github.com/jmagar/axon/pull/282
- Release `v6.1.0`: https://github.com/jmagar/axon/releases/tag/v6.1.0
- Release workflow run: https://github.com/jmagar/axon/actions/runs/28280975992
- Bead `axon_rust-pvl6e.23`: Resolve ignored TOML migration-section drift
- Parent bead `axon_rust-pvl6e`: Freshness schedules for embedding-producing operations

## Open Questions

- Whether older plan files under `docs/plans/` are actually complete is unresolved; no plan was moved without stronger evidence.
- Several worktrees and branches are stale-looking but not proven obsolete. They should be reviewed separately before cleanup.
- The broader Lab/Labby Incus setup work happened earlier in the conversation but is not fully represented by this Axon repository evidence.

## Next Steps

- Continue the open freshness follow-up beads under `axon_rust-pvl6e`, especially REST/MCP freshness management surfaces, web/palette freshness UI, lifecycle controls, dedicated scrape job family, and content-hash skip-before-embed.
- If release flakiness recurs, consider adding a retry/cache hardening bead for crates.io dependency resolution in release workflows.
- Run a separate branch/worktree cleanup pass if desired; use merge ancestry and dirty-worktree checks before deleting anything.
