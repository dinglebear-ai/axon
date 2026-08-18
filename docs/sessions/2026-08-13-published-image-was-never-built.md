---
date: 2026-08-13 00:53:45 EST
repo: git@github.com:dinglebear-ai/axon.git
branch: claude/axon-live-test-restarts-96f3b9
head: 08d8a0e31
working directory: /home/jmagar/workspace/axon/.claude/worktrees/axon-live-test-restarts-96f3b9
worktree: /home/jmagar/workspace/axon/.claude/worktrees/axon-live-test-restarts-96f3b9
pr: 564 — fix(ci): build and publish the runtime image stage — https://github.com/dinglebear-ai/axon/pull/564 (merged as f29ab9f9d)
beads: axon_rust-mmhw8 (created, closed), axon_rust-b1j6b (created), axon_rust-em3uq (created)
---

# Published image was never built

## User Request

A prior session reported a side finding: Axon live-test runs habitually leave a
container restarting once per minute for ~10 minutes after each run, observed
across ten bursts over two days, and asked whether to investigate why those test
containers exit and get restarted. Follow-ups: open a PR, run
`/vibin:review-pr` and address all issues surfaced, then "fix the published
image".

## Session Overview

The restart churn was a symptom. Root cause: both workflows that build
`config/Dockerfile` called `docker/build-push-action` with no `target:`, so
Docker selected the **last** stage — `dev-runtime`, which ships no binary and
execs a bind-mounted host build. That stage was published as
`ghcr.io/dinglebear-ai/axon:latest`.

Pinning `target: runtime` then exposed a second, larger problem: the `builder`
stage did not compile at all. Because `dev-runtime` is `FROM node` and never
depends on `builder`, no build had ever selected a stage that compiled Axon. The
production image path had never been built, and CI was green throughout.

Both were fixed, reviewed across six passes, merged, and the corrected image was
published and verified by pulling and running it from the registry.

## Sequence of Events

1. Located the live harness (`scripts/lib/live-cli-*.sh`) and a real prior run
   directory at `.cache/live-test/20260811-180901`, matching the reported
   container name `axon-live-20260811180901-axon`.
2. Read that run's `logs/cleanup-compose.stderr.log`: cleanup had stopped and
   removed the container correctly. Cleanup was never at fault; the churn was
   inside each run, between `compose up` and the EXIT trap.
3. Confirmed the restart curve in `journalctl -u docker`: restarts ~1s apart at
   12:13:36 doubling out to a flat 60s by 12:15:43 — Docker's capped backoff.
4. Reproduced the crash directly by re-running `axon compose up` from the stored
   fixture home; `docker logs` showed
   `[FATAL tini] exec /home/axon/.axon/dev/axon failed: No such file or directory`.
5. Traced it to the image: `docker inspect` showed the dev entrypoint baked into
   the image itself, confirmed against the registry manifest rather than the
   local tag.
6. Found the cause in `.github/workflows/docker-image.yml` — no `target:`.
   Fixed both workflows, added a smoke run, added a harness assertion, opened
   PR #564.
7. CI failed on `image-build-smoke`: the newly-selected `builder` stage aborted
   on missing `cmake`. Fixed, rebuilt locally, hit a second missing dep
   (`libclang`), fixed that too.
8. Ran six review passes (code, tests, comments, errors, docs-config, simplify).
   Applied every actionable finding across three follow-up commits.
9. Merged PR #564 as `f29ab9f9d`, which triggered the `Docker image` workflow on
   `main` and republished `:latest`.
10. Verified the published artifact by removing the local tag, pulling fresh, and
    running it.

## Key Findings

- **`config/Dockerfile` ends with `dev-runtime`** (`config/Dockerfile:123`),
  whose `ENTRYPOINT` is `/home/axon/.axon/dev/axon` (`config/Dockerfile:149`).
  `docker build` with no `--target` selects the last stage, so that is what
  shipped.
- **The published image carried the dev entrypoint.** Registry manifest for
  `ghcr.io/dinglebear-ai/axon:latest` was `sha256:ef95e89c` with
  `["/usr/bin/tini","--","/home/axon/.axon/dev/axon"]`. Verified via
  `docker buildx imagetools inspect`, so this was the real published artifact and
  not a stale local tag.
- **The `builder` stage never compiled.** `wreq` pulls in `boring-sys2`, which
  builds BoringSSL with CMake and generates bindings with bindgen. The builder
  installed only `pkg-config`, `libsqlite3-dev`, `ca-certificates`
  (`config/Dockerfile:18-22`). Missing `cmake`, `clang`, `libclang-dev`. Nothing
  noticed because `dev-runtime` is `FROM node` and skips `builder` entirely.
- **The harness assertion was phase-only.** `scripts/lib/live-cli-scenarios-admin.sh`
  asserted `compose up` reported its phases ok, which was true — compose did
  start the container. It exited afterwards, unobserved.
- **`RestartCount` lives at the top level of `docker inspect`**, not under
  `.State`, on this Docker version. The first draft of the assertion used
  `.State.RestartCount` and silently reported every container as missing; caught
  by testing the helper rather than assuming it.
- **A manual `docker restart` does not increment `RestartCount`** (verified
  empirically), so the same assertion is valid for `compose restart` and
  `compose rebuild`.
- **`docs/sessions/2026-07-08-incus-nested-docker-deployment.md:63`** had already
  recorded this exact bug as known and "out of scope for this epic", worked
  around locally with `docker build --target runtime`. It was never filed.

## Technical Decisions

- **Pin `target:` explicitly rather than reorder the Dockerfile stages.**
  Reordering would make an untargeted build accidentally correct while leaving
  the same trap for the next stage appended. An explicit target plus a lint that
  enforces it fails loudly instead.
- **Add `scripts/ci/check_dockerfile_build_targets.py` rather than rely on the
  two hand-pinned workflows.** Hand-pinning does not stop the next workflow from
  omitting `target:`, which is precisely how this happened. The check is
  deterministic and instant, unlike the runtime probes.
- **Keep the lint stdlib-only.** PyYAML is not vendored, not in any requirements
  file, and not installed by the pre-commit hook or the self-hosted runners, so a
  `yaml.safe_load` implementation would be shorter but not runnable where it
  needs to run. The indentation-based splitter is documented as producing
  approximate rather than exact step boundaries.
- **Publish by merging rather than by manual `docker push`.** The workflow's path
  filter includes `config/Dockerfile*` and `.github/workflows/docker-image.yml`,
  so merging republishes `:latest` automatically and keeps the tag reproducible
  from `main`. A manual push would have produced an image not derivable from any
  commit.
- **Declined the `jq`-based simplification of `assert_live_container_stable`.**
  It saves one cheap subprocess but rewrites a safety assertion whose four
  failure paths are verified, and `jq -e` would misroute the crashed case since
  `.State.Running` is legitimately `false` exactly then.

## Files Changed

| status | path | previous path | purpose | evidence |
|---|---|---|---|---|
| modified | `config/Dockerfile` | — | add `cmake`, `clang`, `libclang-dev` to builder; document that `dev-runtime` is last and must never be published | local `--target runtime` build succeeded after the change |
| modified | `.github/workflows/docker-image.yml` | — | pin `target: runtime` on the publishing build | published image entrypoint flipped to `/usr/local/bin/axon` |
| modified | `.github/workflows/compose-smoke.yml` | — | pin `target: runtime`, add `load: true`, run the built image, add the lint step | `image-build-smoke` printed `axon 7.2.19` |
| created | `scripts/ci/check_dockerfile_build_targets.py` | — | fail any workflow build of a multi-stage Dockerfile without a valid `target:` | fails pre-fix workflows, typo'd stage, unreadable Dockerfile |
| modified | `lefthook.yml` | — | run the new lint pre-commit, including when the checker itself changes | hook printed `dockerfile-build-targets` OK on commit |
| modified | `scripts/lib/live-cli-runtime.sh` | — | add `assert_live_container_stable` with evidence capture and distinguishable failure reasons | four failure paths exercised against real containers |
| modified | `scripts/lib/live-cli-scenarios-admin.sh` | — | assert container stability after `compose up`, `restart`, `rebuild` | — |
| created | `docs/sessions/2026-08-13-published-image-was-never-built.md` | — | this session log | — |

## Beads Activity

| id | title | actions | final status | why it mattered |
|---|---|---|---|---|
| `axon_rust-mmhw8` | Published axon image is built from the dev-runtime stage | created (P1 bug), closed with fix reference | closed | Tracked the root-cause defect that produced the reported restart churn |
| `axon_rust-b1j6b` | Live CLI harness is not wired into any CI workflow | created (P2 task) | open | The new container assertion only runs on manual harness invocations, so it is not a regression gate; that gap should not live only in prose |
| `axon_rust-em3uq` | Verify or republish pre-2026-08-13 ghcr axon image tags | created (P3 task) | open | `:latest` is fixed, but the historical tag set could not be enumerated without `read:packages`; anything pinning an old tag may still be broken |

## Repository Maintenance

**Plans — no-op, with reason.** 14 plan files sit directly under `docs/plans/`.
This session touched none of them (`git log --name-only origin/main~1..origin/main
| grep -c docs/plans/` returned 0), and no evidence was gathered about whether
any is complete. Moving files on no evidence would be unsafe, so none were moved.

**Beads — three actions.** One bead created and closed for the root cause, two
follow-up beads created for known remaining work rather than leaving it in prose.
See the table above.

**Worktrees and branches — deliberately conservative.**

- `git worktree prune --dry-run -v` produced no output; nothing is prunable.
- The two `/tmp` worktrees (`axon-main-push.sOXz5l`, `axon-main-verify.zZYVLP`)
  still have live directories on disk, so they are not stale by git's definition.
  Not this session's, left alone.
- `claude/axon-live-test-restarts-96f3b9` (this branch) is fully merged:
  `git diff --stat origin/main..HEAD` is empty after the squash merge. It is safe
  to delete, but was not deleted because this session is operating inside its
  worktree.
- `codex/preexisting-live-harness-hardening` and `codex/pr559-review-followup`
  both show `[origin/...: gone]`, which invites deletion, but they still carry 166
  and 172 diff lines against `origin/main`. Not safely deletable; left alone.
- The remaining eight registered worktrees belong to other concurrent sessions.
  Not touched.

**Stale docs — checked, one deliberate no-edit.** Grepped `docs/`, `deploy/`, and
`README.md` for `docker build` invocations against `config/Dockerfile`. Every hit
without a `--target` is inside a dated session log or a historical plan
(`docs/superpowers/plans/2026-05-12-axon-production-readiness.md:1754,1786` is
where the untargeted build was originally specified). Those are point-in-time
records by repo convention, not living guidance, so none were rewritten. In
particular `docs/sessions/2026-07-08-incus-nested-docker-deployment.md:63`
records this bug as open; it is now fixed, but editing a dated log to reflect
later events would misrepresent what was known then. No living doc made a claim
this session invalidated — `CLAUDE.md`, `.env.example`, and the compose files
describe the image reference without asserting anything about its contents.

## Tools and Skills Used

- **Shell commands.** `git`, `docker`, `docker compose`, `docker buildx
  imagetools`, `journalctl`, `rg`, `jq`, `python3`, `shellcheck`, `ruff`, `bd`,
  `gh`. Used for diagnosis, reproduction, verification, and repo state.
- **File tools.** Read/Edit/Write against the Dockerfile, workflows, harness
  scripts, and the new lint.
- **Subagents.** Six review passes via the PR Review Toolkit agents
  (`code-reviewer`, `pr-test-analyzer`, `comment-analyzer`,
  `silent-failure-hunter`, `code-simplifier`) plus one general-purpose agent for
  docs/config drift. All ran on Sonnet. No failures; one agent's premise about
  PyYAML availability was wrong and it correctly self-corrected after checking.
- **Skills.** `vibin:review-pr` (this session's review workflow),
  `vibin:save-to-md` (this log).
- **Issues encountered.** `gh api` for package versions returned HTTP 403 for
  missing `read:packages` scope, which blocked enumerating historical image tags.
  A `sleep`-chained CI poll was rejected by the harness and reworked into an
  `until` loop. No other degraded behavior.

## Commands Executed

| command | result |
|---|---|
| `journalctl -u docker --since ... \| grep axon-live` | restart curve: ~1s apart at 12:13:36, flat 60s by 12:15:43 |
| `docker logs axon-live-20260811180901-axon` | `[FATAL tini] exec /home/axon/.axon/dev/axon failed: No such file or directory` |
| `docker buildx imagetools inspect ghcr.io/dinglebear-ai/axon:latest` | pre-fix `sha256:ef95e89c`, dev entrypoint; post-fix `sha256:77a5122c`, `/usr/local/bin/axon` |
| `docker build --target runtime -t axon:verify-runtime .` | failed on `cmake`, then on `libclang`, then succeeded |
| `timeout 30s docker run --rm axon:verify-runtime --version` | `axon 7.2.19`, exit 0 |
| `timeout 30s docker run --rm axon:verify-dev --version` | exit 127, `exec /home/axon/.axon/dev/axon failed` |
| `python3 scripts/ci/check_dockerfile_build_targets.py <pre-fix workflows>` | exit 1, naming `dev-runtime` as what would ship |
| `gh pr merge 564 --squash` | merged as `f29ab9f9d` |
| `docker pull ghcr.io/dinglebear-ai/axon:latest && docker run --rm ... --version` | `axon 7.2.19`, exit 0 |
| `gh api /orgs/dinglebear-ai/packages/container/axon/versions` | HTTP 403, needs `read:packages` |

## Errors Encountered

- **`image-build-smoke` failed on the first PR push.** Root cause: pinning
  `target: runtime` made CI compile the `builder` stage for the first time, and
  `boring-sys2` aborted with `is cmake not installed?`. Resolved by adding
  `cmake` to the builder.
- **Local rebuild then failed on `Unable to find libclang`.** Root cause: the
  same crate's bindgen step dlopens libclang. Resolved by adding `clang` and
  `libclang-dev`.
- **First draft of `assert_live_container_stable` reported every container as
  missing.** Root cause: used `.State.RestartCount`; this Docker version exposes
  `RestartCount` at the top level. Caught by the helper's own functional test
  before it ever ran in the harness.
- **PR #564 opened as `CONFLICTING` with a 12-file diff.** Root cause: the
  worktree branched from `b06acc0c7`, four commits that never reached
  `origin/main`. Resolved with `git rebase --onto origin/main b06acc0c7`, leaving
  a clean 1-commit, 5-file PR.
- **The new lint passed silently on an unreadable Dockerfile.** Root cause:
  `dockerfile_stages` returned `[]` on `OSError`, which the caller folded into
  the "fewer than two stages, nothing to check" skip — the same failure shape the
  script exists to prevent. Found by the simplify pass, resolved by returning
  `None` and failing on it.

## Behavior Changes (Before/After)

| area | before | after |
|---|---|---|
| Published image | `:latest` was the `dev-runtime` stage; `docker run` exited 127 with `exec ... No such file or directory` | `:latest` is the `runtime` stage; `docker run ... --version` prints `axon 7.2.19` |
| `docker-image.yml` | published without ever compiling Axon | compiles `--target runtime` and publishes that |
| `compose-smoke.yml` | built an image and never ran it | builds the shipped stage, loads it, and executes its entrypoint under a 30s timeout |
| Workflow authoring | a build step omitting `target:` was accepted silently | pre-commit and CI fail with the stage name that would have shipped |
| Live harness | `compose up` passed while the container crash-looped | `up`/`restart`/`rebuild` require the container up with zero restarts, capturing inspect+logs on failure |

## Verification Evidence

| command | expected | actual | status |
|---|---|---|---|
| `docker build --target runtime` | builds | built; entrypoint `/usr/local/bin/axon` | pass |
| `timeout 30s docker run --rm axon:verify-runtime --version` | version prints | `axon 7.2.19`, exit 0 | pass |
| `timeout 30s docker run --rm axon:verify-dev --version` | fails | exit 127, `exec ... failed` | pass |
| lint vs current workflows | pass | `OK: every workflow Dockerfile build pins a valid target stage.` | pass |
| lint vs pre-fix workflows | fail | exit 1, both files flagged with `dev-runtime` | pass |
| lint vs typo'd target | fail | exit 1, `not a stage in Dockerfile` | pass |
| lint vs unreadable Dockerfile | fail | exit 1, `cannot be read` | pass |
| `assert_live_container_stable` (healthy) | PASS | `PASS ... (got: true 0)` | pass |
| `assert_live_container_stable` (real crash-loop) | FAIL | `FAIL ... (got: true 6)` | pass |
| `assert_live_container_stable` (absent / docker down) | FAIL, distinguishable | `no such object` vs `no error output` | pass |
| `docker restart` on a healthy container | `RestartCount` unchanged | `true 0` before and after | pass |
| `gh pr checks 564` | green | 14 pass, 0 fail | pass |
| CI `image-build-smoke` step output | version prints | `axon 7.2.19` | pass |
| registry re-inspect after merge | `/usr/local/bin/axon` | `["/usr/bin/tini","--","/usr/local/bin/axon"]`, digest `77a5122c` | pass |
| fresh pull + run of published image | version prints | `axon 7.2.19`, exit 0 | pass |

## Risks and Rollback

- **Builder image size and build time increased.** `cmake`, `clang`, and
  `libclang-dev` are added to the `builder` stage only, not the `runtime` stage,
  so the shipped image is unaffected. The observed cold `image-build-smoke` build
  ran roughly 15 minutes, inside the job's 60-minute timeout.
- **`docker-image.yml` runs on `[self-hosted, unraid]`**, a different pool from
  where the `runtime` build was proven. It succeeded there on the merge run, so
  this is now observed rather than assumed.
- **Rollback**: reverting `f29ab9f9d` restores the previous workflows, which
  would republish the binary-less image on the next qualifying push. Rolling back
  only the harness or lint portions is safe in isolation; rolling back the
  Dockerfile dependency additions would break `--target runtime` builds.

## Decisions Not Taken

- **Manual `docker build --push` to fix the registry** — rejected; it would have
  produced a `:latest` not reproducible from any commit.
- **Reordering Dockerfile stages so `runtime` is last** — rejected; it makes the
  bug latent again for the next appended stage.
- **`yaml.safe_load` in the lint** — rejected; PyYAML is not available where the
  check runs.
- **`jq`-based single-inspect rewrite of the harness assertion** — rejected; see
  Technical Decisions.
- **Editing `docs/sessions/2026-07-08-incus-nested-docker-deployment.md`** —
  rejected; dated session logs are point-in-time records.
- **Deleting the merged feature branch** — deferred; the session is running
  inside its worktree.

## References

- PR: https://github.com/dinglebear-ai/axon/pull/564
- Merge commit: `f29ab9f9d`
- Prior record of the same bug, unfiled: `docs/sessions/2026-07-08-incus-nested-docker-deployment.md:63`
- Published image digests: pre-fix `sha256:ef95e89c`, post-fix `sha256:77a5122c`

## Open Questions

- Which historical `ghcr.io/dinglebear-ai/axon` tags exist, and are they all
  broken? Not determinable without `read:packages`; probes for `sha-f29ab9f9` and
  `v7.2.18` returned `notfound`, so even the tag naming is unconfirmed. Tracked
  as `axon_rust-em3uq`.
- Should the live harness be wired into CI at all? It needs Qdrant, TEI, Chrome,
  and real network access, so it likely cannot run unmodified on the standard
  pools. Tracked as `axon_rust-b1j6b`.
- Are any of the 14 plans directly under `docs/plans/` complete and movable? Not
  assessed this session.

## Next Steps

**Unfinished from this session:** none. The reported churn is fixed at its
source, the PR is merged, and the corrected image is published and verified.

**Follow-on, not started:**

1. Grant `read:packages` and enumerate the container package's tags, then decide
   whether to delete the broken historical ones (`axon_rust-em3uq`).
2. Decide the live harness's CI status — bounded subset in CI, or documented
   explicitly as a manual canary (`axon_rust-b1j6b`).

**Recommended immediate commands:**

```bash
# Confirm the published image is still good before any pull-based deploy
docker buildx imagetools inspect ghcr.io/dinglebear-ai/axon:latest \
  --format '{{json .Image.Config.Entrypoint}}'
```

```bash
# Delete this branch once no session is using its worktree
git worktree remove /home/jmagar/workspace/axon/.claude/worktrees/axon-live-test-restarts-96f3b9
```
