# Axon Branch and Worktree Cleanup

Date: 2026-05-09

## Context

The user asked whether there were stale local branches or worktrees that could be cleaned up. The goal was to inspect live Git state, remove only verified-safe stale refs, and leave active or ambiguous work untouched.

## Save-Time Repo State

- Repository: `/home/jmagar/workspace/axon_rust`
- Current branch: `chore/canonical-axon-home`
- Current HEAD: `1c8f97bb` (`test: add plugin setup smoke and client-server plan`)
- Upstream: `origin/chore/canonical-axon-home`
- Current worktree status: clean

```text
## chore/canonical-axon-home...origin/chore/canonical-axon-home
```

## Worktree Inventory

At save time there are two live worktrees:

```text
worktree /home/jmagar/workspace/axon_rust
HEAD 1c8f97bbf808020429fd60c06e0a9b206cb7ab8d
branch refs/heads/chore/canonical-axon-home

worktree /home/jmagar/workspace/axon_rust/.worktrees/true-client-server-mode
HEAD 000139e38ca7b786304ab6fbd3db0b691112d71f
branch refs/heads/feat/true-client-server-mode
```

The feature worktree is clean:

```text
## feat/true-client-server-mode...origin/feat/true-client-server-mode
```

## Branch Inventory

Local branches at save time:

```text
chore/canonical-axon-home|origin/chore/canonical-axon-home||2026-05-09 07:29:02 -0400|test: add plugin setup smoke and client-server plan
feat/true-client-server-mode|origin/feat/true-client-server-mode||2026-05-09 09:57:54 -0400|fix(server): persist server-mode scrape artifacts
main|origin/main||2026-05-08 18:58:28 -0400|fix: plugin setup script
```

Branches merged into the current HEAD:

```text
* chore/canonical-axon-home
  main
```

Branches not merged into the current HEAD:

```text
+ feat/true-client-server-mode
```

The `+` marker means `feat/true-client-server-mode` is checked out in another worktree, so it is active and was not a cleanup candidate.

## Cleanup Performed

The stale local branch `chore/docker-compose-reorg` was deleted after verification that it was already merged into `main`.

Deletion result:

```text
Deleted branch chore/docker-compose-reorg (was 50b1a240).
```

No worktrees were removed. `git worktree prune --dry-run --verbose` produced no output, meaning Git did not find stale administrative worktree records to prune.

## Intentionally Left Alone

`feat/true-client-server-mode` was left untouched because it is:

- Not merged into the current HEAD.
- Checked out in `/home/jmagar/workspace/axon_rust/.worktrees/true-client-server-mode`.
- Tracking `origin/feat/true-client-server-mode`.
- Clean at save time.

`chore/canonical-axon-home` was left untouched because it is the current branch and tracks `origin/chore/canonical-axon-home`.

`main` was left untouched because it is the canonical base branch and tracks `origin/main`.

## Verification

Commands used during the cleanup and save pass:

```bash
git fetch --prune
git status --short --branch
git worktree list --porcelain
git branch --merged main
git branch --no-merged main
git for-each-ref --format='%(refname:short)|%(upstream:short)|%(upstream:track)|%(committerdate:iso8601)|%(subject)' refs/heads
git branch -d chore/docker-compose-reorg
git worktree prune --dry-run --verbose
git branch --merged HEAD
git branch --no-merged HEAD
```

Post-cleanup state:

- No stale worktree records were found.
- The one verified merged stale branch was deleted.
- Active worktree branch `feat/true-client-server-mode` remains.
- The main checkout is clean on `chore/canonical-axon-home`.
- The feature worktree is clean on `feat/true-client-server-mode`.

## Open Questions

- Whether `feat/true-client-server-mode` should be merged or kept open depends on that feature's review/runtime status; it was not safe to delete in this cleanup pass.
- Whether `chore/canonical-axon-home` should eventually be merged back to `main` is outside the branch cleanup scope and should be decided from PR or tracker state.
