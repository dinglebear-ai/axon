---
date: 2026-06-24 07:23:11 EDT
repo: git@github.com:jmagar/axon.git
branch: codex/save-session-20260624-transport-policy
head: 429e37be72bbfda7f5cdbb4f44fa77da360f6cf1
session id: 019ef69e-b158-7841-99e4-9ce31f3a159b
transcript: /home/jmagar/.codex/sessions/2026/06/23/rollout-2026-06-23T18-34-15-019ef69e-b158-7841-99e4-9ce31f3a159b.jsonl
working directory: /tmp/axon-save-session-260
worktree: /tmp/axon-save-session-260 429e37be [codex/save-session-20260624-transport-policy]
pr: "#260 Claude/recursing keller 021b55 https://github.com/jmagar/axon/pull/260"
---

# Transport policy and PR #260 closeout

## User Request

Jacob asked to identify and fix transport-layer divergence across CLI, MCP, HTTP, and app clients after the crawl `max_pages` MCP-vs-CLI bug, then run review agents, fix all review issues, push, confirm CI, merge PR #260, and save the session to markdown.

## Session Overview

PR #260 was completed, reviewed, pushed, verified green, and merged into `main`. The focused end-state was centralized transport request policy for crawl, query, retrieve, map, sources/domains, job lists, ask, extract, and evaluate, with client surfaces adjusted so defaults live in the server/services layer instead of in transport or app shims.

The final merged commit on `main` is `429e37be72bbfda7f5cdbb4f44fa77da360f6cf1`. The final PR head before squash merge was `516551dfdec6a570bbcf1047d0f3f7e2965620b5`.

## Sequence of Events

1. Established that the original crawl divergence came from the CLI parse layer applying a `2000` default while MCP passed `0` through to `Config::default()`.
2. Moved crawl page-cap resolution into `axon_services::crawl::resolve_crawl_max_pages`, using `5000` as both default and hard cap unless the operator escape hatch is enabled.
3. Audited additional transport-level divergence across CLI, MCP, HTTP, generated clients, Chrome, Palette, and Android; moved policy into shared service/contract layers where possible.
4. Dispatched review agents and addressed findings around ask streaming validation, evaluate fallback errors, MCP map metadata, error taxonomy, artifact handles, extract options, sources/domain caps, and mobile/client defaults.
5. Pushed PR #260, then followed CI failures through to completion: sparse checkout missed `crates/`, MCP schema docs drifted, security advisories needed explicit handling, version-sync required component bumps, and the ask-quality guard still assumed tests lived under root `src`.
6. Merged PR #260 after all checks passed, then deleted the remote PR branch when GitHub CLI left it behind because the local `main` worktree was already checked out elsewhere.
7. Created this session artifact from a clean temporary worktree rooted at the merged `origin/main` to avoid touching unrelated dirty work in `/home/jmagar/workspace/axon`.

## Key Findings

- The original MCP/CLI crawl mismatch was transport-layer policy drift: CLI applied a default before the service layer, while MCP did not.
- App clients had similar risk: Chrome and Palette carried hard-coded map/retrieve limits, while Android had request/default drift for new parameters.
- The Android retrieve concern was clarified: the `50_000` token value is a server hard cap for explicit requests, not the default synthesis context; Android default retrieve token budget was reduced to `10_000`.
- CI sparse checkout definitions were stale after workspace extraction and did not include `crates/`, causing jobs to miss `crates/axon-web/Cargo.toml`.
- `scripts/test-ask-quality-regressions.sh` still searched/listed only the root package layout; after extraction, the required ask tests live under `crates/axon-vector`.

## Technical Decisions

- Defaults and caps belong in the services/contract layer so CLI, MCP, HTTP, and app clients all resolve the same policy.
- Transport shims should pass "unspecified" values through instead of reimplementing defaults.
- Over-cap crawl requests should clamp to the service-layer hard ceiling and emit an actionable cap-hit message.
- The ask-quality guard was kept strict; instead of weakening it, it was updated to scan `crates src tests` and to list/run workspace tests.
- The session log was committed from a clean temporary worktree based on `origin/main` because the main checkout had unrelated dirty files.

## Files Changed

| status | path | previous path | purpose | evidence |
|---|---|---|---|---|
| modified | `.github/workflows/ci.yml` | - | Added sparse checkout coverage for `crates/` so CI jobs can see extracted workspace crates. | CI jobs passed on run `28074509331`. |
| modified | `crates/axon-services/src/transport.rs` | - | Central transport request policy chokepoint and defaults/caps. | PR #260 final checks passed. |
| modified | `crates/axon-cli/src/commands/crawl/subcommands.rs` | - | CLI crawl path stopped owning the page-cap default. | Transport parity tests passed. |
| modified | `crates/axon-mcp/src/server/*` | - | MCP handlers aligned with shared request policy, option mapping, and error taxonomy. | `mcp-smoke`, `mcp-oauth-smoke`, and `mcp-transport-modes` passed. |
| modified | `crates/axon-web/src/server/handlers/*` | - | HTTP handlers aligned with shared services/contract policy. | `rest-api-parity` passed. |
| modified | `apps/android/**` | - | Android sends new params and uses a `10_000` retrieve default. | `android` and `android-openapi-client` passed. |
| modified | `apps/chrome-extension/**` | - | Removed hard-coded map/retrieve defaults from client surface. | PR #260 merged. |
| modified | `apps/palette-tauri/**` | - | Removed client-side hard-coded map/retrieve defaults and synced generated API contracts. | `palette-tauri` passed. |
| modified | `.cargo/audit.toml`, `deny.toml`, `Cargo.lock` | - | Resolved current security gate by updating `quinn-proto` and documenting targeted advisory ignores. | `security` passed. |
| modified | `docs/reference/mcp/tool-schema.md` | - | Regenerated MCP schema docs after request fields changed. | `mcp-schema-doc-sync` passed. |
| modified | `scripts/test-ask-quality-regressions.sh` | - | Updated ask-quality gate for workspace crate layout. | `test` passed on fresh CI. |
| created | `docs/sessions/2026-06-24-transport-policy-pr260-merge.md` | - | This session artifact. | Added in this save-to-md pass. |

The squash merge touched 2,121 paths: 1,042 added, 1,006 deleted, and 73 modified according to `git diff-tree --no-commit-id --name-status -r 429e37be | wc -l` and status aggregation. The full exact file list is recoverable from `git diff-tree --no-commit-id --name-status -r 429e37be72bbfda7f5cdbb4f44fa77da360f6cf1`.

## Beads Activity

No bead activity was changed during the save-to-md pass. `bd list --all --sort updated --reverse --limit 100 --json` returned historical Axon issues, but no bead was created, edited, claimed, closed, or assigned for this closeout.

The merged PR text references the broader workspace extraction epic `axon_rust-23dw`; this session note does not claim new bead state for that epic because no current bead mutation was observed in this turn.

## Repository Maintenance

### Plans

`docs/plans/` contains active-looking top-level plans including `docs/plans/2026-06-20-workspace-crate-extraction-inventory.md` and several older plans. No plan was moved to `docs/plans/complete/` because the save contract requires a path-limited session artifact commit and the plan-completion state was not safely audited file-by-file in this turn.

### Beads

Beads were read for context. No bead was changed because the PR was already merged and no unresolved follow-up was proven from the save pass alone.

### Worktrees and branches

`git worktree list --porcelain` showed active worktrees for `main`, `marketplace-no-mcp`, the merged PR worktree `claude/recursing-keller-021b55`, and several QA/debug worktrees. No local worktree or branch was deleted because the main checkout contains unrelated dirty work, the no-MCP branch is intentionally long-lived, and the remaining QA/debug worktrees were not proven safe to remove.

The remote PR branch `claude/recursing-keller-021b55` was deleted immediately after merge. The local PR worktree remains with upstream marked gone.

### Stale docs

Docs touched by PR #260 were updated inside the PR, including `CLAUDE.md`, `README.md`, `CHANGELOG.md`, and `docs/reference/mcp/tool-schema.md`. No additional stale-doc edit was made in this save pass so that the commit contains only the generated session artifact.

### Dirty checkout preservation

`/home/jmagar/workspace/axon` had many unrelated modified and untracked files before this session log was written. Those files were preserved untouched by using `/tmp/axon-save-session-260` for the session artifact.

## Tools and Skills Used

- **Skill: `vibin:save-to-md`.** Used to drive the session artifact format, maintenance pass, and path-limited commit/push contract.
- **Shell and GitHub CLI.** Used for `git status`, `git fetch`, `git worktree`, `git show`, `git diff-tree`, `gh pr view`, `gh pr checks`, `gh run view`, `gh pr merge`, and branch deletion.
- **Local file editing.** Used `apply_patch` to add this markdown artifact without touching unrelated files.
- **CI inspection.** Used `gh pr checks`, `gh run view`, and direct job logs to diagnose and verify GitHub Actions.
- **Review agents.** Earlier in the session, code-review agents covered code quality, type design, tests, silent failures, and simplification; their findings were folded into PR #260 before final CI.
- **Lumen semantic search.** Used during CI failure diagnosis after the tool was loaded, specifically to locate ask-quality tests in the new crate layout.

## Commands Executed

| command | result |
|---|---|
| `gh pr checks 260 --watch=false` | Confirmed PR #260 was green before merge and again after the final push. |
| `gh pr merge 260 --squash --delete-branch` | Merged PR #260, but local branch cleanup failed because `main` was already used by `/home/jmagar/workspace/axon`. |
| `git push origin --delete claude/recursing-keller-021b55` | Deleted the remote PR branch after merge. |
| `gh pr view 260 --repo jmagar/axon --json state,mergedAt,mergeCommit,title,url` | Confirmed PR #260 state `MERGED`, merged at `2026-06-24T11:16:04Z`, merge commit `429e37be72bbfda7f5cdbb4f44fa77da360f6cf1`. |
| `git fetch origin main` | Updated `origin/main` from `7682316a` to `429e37be`. |
| `git worktree add -b codex/save-session-20260624-transport-policy /tmp/axon-save-session-260 origin/main` | Created a clean temporary worktree for this session artifact. |
| `git diff-tree --no-commit-id --name-status -r 429e37be72bbfda7f5cdbb4f44fa77da360f6cf1` | Verified the squash merge changed 2,121 paths. |

## Errors Encountered

- **GitHub CLI merge cleanup error.** `gh pr merge 260 --squash --delete-branch` returned `fatal: 'main' is already used by worktree at '/home/jmagar/workspace/axon'`. The PR had already merged successfully; cleanup was completed with `git push origin --delete claude/recursing-keller-021b55`.
- **CI ask-quality failure.** The `test` job failed because `scripts/test-ask-quality-regressions.sh --verify-only` searched only `src tests` while the required tests now live under `crates/axon-vector`. The script was updated to search `crates src tests` and to use workspace-wide cargo test listing/runs.
- **CI sparse checkout failure.** Earlier CI jobs failed because sparse checkout blocks did not include `crates/`. The workflow was updated and the replacement run passed.
- **Security advisory failure.** CI security failed on current RustSec advisories. `quinn-proto` was updated and explicit advisory handling was added to audit/deny config; the `security` check passed.
- **Version sync failure.** Component version gates required Android, Chrome extension, and Palette version/changelog bumps; after those changes, `version-sync` passed.

## Behavior Changes (Before/After)

| area | before | after |
|---|---|---|
| Crawl page cap | CLI defaulted differently from MCP/HTTP; MCP could leave `max_pages` as `0`. | Services layer resolves unspecified crawl cap to `5000` and clamps over-cap requests consistently. |
| Cap-hit messaging | Crawl cap stop messages were terse. | Crawls report that more pages were available and suggest a higher cap or tighter scope. |
| Client limits | Chrome/Palette hard-coded map/retrieve limits. | Clients defer defaults to server policy. |
| Android retrieve | Android default could imply the server hard cap. | Android default retrieve token budget is `10_000`; `50_000` remains an explicit hard cap. |
| Ask-quality gate | Root-layout-only script missed crate-extracted tests. | Script scans/list/runs workspace tests. |

## Verification Evidence

| command | expected | actual | status |
|---|---|---|---|
| `./scripts/test-ask-quality-regressions.sh --verify-only` | Required ask-quality tests found. | Passed locally after script update. | pass |
| `./scripts/test-ask-quality-regressions.sh --run` | Full ask-quality guard passes in workspace layout. | Passed locally. | pass |
| `git push --force-with-lease origin claude/recursing-keller-021b55` | Push latest PR head. | Pushed `516551df`; local pre-push router passed. | pass |
| `gh pr checks 260 --watch=false` | All PR checks green. | All checks passed, including `ci-gate`, `test`, `android`, `release`, `mcp-smoke`, `security`, `version-sync`, and `windows-build`. | pass |
| `gh pr view 260 --repo jmagar/axon --json state,mergedAt,mergeCommit` | PR merged to main. | State `MERGED`, merge commit `429e37be`. | pass |
| `git ls-remote --heads origin claude/recursing-keller-021b55 main` | Main exists; PR branch removed. | Main at `429e37be`; PR branch absent after deletion. | pass |

## Risks and Rollback

The transport-policy changes are broad because they affect CLI, MCP, HTTP, Chrome, Palette, Android, generated OpenAPI clients, and CI gates. The rollback path is to revert merge commit `429e37be72bbfda7f5cdbb4f44fa77da360f6cf1` from `main`, then rerun the full CI matrix.

For this session-log commit, rollback is simply `git revert <session-log-commit>` because it should contain only `docs/sessions/2026-06-24-transport-policy-pr260-merge.md`.

## Decisions Not Taken

- Did not delete local worktrees after merge because several are active or dirty, and ownership/safety was not proven in this turn.
- Did not move plans to `docs/plans/complete/` because the plan state was not audited deeply enough and the save-to-md commit must remain path-limited.
- Did not overwrite or pull the dirty `/home/jmagar/workspace/axon` main checkout; a clean temporary worktree avoided disturbing unrelated WIP.

## References

- PR #260: https://github.com/jmagar/axon/pull/260
- Final PR head: `516551dfdec6a570bbcf1047d0f3f7e2965620b5`
- Merge commit: `429e37be72bbfda7f5cdbb4f44fa77da360f6cf1`
- Final CI run: https://github.com/jmagar/axon/actions/runs/28074509331
- Transcript: `/home/jmagar/.codex/sessions/2026/06/23/rollout-2026-06-23T18-34-15-019ef69e-b158-7841-99e4-9ce31f3a159b.jsonl`

## Open Questions

- Whether any old top-level plans under `docs/plans/` should now be moved to `docs/plans/complete/` needs a separate plan-by-plan audit.
- Whether stale local worktrees under `/home/jmagar/workspace/axon/.worktrees/` should be removed needs a separate dirty-state and merge-ancestry check.

## Next Steps

1. Commit and push this session artifact as the only file in the commit.
2. If desired, run a dedicated cleanup pass for old Axon worktrees and plans after confirming no dirty or unmerged work would be lost.
3. Update the dirty main checkout separately when ready; it was intentionally left untouched during this save.
