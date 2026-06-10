---
date: 2026-06-10 18:49:27 EST
repo: git@github.com:jmagar/axon.git
branch: worktree-feat+unify-file-ingest-engine
head: 8e17bf95
plan: docs/superpowers/plans/2026-06-10-unify-code-file-ingestion-engine.md
working directory: /home/jmagar/workspace/axon/.claude/worktrees/feat+unify-file-ingest-engine
worktree: /home/jmagar/workspace/axon/.claude/worktrees/feat+unify-file-ingest-engine 8e17bf95 [worktree-feat+unify-file-ingest-engine]
pr: "#202 feat(ingest): unify file-ingest engine across all git providers and embed — https://github.com/jmagar/axon/pull/202"
beads: axon_rust-rcbe (created + closed), axon_rust-wavn (closed)
---

# Unify code/file ingestion engine — GH #189

## User Request

Check whether GitHub issues #163 and #189 were implemented or had beads, close #163 (confirmed done via xkv0), then write and execute an implementation plan for #189 (Unify code/file ingestion engine), open a PR, run a full multi-agent review of that PR, and dispatch agents to fix all issues found.

## Session Overview

This session took GitHub issue #189 from an open, untracked state to a reviewed PR with all critical and important issues resolved. The 2026-06-08 plan was found to have four compile-breaking bugs (API changes that landed in PR #192 after the plan was written); a corrected plan was written at `2026-06-10-unify-code-file-ingestion-engine.md`. A subagent executed the plan in a fresh worktree, opening PR #202. A five-dimensional parallel review found one critical data-loss bug (point-ID collision) and 14 additional issues; three parallel agents addressed all 15+ findings in the same session.

## Sequence of Events

1. **Triaged GH #163 and #189.** Confirmed #163 was implemented in the xkv0 epic and closed it via `gh issue close`. Confirmed #189 had no bead and no code implementation.
2. **Invoked `superpowers:writing-plans` for #189.** Reviewed the 2026-06-08 plan and found four compile-breaking bugs: `CodeChunk.symbol_name`/`symbol_kind` fields (replaced by `symbol: Option<Symbol>` in PR #192), `chunk.symbol_kind.is_some()` predicate, `chunk.symbol_name` field access instead of method, and `GitPayload.content_kind: "file"` string instead of `ContentKind::File`.
3. **Wrote corrected plan** at `docs/superpowers/plans/2026-06-10-unify-code-file-ingestion-engine.md`. Verified against live code: `Symbol`/`SymbolKind` public exports, `CHUNK_OVERLAP` constant, `code_symbol_extraction_status` visibility, `GitLabProject` struct fields.
4. **Plan review found two more bugs.** `GitLabProject { id: 1, ... }` — field doesn't exist; and a Task 4 `spawn_blocking` binding that tried to destructure a 3-tuple as a 2-tuple. Both fixed in the plan before execution.
5. **Created worktree** via `EnterWorktree` (`feat/unify-file-ingest-engine`). Dispatched a subagent with `superpowers:executing-plans` to implement all 8 tasks. Subagent committed 6 times, opened PR #202 with 2704 tests passing.
6. **Ran `/pr-review-toolkit:review-pr`** — five parallel agents (code-reviewer, test-analyzer, silent-failure-hunter, comment-analyzer, type-design-analyzer). Found 1 critical, 5 important, 5 suggestion-level, and 5 test gap issues.
7. **Dispatched three parallel fix agents** partitioned by file ownership. All 15+ issues resolved in one round with zero git conflicts.
8. **Pushed** final branch state and closed `axon_rust-wavn` bead.

## Key Findings

- **Point-ID collision (data loss):** GitLab, generic Git, and local-embed paths emitted one `PreparedDoc` per `CodeChunk` with `idx=0` always. For prose-fallback chunks on newline-free files, all chunks got `#L1-L1` → identical UUID v5 → only the last chunk survived upsert. Fixed by appending `#{idx}` to URLs via `enumerate()`. `src/ingest/gitlab/files.rs:152`, `src/ingest/generic_git.rs:262`, `src/vector/ops/tei/prepare.rs:339`.
- **`break` on directory iterator error** in `collect_files` abandoned all remaining entries in a directory. Should be `continue`. `src/vector/ops/file_ingest.rs:68`.
- **`chunking_method` false positives:** `supports_tree_sitter_chunking(ext)` returned `true` for `.rs` even when tree-sitter fell back to prose. Prose chunks were labeled `"tree_sitter"` in Qdrant. Fixed by dropping the grammar-check branch, relying only on `chunk.symbol.is_some()`. `src/vector/ops/file_ingest.rs:120-126`.
- **Local-embed `domain` always `"unknown"`:** `Url::parse` always fails for local paths — dead code. Fixed to derive domain from parent directory name. `src/vector/ops/tei/prepare.rs:319-322`.
- **`ingest/CLAUDE.md` canonical pattern** still showed old `chunk_code` API after the PR. Would cause any new ingest source author to bypass the shared engine.

## Technical Decisions

- **Option (b) for collision fix** (append `#{idx}`) over option (a) (switch to GitHub's batching model with `chunk_extra`). The batching model is safer long-term but would require restructuring all three providers; the index suffix is a minimal, targeted fix with no behavioral regression.
- **Drop `supports_tree_sitter_chunking` from `chunking_method`** rather than introducing a `ChunkingMethod` return enum from `chunk_file`. The enum approach is cleaner architecturally but breaks the public API surface of three callers; the conservative symbol-only check removes false positives without touching call sites.
- **Parallel agent dispatch for both plan execution and review fixes**, with file-ownership partitioning to eliminate git merge conflicts. Agents 1, 2, and 3 each owned disjoint file sets.
- **`anyhow::Result` return from `collect_files`** instead of `Box<dyn Error + Send + Sync>`, consistent with the rest of the ingest module and avoiding error-chain flattening at all three call sites.

## Files Changed

| Status | Path | Purpose |
|--------|------|---------|
| created | `src/vector/ops/file_ingest.rs` | Shared walker (`collect_files`) + `chunk_file` adapter + `SelectionPolicy` enum |
| created | `src/vector/ops/file_ingest_tests.rs` | Engine tests (6 total after review fixes) |
| created | `src/ingest/gitlab/embed_tests.rs` | Payload contract test for `gitlab_file_chunk_payload` |
| created | `src/ingest/gitlab/files_tests.rs` | Owner-derivation edge-case tests (new sidecar) |
| modified | `src/vector/ops.rs` | Added `pub mod file_ingest;` |
| modified | `src/ingest/github/files/prepare.rs` | Rewired onto shared engine; removed `expect()` |
| modified | `src/ingest/github/files/prepare_tests.rs` | Minor updates |
| modified | `src/ingest/gitlab/embed.rs` | Added `gitlab_file_chunk_payload()` |
| modified | `src/ingest/gitlab/files.rs` | Shared engine + per-chunk symbol payload + collision fix + test sidecar declaration |
| modified | `src/ingest/generic_git.rs` | `file_doc` → `file_docs` (per-chunk), collision fix, panic message with file path |
| modified | `src/ingest/generic_git_tests.rs` | Added non-UTF-8 and whitespace-only file tests |
| modified | `src/ingest/git_files.rs` | Removed dead `collect_repo_files` |
| modified | `src/vector/ops/tei/prepare.rs` | Local code files: shared engine + per-chunk payload + collision fix + domain fix + error log level |
| modified | `src/vector/ops/tei/prepare_tests.rs` | Updated + new `dir_embed_code_file_gets_symbol_payload` test |
| modified | `src/ingest/CLAUDE.md` | Updated canonical pattern to `chunk_file`; corrected GitLab/generic-Git descriptions |
| modified | `src/vector/CLAUDE.md` | Added shared engine cross-reference section |
| modified | `CHANGELOG.md` | Version 5.9.0 entry |
| modified | `Cargo.toml` | Version bump 5.8.1 → 5.9.0 |
| modified | `apps/web/package.json` | Version bump |
| modified | `apps/web/openapi/axon.json` | Version bump |
| modified | `README.md` | Version bump |
| modified | `docs/superpowers/plans/2026-06-10-unify-code-file-ingestion-engine.md` | Created (corrected plan); review fixes applied to Task 3 struct literal and Task 4 spawn_blocking binding |

## Beads Activity

| Bead ID | Title | Action | Final Status | Notes |
|---------|-------|--------|--------------|-------|
| `axon_rust-rcbe` | Unify code/file ingestion engine (GH #189) | Created, claimed, closed | Closed | Created at session start to track #189 work; closed after PR #202 opened and review fixes pushed |
| `axon_rust-wavn` | GitLab per-chunk symbol metadata (deferred from xkv0) | Closed | Closed | Was P4/open; resolved as part of Phase 2 of the plan (Task 3+4 in PR #202) |

## Repository Maintenance

**Plans:** `docs/superpowers/plans/2026-06-10-unify-code-file-ingestion-engine.md` is now fully executed (all 8 tasks complete, PR #202 open). Not moved — superpowers plans live under `docs/superpowers/plans/`, not `docs/plans/complete/`; no move needed. The older `docs/superpowers/plans/2026-06-08-unify-code-file-ingestion-engine.md` is superseded but left in place as a historical record (the newer plan references it explicitly).

**Beads:** `axon_rust-rcbe` created and closed this session. `axon_rust-wavn` (GitLab symbol gap, P4, open since 2026-06-08) closed — work was delivered by PR #202 Task 3/4.

**Worktrees/branches:** Worktree `feat+unify-file-ingest-engine` at `.claude/worktrees/feat+unify-file-ingest-engine` is active — PR #202 is open and not yet merged; left in place. Main worktree at `/home/jmagar/workspace/axon` is on `main` and clean. No stale worktrees detected.

**Stale docs:** `src/ingest/CLAUDE.md` canonical pattern was stale post-PR (still showed `chunk_code` API) — updated this session. `src/vector/CLAUDE.md` shared-engine section added. No other stale docs identified.

## Tools and Skills Used

- **`superpowers:writing-plans`** — invoked to write the implementation plan; produced `2026-06-10-unify-code-file-ingestion-engine.md`
- **`superpowers:using-git-worktrees` + `EnterWorktree`** — created isolated worktree for plan execution
- **`superpowers:executing-plans` (subagent)** — executed all 8 plan tasks in the worktree; opened PR #202
- **`pr-review-toolkit:review-pr`** — dispatched 5 parallel specialized review agents (code-reviewer, pr-test-analyzer, silent-failure-hunter, comment-analyzer, type-design-analyzer)
- **3 parallel fix agents** — partitioned by file ownership; addressed all 15+ review findings
- **`gh` CLI** — closed GH #163, viewed PR #202
- **`bd` (beads CLI)** — created `axon_rust-rcbe`, closed `axon_rust-rcbe` and `axon_rust-wavn`
- **`cargo check`, `cargo test --lib`** — used after each agent round to verify compile and test health

## Commands Executed

| Command | Result |
|---------|--------|
| `gh issue close 163` | Closed |
| `git diff main...HEAD --name-only` | 21 files changed |
| `bd close axon_rust-wavn` | Closed successfully |
| `git log --oneline -6` | 9 commits on branch (6 feat/refactor + 3 fix/test) |
| `git push` | Pushed to `origin/worktree-feat+unify-file-ingest-engine` |

## Behavior Changes (Before/After)

| Area | Before | After |
|------|--------|-------|
| GitLab file ingest | `chunk_code()` → `Vec<String>`, one `PreparedDoc` per file, no `symbol_*`/`code_line_*` payload | `chunk_file()` → `Vec<CodeChunk>`, one `PreparedDoc` per chunk with full `symbol_*`/`code_line_*`/`code_chunking_method` metadata |
| generic Git / Gitea file ingest | Same prose-only path as GitLab | Same as GitLab above; Gitea inherits via `ingest_git_repository` |
| Local `embed <dir>` code files | `chunk_code()` → `Vec<String>`, single shared payload per file | Per-chunk `PreparedDoc` with `code_file_type`, `code_chunking_method`, `symbol_*` metadata |
| Qdrant point IDs for prose chunks on newline-free files | Collision: all chunks got same UUID, only last survived | Unique: URL includes `#{idx}` suffix |
| `domain` field for locally embedded files | Always `"unknown"` | Parent directory name |
| `chunking_method` label for grammar-supported ext with prose fallback | `"tree_sitter"` (false positive) | `"prose"` (correct) |

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check --bin axon` (after each agent) | `Finished` | Clean, 0 errors | pass |
| `cargo test --lib -- file_ingest` | 6 pass | 6 passed | pass |
| `cargo test --lib -- gitlab generic_git` | All pass | 24 passed | pass |
| `cargo test --lib` (full) | ≥ 2704 pass | 2704 passed, 0 failed | pass |
| `just verify` (monolith + fmt + clippy) | All clean | All clean | pass |

## Risks and Rollback

- **URL format change for non-GitHub chunks:** The `#{idx}` suffix is a new format. Any downstream filter that matches on exact URL strings (e.g., `retrieve` by URL, watch state URLs) will not match pre-PR chunks for GitLab/generic-Git sources until those are re-ingested. Existing Qdrant data is not broken — just the new chunks get the new format going forward.
- **Rollback:** `git revert` the 9 commits on this branch, or close PR #202 without merging. No schema version bump, so no Qdrant migration needed for rollback.

## Decisions Not Taken

- **GitHub batching model for collision fix** (one `PreparedDoc` per file with `chunk_extra`): would fix the root issue more thoroughly but required restructuring all three providers and changing how `prepare_embed_docs` rebuilds `chunk_extra` for non-GitHub callers. Deferred to a future PR.
- **`ChunkingMethod` enum return from `chunk_file`**: architecturally cleaner for `chunking_method` accuracy, but breaks 3 call sites in a public API. The conservative `symbol.is_some()` check achieves the same result (no false positives) with zero API churn.

## References

- GH #189: https://github.com/jmagar/axon/issues/189
- GH #163: https://github.com/jmagar/axon/issues/163 (closed)
- PR #202: https://github.com/jmagar/axon/pull/202
- PR #192 (xkv0 — introduced `chunk_code_chunks`, `CodeChunk.symbol`): merged prior session
- `docs/superpowers/plans/2026-06-10-unify-code-file-ingestion-engine.md`

## Next Steps

- **Merge PR #202** after CI passes. No follow-on schema migration needed.
- **Re-ingest GitLab and generic-Git repos** to populate `symbol_*`/`code_line_*` metadata for previously indexed content; use `axon refresh --filter gitlab` and `axon refresh --filter git`.
- **Follow-on: GitHub batching model** — align GitLab/generic-Git to use one `PreparedDoc` per file with `chunk_extra` (same as GitHub) to eliminate the stale-tail cleanup gap identified in the review (orphaned chunks when file shrinks).
- **Follow-on: `gitlab/files.rs` direct unit tests** for non-UTF-8 and oversized file skip paths — currently only owner-derivation and payload contract are tested for GitLab; the per-file read logic requires extracting it from `embed_files` or adding integration-style tests.
