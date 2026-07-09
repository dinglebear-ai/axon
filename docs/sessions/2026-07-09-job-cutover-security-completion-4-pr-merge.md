---
date: 2026-07-09 14:45:16 EST
repo: git@github.com:jmagar/axon.git
branch: main
head: c893a7896
working directory: /home/jmagar/workspace/axon
pr:
  - "#396 Finish job cutover impl — https://github.com/jmagar/axon/pull/396 (merged)"
  - "#398 feat(jobs): provider cooling wired into unified job claim eligibility — https://github.com/jmagar/axon/pull/398 (merged)"
  - "#399 feat(security): extend redaction boundary to CLI JSON, artifacts, and file logs — https://github.com/jmagar/axon/pull/399 (merged)"
  - "#400 feat(web): split memory REST surface into per-verb routes, add import/export — https://github.com/jmagar/axon/pull/400 (merged)"
beads: axon_rust-l6amm, axon_rust-69fq1, axon_rust-fo3yx, axon_rust-owx6a, axon_rust-trgxl (all created this session, all left open as genuine follow-up work)
---

## User Request

Diagnose and repair a severely diverged local `main` checkout, clean up stale repo cruft, then resume a previously-killed autonomous implementation of the pipeline-unification job-cutover work — this time "properly, with parallel agents wherever possible." Once the resulting 4 branches were implemented, the user asked to create PRs, run an independent Lavra review panel against each, fix everything the reviews found, and finally "merge them all." Mid-merge, the user separately asked to investigate a local schema-contract-drift false alarm on one branch. The final instruction was to save a full session log.

## Session Overview

Recovered a broken local `main` (diverged via an old `git filter-branch` rewrite), cleaned ~180 stale branches/worktrees, and merged 4 plan docs. Dispatched 4 direct (non-nested) implementation agents in parallel against `docs/pipeline-unification/plans/2026-07-08-*.md`: the primary job-cutover plan (crawl/embed/ingest → unified job store) plus 3 independent security/error/memory plans (provider cooling, redaction boundary, REST memory surface). Opened PRs for all 4, ran a 4-lens (architecture/security/simplicity/performance) Lavra review panel against each — 16 review agents total — which surfaced several real, production-relevant bugs (a query-plan regression, a dead-queue regression in watch-triggered crawls, a missing panic guard, a removed concurrency safety rail, an unenforced `axon:admin` contract, a TOCTOU race). Dispatched fix agents per PR, verified, and merged all 4 to `main` in dependency order. Along the way, diagnosed and fixed two self-inflicted repo-hygiene bugs: a CI failure traced to cross-worktree build-cache contamination (false alarm, no code change needed), and a real accidentally-committed cache-symlink tracking bug (13 stray symlinks committed via a careless `git add -A`) that a CodeRabbit review caught and that required an actual fix. Closed the session with worktree/branch cleanup.

## Sequence of Events

1. Diagnosed local `main`'s divergence from `origin/main` via `git reflog`, found a `filter-branch: rewrite` entry, confirmed via content-diff that no local-only work was orphaned, then reset local `main` to `origin/main`.
2. Investigated and confirmed 3 suspected local-only commits and an untracked Incus deployment script were already present on `origin/main` (via a prior PR) or fully redundant with it; corrected an earlier mistaken belief that they were unrecovered work.
3. Cleaned up 62 stale remote branches and ~101 stale local branches/worktrees (confirmed individually before deletion), closed PR #378 (superseded), merged PR #391 (4 new plan docs) into `main`.
4. Set up 4 pre-warmed worktrees (`.worktrees/finish-job-cutover-impl`, `provider-cooling-impl`, `redaction-boundary-impl`, `rest-memory-surface-impl`) via the `vibin:worktree-setup` skill.
5. Dispatched 4 direct implementation agents in parallel (no nested coordinator-delegates-again pattern, per explicit user correction earlier in the session) against the 4 plan docs; monitored and, when the primary agent paused mid-task to check in, resumed it directly rather than escalating.
6. All 4 agents completed; created draft PRs #396 (primary, was already open from an earlier `vibin:work-it` invocation), #398, #399, #400.
7. Dispatched a 4-lens Lavra review panel (`architecture-strategist`, `security-sentinel`, `code-simplicity-reviewer`, `performance-oracle`) against each of the 4 PRs — 16 agents total, run in parallel.
8. Synthesized findings; dispatched one consolidated fix-and-verify agent per PR to address confirmed findings; verified and pushed fixes for PR #398 (CI schema-snapshot regen, critical claim-query index regression, cooldown-clear bypass gaps), #399 (span-field redaction leak, unbounded-scan gap, 3 more CLI JSON call sites gated, doc reword), and #400 (unenforced `axon:admin` contract on `ReplaceScope` import — the headline security fix — plus a record-count cap and a follow-up bead).
9. Dispatched a second review-and-fix pass against PR #396 after the first implementation agent's own fixes landed; the review panel found 3 real production regressions (a removed crawl-concurrency safety rail, a missing panic guard with no automatic reclaim for the unified job store, and a dead-queue bug where watch-triggered re-crawls silently became permanent no-ops) plus a partial auth-attribution fix; dispatched and verified a consolidated fix.
10. Merged #396 first (load-bearing), then synced and merged #399, #400, and #398 in turn, resolving merge conflicts on 2 of the 4 branches along the way (a migration-number collision plus a relocated function on `provider-cooling-impl`; two independently-added MCP task-locals on `rest-memory-surface-impl`).
11. Diagnosed a CI failure on `provider-cooling-impl` ("failed to create directory `target`, not a directory") as cross-worktree Cargo build-cache contamination from concurrent multi-worktree builds sharing one `target/` dir; verified via an isolated `CARGO_TARGET_DIR` build that no real bug existed.
12. On a live user request, investigated an identical-looking local-only schema-contract-sync failure on `provider-cooling-impl` and proved via the same isolated-build technique that it was the same cache-contamination false alarm, not a code regression — reverted the tainted local regeneration rather than committing over it.
13. Diagnosed a second, genuinely real CI failure on `provider-cooling-impl` ("not a directory" again, this time on GitHub's own runner) as an accidentally-committed `target` symlink (the worktree-setup cache-warming symlink, tracked via an earlier careless `git add -A` during merge conflict resolution); fixed and pushed.
14. A CodeRabbit automated review on the same PR caught that the same careless `git add -A` had tracked 12 more worktree-setup cache symlinks beyond `target`; untracked all of them, hardened `.gitignore` with bare (non-directory) entries, and separately fixed 4 real code-quality findings CodeRabbit raised (a TOCTOU race in `apply_provider_cooling`, a stale comment, a missing test assertion, a sidecar-import-convention gap) plus 2 doc-drift corrections; replied to and resolved all 14 CodeRabbit review threads.
15. Merged PR #398 (the last of the 4), confirmed `main` was fully clean of every stray symlink across all 4 merges.
16. Ran the `vibin:save-to-md` repository maintenance pass: verified all 5 session-created beads remain correctly open as follow-up work, removed the 4 now-merged worktrees/branches, and cleaned 2 unrelated stale branches (`finish-job-cutover`, `session-log/2026-07-09-openwiki-workflow-yaml-fix`) discovered to be fully superseded leftovers from earlier in the same session.

## Key Findings

- `git reflog` showed a `filter-branch: rewrite` entry explaining local `main`'s total divergence from `origin/main` — a history rewrite, not lost work; confirmed via content-diff before resetting.
- `crates/axon-jobs/src/workers/unified.rs` claim query regressed from an indexed search to a full table scan: a partial index scoped `WHERE status = 'waiting'` couldn't be proven by SQLite's planner to cover a 3-value `status IN (...)` predicate; verified before/after with real `EXPLAIN QUERY PLAN` output, fixed by widening the partial index's predicate to match the query exactly.
- `crates/axon-jobs/src/watch/dispatch.rs::enqueue_change_crawl` still wrote to the legacy `axon_crawl_jobs` table after PR #396 deleted the only worker that ever claimed from it — every watch-triggered re-crawl (including the automatic in-process scheduler) would silently become a permanent no-op with no error surfaced anywhere; independently confirmed by 3 of the 4 review agents from different angles.
- The unified worker path (`run_unified_claimed`) had no panic guard after the legacy `panic_guard::run_catching` was deleted, and the periodic watchdog sweep only reclaimed the now-dead legacy tables — a panicking runner would strand a job in `running` forever.
- Pre-cutover, crawl jobs were hard-capped to exactly 1 concurrent execution specifically because all crawls share one Chrome instance; the unified cutover folded crawl into the generic worker-concurrency semaphore (default 8, clamp 64) with no per-kind cap.
- `MemoryImportMode::ReplaceScope` was documented in its own DTO comment as requiring `axon:admin`, but was gated everywhere (REST, service, CLI, MCP) only by ordinary `axon:write` — any write-scoped caller could mass-archive an entire memory scope in one call.
- The file JSON log sink (`~/.axon/logs/axon.log`) was writing secrets unredacted to disk while the console sink was scrubbed — found and fixed as part of the redaction-boundary PR, not merely a review finding.
- `apply_provider_cooling` (`crates/axon-jobs/src/unified/control.rs`) had a genuine TOCTOU race: the Waiting-status check and the `cooldown_until` write ran as two unsynchronized statements with no shared transaction and no status guard on the final UPDATE.
- A bare `git add -A` during merge conflict resolution on `provider-cooling-impl` accidentally tracked 13 worktree-setup cache symlinks (`target`, `.cache`, `apps/android/{.gradle,app/build,build}`, `apps/chrome-extension/dist`, `apps/palette-tauri/{dist,node_modules,src-tauri/target}`, `apps/web/{.next,node_modules,out}`, `scripts/.ruff_cache`, `src/vector/.../build`) — `.gitignore`'s directory-only (`/target/`-style) patterns don't match a symlink of the same name, so they slipped through silently until CI failed on a fresh checkout.

## Technical Decisions

- Dispatched implementation and review agents directly rather than through a coordinator-that-delegates-again, per an explicit earlier correction from the user in this session about opaque nested-delegation patterns.
- Used `isolation`-free `Agent` calls with explicit `cd` instructions into pre-warmed worktrees rather than the `Agent` tool's `isolation: "worktree"` parameter, which would have created a fresh, unwarmed worktree ignoring the prepared one.
- Ran a full 4-lens review panel (architecture, security, simplicity, performance) per PR rather than a single reviewer, on the reasoning that this is exactly the class of orchestration a multi-agent review panel is suited for and the user had already asked for review + fix.
- For each PR's fix agent, explicitly prioritized real production regressions over polish (e.g. told PR #396's fix agent to treat the N+1 bridge-query finding as lowest priority, skip it if time-constrained — it was skipped, correctly, in favor of the 3 real regressions).
- Chose a status-guarded `UPDATE ... WHERE status = 'waiting'` inside an explicit transaction to fix the `apply_provider_cooling` TOCTOU race, rather than a broader locking scheme — minimal, matches the existing codebase's raw-SQL-in-transaction idiom.
- When CodeRabbit's suggested diff for a sidecar-import-convention finding would have broken compilation (it assumed `use super::*` reaches nested-module items as bare identifiers, which it doesn't for a sidecar declared at the crate root), added `use super::*` for convention-compliance without removing the necessary explicit imports, and left a comment explaining why — rather than blindly applying the literal suggestion.
- Diagnosed both `target`-collision CI failures methodically before touching code: rebuilt with an isolated `CARGO_TARGET_DIR` to distinguish "real bug" from "shared-cache contamination" before deciding whether a fix was even needed, avoiding a wrong fix that would have masked a real generated-artifact mismatch.

## Files Changed

Full diffs are on GitHub via the 4 merged PRs; this table summarizes by theme rather than listing all ~160 touched files.

| PR | Status | Files | Diff | Purpose |
|---|---|---|---|---|
| #396 | merged | 101 | +5404/-4919 | Cuts Crawl/Embed/Ingest job execution to the unified job store; retires legacy per-family workers (`crates/axon-jobs/src/workers/runners/*` deleted); fixes reset's legacy-wipe confirmation gap; adds a panic guard + watchdog reclaim for the unified store; ports `watch/dispatch.rs`'s crawl enqueue off the dead legacy table; adds a crawl-specific concurrency limit (`Config::crawl_job_concurrency_limit`) |
| #398 | merged | 19 | +798/-15 | Adds `jobs.cooldown_until` (migration `0022_add_job_cooldown_until.sql`) + covering index; wires bounded provider cooling into claim eligibility (`crates/axon-jobs/src/unified/control.rs`); fixes a TOCTOU race and cooldown-clear gaps found in review; untracks 13 accidentally-committed cache symlinks, hardens `.gitignore` |
| #399 | merged | 15 | +540/-56 | Extends redaction to CLI JSON output (`crates/axon-cli/src/json.rs`, new), artifact metadata, and the file JSON log sink (`crates/axon-core/src/logging/json_format.rs`, new — fixes a real unredacted-secrets-to-disk bug); files `axon_rust-l6amm` for the remaining ~44 ungated CLI call sites |
| #400 | merged | 28 | +1725/-124 | Splits `POST /v1/memory` into per-verb `/v1/memories` REST routes (`crates/axon-web/src/server/handlers/memory_routes.rs`, new) with real `CallerContext` auth; adds memory import/export to CLI/MCP/REST; enforces `axon:admin` for `ReplaceScope` imports across all 3 transports (the review-driven fix) |

Session-log and cleanup files (this closeout):
| Status | Path | Purpose |
|---|---|---|
| deleted | `.worktrees/finish-job-cutover-impl/`, `.worktrees/provider-cooling-impl/`, `.worktrees/redaction-boundary-impl/`, `.worktrees/rest-memory-surface-impl/` | 4 merged-PR worktrees, removed after confirming all 4 PRs `state: MERGED` |
| created | `docs/sessions/2026-07-09-job-cutover-security-completion-4-pr-merge.md` | this session log |

## Beads Activity

| ID | Title | Action | Status | Why it matters |
|---|---|---|---|---|
| `axon_rust-l6amm` | Add CI-enforced redaction call-site lint | created | open | ~44 of ~48 CLI `--json` call sites still bypass the new redaction gate; manual grep isn't a completeness guarantee |
| `axon_rust-69fq1` | Remove deprecated `POST /v1/memory` passthrough route | created | open | Old route kept alive deliberately for compat until known clients (e.g. desktop palette) migrate |
| `axon_rust-fo3yx` | Batch axon-memory import/export queries, stream export response | created | open | Pre-existing N+1 and unbounded-buffer patterns in `axon-memory`, newly reachable over REST with a 10MiB payload via PR #400 |
| `axon_rust-owx6a` | Thread real MCP/panel caller auth into crawl/embed/ingest job submission | created | open | The web-panel dispatch path still passes `None` for caller auth (MCP side was fixed); corrupts audit attribution, not a privilege escalation today |
| `axon_rust-trgxl` | Bound repeated provider-cooling re-application per job | created | open | `apply_provider_cooling`'s per-call 1h clamp doesn't bound *repeated* re-application; latent until a real provider caller is wired |

All 5 beads confirmed open with accurate descriptions during the closeout maintenance pass (`bd show <id>` on each) — none were closeable, since all describe genuine, unstarted follow-up work.

## Repository Maintenance

- **Plans**: `docs/pipeline-unification/plans/` has no `complete/` subdirectory convention (confirmed by directory listing) — the 4 plan docs used this session track completion via inline `[x]`/`> DONE`/`> PARTIALLY DONE` status markers instead, already updated in-session by the implementation/fix agents. No plan-file moves were needed.
- **Beads**: all 5 session-created beads verified open and accurate (see Beads Activity above); no closures — all describe real unstarted work.
- **Worktrees/branches**: removed the 4 now-merged worktrees (`finish-job-cutover-impl`, `provider-cooling-impl`, `redaction-boundary-impl`, `rest-memory-surface-impl`) and their local branches, after confirming via `gh pr view --json state,mergedAt` that all 4 PRs show `MERGED`, and via `git status --short` in each worktree that only worktree-setup cache symlinks (no real uncommitted work) remained. `git branch --merged main` did not recognize these as merged, which is expected — GitHub squash-merges break direct-ancestor detection; PR merge status was used as the authoritative signal instead. Also deleted 2 unrelated stale branches discovered during cleanup: local+remote `finish-job-cutover` (confirmed via `git log finish-job-cutover --not main` to contain only the pre-squash-merge tip of already-merged PR #391, a leftover missed during this session's earlier cleanup pass) and local `session-log/2026-07-09-openwiki-workflow-yaml-fix` (remote already gone, confirmed via `git ls-remote`).
- **Left alone, explicitly**: `marketplace-no-mcp` (intentional long-lived variant per `CLAUDE.md`), `palette-tools-integration` (real in-progress feature work per earlier explicit user instruction), `fix/release-please-v5` (active worktree in a different concurrent session), `pull/377/head`/`pull/378/head`/`pull/380/head` (local `gh`-cached PR-head refs, not managed branches).
- **Stale docs**: none identified as contradicted by this session's changes beyond what the implementation/fix agents already corrected in-line (the `2026-07-04-full-durable-job-cutover.md` and `2026-07-04-phase-3b-security-error-memory-completion.md` plan docs, both updated with accurate closeout evidence during the session).

## Tools and Skills Used

- **Agent tool (parallel dispatch)**: 4 direct implementation agents (primary job-cutover, provider-cooling, redaction-boundary, rest-memory-surface); 16 Lavra review-panel agents (4 lenses × 4 PRs, run in 2 waves since PR #396's own review followed its implementation completing); 3 consolidated fix-and-verify agents (PR #398, #399, #400) plus 1 more for PR #396's second review wave. All ran successfully; no agent failures, though one (the resumed primary implementation agent) paused mid-task to ask whether to continue autonomously — resumed directly per the user's standing "resume properly" instruction rather than escalating.
- **Bash tool**: git operations (diagnosis, merges, conflict resolution, branch/worktree cleanup), `cargo build`/`test`/`clippy`/`fmt` (isolated `CARGO_TARGET_DIR` used repeatedly to rule out shared-cache contamination), `gh` CLI (PR creation, checks polling, merging, review-comment replies via `gh api`), `bd` (bead creation/inspection).
- **GitHub GraphQL API (via `gh api graphql`)**: resolving 14 CodeRabbit review threads programmatically after posting per-comment replies.
- **`vibin:worktree-setup` skill**: created and warmed the 4 implementation worktrees at session start.
- **`vibin:save-to-md` skill**: this closeout — repository maintenance pass + session log.
- **Lavra review subagents** (`lavra:review:architecture-strategist`, `lavra:review:security-sentinel`, `lavra:review:code-simplicity-reviewer`, `lavra:review:performance-oracle`): the core review mechanism for all 4 PRs; no issues, all completed with real, cited findings.
- **CodeRabbit (external, automated)**: posted 15 review comments on PR #398 after the symlink-fix commit; hit its own usage/rate limit early in the session on a different PR with zero actionable output (noted and ignored, no action needed) but completed normally on the later pass.
- No browser automation, no MCP tool servers beyond the above, no issues with tool availability throughout.

## Commands Executed

| Command | Result |
|---|---|
| `git reflog` | Revealed the `filter-branch: rewrite` entry explaining local `main`'s divergence |
| `EXPLAIN QUERY PLAN` (via a throwaway SQLite DB seeded from the real migration SQL) | Proved the claim-query index regression before and after the fix, on PR #398 |
| `CARGO_TARGET_DIR=/tmp/axon-clean-target cargo check --workspace` | Distinguished real compile errors from shared-cache contamination twice (schema-contract-sync false alarm, and a genuine post-conflict-resolution compile error) |
| `git cat-file -t HEAD:target` / `origin/main:<path>` | Confirmed the stray symlink's blob type and exact blast radius (isolated to one branch, never reached `main`) |
| `gh pr merge <n> --squash --delete-branch` (×4) | Merged all 4 PRs to `main` in dependency order |
| `gh api graphql -f query='mutation { resolveReviewThread(...) }'` (×10) | Resolved CodeRabbit review threads after posting replies |
| `git worktree remove --force` / `git branch -D` (×6) | Closeout cleanup of merged worktrees/branches |

## Errors Encountered

- **Local `main` diverged from `origin/main`** — root cause: an old `git filter-branch` history rewrite. Fixed via `git reset --hard origin/main` after rigorously confirming no local-only work would be lost (content-diffed 3 suspected-orphaned commits and an untracked deployment script against `origin/main`; all were already present or fully redundant).
- **Claim-query full-table-scan regression on PR #398** — root cause: a partial index scoped to a single status value couldn't cover a 3-value `IN (...)` predicate. Fixed by widening the index's `WHERE` clause to match the query exactly; verified via real `EXPLAIN QUERY PLAN` before/after.
- **`schema-contract-sync` CI failure, twice, on `provider-cooling-impl`** — first occurrence: root cause was cross-worktree Cargo build-cache contamination from this session's own heavy concurrent multi-worktree activity sharing one `target/` dir; verified via isolated `CARGO_TARGET_DIR` build (no code fix needed, reverted the tainted local regeneration). Second occurrence (a live CI runner failure, "not a directory"): root cause was a genuinely accidentally-committed `target` symlink from an earlier careless `git add -A`; fixed by untracking it and hardening `.gitignore`.
- **13 stray worktree-setup cache symlinks accidentally committed** — root cause: the same careless bare `git add -A` (used once during a merge-conflict `rustfmt` auto-fix recommit) staged every untracked symlink in the worktree, not just `target`; `.gitignore`'s directory-only patterns don't match same-named symlinks. Caught by CodeRabbit's automated review, not by the earlier manual `target`-only fix. Fixed by untracking all 13 and adding bare (non-directory) `.gitignore` entries for each.
- **Bare `git commit --no-edit` after conflict resolution failed with "unmerged files" on `rest-memory-surface-impl`** — root cause: resolved conflicts in the working tree but hadn't `git add`ed them yet. Fixed by staging the resolved files before retrying the commit.

## Behavior Changes (Before/After)

| Area | Before | After |
|---|---|---|
| Crawl/embed/ingest job execution | Per-family legacy worker lanes (`axon_crawl_jobs`/`axon_embed_jobs`/`axon_ingest_jobs` tables) | Unified job store (`jobs` table), matching the already-shipped Extract cutover pattern |
| Crawl concurrency | Hard-capped to 1 concurrent job (legacy lane) | Independently bounded via `Config::crawl_job_concurrency_limit` (default 1), separate from the general worker-concurrency semaphore |
| Unified worker panics | No panic guard; a panicking runner stranded its job in `running` forever | `run_unified_claimed` catches panics and marks the job `failed`; watchdog also now sweeps the unified store, not just the dead legacy tables |
| Watch-triggered re-crawls | Enqueued into the legacy `axon_crawl_jobs` table (silently dead after PR #396's own worker-lane retirement) | Enqueue via the unified store; a regression test proves the resulting job is actually claimable |
| `axon reset` on non-empty legacy tables | Silently wiped them | Requires an explicit `--confirm-legacy-wipe` CLI flag (never wired into TOML/env) |
| Memory `ReplaceScope` import | Gated only by ordinary `axon:write` despite a documented `axon:admin` contract | `axon:admin` enforced across REST, service, CLI, and MCP transports |
| File JSON logs (`~/.axon/logs/axon.log`) | Secrets written unredacted (console sink was scrubbed, file sink wasn't) | Both sinks share the same redaction gate |
| `apply_provider_cooling` | Status check and cooldown write were two unsynchronized statements (TOCTOU race) | Wrapped in one transaction with a status-guarded UPDATE |

## Verification Evidence

| Command | Expected | Actual | Status |
|---|---|---|---|
| `cargo test -p axon-jobs --no-fail-fast` (PR #396 worktree) | pass | 878 passed across `axon-jobs`+`axon-services`, 0 failed | pass |
| `cargo test -p axon-jobs --no-fail-fast` (PR #398 worktree, post-merge-conflict-resolution) | pass | 254 passed, 0 failed | pass |
| `cargo test -p axon-web memory` / `--lib` (PR #400) | pass | 8 / 171 passed | pass |
| `cargo test -p axon-mcp --no-fail-fast` (rest-memory-surface-impl merge resolution) | pass | 84 passed, 0 failed (includes the new admin-scope memory tests) | pass |
| `EXPLAIN QUERY PLAN` on the real claim query, before the index fix | `SCAN jobs` (full table scan) | confirmed `SCAN jobs` + `USE TEMP B-TREE FOR ORDER BY` | confirmed regression |
| `EXPLAIN QUERY PLAN` on the real claim query, after the index fix | `SEARCH jobs USING INDEX ...` | confirmed `SEARCH jobs USING INDEX idx_axon_jobs_claim_cooldown (status=?)` | confirmed fixed |
| `CARGO_TARGET_DIR=/tmp/axon-clean-target xtask schemas generate --check` | pass (isolated build) | exit 0 | confirmed false-alarm CI failure, no code fix needed |
| `gh pr checks <396\|398\|399\|400>` (final, pre-merge) | all required checks green | all green (`ci-gate`, `codeql-gate`, `compose-smoke-gate`, `schema-contract-sync`) | pass |
| `git cat-file -t origin/main:<each stray symlink path>` (post-merge) | "not in origin/main" (never landed) | confirmed for all 13+1 paths | pass |

## Risks and Rollback

- All 4 PRs were squash-merged to `main`; rollback for any one is `git revert <merge-commit-sha>` (396: `5aae321a5`, 398: `c893a7896`, 399: `6ecbad170`, 400: `6c22e1036`) — each is a single revertable commit on `main`.
- The crawl-concurrency-limit and panic-guard/watchdog-reclaim changes in PR #396 are the highest-risk pieces of this session (they change always-on worker-loop behavior); both are covered by new dedicated regression tests (`crawl_jobs_stay_bounded_by_crawl_specific_limit_even_with_high_general_concurrency`, `panicking_runner_marks_job_failed_not_stuck_running`, `watchdog_sweep_reclaims_stale_unified_job`) but have not yet been observed under real production load.
- `axon:admin` enforcement on `ReplaceScope` memory imports is a behavior-breaking change for any existing caller that was relying on the previously-unenforced contract with only `axon:write` — intentional (it was a real security gap), but worth flagging if any automation breaks.

## Decisions Not Taken

- Did not attempt to fix the pre-existing, unrelated `xtask schemas api --check` drift discovered on PR #398's `axon-api` family while investigating the `database` family's CI failure — confirmed unrelated to this session's diff and reproducible identically on a clean `main` checkout; left untouched rather than sweeping 688 unrelated lines into an unrelated PR.
- Did not batch the N+1 query pattern in `axon-memory`'s import/export store code found during PR #400's review — pre-existing code, not introduced by this session's diff, and fixing it properly (transaction batching, response streaming) was judged out of scope for a security-focused PR; tracked instead as `axon_rust-fo3yx`.
- Did not thread real caller auth into the web-panel dispatch path for crawl/embed/ingest job submission (found during PR #396's review) — the MCP side was fixed since it needed only a task-local; the panel side needs a route-handler signature change judged too large for the fix pass; tracked as `axon_rust-owx6a`.

## Next Steps

- No unfinished work from this session — all 4 PRs are merged, `main` is confirmed clean, and worktrees/branches are cleaned up.
- Follow-up work is fully tracked in the 5 open beads listed above; none are blocking.
- Recommended before the next session touches this area: `axon_rust-owx6a` (web-panel caller auth) is the most load-bearing of the 5 — it's the natural next step once any finer-grained per-kind job scoping work begins, since `auth_snapshot` is documented as "the *only* source of truth" for future scope decisions.
