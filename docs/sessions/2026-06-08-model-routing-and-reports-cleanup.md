---
date: 2026-06-08 11:06:59 EST
repo: git@github.com:jmagar/axon.git
branch: main
head: 9b6e2758
session id: f48f7429-37e4-4b6c-a690-5530abdd50d7
transcript: /home/jmagar/.claude/projects/-home-jmagar-workspace-axon/f48f7429-37e4-4b6c-a690-5530abdd50d7.jsonl
working directory: /home/jmagar/workspace/axon
worktree: /home/jmagar/workspace/axon 9b6e2758 [main]
---

# Model routing, ask-runner cleanup, and reports tracking closeout

## User Request

The session began as a broad Axon review/remediation workflow: dispatch review agents, create GitHub issues, address shared-services and Android findings, merge PRs, build release artifacts, test the desktop palette, configure local Gemma, run the ask comparison script, and fix the failures. The final requests were to fix GPT/Codex model tiering, cap local Gemma context to 128k, stop tracking generated `reports/`, attempt removal from GitHub history, and save the session to markdown.

## Session Overview

The session moved through review orchestration, PR verification and merges, release-artifact work, desktop app testing, local model configuration, ask-runner debugging, model-tier fixes, and generated-report cleanup. The committed repository state now has `reports/` ignored and no tracked `reports/` files at `HEAD`; the model-tier and ask-runner changes remain intentionally unstaged and uncommitted.

## Sequence of Events

1. Full-review work was coordinated across Android, palette, services, CLI, API, and MCP scopes, with artifacts copied into `.full-review/` and GitHub issues requested per review.
2. Shared-services and Android remediation work was routed through isolated worktrees and PRs; PRs #182 and #183 were reviewed, fixed, checked, merged, and cleaned up.
3. Latest CLI, palette `.exe`, and Android APK builds were produced and moved to the requested locations; the desktop palette installer was tested through the Windows app-testing flow.
4. Local Gemma model configuration was inspected, the ask comparison script was run and debugged, and the failure analysis showed `gpt-5.5` / `gpt-5.4-mini` were being treated as Small-tier models.
5. The ask model-tier detector was changed so `gpt-*` models use the Medium GPT/Codex tier, local Gemma 4 26B-A4B was capped to 128k context, and runner/docs defaults were updated.
6. `reports/` was added to `.gitignore`, tracked report files were removed from the index, the cleanup was committed and pushed, and a protected-branch force-push rejection blocked true GitHub history rewriting.

## Key Findings

- `src/core/config/parse/tuning.rs` only classified model names containing `codex` as Medium; plain `gpt-5.5` and `gpt-5.4-mini` fell through to Small.
- Local Gemma 4 26B-A4B is configured for a 128k context profile, but code and scripts still used 300000 context characters before the dirty model-routing changes.
- The ask comparison runner had committed/generated material under `reports/llm-ask-comparison-2026-06-07/`, and newer report runs were also present locally.
- `git ls-files reports` returned `0` after the cleanup commit, and `git status --short --ignored=matching reports` showed `!! reports/`.
- True history removal was attempted with `git filter-branch`, but GitHub rejected the `main` force-push because the branch is protected.

## Technical Decisions

- Treat `gpt-*` model names as Medium-tier retrieval instead of Small because the observed `gpt-5.5` and `gpt-5.4-mini` failures were caused by under-sized retrieval budgets.
- Keep Gemma local at `AXON_ASK_MAX_CONTEXT_CHARS=128000` because the current local 26B-A4B llama.cpp fit profile has a 131072-token context limit.
- Ignore all of `reports/` rather than one dated report subtree because comparison reports are generated artifacts and future runs should not appear as untracked work.
- Use `git rm --cached -r reports` rather than deleting local files, preserving local report artifacts while removing them from Git tracking.
- Preserve the failed history-rewrite attempt as `codex/reports-history-rewrite-attempt` instead of deleting it, because the force-push was blocked and the branch is useful if an admin path later permits rewriting.

## Files Changed

| status | path | previous path | purpose | evidence |
|---|---|---|---|---|
| modified | `.gitignore` | - | Added `reports/` to ignored runtime/generated artifacts. | Commit `9b6e2758`; `git ls-files reports` returned `0`. |
| deleted from index | `reports/llm-ask-comparison-2026-06-07/**` | - | Removed 127 generated report files from Git tracking while leaving local files ignored. | Commit `9b6e2758`; `git diff-tree --no-commit-id --name-only -r HEAD` lists `.gitignore` and report deletions. |
| modified | `src/core/config/parse/tuning.rs` | - | Dirty change: classify `gpt-*` as Medium and set LocalGemma context fallback to 128000. | `git diff --stat` shows dirty file; focused tuning tests passed. |
| modified | `src/core/config/parse/tuning_tests.rs` | - | Dirty change: added regression coverage for `gpt-5.5`, `gpt-5.4-mini`, and Gemma 4 26B-A4B. | `cargo test core::config::parse::tuning::tests --lib` passed 10 tests. |
| modified | `scripts/test-ask-gemma4.sh` | - | Dirty change: switched Gemma smoke helper to OpenAI-compatible local Gemma 4 26B-A4B defaults and 128k context. | `bash -n` and runner self-test passed. |
| modified | `scripts/run-ask-model-comparison.d/common.sh` | - | Dirty change: updated help/default model context text for Gemma 4 26B-A4B. | `git diff --stat` shows dirty file. |
| modified | `scripts/run-ask-model-comparison.d/profiles.sh` | - | Dirty change: updated Gemma defaults, base-env handling, runtime URL translation, and temp SQLite path handling. | `scripts/run-ask-model-comparison.sh --self-test` passed. |
| modified | `scripts/run-ask-model-comparison.d/runner.sh` | - | Dirty change: comparison runner preflight and profile handling improvements from ask-runner debugging. | `git diff --stat` shows dirty file. |
| modified | `scripts/run-ask-model-comparison.d/self-test.sh` | - | Dirty change: fake explain payload now reports 128000 context. | `scripts/run-ask-model-comparison.sh --self-test` passed. |
| modified | `docs/guides/ask-model-comparison-runner.md` | - | Dirty change: documented 26B-A4B defaults, 128k Gemma cap, preflights, and host URL translation. | `git diff --stat` shows dirty file. |
| modified | `docs/guides/configuration.md` | - | Dirty change: documents model-tiered ask context fallback instead of a flat 300000 default. | `git diff --stat` shows dirty file. |
| modified | `docs/guides/context-injection.md` | - | Dirty change: documents model-tiered ask context fallback. | `git diff --stat` shows dirty file. |
| modified | `docs/reference/commands/ask.md` | - | Dirty change: ask command docs now describe tiered context behavior. | `git diff --stat` shows dirty file. |
| created | `docs/sessions/2026-06-08-model-routing-and-reports-cleanup.md` | - | This session artifact. | Created by `vibin:save-to-md` workflow. |

## Beads Activity

| bead | title | action(s) | final status | why it mattered |
|---|---|---|---|---|
| `axon_rust-sm3j` | `feat(chunker): add CodeChunk struct with symbol/kind metadata and header injection` | Observed in injected transcript as closed as superseded. | closed | The transcript records it was superseded by `axon_rust-xkv0`. |
| `axon_rust-sm3j.1` | Add `CodeChunk` struct and `chunk_code_ast()` with direct tree-sitter queries | Observed in injected transcript as closed as superseded. | closed | Duplicate/stale child of the superseded chunker epic. |
| `axon_rust-sm3j.2` | Write `symbol_name`, `symbol_kind`, `chunking_method` to Qdrant payload; bump schema to v5 | Observed in injected transcript as closed as superseded. | closed | Stale schema target after seed URL work took schema v5. |
| `axon_rust-sm3j.3` | Header injection: prepend symbol context to embed texts for code chunks | Observed in injected transcript as closed as superseded. | closed | Replaced by broader `xkv0` plan. |
| `axon_rust-xkv0.7` | Fix code-chunk reindex identity so boundary shifts do not orphan the corpus | Observed in injected transcript as updated. | open in transcript evidence | CEO-review resolution locked Option 1 delete-by-path. |
| `axon_rust-xkv0` | Active code-chunking epic | Observed in injected transcript as commented. | open in transcript evidence | Comments recorded HOLD SCOPE, duplicate epic closure, and reindex-identity decision. |

No bead changes were made during the save-to-md closeout itself.

## Repository Maintenance

### Plans

`docs/plans/` was inspected with `find docs/plans -maxdepth 2 -type f`. No plan files were moved. Several top-level plans remain old or ambiguous, but none were proven completed by this save pass, so moving them would have been speculative.

### Beads

`bd list --all --sort updated --reverse --limit 100 --json` and `.beads/interactions.jsonl` were read. No new bead was created or closed during this closeout. Recent injected-transcript bead activity around `sm3j` / `xkv0` is documented above.

### Worktrees and branches

`git worktree list --porcelain` showed only `/home/jmagar/workspace/axon` attached to `main`. Local branches showed `main` and `codex/reports-history-rewrite-attempt`. The backup branch was not deleted because it preserves the rejected history rewrite attempt and is not proven obsolete.

### Stale docs

Stale ask/model docs were updated as dirty files during the model-routing work. They were not committed by this save-to-md pass because the skill contract requires committing only the generated session artifact.

### Transparency

The current `main` checkout is at `9b6e2758` and matches `origin/main`. The worktree still has 11 dirty model-routing / ask-runner files. `reports/` is ignored and no longer tracked at the current branch tip, but historical GitHub purge remains blocked by protected-branch policy.

## Tools and Skills Used

- **Skills.** `vibin:save-to-md` was used for this closeout; `superpowers:test-driven-development` was used earlier for the model-tier fix; Axon repo guidance was consulted during cleanup.
- **Shell commands.** Git, Cargo, Bash, `rg`, `sed`, `bd`, and `gh` were used for repo state, tests, cleanup, issue/PR inspection, and session evidence.
- **GitHub CLI.** `gh pr view`, `gh pr list`, and issue/PR operations were used across the session for PR checks, merges, and status.
- **Subagents/agents.** Parallel review/remediation agents were dispatched earlier for review scopes, PR review, Android remediation, desktop testing, and docs review; their detailed work happened in isolated worktrees and PRs.
- **Windows/desktop testing tools.** Windows app testing used the desktop testing workflow through the agent-os/Windows automation path.
- **External CLIs.** `git-filter-repo` was attempted but failed locally due a missing Python module; `git filter-branch` completed locally but remote force-push was rejected.

## Commands Executed

| command | result |
|---|---|
| `cargo test gpt_models_get_medium_context_budget --lib` | Failed before the fix, proving `gpt-5.5` resolved to `40000` instead of `400000`. |
| `cargo test gemma_model_gets_local_context_budget --lib` | Failed before the fix, proving Gemma Local resolved to `300000` instead of `128000`. |
| `cargo test core::config::parse::tuning::tests --lib` | Passed after the fix: 10 tests passed. |
| `bash -n scripts/run-ask-model-comparison.sh ... scripts/test-ask-gemma4.sh` | Passed shell syntax checks. |
| `scripts/run-ask-model-comparison.sh --self-test` | Passed. |
| `git rm --cached -r reports` | Removed tracked reports from the index while leaving files locally. |
| `git commit -m "Stop tracking generated reports"` | Created cleanup commit `9b6e2758`. |
| `git push origin main` | Pushed cleanup; pre-push clippy and nextest passed. |
| `git filter-repo --path reports/ --invert-paths --refs main --force` | Failed because executable existed but Python module `git_filter_repo` was missing. |
| `git filter-branch --force --index-filter 'git rm -r --cached --ignore-unmatch reports' --prune-empty main` | Rewrote local `main` history. |
| `git push --force-with-lease origin main` | Rejected by GitHub protected branch policy: force-push to `main` is not allowed. |
| `git reset --hard origin/main` | Restored local `main` after the rejected force-push; backup branch preserved the rewrite attempt. |

## Errors Encountered

- `cargo test tuning_tests --lib` initially selected zero tests because the module path filter was wrong. It was corrected to exact test names and then to `core::config::parse::tuning::tests`.
- `git add .gitignore` initially failed under a read-only `.git` sandbox before the session switched to unrestricted permissions; rerun succeeded later.
- `git-filter-repo` failed with `ModuleNotFoundError: No module named 'git_filter_repo'`; fallback used `git filter-branch`.
- GitHub rejected the force-push after history rewrite with `GH006: Protected branch update failed ... Cannot force-push to this branch`; local state was restored to `origin/main`.

## Behavior Changes (Before/After)

| area | before | after |
|---|---|---|
| GPT/Codex ask tiering | Plain `gpt-5.5` and `gpt-5.4-mini` were treated as Small tier. | Dirty code change treats `gpt-*` and `codex` as Medium tier. |
| Local Gemma ask context | Gemma local fallback/scripts used 300000 context characters. | Dirty code/script/doc change uses 128000 by default. |
| Generated reports | `reports/llm-ask-comparison-2026-06-07/**` was tracked in Git. | Current `main` ignores `reports/` and tracks no `reports/` files at `HEAD`. |
| GitHub history | Report files existed in previous commits. | History purge was attempted locally but could not be pushed because `main` is protected. |

## Verification Evidence

| command | expected | actual | status |
|---|---|---|---|
| `cargo test core::config::parse::tuning::tests --lib` | Model-tier regression tests pass. | 10 passed, 0 failed. | pass |
| `bash -n ...` | Ask-runner shell scripts parse. | No syntax errors. | pass |
| `scripts/run-ask-model-comparison.sh --self-test` | Runner smoke test passes. | `self-test passed`. | pass |
| `git diff --check` | No whitespace errors after model-routing patch. | No output. | pass |
| `git push origin main` for cleanup commit | Push succeeds. | Pushed `f97bef41..9b6e2758`; pre-push clippy and nextest passed. | pass |
| pre-push `cargo nextest` | Test suite passes. | 2476 passed, 6 skipped. | pass |
| `git ls-files reports \| wc -l` | `0`. | `0`. | pass |
| `git status --short --ignored=matching reports` | `reports/` ignored. | `!! reports/`. | pass |
| `git push --force-with-lease origin main` after history rewrite | Publish rewritten history. | Rejected: protected branch disallows force-push. | fail |

## Risks and Rollback

- The dirty model-tier changes are not yet committed. Roll back with targeted `git restore` on the 11 dirty files if the direction changes.
- `codex/reports-history-rewrite-attempt` preserves a locally rewritten branch. Delete it only after deciding no history rewrite will be pursued.
- The current `main` cleanup commit removes reports from the branch tip but not from older GitHub commits. A real purge requires temporary branch-policy change or an admin-mediated rewrite.

## Decisions Not Taken

- Did not delete local `reports/` files because the user asked to stop Git tracking/history exposure, not erase local artifacts.
- Did not delete `codex/reports-history-rewrite-attempt` because it is the only local evidence of the completed history rewrite attempt.
- Did not move old `docs/plans/*.md` files because no plan was proven complete by this closeout.
- Did not commit model-tier changes during this save workflow because `vibin:save-to-md` requires committing only the generated session artifact.

## References

- PR #182 and PR #183 were discussed and merged earlier in the session.
- GitHub issue #176 was the Android full-review remediation target.
- GitHub issue #163 / bead `axon_rust-xkv0` appeared in the injected transcript as the active code-chunking plan.
- Commit `9b6e2758` is the pushed report-tracking cleanup commit.
- Commit `f97bef41` is the immediately previous session-log commit before the cleanup.

## Open Questions

- Should `main` branch protection be temporarily adjusted so `reports/` can be purged from remote history?
- Should `codex/reports-history-rewrite-attempt` be deleted after the history-rewrite decision is made?
- Should the dirty model-tier changes be committed directly to `main` or moved to a PR?
- The injected transcript path refers to a Claude/lavra-design session, not this Codex conversation; this note relies on visible conversation context plus live repo evidence for the Codex-side work.

## Next Steps

1. Decide whether to publish the dirty model-tier / ask-runner changes. The immediate verification commands already passed, but the files remain unstaged.
2. If remote history purge is still required, temporarily allow force-push to `main` or perform the rewrite through an admin-approved path, then push `codex/reports-history-rewrite-attempt` or repeat the rewrite.
3. After committing model-tier changes, rerun the full ask comparison script if live model/API availability is important.
4. Consider opening a small follow-up issue or bead for the protected-branch history-purge blocker so it is not lost.
