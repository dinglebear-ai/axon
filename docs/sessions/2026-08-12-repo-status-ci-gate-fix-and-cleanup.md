# Repo status audit, scheduled-CI ci-gate fix (PR #551), and stale worktree/branch cleanup

```yaml
date: 2026-08-12 17:01:03 EST
repo: git@github.com:dinglebear-ai/axon.git
branch: claude/repo-status-becf32
head: 5c79e7ce0
working directory: /home/jmagar/workspace/axon/.claude/worktrees/repo-status-becf32
worktree: /home/jmagar/workspace/axon/.claude/worktrees/repo-status-becf32
pr: #551 fix(ci): run the scheduled security audit through skipped rust-contracts (https://github.com/dinglebear-ai/axon/pull/551) — merged
beads: axon_rust-f9bma
```

## User Request

Four sequential asks: (1) `repo-status` — audit the checkout, worktrees, branches, merge readiness; (2) "ok fix-ci" — fix the scheduled CI failure found by the audit; (3) `/vibin:repo-status cleanup all stale branches/trees and get us synced w main` plus a follow-up "Clean up the stale worktrees/branches"; (4) save this session log.

## Session Overview

A repo-status audit found no mergeable work but surfaced one anomaly: every weekly scheduled CI run on `main` failed at `ci-gate` with `security was skipped even though its required condition is true`. The root cause was diagnosed and fixed in PR #551 (merged to `main` as `baaa535e6`), with the contract pinned in `tests/workflow_shapes.rs`. Two cleanup passes then removed two orphan worktree directories, one merged worktree, and four merged `codex/repair-session-ask*` branches, and fast-forwarded the main checkout and this session worktree — while deliberately leaving the live `codex/secret-redaction-investigation` session untouched.

## Sequence of Events

1. **Repo-status audit** — ran the skill's context collector (`repo_context.sh --json --include-gh`); found 3 worktrees, no open PRs, no stale remote branches, one dirty active investigation worktree, and a red scheduled CI run on `main`.
2. **CI failure diagnosis** — the scheduled run 31355298379 failed at `ci-gate` while the push run on the same SHA (`24d311b62`) passed; job list showed `security` skipped with `changes` and `live-qdrant` green.
3. **Root cause** — on `schedule`, `scripts/ci/changed_paths.py` routes only `security` + CodeQL keys (`SCHEDULE_KEYS`, line 142), so `rust-contracts` is intentionally skipped; the `security` job's `needs: [changes, rust-contracts]` with an `if:` lacking `always()` let the skipped ancestor cascade, skipping `security`, which `ci-gate` then correctly flagged.
4. **Fix** — created bead `axon_rust-f9bma`, branched `fix/ci-security-scheduled-skip` from `origin/main`, changed the `security` job's `if:` to evaluate through skipped ancestors (mirroring the mcp-smoke treatment from #547), and added shape test `security_survives_the_scheduled_skip_of_rust_contracts`.
5. **Verify + land** — `cargo test --test workflow_shapes` 53/53 pass, `actionlint` clean, lefthook pre-commit green in 3.4s; pushed, opened PR #551, watched all checks green (`ci-gate` pass, `security` pass in 24s), squash-merged as `baaa535e6`, closed the bead, `bd dolt push`, deleted the fix branch local+remote.
6. **Cleanup pass 1** — fast-forwarded main checkout and session worktree to `baaa535e6`; deleted empty orphan dir `.worktrees/ci` and 37M dead husk `.claude/worktrees/agent-ae1a0e26ffc407fb4` (no `.git`; newest files verified present in the git object store before deletion).
7. **Cleanup pass 2** — codex sessions had merged PRs #554–#558 meanwhile; removed clean worktree `.worktrees/repair-session-ask` and force-deleted four `codex/repair-session-ask*` branches after matching each tip to a merged squash PR via `gh pr list --head`; fast-forwarded both main-tracking checkouts to `546a72178`.
8. **Session save** — maintenance pass (this document), fast-forwarded session worktree to `5c79e7ce0` (#559), landed this log on `main`.

## Key Findings

- `scripts/ci/changed_paths.py:142` — `SCHEDULE_KEYS = {security, codeql_*}`: the weekly cron exists only for the live-qdrant suite (gated directly on event name) and the security audit; every other lane is intentionally skipped.
- `.github/workflows/ci.yml:903-908` (pre-fix) — `security` needed `rust-contracts` without `always()`; a skipped required ancestor cascades in GitHub Actions, so `security` could never run on cron.
- `ci-gate`'s `require_success_or_intentional_skip` is working as designed — the failure was real signal, not gate noise.
- This repo squash-merges, so `git branch --merged` can never prove a feature branch merged; PR head lookup (`gh pr list --head <branch> --state all`) is the reliable merged-evidence path.
- Orphaned agent worktree husks (directories without `.git` after `git worktree prune`) can be safely verified by hashing their newest files with `git hash-object` and checking `git cat-file -e` before deletion.

## Technical Decisions

- **Mirror the #547 pattern rather than invent a new one**: `if: ${{ always() && needs.changes.outputs.run_security == 'true' && (needs.rust-contracts.result == 'success' || needs.rust-contracts.result == 'skipped') }}` — keeps "don't audit if contracts actually failed" while surviving intentional skips.
- **Pin the exact `if:` string in `workflow_shapes.rs`**, matching the existing mcp-smoke exact-string test, so the contract cannot regress silently.
- **Merge PR #551 without waiting for a human pass** — repo auto-merge is off and the established workflow is manual `gh pr merge` after green checks; the fix only takes effect on `main` where the cron runs.
- **Never touch the `codex/secret-redaction-investigation` worktree** — dirty (35–50 modified files across passes), gaining commits during the session, later tied to open PR #552: a live session owns it.
- **Delete squash-merged branches with `-D`** only after per-branch PR-merge evidence, since `-d` can never succeed under squash merges.

## Files Changed

| status | path | previous path | purpose | evidence |
|---|---|---|---|---|
| modified | `.github/workflows/ci.yml` | — | `security` job `if:` evaluates through skipped `rust-contracts` ancestor + explanatory comment | commit `77246c9a7`, merged as `baaa535e6` (PR #551) |
| modified | `tests/workflow_shapes.rs` | — | new test `security_survives_the_scheduled_skip_of_rust_contracts` pinning the exact `if:` contract | same commit; `cargo test --test workflow_shapes` 53 pass |
| created | `docs/sessions/2026-08-12-repo-status-ci-gate-fix-and-cleanup.md` | — | this session log | this file |
| deleted | `.worktrees/ci/` (empty dir) | — | orphan, not a registered worktree | `ls -A` showed 0 entries |
| deleted | `.claude/worktrees/agent-ae1a0e26ffc407fb4/` (37M dir) | — | dead husk of pruned agent worktree, no `.git`, newest content mid-July | `git hash-object` + `cat-file -e`: all sampled newest files already in object store |
| deleted | worktree `.worktrees/repair-session-ask` + branches `codex/repair-session-ask{,-citations,-context-diversity,-source-diversity}` | — | all merged as PRs #554–#558 | `gh pr list --head <branch> --state all` → MERGED for each; worktree clean (`status --short` empty) |

## Beads Activity

| bead | title | actions | final status | why it mattered |
|---|---|---|---|---|
| `axon_rust-f9bma` | ci: security job skipped on scheduled runs despite run_security=true | created (P1 bug) → claimed → closed with reason referencing PR #551 / `baaa535e6`; `bd dolt push` completed | CLOSED | tracked the only code change of the session per repo Beads policy |

## Repository Maintenance

- **Plans**: no plan files were touched or contradicted by this session; no moves made. The injected "Active plan" path points at a different repo (`~/workspace/axon_rust/...`) and was ignored. Assessing completion of the 13 non-`complete/` plans in `docs/plans/` is out of scope for a CI-fix session — skipped deliberately.
- **Beads**: `axon_rust-f9bma` closed with verification evidence; no follow-up beads needed — no remaining work from this session's scope. `bd dolt push` ran successfully after close.
- **Worktrees/branches** (two passes during the session, one check at save time):
  - Removed: `.worktrees/ci` (empty), `.claude/worktrees/agent-ae1a0e26ffc407fb4` (no-`.git` husk), `.worktrees/repair-session-ask` (clean, merged), branches `codex/repair-session-ask*` ×4 (all MERGED per `gh pr list --head`), `fix/ci-security-scheduled-skip` (merged as #551, remote auto-deleted).
  - Left alone at save time, with reasons: `codex/secret-redaction-investigation` (dirty, live, open PR #552); the fleet of new codex worktrees that appeared during the session (`chrome-cli-live-contracts`, `codex/artifact-cli-contracts`, `codex/pr560-simplicity-fixes`, `harness-timeout-unique-run`, `live-harness-worktree-isolation`, `live-source-benchmark`, `pr559-review-fixes`) — several are ahead of main or tied to open PR #560, ownership is live and unclear; local `main` branch showing ahead-of-origin commits and a detached main checkout plus `/tmp/axon-main-push.*`/`/tmp/axon-main-verify.*` worktrees — evidence of another session's in-flight push pipeline. Touching any of these risks destroying concurrent work.
  - This session's own worktree (`.claude/worktrees/repo-status-becf32`) prunes itself when the session ends.
- **Stale docs**: none touched or contradicted; no updates needed. CI comments added in `ci.yml` document the cron routing at the point of the fix.
- **Sync**: main checkout and session worktree fast-forwarded twice during the session (`1e764d193 → baaa535e6 → 546a72178`); session worktree fast-forwarded again at save time to `5c79e7ce0`. The main checkout was **not** synced at save time because it is currently detached under another session's control.

## Tools and Skills Used

- **Skills**: `vibin:repo-status` (twice — audit + cleanup), `vibin:gh-fix-ci` (CI fix workflow), `vibin:save-to-md` (this document). All behaved as documented.
- **Shell/git**: evidence collection, ff-merges, worktree/branch removal, temp-worktree-free mergeability reasoning. One quirk: `gh pr merge --delete-branch` failed local branch cleanup ("'main' is already used by worktree") — merge itself succeeded; handled remote prune + local delete manually.
- **gh CLI**: PR/run/job/check evidence, PR creation, squash merge, background `gh pr checks --watch`.
- **bd (beads)**: create/claim/close + `bd dolt push`; no issues.
- **Skill scripts**: `repo_context.sh` + `summarize_context.py` collectors; worked as intended.
- **actionlint** (via mise shim): validated `ci.yml`; clean.
- **cargo**: `cargo test --test workflow_shapes` with `CARGO_TARGET_DIR` pointed at the main checkout's warm target to dodge the cold-worktree/kache rebuild cost.
- No subagents, browser tools, or MCP upstream tools were needed.

## Commands Executed

| command | result |
|---|---|
| `repo_context.sh --json --include-gh` | 3 worktrees, 3 branches, no PRs; triage table |
| `gh run view 31355298379 --json jobs/--log` | `security` skipped, `ci-gate` error `security was skipped even though its required condition is true` |
| `cargo test --test workflow_shapes` (warm target, isolated `AXON_DATA_DIR`) | 53 passed, 0 failed |
| `actionlint .github/workflows/ci.yml` | clean |
| `gh pr create` → `gh pr checks 551 --watch` → `gh pr merge 551 --squash --delete-branch` | all checks pass; merged `baaa535e6`; local `--delete-branch` step errored harmlessly |
| `bd close axon_rust-f9bma ... && bd dolt push` | closed; push complete |
| `git worktree remove .worktrees/repair-session-ask && git branch -D codex/repair-session-ask...` (×4) | removed after per-branch MERGED evidence |
| `rmdir .worktrees/ci && rm -rf .claude/worktrees/agent-ae1a0e26ffc407fb4` | orphans deleted after content verification |
| `git pull --ff-only` / `git merge --ff-only origin/main` (both checkouts, multiple passes) | fast-forwards only, no merges created |

## Errors Encountered

- **`gh pr merge --delete-branch` local cleanup failure** — `fatal: 'main' is already used by worktree at '/home/jmagar/workspace/axon'`: gh tried to switch this worktree to `main` after merging. Merge itself had succeeded (verified via `gh pr view 551 --json state`); resolved with `git fetch --prune` + manual local branch delete. No data impact.

## Behavior Changes (Before/After)

| area | before | after |
|---|---|---|
| Weekly scheduled CI on `main` | always red: `security` skipped via cascade, `ci-gate` fails | `security` runs its cargo audit + cargo deny on cron; gate can pass |
| Workflow contract coverage | mcp-smoke/binary-smoke skipped-ancestor pins only | `security` `if:` string also pinned in `workflow_shapes.rs` |
| Repo hygiene | 2 orphan dirs (one 37M), 1 merged worktree, 5 merged branches lingering | removed; only live work remains |

## Verification Evidence

| command | expected | actual | status |
|---|---|---|---|
| `cargo test --test workflow_shapes` | all pass incl. new test | 53 passed, 0 failed | pass |
| `actionlint .github/workflows/ci.yml` | no findings | clean exit | pass |
| `gh pr checks 551 --watch` | all required green | `ci-gate` pass, `security` pass (24s), test/clippy/live-rag/palette/web all pass | pass |
| `gh pr view 551 --json state,mergedAt` | MERGED | MERGED 2026-08-11T05:53:04Z, `baaa535e6` | pass |
| `gh pr list --head codex/repair-session-ask*` (×4) | MERGED PR per branch | #554/#555, #556, #557, #558 all MERGED | pass |
| `git hash-object` + `git cat-file -e` on husk's newest files | blobs exist in object store | all 3 sampled blobs exist | pass |

## Risks and Rollback

- **CI fix**: if `rust-contracts` legitimately fails on a push, `security` now skips (instead of skipping via cascade) and the run is already red from `rust-contracts` — same net outcome as before. Rollback: revert `baaa535e6`.
- **Scheduled-path proof is indirect**: a `pull_request` run cannot exercise the cron routing; the shape test pins the contract, but end-to-end confirmation is the next weekly cron (or a manual full-fanout `workflow_dispatch`).
- **Deleted husk directory**: only the newest files were hash-verified; residual risk that some older file held unique content is negligible (no `.git`, two-plus weeks stale, all sampled content in history) but nonzero and now unrecoverable.

## Decisions Not Taken

- Did not trigger a manual `workflow_dispatch` of CI to prove the cron path — dispatch routes all lanes (full fan-out) and was judged not worth the runner cost; next cron will confirm.
- Did not ff-sync or rebase the `codex/secret-redaction-investigation` worktree despite it being behind main — dirty, actively driven by another session.
- Did not touch the post-#559 fleet of codex worktrees/branches or the detached main checkout at save time — live concurrent sessions own them.

## References

- PR #551 (this session's fix): https://github.com/dinglebear-ai/axon/pull/551
- Failing scheduled run: https://github.com/dinglebear-ai/axon/actions/runs/31355298379
- Precedent fix for skipped ancestors: #547 (mcp-smoke), commit `8e57cf20e`
- Cleanup-evidence PRs: #554–#558 (codex repair-session-ask series)

## Open Questions

- Will next week's cron go green end-to-end? Expected yes; unverifiable until it fires.
- Local `main` in the primary checkout showed "ahead 4, behind 1" with a detached HEAD at save time — presumed another session's in-flight push pipeline (`/tmp/axon-main-push.*` worktrees); if that session died mid-flight, `main`'s local state may need reconciling later.

## Next Steps

- **Nothing unfinished from this session's scope.** The CI fix is merged and its bead closed.
- After the next scheduled CI run (weekly cron), confirm `ci-gate` passes; if it still fails, the failure mode will name a different job — repeat the same skipped-ancestor analysis for it.
- The post-#559 codex worktree fleet (`pr560-simplicity-fixes`, `harness-timeout-unique-run`, etc.) will need its own repo-status cleanup pass once PRs #560/#552 land and those sessions finish — same evidence pattern as this session: `gh pr list --head`, then remove.
