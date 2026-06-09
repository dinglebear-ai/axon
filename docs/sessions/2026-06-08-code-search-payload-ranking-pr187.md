---
date: 2026-06-08 16:03:11 EST
repo: git@github.com:jmagar/axon.git
branch: codex/axon_rust-xkv0
head: dea92152
plan: /home/jmagar/workspace/axon_rust/docs/plans/2026-05-27-android-phase2-stubbed-modes.md
working directory: /home/jmagar/workspace/axon/.worktrees/codex/axon_rust-xkv0
worktree: /home/jmagar/workspace/axon/.worktrees/codex/axon_rust-xkv0
pr: "#187 Improve GitHub code chunk metadata (https://github.com/jmagar/axon/pull/187)"
---

# Code search payload and ranking PR 187

## User Request

The session began with a request to create a new worktree, dispatch `lavra-work` on the `axon_rust-xkv0` epic, and create a PR. Later requests focused on reviewing that PR, addressing all findings, running a full live smoke, comparing Axon query output against Lumen, and fixing ranking/payload schema issues without hurting regular docs search.

## Session Overview

PR #187 was created and iteratively updated for GitHub code-aware chunk metadata, clean canonical Qdrant payload fields, Lumen-like code result shape, and code-search ranking. The last implementation pass fixed the selected review findings: the stale GitHub file-prep assertion, stale `migrate` payload indexes, and the risk that source-symbol ranking could leak into regular RAG/docs search.

The branch was pushed at `dea92152` after pre-push verification passed: clippy succeeded and `2490` tests passed with `6` skipped.

## Sequence of Events

1. Created and worked in `/home/jmagar/workspace/axon/.worktrees/codex/axon_rust-xkv0` on branch `codex/axon_rust-xkv0`.
2. Implemented GitHub code-aware chunking and opened PR #187, then dispatched review agents and addressed their findings.
3. Ran live Axon smoke against a temporary Qdrant collection for `dtolnay/itoa`, confirmed source-code query output, and compared ranking against Lumen.
4. Explored Lumen's code search behavior and result shape, then added Axon code metadata fields and source-symbol-aware query ranking.
5. Reworked payloads to a clean-break canonical schema with `git_*`, `code_*`, and `symbol_*` fields instead of legacy GitHub-specific duplicates.
6. Reviewed the code and fixed the two selected findings plus the docs-search isolation issue by making code-search adjustment an explicit score-policy option.
7. Committed and pushed the PR update after resolving pre-push clippy/test issues.
8. Saved this session artifact with the `vibin:save-to-md` workflow.

## Key Findings

- Lumen ranks source declarations above docs for code-intent queries by boosting source declarations, demoting tests, merging overlapping chunks, and resorting after adjustments. This informed the Axon query scoring adjustment.
- Axon had one shared reranking path for `query` and `ask`; without a mode switch, code search boosts could demote README/docs chunks in RAG answers. The fix makes `axon query` opt in at `src/vector/ops/commands/query.rs:97`, while `ask` opts out at `src/vector/ops/commands/ask/context/retrieval.rs:63`.
- The code-search adjustment is gated in the shared scorer at `src/vector/ops/commands/retrieval/trace.rs:115` and only computes source-symbol boosts/doc demotions in `code_search_adjustment` at `src/vector/ops/commands/retrieval/trace.rs:233`.
- The stale GitHub file-prep test still expected `chunking_method`; the canonical field is `code_chunking_method`, now asserted at `src/ingest/github/files/prepare_tests.rs:53`.
- `migrate` still created indexes for `gh_file_language` and `chunking_method`; destination collections now index canonical code fields at `src/services/migrate.rs:238`.
- `plugins/axon/bin/axon` was dirty before the final session-note work and remained unrelated and unstaged.

## Technical Decisions

- Code result shape was moved closer to Lumen by exposing `file_path`, `symbol`, `kind`, line range, file type, language, provider, content kind, chunking method, and symbol extraction status in `QueryHit`.
- Payload schema took a clean-break approach: GitHub-specific `gh_*` duplicates were removed in favor of canonical `git_*`, `code_*`, and `symbol_*` fields.
- Code ranking is policy-driven, not globally inferred. `axon query` enables `apply_code_search_adjustment`; `ask` disables it so docs/RAG behavior remains docs-friendly.
- Migration index setup follows the new schema and indexes canonical code fields for migrated named-vector collections.
- The session note commit is path-limited to this file per the `save-to-md` contract.

## Files Changed

| status | path | previous path | purpose | evidence |
|---|---|---|---|---|
| modified | `docs/guides/ingest/github.md` | - | Document canonical GitHub ingest payload behavior | `git diff-tree --no-commit-id --name-status -r HEAD` |
| modified | `docs/reference/qdrant-payload-schema.md` | - | Document clean Qdrant payload schema | same |
| modified | `src/ingest/CLAUDE.md` | - | Update ingest-local guidance for canonical payload fields | same |
| modified | `src/ingest/generic_git.rs` | - | Emit canonical code metadata for generic git files | same |
| modified | `src/ingest/git_payload.rs` | - | Add shared canonical git/code/symbol payload fields | same |
| modified | `src/ingest/gitea/embed.rs` | - | Compile with expanded `GitPayload` defaults | same |
| modified | `src/ingest/github/files/prepare.rs` | - | Pass canonical line/chunking fields from GitHub file chunks | same |
| modified | `src/ingest/github/files/prepare_tests.rs` | - | Assert `code_chunking_method` contract | same |
| modified | `src/ingest/github/meta.rs` | - | Emit canonical GitHub payload through shared builder | same |
| modified | `src/ingest/github/meta_tests.rs` | - | Update GitHub payload tests for clean schema | same |
| modified | `src/ingest/gitlab/embed.rs` | - | Classify GitLab file payloads into canonical code fields | same |
| modified | `src/services/migrate.rs` | - | Create canonical code payload indexes during migration | same |
| modified | `src/services/types/service/query.rs` | - | Add optional code result metadata to `QueryHit` | same |
| modified | `src/vector/cache_tests.rs` | - | Update test helpers for new retrieved candidate shape | same |
| modified | `src/vector/ops/commands/ask/context/build/appenders.rs` | - | Update retrieved candidate construction | same |
| modified | `src/vector/ops/commands/ask/context/retrieval.rs` | - | Keep code adjustment disabled for `ask` reranking | same |
| modified | `src/vector/ops/commands/ask/context_tests.rs` | - | Add regression proving `ask` does not use code adjustment | same |
| modified | `src/vector/ops/commands/query.rs` | - | Return code metadata and enable code-search ranking | same |
| modified | `src/vector/ops/commands/query_tests.rs` | - | Cover query score policy and new candidate shape | same |
| modified | `src/vector/ops/commands/retrieval.rs` | - | Carry code metadata and score-policy switch | same |
| modified | `src/vector/ops/commands/retrieval/product_authority_tests.rs` | - | Explicitly disable code adjustment in docs-ranking test | same |
| modified | `src/vector/ops/commands/retrieval/trace.rs` | - | Implement gated code-search score adjustment | same |
| modified | `src/vector/ops/commands/retrieval_tests.rs` | - | Add code-intent ranking regressions | same |
| modified | `src/vector/ops/qdrant/client/delete.rs` | - | Use canonical repo-file cleanup filter | same |
| modified | `src/vector/ops/qdrant/types.rs` | - | Deserialize canonical payload metadata | same |
| modified | `src/vector/ops/qdrant/utils.rs` | - | Bump payload schema version | same |
| modified | `src/vector/ops/qdrant/utils_tests.rs` | - | Update schema/version expectations | same |
| modified | `src/vector/ops/tei/qdrant_store/payload_indexes.rs` | - | Create canonical payload indexes for normal collection init | same |
| modified | `src/vector/ops/tei/qdrant_store/payload_indexes_tests.rs` | - | Update index test expectations | same |
| modified | `tests/mcp_contract_parity.rs` | - | Fill optional `QueryHit` metadata fields in integration test literal | same |
| created | `docs/sessions/2026-06-08-code-search-payload-ranking-pr187.md` | - | Session artifact generated by `vibin:save-to-md` | this file |

## Beads Activity

No bead activity observed for this session. Evidence: `bd list --all --sort updated --reverse --limit 100 --json` returned older closed issues, and `tail -200 .beads/interactions.jsonl` returned `none` because the interactions file was absent in this worktree.

## Repository Maintenance

### Plans

Checked `docs/plans` with `find docs/plans -maxdepth 2 -type f`. No plan was moved. The top-level plan files appeared broad or historical rather than clearly completed by this PR session, and the active plan path from `.claude/current-plan` points at `/home/jmagar/workspace/axon_rust/docs/plans/2026-05-27-android-phase2-stubbed-modes.md`, which is outside this worktree and unrelated to the PR #187 code-search work.

### Beads

Checked recent Beads state and interactions. No directly relevant bead was created, closed, edited, claimed, or assigned during this session, so no tracker mutation was performed.

### Worktrees and branches

Inspected `git worktree list --porcelain`, local branches, remote branches, and merge ancestry. No worktree or branch was removed. Evidence:

- `/home/jmagar/workspace/axon` is `main`.
- `/home/jmagar/workspace/axon/.worktrees/codex/axon_rust-xkv0` is the active PR branch.
- `/home/jmagar/workspace/axon/.worktrees/feat/axon_rust-mzj9-local-embed-ast` is an active feature branch.
- `/tmp/axon-pr187-review` is detached review evidence.
- Merge ancestry checks returned non-zero for the feature branch, current branch, and `origin/pr-187` against `main`, so none were proven safe to delete.

### Stale docs

Docs touched by this implementation were updated in the PR: `docs/guides/ingest/github.md` and `docs/reference/qdrant-payload-schema.md`. A broader stale-doc sweep was not attempted beyond the PR scope; historical session/plan docs may still mention old `gh_*` fields.

### Transparency

No cleanup was hidden or performed implicitly. The only remaining dirty file after the PR push was the pre-existing unrelated `plugins/axon/bin/axon`, confirmed by `git status --short`.

## Tools and Skills Used

- **Skills.** `axon` for project context, `superpowers:receiving-code-review` for review-feedback handling, and `vibin:save-to-md` for this session artifact.
- **Shell commands.** Used `git`, `cargo`, `gh`, `bd`, `find`, `tail`, `nl`, `sed`, and `rg` for repository inspection, implementation verification, PR state, and maintenance checks.
- **File tools.** Used `apply_patch` for manual edits and session artifact creation.
- **MCP tools.** Used Lumen semantic search to inspect Axon code paths and compare code-search behavior. Used GitHub app context earlier in the session for PR-oriented work.
- **Subagents/agents.** PR review was delegated to review agents, including code review, silent failure hunting, type design analysis, and PTA-style review.
- **External CLIs.** Used `cargo`, `cargo clippy`, `cargo test`, pre-commit/pre-push `lefthook`, `gh`, and `bd`.

## Commands Executed

| command | result |
|---|---|
| `cargo test --lib ingest::github::files -- --nocapture` | Passed: 8 tests |
| `cargo test --lib commands::retrieval -- --nocapture` | Passed: 28 tests |
| `cargo test --lib commands::ask::context -- --nocapture` | Passed: 110 tests |
| `cargo test --lib commands::query -- --nocapture` | Passed: 5 tests |
| `cargo test --lib migrate -- --nocapture` | Passed: 14 tests |
| `cargo clippy --all-targets --all-features -- -D warnings` | Passed after fixing a `let_and_return` and an integration-test struct literal |
| `git push origin codex/axon_rust-xkv0` | First attempt failed on clippy; second attempt succeeded |
| pre-push hook | Passed clippy and nextest: `2490` passed, `6` skipped |
| `gh pr view --json number,title,url,headRefName,state` | Confirmed PR #187 open on `codex/axon_rust-xkv0` |
| `git status --short` | After PR push, only `plugins/axon/bin/axon` remained dirty |

## Errors Encountered

- Parallel focused `cargo test` commands initially waited on Cargo package/artifact locks. The jobs were allowed to drain, and the focused suites passed.
- First `git push origin codex/axon_rust-xkv0` failed in pre-push clippy due to `clippy::let_and_return` in `src/ingest/github/meta.rs:125`. The builder expression was returned directly.
- A subsequent all-target clippy run exposed `tests/mcp_contract_parity.rs:244`, where `QueryHit` was constructed without the new optional metadata fields. The test literal was updated with `None` values.
- GitHub reported one existing moderate Dependabot vulnerability on the default branch during push; this was unrelated to PR #187 and was not addressed in this session.

## Behavior Changes (Before/After)

| area | before | after |
|---|---|---|
| GitHub payload schema | Mixed canonical fields with GitHub-specific duplicates and legacy names | Clean canonical `git_*`, `code_*`, and `symbol_*` fields |
| Code file query output | Query hits did not expose Lumen-like code metadata | Query hits include file path, symbol, kind, line range, language, provider, and chunking metadata |
| Code-intent ranking | README/docs chunks could outrank source declarations for source-code lookup | `axon query` can boost source symbols and demote docs/examples/tests for code-intent queries |
| RAG/docs search | Shared scoring risked inheriting code-specific doc demotions | `ask` keeps code adjustment disabled and preserves docs-oriented retrieval |
| Migration indexes | Destination collections indexed old `gh_file_language` and `chunking_method` | Destination collections index canonical code fields |

## Verification Evidence

| command | expected | actual | status |
|---|---|---|---|
| `cargo test --lib ingest::github::files -- --nocapture` | GitHub file-prep contract passes | 8 passed | pass |
| `cargo test --lib commands::retrieval -- --nocapture` | Code ranking and retrieval tests pass | 28 passed | pass |
| `cargo test --lib commands::ask::context -- --nocapture` | Ask context and no-code-adjustment regression pass | 110 passed | pass |
| `cargo test --lib commands::query -- --nocapture` | Query policy/result-shape tests pass | 5 passed | pass |
| `cargo test --lib migrate -- --nocapture` | Migration tests pass | 14 passed | pass |
| `cargo clippy --all-targets --all-features -- -D warnings` | All-target clippy clean | Passed after fixes | pass |
| pre-push hook | Clippy plus full nextest suite pass | `2490` passed, `6` skipped | pass |
| live Axon smoke on `dtolnay/itoa` | Query should rank source symbol above README after ranking fix | `src/lib.rs#L62-L114` ranked first for `itoa Buffer format function` | pass |

## Risks and Rollback

The clean-break payload schema requires reindexing for existing collections to gain the new canonical fields. Collections with old payloads may not support new code facets until refreshed or reingested.

Rollback path: revert PR #187 commits on `codex/axon_rust-xkv0`, or specifically revert `dea92152` to remove the canonical payload/ranking update. For local runtime data, reingest affected Git repositories after deciding on the final schema.

## Decisions Not Taken

- Did not enable code-search adjustment for `ask`, because RAG should preserve docs-heavy context even when the query includes code-ish words.
- Did not keep backward-compatible `gh_*` payload duplicates, because the user explicitly requested a clean break.
- Did not delete stale worktrees or branches, because merge ancestry did not prove them safe and one detached worktree still represented PR-review evidence.
- Did not stage or commit `plugins/axon/bin/axon`, because it was a pre-existing unrelated dirty binary.

## References

- PR #187: https://github.com/jmagar/axon/pull/187
- Lumen code search behavior observed from `/home/jmagar/workspace/lumen`, especially its source-declaration boost/test demotion approach.
- Axon code-search docs updated in `docs/guides/ingest/github.md`.
- Qdrant payload schema docs updated in `docs/reference/qdrant-payload-schema.md`.

## Open Questions

- Historical docs and session logs may still mention old `gh_*` payload fields; they were not exhaustively rewritten.
- Existing Qdrant collections need reingest or migration planning to fully reflect the clean schema.

## Next Steps

- Watch PR #187 CI after the pushed update.
- Decide whether to run a broader stale-doc cleanup for old `gh_*` references outside the primary schema docs.
- Reingest important GitHub repositories after merge so code files carry the canonical payload fields.
- Leave `plugins/axon/bin/axon` untouched unless a separate task decides whether that binary pointer should be restored or updated.
