---
date: 2026-06-08 09:48:21 EST
repo: git@github.com:jmagar/axon.git
branch: main
head_at_start: 2fc65940
working_dir: /home/jmagar/workspace/axon
worktree: /home/jmagar/workspace/axon
pull_request: https://github.com/jmagar/axon/pull/185
pull_request_state: merged
merge_commit: 2fc65940c6d594c72b836310a6092eb5c0690696
merged_at: 2026-06-08T06:31:35Z
---

# Ask Retrieval Evaluation and PR #185 Closeout

## User Request

The session began with local llama.cpp/Gemma setup questions and grew into an end-to-end Axon `ask` evaluation pass. The final requested outcome was to merge the completed branch into `main`, then save this session log using `vibin:save-to-md`.

## Session Overview

We brought the local LLM evaluation work from experimentation through implementation, review, CI repair, and merge. The work centered on model-comparison tooling for `axon ask`, capped-model retrieval behavior for local Gemma, synthesis prompt tuning, explain/debug output, and chunk/context improvements.

The merged PR was #185, "Improve ask retrieval evaluation tooling".

## Sequence of Events

1. Investigated `docker-compose.llama.yaml`, llama.cpp runtime wiring, Gemma 4 E4B Q4 feasibility on 12 GB VRAM, and expected context limits.
2. Confirmed Axon LLM action configuration and the distinction between the existing `cli-api.tootie.tv` provider and local llama.cpp.
3. Ran and refined an `axon ask` benchmark plan using the current model, `gemini-3.5-flash-low`, local Gemma 4 E4B Q4, then additional `gpt-5.4-mini` and `gemini-3.1-flash-lite` profiles.
4. Created Qdrant-derived benchmark questions with full answer keys for grading.
5. Built orchestration around `scripts/run-ask-model-comparison.sh`, including JSON output, per-question timing, richer profile/config metadata, and more informative run summaries.
6. Reviewed Gemma results and identified that capped-context models needed stronger retrieval/context discipline.
7. Reviewed the synthesis prompt, implemented prompt improvements, and later checked mostly-correct Gemma answers for general steering fixes.
8. Investigated retrieval and chunking, including visibility into injected chunks/docs and heading-context behavior.
9. Created branch `codex/ask-retrieval-eval-improvements`, pushed it, opened PR #185, and dispatched review agents.
10. Posted a PR comment listing all agent-identified review items, then dispatched five agents to address disjoint issue lists.
11. Integrated the agent outputs, fixed compile/format blockers, and split monolith-sized modules.
12. Repaired CI after discovering an ignored `build/` path swallowed a required Rust module.
13. Repaired env-boundary CI by classifying script-only env vars in the env matrix.
14. Verified all PR checks were green, merged PR #185 into `main`, and fast-forwarded local `main`.

## Key Findings

- The model tier logic must treat `gemini` as large, not every model containing `gemma`. Gemma's local 131k-token slot and 12 GB VRAM envelope need explicit capped-model defaults.
- `ask --explain` is important for tuning retrieval because it exposes the context and scoring path behind an answer.
- The benchmark runner needs to preserve model/provider settings and per-question timings in machine-readable JSON to make repeated comparisons useful.
- Local compiled success can hide a Git tracking issue when a new Rust file lives under a directory name matched by `.gitignore`.
- Env-boundary checks are useful, but script-only knobs need explicit classification to avoid false drift failures.

## Technical Decisions

- Keep the OpenAI-compatible provider shape for `cli-api.tootie.tv` profiles while adding separate model profiles.
- Use JSON run metadata instead of TSV for benchmark outputs.
- Keep capped models from receiving oversized contexts by modeling limits explicitly instead of relying on generic "large model" heuristics.
- Preserve source chunks during heading-context augmentation and truncate only synthetic breadcrumb text when the chunk budget is tight.
- Split large ask context and service query type files into focused submodules rather than adding allowlist exceptions.

## Files Changed

The merged PR changed 148 files. The highest-signal groups were:

- `scripts/run-ask-model-comparison.sh` and `scripts/run-ask-model-comparison.d/*`: runner modularization, model profiles, JSON metadata, parallel execution, self-test support, and timing output.
- `docs/guides/ask-model-comparison-runner.md`: durable documentation for the benchmark setup.
- `reports/llm-ask-comparison-2026-06-07/*`: benchmark question sets, model responses, run JSON, and analysis artifacts.
- `src/vector/ops/commands/ask/synthesis_prompt.rs` and tests: synthesis prompt steering improvements.
- `src/vector/ops/commands/ask/context/*`: explain/context trace handling, capped context assembly, and finalization split.
- `src/vector/ops/input.rs` and `src/vector/ops/input_tests.rs`: heading breadcrumb preservation and chunking tests.
- `src/services/types/service/query.rs` and `src/services/types/service/query/ask_explain.rs`: ask explain response type split.
- `docs/reference/env-matrix.toml` and `scripts/check-env-config-boundary.py`: env-boundary drift fixes.
- `bin/axon` and `plugins/axon/bin/axon`: refreshed built binary artifacts.

## Beads Activity

Repository reads were performed with `bd list --all --sort updated --reverse --limit 100 --json` and `.beads/interactions.jsonl`. No bead was created, updated, or closed during the save-to-md closeout because the PR was already merged and no direct active bead ID was tied to this final documentation step.

## Repository Maintenance

- Plans: inspected `docs/plans/` and found historical active-looking plans plus an existing `docs/plans/complete/` bucket. The relevant current plan was under `docs/superpowers/plans/2026-06-07-ask-eval-and-capped-retrieval.md`, so no `docs/plans/` files were moved.
- Beads: read-only inspection performed; no tracker mutation made.
- Worktrees: `git worktree list --porcelain` shows the main worktree plus older `full-review-*` worktrees under `.worktrees/`. They were not removed during this doc-only save.
- Branches: local and remote `codex/ask-retrieval-eval-improvements` still exist even though PR #185 is merged. They were left intact to avoid cleanup beyond the requested session artifact.
- Stale docs: no additional docs were modified in the maintenance pass. Existing uncommitted changes were observed in `docs/guides/ask-model-comparison-runner.md`, `scripts/run-ask-model-comparison.d/common.sh`, `scripts/run-ask-model-comparison.d/profiles.sh`, `scripts/test-ask-gemma4.sh`, and `reports/llm-ask-comparison-2026-06-07/run-20260608-094559/`.

## Tools and Skills Used

- `vibin:save-to-md` for this session artifact.
- `superpowers:finishing-a-development-branch` for merge readiness.
- Axon CLI and runner scripts for model comparison.
- Git, GitHub CLI, cargo, taplo, and the env-boundary checker.
- PR review toolkit agents and five follow-up remediation agents.
- llama.cpp/Gemma, `cli-api.tootie.tv`, Qdrant-backed retrieval, and Axon `ask --explain` during evaluation work.

## Commands Executed

Representative commands and checks included:

```bash
cargo fmt
cargo fmt --check
cargo test context --message-format short
cargo test input --message-format short
scripts/run-ask-model-comparison.sh --self-test
cargo xtask check-secrets
git diff --check
python3 scripts/check-env-config-boundary.py
cargo test -p axon --test env_config_boundary --message-format short
taplo fmt --check docs/reference/env-matrix.toml
gh pr checks 185
gh pr merge 185 --merge
git checkout main
git pull --ff-only
```

## Errors Encountered

- Initial Gemma runs exposed context-limit risk because model-tier detection treated `gemma` as large.
- Some targeted cargo tests were blocked while agent branches still had compile/format issues in other scopes.
- CI failed once because `src/vector/ops/commands/ask/context/build/finalize.rs` was under a `build/` path ignored by `.gitignore`.
- CI failed once because the env-boundary checker saw `GEMINI_SKILL_INVOCATION` without a matrix classification.
- Local checker then surfaced script-only env knobs `AXON_BASE_ENV_FILE`, `GEMINI_FLASH_MODEL`, and `GEMINI_3_1_FLASH_LITE_MODEL`, which were added as external/test-only matrix entries.

## Behavior Changes

- `axon ask` benchmarking now captures richer provider/model/run configuration and per-question timing.
- The runner supports multiple profiles, including additional `cli-api.tootie.tv` models and local Gemma.
- Capped-model behavior is safer for local Gemma hardware.
- Ask explain/context structures are available through typed service response types.
- Markdown chunking with heading context better preserves original chunk content.
- The synthesis prompt more directly asks for source-grounded, complete explanations, including pool-risk reasoning cases like the ZFS special-vdev question.

## Verification Evidence

- Local `cargo fmt --check` passed.
- Local `cargo test context --message-format short` passed with 134 tests.
- Local `cargo test input --message-format short` passed with 133 tests.
- `scripts/run-ask-model-comparison.sh --self-test` passed.
- `cargo xtask check-secrets` passed.
- `git diff --check` passed.
- `python3 scripts/check-env-config-boundary.py` passed with `env/config boundary ok: 233 classified keys`.
- `cargo test -p axon --test env_config_boundary --message-format short` passed.
- `taplo fmt --check docs/reference/env-matrix.toml` passed.
- GitHub PR #185 checks passed, including `check`, `clippy`, `fmt`, `test`, `release`, `release-smoke`, `mcp-smoke`, `windows-build`, `production-gate`, `rest-api-parity`, `security`, CodeRabbit, and GitGuardian.
- PR #185 merged at `2026-06-08T06:31:35Z` with merge commit `2fc65940c6d594c72b836310a6092eb5c0690696`.

## Risks and Rollback

- Roll back the merged feature with a revert of merge commit `2fc65940` if the ask evaluation changes regress production behavior.
- The post-merge dirty files listed in Repository Maintenance are not part of this session-log commit and should be reviewed separately before staging.
- Old review worktrees and the merged feature branch remain available for forensic comparison.

## Decisions Not Taken

- Did not commit `jakenet` wiring into `docker-compose.llama.yaml`.
- Did not delete local or remote merged branches during the save-to-md pass.
- Did not move historical `docs/plans/` files because none were clearly tied to this completed PR from the maintenance evidence.
- Did not include uncommitted runner/report changes in the session-log commit.

## References

- PR #185: https://github.com/jmagar/axon/pull/185
- Runner guide: `docs/guides/ask-model-comparison-runner.md`
- Implementation plan: `docs/superpowers/plans/2026-06-07-ask-eval-and-capped-retrieval.md`
- Main benchmark reports: `reports/llm-ask-comparison-2026-06-07/`
- GitHub Actions run group: `27118443724` and compose-related run `27118443734`

## Open Questions

- Whether to delete the merged `codex/ask-retrieval-eval-improvements` branch locally and remotely.
- Whether to prune older `full-review-*` worktrees after confirming they contain no unique unmerged work.
- Whether the uncommitted `run-20260608-094559` report should be promoted into tracked benchmark evidence.

## Next Steps

1. Review the remaining dirty runner/report changes independently.
2. Decide whether to clean merged branches and old review worktrees.
3. Run another benchmark pass once Gemma prompt/retrieval tuning is stable enough for comparison.
