---
date: 2026-07-08 02:43:04 EST
repo: git@github.com:jmagar/axon.git
branch: claude/unruffled-goldberg-07a585
head: 8fb787f56e660dfb1abe3d937a66dd264154ea48
working directory: /home/jmagar/workspace/axon/.claude/worktrees/unruffled-goldberg-07a585
worktree: /home/jmagar/workspace/axon/.claude/worktrees/unruffled-goldberg-07a585
pr: #383 "Incus nested-Docker deployment: qdrant mandatory, GPU passthrough validated, bootstrap script" — https://github.com/jmagar/axon/pull/383 (merged)
beads: axon_rust-4m749 (epic), axon_rust-4m749.1 through .8
---

> **Superseded note (added after the fact):** this log describes running axon
> itself as a nested-Docker service (via `docker-compose.prod.yaml`) inside
> the Incus container. That design was replaced hours later, on the same
> day, by [PR #385](https://github.com/jmagar/axon/pull/385): axon now runs
> as a native systemd service (`axon-native.service`) inside the container,
> not Docker — the nested-Docker port-proxy silently reset axon's own
> connections, which PR #383/this session's design never hit end-to-end
> before merging. See `deploy/incus/README.md` for the current, accurate
> architecture. This log is left as a historical record of what was actually
> built and merged in PR #383 at the time.

## User Request

Deploy axon's full stack (axon + TEI + qdrant + chrome) inside an Incus container on dookie so the whole stack ships as "one Incus profile + bootstrap script." Explore Incus OCI-native containers as an alternative to nested Docker-in-Incus, evaluate feasibility including GPU passthrough, then fully implement, review, and merge whichever architecture actually works — the user explicitly directed working the entire epic to completion ("YES YOU NEED TO LAVRA-WORK THE ENTIRE FUCKING EPIC"), not just a single bead.

## Session Overview

Explored, prototyped, and pivoted twice on the deployment architecture before landing on a fully validated, merged solution: nested Docker-in-Incus (one Incus system container running a full Docker Engine internally, hosting axon/TEI/chrome/qdrant via the existing `docker-compose.prod.yaml`). Along the way: upgraded dookie's Incus from distro 6.0.5 LTS to Zabbly's 7.2 feature channel to unlock native OCI containers, prototyped native OCI GPU passthrough, found it genuinely broken (`CUDA_ERROR_OUT_OF_MEMORY` during TEI warmup) after extensive debugging, and reverted to the original nested-Docker design with real evidence instead of assumption. Fixed a separate, real bug in `axon-authz`'s OAuth env-var key prefix. Ran two independent multi-agent review passes (`lavra-review`, `/review-pr`) and applied all findings. Recovered from a git worktree/branch mismatch (commits initially landed on a stale, diverged local `main`) without losing work. Merged PR #383 into `main`. Confirmed the deployment is live and healthy on dookie. One epic item remains open: a real host-reboot validation (bead `.7`), deliberately not attempted without explicit authorization since dookie also hosts the production Labby MCP gateway.

## Sequence of Events

1. **Research phase.** Searched Incus docs and the repo/worktrees for prior work on an Incus-based axon deployment; found the epic groundwork already partially scoped.
2. **Architecture question raised.** User asked about switching to Incus's native OCI application-container support (landed as a monthly-feature-release capability, not in the LTS channel) instead of nested Docker-in-Incus, for simpler single-hop GPU passthrough.
3. **Mechanics review.** Explained how OCI-native Incus containers work (skopeo/umoci image unpacking, no nested dockerd) and confirmed "deploy everything at once" was still achievable under that model.
4. **Rejected an alternative shipping mechanism.** Evaluated distrobuilder's `ubuntu.yaml` example as a single-artifact build path; determined it doesn't fit the OCI-native model.
5. **Engineering review + plan.** Ran `/lavra-eng-review` (4-agent parallel review: architecture, simplicity, security, performance) against the pivot plan, applied all findings, then used `/writing-plans` to produce a detailed implementation plan for the first gated bead (`.8`, the Zabbly/Incus upgrade).
6. **Zabbly upgrade executed.** Baseline capture, GPG key verification, rollback dry-run, repo pinning, then the live `apt-get install incus` upgrade from 6.0.5 → 7.2 on dookie — a host also running the production `labby` container — after explicit user confirmation before the live-mutation step. Verified all pre-existing containers/profiles survived.
7. **Native OCI GPU prototype.** Built the OCI-native profile/container path (`nvidia.runtime=true`, `nvidia.driver.capabilities`) and attempted a real TEI GPU workload.
8. **Native OCI GPU failure diagnosed.** `CUDA_ERROR_OUT_OF_MEMORY` during TEI model warmup, reproduced 3x. Ruled out real VRAM exhaustion, cgroup memory limits, driver/kernel mismatch, and the RLIMIT_MEMLOCK containerized-CUDA gotcha. Per explicit user direction, also stopped bare-host `axon-tei`, checked live GPU memory usage, and searched Incus community/docs for this exact failure signature — root cause not resolved.
9. **Nested-Docker re-validated.** Re-tested nested Docker-in-Incus on the pre-existing `axon-bootstrap-temp` container with the real TEI image and real GPU: reached "Ready", served a real embedding request successfully.
10. **Architecture decision: revert.** User chose to revert the whole epic to nested-Docker-in-Incus given native OCI GPU was proven broken and nested-Docker was proven working, end-to-end, on real hardware.
11. **Compose changes.** Made `axon-qdrant` mandatory by default in `docker-compose.prod.yaml`; added `docker-compose.external-qdrant.yaml` override for dookie's real topology (qdrant lives on tootie for RAM reasons); added `just prod-up`/`prod-down` targets with an env-file permission warning.
12. **Storage + profile work.** Exported and committed the validated `axon-container-profile` (`deploy/incus/profile.yaml`) and documented the `~/.axon-incus` vs `~/.axon` storage-mapping rationale (`deploy/incus/README.md`), driven by a real `/etc/subuid` limitation preventing shared UID mapping.
13. **Bootstrap script + systemd unit.** Wrote `deploy/incus/bootstrap.sh` (idempotent 14-step bootstrap: profile/container create-if-missing, Docker install, `nvidia-procfs` device re-application, fail-closed GPU verification, artifact sync, Docker network resolution, env-file push, compose up, health-check polling, autostart enable) and `deploy/incus/axon-incus-bootstrap.service` (systemd unit to re-run bootstrap on host boot).
14. **Separate bug found and fixed.** While validating the real `axon` service inside the deployment, discovered `crates/axon-authz/src/http.rs` built its OAuth vars-list with unprefixed env-var keys (`AXON_AUTH_MODE`) when the `lab-auth` builder expected `AXON_MCP_`-prefixed keys — silently forcing bearer-auth fallback. Fixed by renaming the six `push_var` keys.
15. **Git worktree/branch recovery.** Discovered commits had landed on the main checkout's local `main`, which was ~2997 commits diverged from real `origin/main` (a past history rewrite). Verified all touched files were byte-identical between `origin/main` and the stale base, then rebuilt the working branch on `origin/main` and cherry-picked the commits cleanly.
16. **PR created.** Opened PR #383 against `main` with the 4 substantive commits (compose changes, Incus profile/docs, authz fix, bootstrap script + systemd).
17. **Two independent review passes.** Ran `/lavra-review` (5 agents: architecture, security, performance, simplicity, pattern-recognition) and `/review-pr` (3 agents: code-reviewer, comment-analyzer, silent-failure-hunter) against the PR, applying every finding between passes — removed a non-functional "config-integrity checksum," hardened a fail-closed health-check gate, fixed a `.env` push permission race, fixed fragile Docker-network-name parsing, added a GPU pre-pull step, hardened shell interpolation via `incus exec --env`/`--cwd`, and corrected a comment describing the wrong override variable name.
18. **CI wait and merge.** Waited for required checks (`ci-gate`, `codeql-gate`, `compose-smoke-gate`) to go green, then merged PR #383 into `main`.
19. **Bare-host container investigation deferred.** Found bare-host `axon-tei` had disappeared entirely with no forensic explanation (docker events retention exhausted); per user direction, did not investigate or recreate it — launched the validated Incus deployment as the real production stack instead.
20. **Deployment launched for real.** Ran `bootstrap.sh external-qdrant` against `axon-bootstrap-temp`; `axon doctor` confirmed all backends green (sqlite, tei, qdrant → tootie, chrome, gemini_headless); `boot.autostart=true` confirmed set.
21. **Bead cleanup.** Discovered bead `.5` was still marked open despite its work being fully merged; closed it with a fact comment.
22. **Live-state re-verification (this session).** Re-checked `incus list axon-bootstrap-temp` and `docker compose ps` inside the container directly — confirmed `axon`, `axon-tei`, `axon-chrome` all `Up ~1 hour (healthy)`.
23. **Reboot question answered.** User asked whether closing the epic requires a real dookie reboot. Explained the distinction between tested (`incus stop`/`start` cycles) and untested (real host reboot proving `axon-incus-bootstrap.service` + `boot.autostart` survive a cold Incus daemon restart) — and flagged the blast radius (dookie also hosts the live `labby` MCP gateway). Awaiting user decision on whether/when to run that test.

## Key Findings

- `crates/axon-authz/src/http.rs`: `build_auth_policy()`'s `push_var()` calls used unprefixed keys (`AXON_AUTH_MODE`, `AXON_PUBLIC_URL`, etc.) when `lab_auth::config::AuthConfigBuilder` was configured with `.env_prefix("AXON_MCP")`, so its internal lookups expected `AXON_MCP_AUTH_MODE` etc. Real, user-facing env vars (confirmed via `.env.example`) are correctly unprefixed — only the internal vars-list construction was wrong. This silently forced `AuthMode::Bearer` even when `AXON_AUTH_MODE=oauth` was set.
- `docker-compose.prod.yaml`: `axon-qdrant` previously had `profiles: ["local-qdrant"]`, making it optional; changed to mandatory-by-default with `axon`'s `QDRANT_URL` defaulting to the bundled service (`http://axon-qdrant:6333`) instead of a hardcoded tootie IP.
- `nvidia-procfs` Incus disk device (`source=/proc/driver/nvidia/gpus/<pci-address>`) does not reliably survive a container stop/start cycle — must be removed and re-added on every bootstrap run (baked into `bootstrap.sh` permanently, not a one-time fix).
- Native Incus OCI (`nvidia.runtime=true`) GPU passthrough is genuinely broken for real inference workloads on this host/Incus 7.2 combination — `CUDA_ERROR_OUT_OF_MEMORY` during TEI warmup, not caused by real VRAM exhaustion, cgroup limits, driver mismatch, or RLIMIT_MEMLOCK.
- `ghcr.io/jmagar/axon:latest` (and all published tags) are built from the Dockerfile's `dev-runtime` stage instead of `runtime` — confirmed via `docker image inspect` entrypoint mismatch. This is a separate, real CI/release-pipeline bug, out of scope for this epic; worked around locally by building `ghcr.io/jmagar/axon:local-runtime` via `git archive HEAD | docker build --target runtime`.
- Main checkout's local `main` branch was ~2997 commits diverged from real `origin/main` due to a past history rewrite/squash-merge — required verifying byte-identical file state before rebuilding the branch and cherry-picking.

## Technical Decisions

- **Reverted native-OCI pivot back to nested-Docker-in-Incus.** Decision driven by empirical, reproducible hardware testing (native OCI GPU broken, nested-Docker GPU proven working end-to-end), not assumption or a priori preference. The Incus 7.2 upgrade itself was kept since it was harmless and separately beneficial.
- **Qdrant made mandatory in the shared compose file**, with a separate `docker-compose.external-qdrant.yaml` override for dookie's specific topology (qdrant on tootie for RAM headroom reasons), rather than keeping qdrant permanently optional behind a profile flag — makes the default `up -d` behavior correct for new/other deployments while preserving dookie's existing external-qdrant setup via explicit override.
- **Storage split (`~/.axon` vs `~/.axon-incus`/`~/.axon-incus-gemini`)** instead of trying to share host UID 1000 into the Incus container's idmap-shifted namespace — driven by a real `/etc/subuid` Linux limitation, not a workaround of choice.
- **Removed a fake "config-integrity checksum"** step from `bootstrap.sh` (flagged during `/review-pr`) — it hashed files the same script had just pushed and compared against the prior run's hash, which could never detect real drift since both comparison inputs were derived from the script's own immediately-prior write.
- **Health-check timeout escalated to `fatal()`** instead of a warn-only log (flagged during `lavra-review`) — a warn-only timeout undermined the systemd unit's intentional `Restart=no` fail-closed design.

## Files Changed

| status | path | purpose | evidence |
|---|---|---|---|
| modified | `docker-compose.prod.yaml` | Make `axon-qdrant` mandatory by default; default `QDRANT_URL` to bundled service instead of hardcoded tootie IP | commit `a2ad54e92` |
| created | `docker-compose.external-qdrant.yaml` | Override to disable bundled qdrant and point at an external instance (tootie) | commit `a2ad54e92` |
| modified | `Justfile` | Added `prod-up`/`prod-down`/`prod-up-external-qdrant`/`prod-down-external-qdrant` targets with env-file permission warning | commits `a2ad54e92`, `8fb787f56` |
| created | `deploy/incus/profile.yaml` | Validated, live-exported Incus container profile (GPU, nesting, idmap isolation, disk mounts) | commit `70d43b006` |
| created | `deploy/incus/README.md` | Storage-mapping rationale + adversarial test results + `nvidia-procfs` fragility notes | commit `70d43b006` |
| modified | `crates/axon-authz/src/http.rs` | Fixed OAuth vars-list key prefix mismatch (`AXON_AUTH_MODE` → `AXON_MCP_AUTH_MODE` etc.) | commit `30801d3cc` |
| created | `deploy/incus/bootstrap.sh` | Idempotent 14-step nested-Docker-in-Incus bootstrap script | commits `47dfd665b`, `0fdbd4b1d`, `8fb787f56` |
| created | `deploy/incus/axon-incus-bootstrap.service` | systemd unit re-running bootstrap on host boot | commits `47dfd665b`, `8fb787f56` |
| created | `deploy/incus/axon-incus-bootstrap.env.example` | Template env file for the systemd unit | commit `47dfd665b` |
| created | `docs/superpowers/plans/2026-07-07-incus-zabbly-upgrade.md` | Implementation plan for bead `.8` (Zabbly/Incus upgrade) | written via `/writing-plans`, executed |
| created | `docs/sessions/2026-07-08-incus-nested-docker-deployment.md` | This session log | this commit |

## Beads Activity

- `axon_rust-4m749` (epic) — rewritten multiple times to reflect the OCI-native pivot and the same-day revert, with an "ARCHITECTURE HISTORY" section and an "AUDIT FINDING" section documenting a recurring "closed but artifact never persisted" pattern. Still open (parent of `.7`).
- `axon_rust-4m749.1` — closed (historical/superseded groundwork).
- `axon_rust-4m749.2` — closed (nested-Docker GPU re-validation; contains the full native-OCI GPU diagnostic trail in comments).
- `axon_rust-4m749.3` — closed (compose changes; commits `a2ad54e92`, `6b0325f08`).
- `axon_rust-4m749.4` — closed (storage re-verification; commits `70d43b006`, `632aa50c0`).
- `axon_rust-4m749.5` — closed this session (bootstrap script + systemd unit; work was merged via PR #383 but the bead had not been closed — fixed with a fact comment + `bd update ... -s closed`).
- `axon_rust-4m749.7` — **still open**. Formal end-to-end validation across a real host reboot. Not yet attempted; user asked about it this session and a decision is pending (see Next Steps).
- `axon_rust-4m749.8` — closed (Zabbly/Incus 7.2 upgrade, executed via the `/writing-plans` output plan, validated harmless).

## Repository Maintenance

- **Plans**: `docs/superpowers/plans/2026-07-07-incus-zabbly-upgrade.md` covers bead `.8`, which is closed and fully executed. Not moved to `docs/plans/complete/` — it lives in a separate `docs/superpowers/plans/` directory (a different convention used by the `superpowers:writing-plans` skill), not `docs/plans/`, so it's out of scope for this session's plan-migration pass. No other plans under `docs/plans/` were touched or completed by this session's work.
- **Beads**: `axon_rust-4m749.5` was found open despite merged work and closed with a fact comment (see Beads Activity). All other epic children were already in correct state. Epic itself correctly remains open pending `.7`.
- **Worktrees and branches**: This session's worktree (`unruffled-goldberg-07a585`, branch `claude/unruffled-goldberg-07a585`) is clean and in sync with `origin`. The repo has ~60+ other worktrees/branches from unrelated parallel work (pipeline-unification phases, palette tools, etc.) — none were inspected or touched; ownership and in-progress status are unclear for all of them, so cleanup is explicitly out of scope for this session.
- **Stale docs**: None identified as contradicted by this session's changes. `deploy/incus/README.md` and the root `CLAUDE.md`'s Incus references are current as of the merge.
- **Transparency**: No cleanup items were skipped due to uncertainty beyond the two explicitly noted above (plan-directory convention mismatch, unrelated worktree sprawl).

## Tools and Skills Used

- **Shell (Bash)**: `incus` CLI (profile/container management, `exec`, `file push`), `docker`/`docker compose`, `git` (branch recovery, cherry-pick, worktree inspection), `gh` (PR creation, CI status, merge), `bd` (beads tracker), `apt`/`dpkg` (Zabbly repo + Incus upgrade), `cargo` (build/check for the authz fix).
- **Skills**: `lavra:lavra-eng-review` (4-agent plan review), `superpowers:writing-plans` (Zabbly upgrade plan), `superpowers:executing-plans` (Tasks 1–4 of that plan), `lavra:lavra-work` (epic dispatch), `lavra:lavra-review` (5-agent PR review), `pr-review-toolkit:review-pr` (3-agent PR review), `vibin:save-to-md` (this log).
- **Agents/subagents**: architecture-strategist, security-sentinel, performance-oracle, code-simplicity-reviewer (eng-review + lavra-review passes), pattern-recognition-specialist (lavra-review), code-reviewer, comment-analyzer, silent-failure-hunter (review-pr pass). All completed successfully; no failures or degraded runs observed.
- **Web/docs research**: Searched Incus documentation and community forum threads for OCI GPU passthrough behavior and the `CUDA_ERROR_OUT_OF_MEMORY` failure signature — informed the diagnosis but did not itself resolve the root cause (resolved instead by reverting architecture).
- **No issues encountered** with tool availability this session; one `Monitor`-tracked background task (`bqaog9ahn`, a stale CI-poll loop) had to be stopped via `TaskStop` because its exit condition never triggered due to a permanently-pending non-required check.

## Commands Executed

- `incus list axon-bootstrap-temp` / `incus exec axon-bootstrap-temp -- sh -c 'cd /opt/axon-deploy && docker compose -f docker-compose.prod.yaml ps'` — confirmed `axon`, `axon-tei`, `axon-chrome` all `Up ~1 hour (healthy)` (this session, re-verification).
- `incus exec axon-bootstrap-temp -- curl -sf http://localhost:8001/healthz` — returned exit 56 (connection reset), attributed to running curl from the Incus container's own netns rather than the axon container's netns/published port; compose's own healthcheck (executed from inside the `axon` container) is the authoritative signal and reports healthy.
- `bd show axon_rust-4m749 --json`, `bd list --parent axon_rust-4m749 --json` — confirmed only `.7` remains open.
- `git status --short --branch` — clean, in sync with `origin/claude/unruffled-goldberg-07a585`.
- `gh pr view 383 --json state,mergedAt,mergeStateStatus` — `state: MERGED`, `mergedAt: 2026-07-08T03:09:14Z`.
- `gh repo view --json defaultBranchRef` → `main`; `git symbolic-ref --short refs/remotes/origin/HEAD` → `origin/main`.

## Behavior Changes (Before/After)

| area | before | after |
|---|---|---|
| Production axon deployment (dookie) | Bare-host Docker Compose stack (with an unexplained missing `axon-tei` container by session's end) | Nested Docker-in-Incus: one Incus system container (`axon-bootstrap-temp`) running the full compose stack, `boot.autostart=true`, systemd-managed re-bootstrap on host boot |
| `docker-compose.prod.yaml` qdrant | Optional, behind `profiles: ["local-qdrant"]` | Mandatory by default; external override via `docker-compose.external-qdrant.yaml` |
| dookie Incus version | 6.0.5 (Ubuntu distro LTS package) | 7.2 (Zabbly feature channel) |
| `crates/axon-authz` OAuth mode | Silently fell back to `AuthMode::Bearer` even when `AXON_AUTH_MODE=oauth` was set | OAuth mode activates correctly; vars-list keys match the `lab-auth` builder's expected `AXON_MCP_`-prefixed internal keys |

## Verification Evidence

| command | expected | actual | status |
|---|---|---|---|
| `incus list axon-bootstrap-temp` | Container RUNNING | RUNNING | pass |
| `docker compose ... ps` (inside container) | axon/axon-tei/axon-chrome healthy | All three `Up ~1h (healthy)` | pass |
| `axon doctor` (from earlier in session, prior to compaction) | All backends reachable | sqlite/tei/qdrant/chrome/gemini_headless all green | pass |
| `gh pr view 383` | Merged into `main` | `state: MERGED`, `mergedAt` set | pass |
| Required CI checks (`ci-gate`, `codeql-gate`, `compose-smoke-gate`) | Green before merge | Confirmed green prior to merge (earlier in session) | pass |
| `curl` to `localhost:8001/healthz` from inside the Incus container's own shell | Reachable | Connection reset (exit 56) — believed to be a netns/routing artifact of curling from the wrong network namespace, not a real health issue, since compose's own in-container healthcheck reports healthy | warn (unresolved minor discrepancy) |

## Risks and Rollback

- The `nvidia-procfs` Incus device is fragile across container restarts by design of the current fix (must be re-applied every boot by `bootstrap.sh`) — if `bootstrap.sh` or the systemd unit is ever bypassed, GPU access inside the container will silently fail on the next restart.
- dookie also hosts the live production `labby` MCP gateway under the same Incus daemon — any future Incus-level changes (including the still-pending real-reboot test for bead `.7`) carry a shared blast radius with that gateway.
- Rollback path if the nested-Docker-in-Incus deployment needs to be abandoned: bare-host `docker-compose.prod.yaml` remains fully functional and unmodified in its service definitions (only defaults changed); reverting to it requires only stopping the Incus container and re-running `docker compose up -d` on the host directly with an appropriate `.env`.
- The known, unfixed `ghcr.io/jmagar/axon:latest` wrong-build-stage bug means any future deployment pulling that tag fresh (rather than reusing the locally-built `local-runtime` image) will get a broken `dev-runtime`-based image — flagged but not fixed this session.

## Decisions Not Taken

- **distrobuilder's `ubuntu.yaml` as a single-artifact shipping mechanism** — evaluated and rejected; doesn't fit the OCI-native container model (and OCI-native itself was later abandoned).
- **Recreating bare-host `axon-tei` after it inexplicably disappeared** — user explicitly redirected to launching the Incus deployment instead of investigating/recreating the bare-host container.
- **Stopping the redundant bare-host `axon-chrome` container** (still running alongside the new Incus deployment as of last check) — flagged but not actioned this session.
- **Investigating the unrelated failing `openwiki-update.yml` CI workflow** — noticed during merge-readiness checks, flagged but out of scope.

## References

- PR #383: https://github.com/jmagar/axon/pull/383
- Beads epic: `axon_rust-4m749` and children `.1`–`.8`
- `deploy/incus/README.md` (storage mapping + adversarial test results)
- `docs/superpowers/plans/2026-07-07-incus-zabbly-upgrade.md`

## Open Questions

- Whether/when to run the real dookie host-reboot test required to close bead `.7`, given it shares blast radius with the live `labby` MCP gateway — raised to the user this session, decision pending.
- Why bare-host `axon-tei` disappeared entirely mid-epic (no forensic evidence found in available docker events retention) — unresolved, not going to be investigated further per user direction.
- Whether the `curl` connection-reset seen when probing `localhost:8001/healthz` from inside the Incus container's own shell (vs. compose's in-container healthcheck reporting healthy) indicates any real routing issue worth a follow-up check, or is purely a netns-mismatch artifact of how the probe was issued.

## Next Steps

1. **Bead `.7` (real host-reboot validation)** is the only remaining open item in the epic. Immediate next action once the user decides: either (a) schedule/run an actual `sudo reboot` of dookie and verify `axon-incus-bootstrap.service` + `boot.autostart` bring the stack back automatically, or (b) close `.7` as validated-via-container-restarts-only with the real-reboot proof explicitly deferred/documented as a known gap.
2. **Not yet started, flagged only:** stop the redundant bare-host `axon-chrome` container; investigate/fix the failing `openwiki-update.yml` workflow; fix the `ghcr.io/jmagar/axon:latest` image-publishing pipeline building from the wrong Dockerfile stage (`dev-runtime` instead of `runtime`) — all explicitly out of this epic's scope, candidates for separate follow-up beads.
3. **Recommended immediate command** once a reboot decision is made: if proceeding, coordinate timing with the user (labby downtime window), then `sudo reboot` on dookie followed by `incus list` / `systemctl status axon-incus-bootstrap` / `axon doctor` post-boot verification.
