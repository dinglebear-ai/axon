---
date: 2026-06-14 10:16:51 EST
repo: git@github.com:jmagar/axon.git
branch: main
head: caaa491ff64a9ae612eabce3ab32a160446798ad
plan: docs/superpowers/plans/2026-06-14-codex-app-server-llm-backend.md
working directory: /home/jmagar/workspace/axon/.worktrees/codex-app-server-llm-backend
worktree: /home/jmagar/workspace/axon/.worktrees/codex-app-server-llm-backend
pr: "#213 feat(llm): add codex app-server backend https://github.com/jmagar/axon/pull/213"
beads: axon_rust-it6j, axon_rust-it6j.1, axon_rust-it6j.2, axon_rust-it6j.3, axon_rust-it6j.4, axon_rust-it6j.5, axon_rust-it6j.6, axon_rust-it6j.7, axon_rust-it6j.8
---

# Codex app-server LLM backend merge closeout

## User Request

The user asked to commit the Codex app-server LLM backend work, create/update the PR, dispatch PR review toolkit agents across the whole PR, address all review issues, merge it, and then save the session to markdown.

## Session Overview

The Codex app-server LLM backend work was reviewed, corrected, verified, pushed, and merged as PR #213. After the merge, the local checkout was fast-forwarded to current `origin/main` after PR #214 landed, and this session artifact records the implementation, review remediation, merge, and maintenance evidence.

## Sequence of Events

1. Reviewed the completed worker changes for the Codex app-server backend and ran local verification.
2. Committed and pushed the initial review-fix commit, then created/updated PR #213.
3. Dispatched four PR review toolkit agents: code review, silent failure hunting, type design analysis, and test analysis.
4. Addressed review findings: subprocess cleanup, host-only Codex configuration, job snapshot boundaries, source-content budgeting, process env isolation tests, and accidental `axon update` removal.
5. Re-ran focused tests, clippy, full pre-push nextest, resolved a merge conflict with `origin/main`, and pushed the final PR branch.
6. Merged PR #213 through GitHub, pruned the deleted remote branch metadata, then fast-forwarded local `main` to include PR #214 before saving this note.

## Key Findings

- PR #213 was mergeable after conflict resolution and merged at `3b29ec46f2ca786de72dcb26b809196cb7226d45`.
- GitHub CI for PR #213 was green before merge: `fmt`, `check`, `clippy`, `test`, `release`, `mcp-smoke`, `production-gate`, CodeRabbit, and GitGuardian all completed successfully; `claude`, `test-infra`, `live-qdrant`, and Cubic were skipped.
- `origin/main` advanced again after PR #213 because PR #214 merged; local `main` was fast-forwarded to `caaa491ff64a9ae612eabce3ab32a160446798ad` before this note was written.
- The Claude transcript lookup path for this worktree had no matching JSONL file, so transcript metadata is omitted.
- `.claude/current-plan` pointed at `/home/jmagar/workspace/axon_rust/docs/plans/2026-05-27-android-phase2-stubbed-modes.md`, which is outside this repo and unrelated to this session.

## Technical Decisions

- Kept `AXON_LLM_BACKEND=codex-app-server` host-oriented unless a container image explicitly installs Codex; production compose now clears host-only Codex command/home values inside the container.
- Treated `codex_cmd` and `codex_home` as worker-local runtime configuration rather than serialized job snapshot state.
- Made Codex subprocess cleanup bounded and surfaced cleanup failures even after successful model output.
- Kept the restored `axon update` surface from `origin/main` instead of allowing the PR diff to remove it.
- Used a follow-up PR-review commit instead of rewriting the already-pushed branch, then merged `origin/main` to clear GitHub conflict state.

## Files Changed

| status | path | previous path | purpose | evidence |
|---|---|---|---|---|
| modified | `.env.example` | - | Documented supported LLM backend env while avoiding host-only Codex paths in shared env template. | PR #213 file list |
| modified | `CLAUDE.md` | - | Updated project memory around Codex backend configuration. | PR #213 file list |
| modified | `config.example.toml` | - | Added Codex/backend config examples. | PR #213 file list |
| modified | `docker-compose.prod.yaml` | - | Scrubbed host-only Codex env values in production container. | PR #213 file list |
| modified | `docs/guides/configuration.md` | - | Documented Codex app-server backend and host-only settings. | PR #213 file list |
| modified | `docs/reference/env-matrix.md` | - | Added LLM/Codex env classifications. | PR #213 file list |
| modified | `docs/reference/env-matrix.toml` | - | Added machine-readable env classifications. | PR #213 file list |
| created | `docs/sessions/2026-06-14-codex-app-server-llm-backend.md` | - | Captured implementation session notes. | PR #213 file list |
| created | `docs/superpowers/plans/2026-06-14-codex-app-server-llm-backend.md` | - | Captured implementation plan. | PR #213 file list |
| modified | `scripts/check-env-config-boundary.py` | - | Allowed/checked new backend env classifications. | PR #213 file list |
| modified | `src/core/config/parse/build_config/config_literal.rs` | - | Parsed backend settings into config literal flow. | PR #213 file list |
| modified | `src/core/config/parse/build_config/tests/env_required.rs` | - | Added env parsing tests. | PR #213 file list |
| modified | `src/core/config/parse/env_registry/advanced.rs` | - | Registered host-only Codex and update env keys. | PR #213 file list |
| modified | `src/core/config/parse/env_registry/runtime.rs` | - | Registered runtime LLM backend keys. | PR #213 file list |
| modified | `src/core/config/parse/tuning.rs` | - | Routed model-aware tuning through shared LLM model profile. | PR #213 file list |
| modified | `src/core/config/parse/tuning_tests.rs` | - | Covered Codex/GPT context budgeting. | PR #213 file list |
| modified | `src/core/config/types/config.rs` | - | Added Codex backend config fields. | PR #213 file list |
| modified | `src/core/config/types/config_impls.rs` | - | Added defaults/config impl wiring. | PR #213 file list |
| modified | `src/core/llm.rs` | - | Exposed backend dispatch/types. | PR #213 file list |
| modified | `src/core/llm/codex_app_server.rs` | - | Implemented and hardened Codex app-server subprocess backend. | PR #213 file list |
| modified | `src/core/llm/codex_app_server/home.rs` | - | Isolated Codex home/auth handling. | PR #213 file list |
| created | `src/core/llm/codex_app_server/home_tests.rs` | - | Added home/auth hardening tests. | PR #213 file list |
| modified | `src/core/llm/codex_app_server/protocol.rs` | - | Added app-server protocol parsing/sanitization. | PR #213 file list |
| created | `src/core/llm/codex_app_server/protocol_tests.rs` | - | Added protocol parsing tests. | PR #213 file list |
| modified | `src/core/llm/codex_app_server_tests.rs` | - | Added backend process, env, timeout, cleanup, and safety tests. | PR #213 file list |
| modified | `src/core/llm/concurrency.rs` | - | Included Codex backend in shared completion concurrency. | PR #213 file list |
| created | `src/core/llm/headless/common.rs` | - | Shared safe headless command/env helpers. | PR #213 file list |
| created | `src/core/llm/headless/common_tests.rs` | - | Added headless safety helper tests. | PR #213 file list |
| modified | `src/core/llm/headless/gemini.rs` | - | Reused shared headless helper path. | PR #213 file list |
| modified | `src/core/llm/headless/gemini_tests.rs` | - | Adjusted Gemini headless tests. | PR #213 file list |
| modified | `src/core/llm/openai_compat_tests.rs` | - | Added backend regression coverage. | PR #213 file list |
| modified | `src/core/llm/types.rs` | - | Added LLM backend/model profile fields and decisions. | PR #213 file list |
| created | `src/core/llm/types_tests.rs` | - | Added backend/profile tests. | PR #213 file list |
| modified | `src/core/llm_backend_tests.rs` | - | Extended backend dispatch tests. | PR #213 file list |
| modified | `src/jobs/config_snapshot.rs` | - | Preserved non-secret Codex model/concurrency while keeping host command/home local. | PR #213 file list |
| modified | `src/jobs/workers/runners_tests.rs` | - | Added snapshot replay tests. | PR #213 file list |
| modified | `src/services/search/synthesis/source.rs` | - | Enforced per-source budget even for large context models. | PR #213 file list |
| modified | `src/services/search/synthesis_tests.rs` | - | Added source budget and Codex model profile tests. | PR #213 file list |
| modified | `src/vector/ops/commands/ask.rs` | - | Wired ask command through backend-aware synthesis. | PR #213 file list |
| modified | `src/vector/ops/commands/ask/context.rs` | - | Updated ask context construction for backend budget behavior. | PR #213 file list |
| created | `src/vector/ops/commands/ask/context_tests.rs` | - | Added ask context tests. | PR #213 file list |
| modified | `src/vector/ops/commands/ask/synthesis_prompt.rs` | - | Adjusted synthesis prompt contract. | PR #213 file list |
| created | `src/vector/ops/commands/ask/synthesis_prompt_tests.rs` | - | Added synthesis prompt tests. | PR #213 file list |
| modified | `src/vector/ops/commands/ask_tests.rs` | - | Added ask backend tests. | PR #213 file list |
| created | `src/vector/ops/commands/streaming_tests.rs` | - | Added streaming/backend tests. | PR #213 file list |
| modified | `tests/compose_env_contract.rs` | - | Verified production compose clears host-only Codex env. | PR #213 file list |
| created | `docs/sessions/2026-06-14-codex-app-server-llm-backend-merge-closeout.md` | - | Saved this merge closeout session. | This save-to-md step |

## Beads Activity

| bead | title | action(s) | final status | why it mattered |
|---|---|---|---|---|
| `axon_rust-it6j` | Codex app-server LLM backend implementation | Observed closed during save-to-md pass. | closed | Parent tracker for the PR #213 implementation. |
| `axon_rust-it6j.1` | Codex backend config shape and model selection | Observed closed. | closed | Tracked config/model selection task. |
| `axon_rust-it6j.2` | Activate Codex app-server backend dispatch | Observed closed. | closed | Tracked backend dispatch activation. |
| `axon_rust-it6j.3` | Parse Codex LLM environment settings | Observed closed. | closed | Tracked env parsing and config. |
| `axon_rust-it6j.4` | Wire Codex into ask/RAG synthesis | Observed closed. | closed | Tracked ask/RAG integration. |
| `axon_rust-it6j.5` | Document Codex app-server LLM backend | Observed closed. | closed | Tracked documentation updates. |
| `axon_rust-it6j.6` | Verify Codex app-server backend implementation | Observed closed. | closed | Tracked verification. |
| `axon_rust-it6j.7` | Harden Codex app-server subprocess boundary | Observed closed. | closed | Tracked subprocess/auth hardening. |
| `axon_rust-it6j.8` | Preserve Codex config in async jobs and model tuning | Observed closed. | closed | Tracked config snapshot and model-budget behavior. |

All observed `axon_rust-it6j.*` close reasons cited implementation in PR #213 and verification with `cargo fmt --check`, the env boundary check, `cargo clippy --all-targets -- -D warnings`, `cargo test`, and pre-push nextest. No new bead changes were made during the save-to-md pass.

## Repository Maintenance

### Plans

- Checked `docs/plans/` and `docs/plans/complete/`; several older plans remain outside `complete`, but none were clearly part of this Codex backend session.
- Checked `docs/superpowers/plans/`; the relevant plan `docs/superpowers/plans/2026-06-14-codex-app-server-llm-backend.md` is intentionally retained as PR evidence rather than moved.
- `.claude/current-plan` pointed to `/home/jmagar/workspace/axon_rust/docs/plans/2026-05-27-android-phase2-stubbed-modes.md`, outside this repo and unrelated; no action taken.

### Beads

- Ran `bd list --all --json` and a focused Codex/backend search. Relevant `axon_rust-it6j.*` beads were already closed.
- `.beads/interactions.jsonl` was absent in this worktree, so no interaction tail was available.
- No bead was created or edited in this closeout because all directly related implementation beads were already closed.

### Worktrees and branches

- Inspected `git worktree list --porcelain`, `git branch -vv`, remote branches, and merge ancestry.
- Deleted no local worktrees. Several registered worktrees point at active or unclear branches (`claude/intelligent-murdock-36fc6d`, `claude/naughty-hellman-4d12e5`, `claude/priceless-blackburn-7003fa`, `claude/unify-secret-redaction`, `codex/fix-source-doc-char-boundary`); ownership/state was not clear enough for safe cleanup.
- Verified the PR #213 remote feature branch was deleted on GitHub and pruned stale local remote-tracking metadata earlier in the merge step.

### Stale docs

- PR #213 already updated backend and configuration docs. No additional stale-doc edits were made in this save step.
- The local checkout was fast-forwarded from PR #213 merge tip `3b29ec46` to `origin/main` `caaa491f` after PR #214 landed, so this session note was written on current `main`.

## Tools and Skills Used

- **Skills.** Used `vibin:save-to-md` for this artifact. Earlier work in the session used Superpowers planning, review, dispatch, receiving-review, verification, and finishing-branch skills, plus Lavra engineering review and Vibin work-it.
- **Shell and GitHub CLI.** Used `git`, `gh`, `cargo`, `python3`, `bd`, and `rg` for repository state, PR metadata, verification, bead inspection, merge, and push flows.
- **Subagents/agents.** Dispatched PR review toolkit agents for code review, silent failure hunting, type design analysis, and test analysis.
- **File tools.** Used patch-based edits for implementation and this session note. No browser tools were used in this closeout.
- **Issues observed.** No Codex/Claude transcript file was available for this worktree; `bd list` output was large and required focused filtering; GitHub initially reported PR #213 conflicting until `origin/main` was merged into the branch.

## Commands Executed

| command | result |
|---|---|
| `cargo fmt --check` | Passed before commit/push and during merge conflict cleanup. |
| `python3 scripts/check-env-config-boundary.py` | Passed with `env/config boundary ok: 251 classified keys`. |
| `cargo test -q codex_app_server --lib` | Passed, 58 tests. |
| `cargo test -q config_snapshot --lib` | Passed, 16 tests. |
| `cargo test -q build_extraction --lib` | Passed, 3 tests. |
| `cargo test --test compose_env_contract -- --nocapture` | Passed, 13 tests. |
| `cargo clippy --all-targets -- -D warnings` | Passed. |
| `git push origin codex/codex-app-server-llm-backend` | Passed; pre-push ran clippy and nextest. |
| `gh pr view 213 --json ...` | Confirmed PR #213 was mergeable, then later merged. |
| `gh pr merge 213 --merge --delete-branch` | Merged PR #213 and deleted the remote feature branch. |
| `git remote prune origin` | Pruned stale `origin/codex/codex-app-server-llm-backend`. |
| `git pull --ff-only origin main` | Fast-forwarded local `main` to `caaa491f` after PR #214. |

## Errors Encountered

- Pre-push clippy initially failed on nonminimal boolean logic and a default-field reassignment test pattern. Both were patched and rechecked.
- PR review found cleanup, env, snapshot, budgeting, and accidental `axon update` deletion issues. The implementation was updated to address them before merge.
- GitHub reported PR #213 as `CONFLICTING`; merging `origin/main` produced a conflict in `src/core/config/parse/env_registry/advanced.rs`. The resolution kept both `AXON_UPDATE_INSTALL_PATH` and `AXON_UPDATE_FILE_RELEASE_DIR`.
- The first conflict-resolution check exposed a missing `spec(` opener and a duplicate `AXON_UPDATE_INSTALL_PATH` entry in `docs/reference/env-matrix.toml`; both were fixed and rechecked.

## Behavior Changes (Before/After)

| area | before | after |
|---|---|---|
| LLM backend selection | Axon had Gemini headless and OpenAI-compatible completion paths, but no Codex app-server backend. | `AXON_LLM_BACKEND=codex-app-server` routes synthesis through Codex app-server when configured. |
| Codex process boundary | Initial implementation could hide cleanup failures and wait unboundedly. | Cleanup is bounded and cleanup failures surface in the result path. |
| Container configuration | Host Codex command/home values could leak through shared compose env. | Production compose clears host-only Codex values inside the container. |
| Job snapshots | Initial review state serialized host Codex command/home details too broadly. | Worker-local host command/home stays local; non-secret model/concurrency config is preserved. |
| Research synthesis budgeting | Large-context model path could bypass per-source caps. | Full source content is still bounded by model-aware per-source limits. |
| CLI surface | Review found the PR diff had removed `axon update`. | `axon update` was restored from `origin/main`. |

## Verification Evidence

| command | expected | actual | status |
|---|---|---|---|
| `cargo fmt --check` | Rust formatting clean. | Passed. | pass |
| `python3 scripts/check-env-config-boundary.py` | Env/config classifications valid. | Passed: `251 classified keys`. | pass |
| `cargo test -q codex_app_server --lib` | Codex backend tests pass. | Passed: 58 tests. | pass |
| `cargo test -q config_snapshot --lib` | Snapshot tests pass. | Passed: 16 tests. | pass |
| `cargo test -q build_extraction --lib` | Research source extraction tests pass. | Passed: 3 tests. | pass |
| `cargo test --test compose_env_contract -- --nocapture` | Compose env contract passes. | Passed: 13 tests. | pass |
| `cargo clippy --all-targets -- -D warnings` | No clippy warnings. | Passed. | pass |
| pre-push nextest | Full local test gate passes before push. | Passed: 2975 tests, 6 skipped. | pass |
| GitHub PR checks for #213 | Required CI green before merge. | CI, compose smoke, CodeRabbit, GitGuardian, and production gate succeeded; known optional checks skipped. | pass |

## Risks and Rollback

- Codex app-server depends on a host Codex CLI and valid Codex auth; production containers intentionally do not enable it unless Codex is installed and configured there.
- The safest rollback is to revert PR #213 merge commit `3b29ec46f2ca786de72dcb26b809196cb7226d45`, then re-run the env boundary check, clippy, and tests.
- The merged branch is now behind later `main` changes from PR #214, so rollback should be done from current `main` and reviewed for interaction with the new redaction work.

## Decisions Not Taken

- Did not make Codex app-server a production-container default because the published image does not include Codex CLI.
- Did not serialize Codex command/home in job snapshots because those are host-local runtime details.
- Did not remove stale/unclear worktrees during the maintenance pass because several were active, unmerged, or had unclear ownership.
- Did not create new beads because the directly relevant Codex backend bead family was already closed.

## References

- PR #213: https://github.com/jmagar/axon/pull/213
- Merge commit: `3b29ec46f2ca786de72dcb26b809196cb7226d45`
- Current `main` after follow-up pull: `caaa491ff64a9ae612eabce3ab32a160446798ad`
- Plan: `docs/superpowers/plans/2026-06-14-codex-app-server-llm-backend.md`
- Implementation session note: `docs/sessions/2026-06-14-codex-app-server-llm-backend.md`

## Open Questions

- Whether the unrelated worktrees with gone or divergent upstreams should be cleaned up is still unresolved and should be handled only with owner confirmation or clear branch ancestry evidence.
- Live Codex auth/quota smoke testing was not recorded in the closeout evidence; the merged verification is local/unit/integration/CI-backed.

## Next Steps

- Use current `main` for any follow-up work; it is at `caaa491f` in this worktree after PR #214.
- If enabling Codex app-server in a production-like environment, install/configure Codex in that environment explicitly and set host-appropriate `AXON_CODEX_*` settings there.
- For cleanup, separately audit and remove only proven-stale worktrees/branches after checking dirty state, upstream existence, and merge ancestry.
