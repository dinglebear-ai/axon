---
date: 2026-06-14 01:48:28 EDT
repo: git@github.com:jmagar/axon.git
branch: codex/codex-app-server-llm-backend
head: 2f4aaa84
plan: docs/superpowers/plans/2026-06-14-codex-app-server-llm-backend.md
working directory: /home/jmagar/workspace/axon/.worktrees/codex-app-server-llm-backend
worktree: /home/jmagar/workspace/axon/.worktrees/codex-app-server-llm-backend
pr: "#213 feat(llm): add codex app-server backend https://github.com/jmagar/axon/pull/213"
beads: axon_rust-it6j, axon_rust-it6j.1, axon_rust-it6j.2, axon_rust-it6j.3, axon_rust-it6j.4, axon_rust-it6j.5, axon_rust-it6j.6, axon_rust-it6j.7, axon_rust-it6j.8
---

# Codex app-server LLM backend

## User Request

Determine what remained to finish the Codex app-server implementation, review the plan with the requested skills, address all engineering review feedback, then execute the work-it flow through implementation, review, verification, PR, and session capture.

## Session Overview

Implemented `AXON_LLM_BACKEND=codex-app-server` as a real Axon LLM backend and published PR #213. The branch now includes subprocess dispatch, isolated Codex home/auth handling, ask/RAG integration, async job snapshot support, docs/env wiring, review hardening, and focused process-level tests.

## Sequence of Events

1. Created and reviewed the implementation plan with `superpowers:writing-plans`, `lavra:lavra-eng-review`, and `superpowers:writing-skills`.
2. Created bead epic `axon_rust-it6j` and eight child tasks for config, dispatch, env parsing, ask/RAG, docs, verification, subprocess hardening, and snapshot/tuning work.
3. Implemented the backend on branch `codex/codex-app-server-llm-backend`, opened PR #213, and ran local verification.
4. Ran multiple review passes and addressed findings from Lavra review, simplifier passes, PR review toolkit agents, CodeRabbit comments, silent failure hunter, type-design analyzer, and test analyzer.
5. Committed final review fixes as `2f4aaa84 fix(llm): address codex app-server review gaps`, pushed the branch, and closed the bead epic plus children after verification passed.

## Key Findings

- `src/core/llm/codex_app_server.rs` had an existing app-server adapter, but it was not wired into `core::llm` dispatch or config.
- Review found cleanup risks around timeout paths, stderr collection, UTF-8 truncation, and process-level success coverage.
- Review found Codex app-server without an explicit model was valid but needed medium-tier ask/RAG defaults because Codex can use its own configured default model.
- An orphaned test file under `src/core/llm/llm_backend_tests.rs` was not wired by `src/core/llm.rs`; the test was moved to `src/core/llm_backend_tests.rs`.
- `OPENAI_API_KEY` is intentionally ignored by Axon config but may be forwarded only into the isolated Codex child as optional auth fallback.

## Technical Decisions

- Used child-process `codex app-server` over stdio for this slice; desktop socket transport remains deferred.
- Kept Codex runtime isolated with throwaway `CODEX_HOME`, rehomed `HOME` and XDG paths, and no user hooks, MCP servers, apps, or skills.
- Used direct synthesis prompts for Codex because the isolated app-server runtime should not rely on skill loading.
- Added `codex-child-auth` / `child-only` to the env boundary checker instead of registering bare `OPENAI_API_KEY` as Axon runtime config.
- Classified Codex app-server as medium context when no explicit model is configured.

## Files Changed

| status | path | previous path | purpose | evidence |
|---|---|---|---|---|
| modified | `.env.example` | - | Document Codex backend env vars in linter-safe order | commit `2f4aaa84` |
| modified | `CLAUDE.md` | - | Clarify Codex app-server backend and bare `OPENAI_API_KEY` behavior | commit `2f4aaa84` |
| modified | `docs/reference/env-matrix.toml` | - | Align Codex env surfaces and child-only auth classification | commit `2f4aaa84` |
| modified | `scripts/check-env-config-boundary.py` | - | Teach checker child-only Codex auth classification | commit `2f4aaa84` |
| modified | `src/core/llm.rs` | - | Make limiter keys use request-effective model consistently | commit `2f4aaa84` |
| modified | `src/core/llm/codex_app_server.rs` | - | Report stderr collection failures explicitly | commit `2f4aaa84` |
| modified | `src/core/llm/codex_app_server_tests.rs` | - | Add fake app-server success test and stderr failure tests | commit `2f4aaa84` |
| deleted | `src/core/llm/llm_backend_tests.rs` | - | Remove orphaned test module | commit `2f4aaa84` |
| modified | `src/core/llm/types.rs` | - | Preserve blank Codex command for validation and medium-tier Codex defaults | commit `2f4aaa84` |
| modified | `src/core/llm_backend_tests.rs` | - | Add wired limiter-key tests | commit `2f4aaa84` |
| modified | `src/core/config/parse/tuning_tests.rs` | - | Cover unset Codex model tuning | commit `2f4aaa84` |
| modified | `src/services/search/synthesis_tests.rs` | - | Cover unset Codex model research-source preservation | commit `2f4aaa84` |
| modified | `src/vector/ops/commands/ask.rs` | - | Use shared Codex backend validator directly | commit `2f4aaa84` |
| modified | `src/vector/ops/commands/ask/context_tests.rs` | - | Cover unset Codex model high-context detection | commit `2f4aaa84` |

## Beads Activity

| bead | title | action | final status | why |
|---|---|---|---|---|
| `axon_rust-it6j` | Codex app-server LLM backend implementation | created, updated, closed | closed | Epic for the implementation plan and acceptance bar |
| `axon_rust-it6j.1` | Codex backend config shape and model selection | created, closed | closed | Config/model selection implemented and verified |
| `axon_rust-it6j.2` | Activate Codex app-server backend dispatch | created, closed | closed | Dispatch and limiter wiring implemented and verified |
| `axon_rust-it6j.3` | Parse Codex LLM environment settings | created, closed | closed | Env parsing and matrix updates implemented and verified |
| `axon_rust-it6j.4` | Wire Codex into ask/RAG synthesis | created, closed | closed | Ask/RAG backend integration implemented and verified |
| `axon_rust-it6j.5` | Document Codex app-server LLM backend | created, closed | closed | Docs/env examples updated and verified |
| `axon_rust-it6j.6` | Verify Codex app-server backend implementation | created, closed | closed | Local and pre-push gates passed |
| `axon_rust-it6j.7` | Harden Codex app-server subprocess boundary | created, closed | closed | Isolation, timeout, redaction, stderr, and cleanup hardening completed |
| `axon_rust-it6j.8` | Preserve Codex config in async jobs and model tuning | created, closed | closed | Snapshot and tuning behavior implemented and verified |

## Repository Maintenance

Plans: `docs/superpowers/plans/2026-06-14-codex-app-server-llm-backend.md` remains in place because it is part of the PR evidence and not under `docs/plans/`.

Beads: Closed the epic and all eight children after `cargo test`, clippy, env boundary, and pre-push nextest passed.

Worktrees and branches: Observed active worktrees for `main`, `codex/axon-update-release-sync`, `codex/codex-app-server-llm-backend`, and `codex/fix-source-doc-char-boundary`; no worktrees or branches were removed because they are active or unrelated.

Stale docs: Updated `CLAUDE.md`, `.env.example`, and `docs/reference/env-matrix.toml` for the Codex backend and child-only auth behavior. No generated env-matrix script was found.

Transparency: PR checks were still queued/running on GitHub after the final push; local and pre-push verification passed.

## Tools and Skills Used

- Skills: `superpowers:writing-plans`, `lavra:lavra-eng-review`, `superpowers:writing-skills`, `vibin:work-it`, and `vibin:save-to-md`.
- Subagents: Lavra reviewers, code simplifier agents, PR review toolkit agents, silent failure hunter, type-design analyzer, test analyzer, comment analyzer, and code reviewer.
- Shell and Git: cargo, git, gh, bd, rg, Python one-liners for local evidence gathering.
- File tools: `apply_patch` for code and documentation edits.
- GitHub: `gh pr view` for PR status and checks.

## Commands Executed

| command | result |
|---|---|
| `cargo fmt --check && python3 scripts/check-env-config-boundary.py` | passed; env/config boundary reported 249 classified keys |
| `cargo test -q --test env_config_boundary --test compose_env_contract` | passed; 13 compose tests and 1 env boundary test |
| `cargo test codex_app_server --lib -- --nocapture` | passed; 53 Codex-related tests |
| `cargo test limiter_key_distinguishes_codex_command_and_model --all-targets -- --nocapture` | passed; confirmed the test is wired |
| `cargo test codex_backend_without_explicit_model --lib -- --nocapture` | passed; 3 unset-model tuning/source tests |
| `cargo clippy --all-targets -- -D warnings` | passed |
| `cargo test` | passed; 2949 lib tests passed, 6 ignored/skipped, plus integration/doc tests |
| `git push` | passed; pre-push clippy and nextest passed, 2949 tests run and 6 skipped |

## Errors Encountered

- Env boundary initially rejected `codex-child-auth` and `child-only`; fixed by adding a narrow child-only auth classification to the checker.
- A test compile failed because new limiter-key tests referenced `Config` without importing it; fixed by importing `crate::Config`.
- A named `cargo test` command attempted multiple filters in one invocation; rerun with separate filters.
- A spawned panic-based stderr join test printed noisy panic output despite passing; replaced with an aborted pending task.
- Code reviewer found the Codex limiter-key test in an orphaned module; moved it to the wired sibling test file and deleted the orphan file.

## Behavior Changes (Before/After)

| area | before | after |
|---|---|---|
| LLM backend selection | `codex-app-server` was not a complete backend path | `AXON_LLM_BACKEND=codex-app-server` dispatches through `core::llm` |
| Codex process runtime | backend path was not fully exposed or hardened | child app-server runs with isolated home/XDG and bounded cleanup |
| Errors | stderr collection failures could disappear | errors now say when stderr diagnostics are unavailable |
| Ask/RAG defaults | Codex without explicit model could be treated as small context | Codex backend is medium context by default |
| Tests | no process-level successful fake app-server wrapper test | success and timeout wrapper paths are covered |

## Verification Evidence

| command | expected | actual | status |
|---|---|---|---|
| `cargo fmt --check && python3 scripts/check-env-config-boundary.py` | formatting and env matrix pass | passed | pass |
| `cargo test -q --test env_config_boundary --test compose_env_contract` | env/docs contracts pass | passed | pass |
| `cargo test codex_app_server --lib -- --nocapture` | Codex backend/protocol/home/process tests pass | 53 passed | pass |
| `cargo test limiter_key_distinguishes_codex_command_and_model --all-targets -- --nocapture` | one wired test runs | 1 passed | pass |
| `cargo test codex_backend_without_explicit_model --lib -- --nocapture` | unset Codex model defaults pass | 3 passed | pass |
| `cargo clippy --all-targets -- -D warnings` | no warnings | passed | pass |
| `cargo test` | full local suite passes | passed | pass |
| `git push` | branch pushes after pre-push gates | clippy and nextest passed; branch pushed | pass |

## Risks and Rollback

Risk is mostly operational: Codex app-server depends on local Codex CLI auth/model availability and spawns a child process per completion. Rollback is to revert PR #213 or set `AXON_LLM_BACKEND` back to `gemini-headless` or `openai-compat`.

## Decisions Not Taken

- Desktop Unix socket transport was deferred.
- Provider profile UX and stale provider overlay code were left out of scope.
- `AXON_CHAT_CODEX_MODEL` was deferred until there is a concrete Codex chat surface.
- No live Codex auth smoke was rerun in this closeout pass; earlier worker evidence had covered a live summarize smoke, while final verification relied on deterministic local tests.

## References

- PR #213: https://github.com/jmagar/axon/pull/213
- Plan: `docs/superpowers/plans/2026-06-14-codex-app-server-llm-backend.md`
- Bead epic: `axon_rust-it6j`

## Open Questions

- GitHub CI was still running after final push at the time this note was written.
- Future work remains for desktop socket transport and provider profile UX.

## Next Steps

Watch PR #213 CI to completion, then merge when checks and review status are acceptable. If CI fails, start from the failing job log for commit `2f4aaa84` and keep fixes scoped to the PR branch.
