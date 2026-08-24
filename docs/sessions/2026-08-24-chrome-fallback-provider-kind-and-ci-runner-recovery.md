---
date: 2026-08-24 18:33:22 EST
repo: git@github.com:dinglebear-ai/axon.git
branch: claude/systematic-debugging-issue-cb9d95
head: 61fbb7916
working directory: /home/jmagar/workspace/axon/.claude/worktrees/systematic-debugging-issue-cb9d95
worktree: /home/jmagar/workspace/axon/.claude/worktrees/systematic-debugging-issue-cb9d95
pr: |
  #582 fix(render): fall back to local Chrome when the remote CDP probe fails — https://github.com/dinglebear-ai/axon/pull/582
  #583 fix(jobs): widen provider_kind CHECK to the full ProviderKind registry — https://github.com/dinglebear-ai/axon/pull/583
  #585 ci: build Docker jobs on hosted runners + fill in the 7.2.23 changelog — https://github.com/dinglebear-ai/axon/pull/585
  dinglebear-ai/ci-runner-farm#82 fix(scaleset): reassert commanded eligibility across restarts — https://github.com/dinglebear-ai/ci-runner-farm/pull/82
beads: axon_rust-nkh6y (closed), axon_rust-a0qd7 (closed), axon_rust-urwjh (created)
---

# Chrome fallback, provider_kind CHECK, and CI runner-fleet recovery

## User request

Started with a screenshot: Labby's "Edit server" dialog failing to connect to
`axon.dinglebear.ai/mcp` with `internal_error: dynamic registration: Registration
failed: HTTP 422 ... redirect URI must target a loopback host, match the native
callback endpoint, or match an allowed redirect pattern`. Request was to debug it
systematically. The session then expanded through follow-on requests: fix the
Chrome fallback trap, fix a `provider_kind` CHECK failure, merge everything,
investigate stuck CI runs, fix the broken Docker publish, and deploy.

## Session overview

Six distinct problems were diagnosed and five were fixed and shipped. Four pull
requests merged (three in axon, one in ci-runner-farm), both axon fixes were
deployed to the live host service and verified end-to-end against production
data, and a CI runner-fleet outage caused mid-session was diagnosed and restored.

Two diagnoses in this session were initially wrong and were corrected by
evidence: the Docker publish failure was blamed on PR #578's pool routing (it was
not), and the DinD flag was assumed to be an accidental regression (it is
required by a fail-closed security gate). Both corrections are recorded below.

## Sequence of events

1. **Labby OAuth 422.** Reproduced the exact error with a direct `POST /register`
   against the live axon endpoint. Traced the rejection to the vendored lab-auth
   allowlist check; fixed by adding Labby's callback to
   `AXON_ALLOWED_REDIRECT_URIS` and restarting `axon.service`. Verified the
   registration then returns HTTP 200 and that an unrelated URI is still rejected.
2. **Chrome deployment audit.** Established that the `axon` Incus container is
   stopped and axon runs host-native, and that `AXON_CHROME_REMOTE_URL` pointed at
   port 9222 — occupied by an unrelated Codex session's scratch browser.
3. **Chrome container vs fallback research.** A three-agent workflow compared the
   dedicated Chrome container against the Spider local-launcher fallback, with an
   adversarial verify pass over each feature-gate claim. This surfaced the real
   defect: a *set-but-dead* remote does not fall back to local Chrome at all.
4. **PR #582.** Fixed the dead-remote degradation; built and started the
   `axon-chrome` container; repointed axon at its management endpoint.
5. **PR #583.** Fixed the `provider_kind` CHECK constraint that was degrading
   baseline graph upserts in production.
6. **CI runner investigation.** A three-agent evidence sweep across the tootie
   fleet, GitHub API forensics, and the provisioner design explained why some
   `ci-pool-ops` jobs sat queued forever. Fixed live, then made durable via
   ci-runner-farm#82.
7. **Docker publish.** Diagnosed (twice — see corrections), attempted a DinD
   restore that tripped a fail-closed gate and took the fleet down, restored
   service, then fixed properly by moving both Docker-dependent jobs to hosted
   runners in PR #585.
8. **Deploy.** Built from main, installed, restarted, and verified both fixes live.

## Key findings

- **Dead remote Chrome never fell back.** `bootstrap_chrome_runtime` logged
  "falling back to local Chrome launcher" but never cleared `cfg.chrome_remote_url`
  (`crates/axon-adapters/src/web_engine/chrome_bootstrap.rs`), so spider redialed
  the dead endpoint ~11 times and then degraded to a **browserless HTTP crawl**.
  A true local launch only ever happened when the variable was *unset*.
  `configure_spider_browser` (`crates/axon-adapters/src/web_engine/browser.rs:86`)
  had the same defect with no probe at all.
- **`provider_kind` CHECK lagged the enum by 9 variants.** Migration 0004 allowed
  11 kinds; `ProviderKind` (`crates/axon-api/src/source/enums/runtime.rs:112`) had
  grown to 20. The `sqlite-graph` scheduler
  (`crates/axon-services/src/context/target_runtime/schedulers.rs:97`) reserves as
  `ProviderKind::Graph`, so every baseline graph upsert failed with SQLite error
  275 and returned a degraded summary.
- **CI queue starvation was dual-backend routing.** `ci-pool-*` labels were served
  simultaneously by classic tootie listeners *and* a GitHub runner scale set in the
  same runner group (id 4). Jobs routed to an unhealthy scale-set session showed
  `runner_id: 0` forever; sibling jobs with identical labels assigned in 1–2s.
  Plain cancel could not land (no runner to receive it); only force-cancel worked.
- **Docker publish: two wrong diagnoses, then the real one.** It was *not* PR
  #578's pool routing — the same runner (`tootie-ci-runner-system-1`) succeeded on
  08-21 and 08-23 *after* #578 merged. It was *not* an accidental DinD regression
  either. ci-runner-farm 1.11.1 added a fail-closed gate that refuses to start
  privileged (DinD) runners while org-scoped targeting cannot prove the runner
  group excludes public repos. axon is public, so `DIND=true` cannot start, and
  no self-hosted pool can offer a Docker daemon.
- **auto-tag leaves main bumped-but-untagged.** `7.2.23` is in `Cargo.toml` on
  main but was never tagged. auto-tag depends on a CI-produced release-plan
  artifact that only exists when CLI shipping paths change; it was skipped on
  #584 and found no plan on #585. Filed as `axon_rust-urwjh`.

## Technical decisions

- **Tri-state probe result rather than "clear on failure".** `resolve_cdp_ws_url`
  returns `None` both for a dead remote *and* for the normal in-Docker path, so a
  naive clear-on-`None` would have broken Docker deployments. Added
  `remote_unreachable`, set only when the probe actually ran and exhausted retries.
- **Compiler-enforced enum/migration coupling.** Beyond widening the CHECK,
  `provider_kind_registry_is_exhaustive` is a wildcard-free `match` over
  `ProviderKind`, so adding a variant breaks the build until a widening migration
  ships. Chosen over a runtime assertion so drift cannot recur silently.
- **Controller-scoped eligibility fix.** The chosen option was described as
  "derive eligibility from `backend-transition.json`", but reading the code showed
  the controller has no path to that file (it is tootie-local Unraid plugin state).
  Implemented the equivalent guarantee instead — persist the controller's own
  commanded intent and reassert it — rather than inventing cross-host plumbing.
  This scope adaptation was stated explicitly in the PR.
- **Hosted runners over bypassing the security gate.** The gate is deliberate
  first-party code; overriding it via
  `CRF_I_ACCEPT_UNRESTRICTED_PRIVILEGED_RUNNER_HOST_ROOT_RISK` for a public repo
  would have undermined its purpose. Both Docker jobs moved to `ubuntu-latest`,
  with `timeout-minutes` raised 30 → 60 based on the measured 17-minute cold build.
- **Built from main rather than a release artifact.** No `v7.2.23` tag exists
  (see the auto-tag gap), so there was no release artifact to install.

## Files changed

| status | path | previous path | purpose | evidence |
|---|---|---|---|---|
| modified | `crates/axon-adapters/src/web_engine/chrome_bootstrap.rs` | — | tri-state probe outcome + `apply_bootstrap_outcome` | PR #582 |
| created | `crates/axon-adapters/src/web_engine/chrome_bootstrap_tests.rs` | — | 6 tests for probe/persist branches | PR #582 |
| modified | `crates/axon-adapters/src/web_engine/browser.rs` | — | skip connection on confirmed-dead remote | PR #582 |
| modified | `crates/axon-adapters/src/web_engine/browser_tests.rs` | — | ws passthrough + dead-remote tests | PR #582 |
| modified | `crates/axon-adapters/src/providers/chrome_render.rs` | — | use shared outcome helper | PR #582 |
| modified | `crates/axon-adapters/src/web_engine/engine.rs` | — | export `cdp_probe_skipped_in_docker` | PR #582 |
| modified | `crates/axon-adapters/src/web_engine/engine/runtime.rs` | — | extract Docker-skip predicate | PR #582 |
| created | `crates/axon-jobs/src/migrations/0009_provider_scheduler_kind_registry.sql` | — | widen `provider_kind` CHECK to 20 kinds + legacy `storage` | PR #583 |
| modified | `crates/axon-jobs/src/migrations.rs` | — | register migration 9 | PR #583 |
| modified | `crates/axon-jobs/src/migration-checksums.txt` | — | pin 0009 checksum | PR #583 |
| modified | `crates/axon-jobs/src/scheduler_tests.rs` | — | registry round-trip + exhaustiveness witness | PR #583 |
| modified | `xtask/src/schemas/database_defs_tests.rs` | — | expect nine jobs migrations | PR #583 |
| modified | `docs/reference/runtime/database-schema.json` / `.md`, `docs/reference/runtime/schema.md`, `docs/reference/generated/memory.md`, `docs/reference/source-input-manifest.json`, `xtask/tests/fixtures/schemas/database/snapshots/database-schema.json` | — | regenerated contracts | `cargo xtask generated-contracts refresh` |
| modified | `.github/workflows/docker-image.yml` | — | build job → `ubuntu-latest`, timeout 30→60 | PR #585 |
| modified | `.github/workflows/compose-smoke.yml` | — | `image-build-smoke` → `ubuntu-latest` | PR #585 |
| modified | `CHANGELOG.md` | — | fill empty `[7.2.23]` section | PR #585 |
| created | `controller/lib/crf_controller/scaleset_eligibility.ex` (ci-runner-farm) | — | durable eligibility state | ci-runner-farm#82 |
| created | `controller/test/scaleset_eligibility_test.exs` (ci-runner-farm) | — | persistence tests | ci-runner-farm#82 |
| modified | `controller/lib/crf_controller/scaleset_client.ex` (ci-runner-farm) | — | reassert on start + periodic retry | ci-runner-farm#82 |
| modified | `controller/test/scaleset_client_test.exs` (ci-runner-farm) | — | reassert + retry tests | ci-runner-farm#82 |
| created | `docs/sessions/2026-08-24-chrome-fallback-provider-kind-and-ci-runner-recovery.md` | — | this session note | this commit |

Non-repo files changed on hosts: `/home/jmagar/.axon/.env`
(`AXON_ALLOWED_REDIRECT_URIS`, `AXON_CHROME_REMOTE_URL`), `/usr/local/bin/axon`
(binary replaced, backup at `/usr/local/bin/axon.bak-pre-7223-20260824T223204Z`),
and tootie's `/boot/config/plugins/ci-runner-farm/ci-runner-farm.cfg` (DinD
toggled true then reverted to false; backup
`ci-runner-farm.cfg.bak-pre-dind-restore-20260824T160938Z`).

## Beads activity

| id | title | actions | final status | why it mattered |
|---|---|---|---|---|
| `axon_rust-nkh6y` | Dead remote Chrome degrades renders to raw HTTP instead of local launcher | created, claimed, closed | closed | Tracked PR #582; closed once merged |
| `axon_rust-a0qd7` | provider_kind CHECK rejects graph (and other current enum kinds) in scheduler reservations | created, claimed, closed | closed | Tracked PR #583. Was still `in_progress` after merge and was found and closed during the wrap-up sweep |
| `axon_rust-urwjh` | auto-tag leaves main bumped-but-untagged when no CLI shipping paths change | created | open | Captures the release-pipeline gap found while deploying; not fixed this session |

No beads were created in ci-runner-farm; that work is tracked by PR #82.

## Repository maintenance

**Plans.** `docs/plans/` holds 16 non-complete plans. None relate to this
session's work and none were completed by it, so none were moved. Evidence:
`ls docs/plans/*.md`; no plan file was read or modified during the session.

**Beads.** Three beads touched (table above). `axon_rust-a0qd7` was left
`in_progress` after its PR merged and was corrected during this pass.

**Worktrees and branches.** Removed `.worktrees/fix-provider-kind` (PR #583
merged) via `git worktree remove`. Deleted local `claude/fix-provider-kind-check`
and `claude/bump-7.2.23` after confirming their remotes were auto-deleted on
merge (`git fetch --prune`, then `git branch -vv` showing `: gone`). Remote
branches for #582/#583/#585 were already auto-deleted by GitHub.

Deliberately left alone:
- `.claude/worktrees/systematic-debugging-issue-cb9d95` — the live session
  worktree; cannot be removed from within itself.
- `.worktrees/bump-7223` (branch `deploy-main`) — retained because its
  `target/release/axon` is the exact tree the deployed binary was built from,
  which is useful for verification or rebuild before a tagged release exists.
- `.claude/worktrees/connected-tools-not-exposed-0e199c`,
  `.claude/worktrees/pr-581-review-08c523`, both `.worktrees/codex/*` — other
  sessions' work, unknown ownership.
- Stale `codex/live-source-benchmark`, `codex/release-v7.2.20`,
  `codex/speed-ci-contracts` (remotes gone) — pre-existing and not this session's
  to reap.

**Stale docs.** Searched `docs/` and `CLAUDE.md` for `ci-pool-system` and found
zero references, so the Docker runner change made no documentation stale. No
other doc was contradicted by the session's changes. The auto-tag gap is captured
in `axon_rust-urwjh` rather than edited into the release docs, because the
correct fix is not yet known.

**Transparency.** The DinD restore attempt took the tootie runner fleet down for
roughly six minutes (all 18 runners deregistered, `cmd_start` blocked by the
security gate). Service was restored by reverting `DIND` to `false` and running
`runner-farm.sh start`; all 18 runners returned online and a dispatched
`repository-contract` run completed successfully in ~36s. The tootie config now
differs from its pre-session state only in the backup files added.

## Tools and skills used

- **Skills.** `superpowers:systematic-debugging` (drove the root-cause-first
  approach on the OAuth 422 and every subsequent bug); `vibin:save-to-md` (this
  note).
- **Workflow tool (multi-agent).** Two fan-outs: a 3-agent Chrome
  container-vs-fallback comparison with an 8-agent adversarial verify phase
  (11 agents, ~1.5M tokens), and a 3-agent CI runner-fleet evidence sweep. One
  verify agent correctly refuted a claim about the AutoSwitch thin-page path,
  which was then corrected before reporting.
- **Shell.** Extensive `git`, `gh`, `cargo`, `sqlite3`, `systemctl`,
  `journalctl`, `docker`, `ssh` to tootie, and the ci-runner-farm plugin CLI.
- **Monitor / background tasks.** Used for CI watches and long builds. One
  monitor produced a **false green** (see Errors) and was replaced with one that
  watches the run object rather than the check list.
- **MCP.** `ccd_session` `spawn_task` / `dismiss_task` for out-of-scope findings.
  Several MCP servers disconnected and reconnected mid-session; none were needed
  at those moments. Numerous plugin MCP servers remain unauthenticated and were
  not used.
- **Issues encountered.** `gh api .../packages/...` returned HTTP 403 (token
  lacks `read:packages`), so ghcr image staleness was inferred from workflow run
  history instead. `bd list --status=open` did not surface a newly created bead;
  `bd search` did.

## Commands executed

| command | result |
|---|---|
| `curl -X POST https://axon.dinglebear.ai/register -d '{"redirect_uris":["https://dinglebear.ai/auth/upstream/callback"]}'` | HTTP 422 before fix; HTTP 200 with `client_id` after |
| `cargo test -p axon-adapters --lib` | 789 passed |
| `cargo test -p axon-jobs --lib` | 191 passed |
| `mix test` (ci-runner-farm controller) | 149 passed |
| `cargo xtask generated-contracts refresh` / `check` | 8 artifacts + 16 docs written; check passed |
| `cargo xtask check-release-versions --base origin/main --head HEAD --mode pr` | passed for #583 and #585 |
| `runner-farm.sh apply-config <rev> <staged>` | `{"ok":true,...,"DIND":"true"}` |
| `runner-farm.sh restart` | stop succeeded; **start failed** on the privileged-runner security gate |
| `runner-farm.sh start` (after reverting DinD) | `fleet up: 18 runner(s)` |
| `sudo install -m 0755 target/release/axon /usr/local/bin/axon` | installed, `axon 7.2.23` |
| `axon https://example.com --scope page --wait true` | `Graph: 4 nodes 2 edges 2 evidence` |

## Errors encountered

- **CI caught a Windows-portability bug in my own test.** `ScaleSetEligibility`'s
  identity test used a hardcoded `/tmp/eligibility.json`, which is
  `:volumerelative` (not `:absolute`) on Windows, so the path check fired before
  the identity check under test. Fixed by deriving the path from
  `System.tmp_dir!()`. Production code was unaffected — `eligibility_path` is
  derived from the already-validated absolute `socket_path`.
- **Fleet outage during the DinD restore.** `runner-farm.sh restart` stopped and
  deregistered all 18 runners, then `cmd_start` refused to start privileged
  runners because the org runner group covers public repos. Restored by reverting
  `DIND=false` and starting the fleet; verified 18/18 online plus a green
  dispatched run.
- **A monitor reported a false green.** It checked for *pending rows* in
  `gh pr checks`; a workflow that has not started produces no rows at all, so the
  required `ci-gate` was absent rather than pending. Replaced with a monitor that
  polls the run object's status directly.
- **A stale queued CI run blocked its successor for over an hour.** After a
  force-push, the superseded run stayed `queued` holding ci.yml's concurrency
  group (`cancel-in-progress` does not reap queued runs), and a plain cancel could
  not land. Force-cancel cleared it.
- **Incomplete first Docker fix.** Moving only `docker-image.yml` left
  `image-build-smoke` failing identically; it had been silently skipping. A
  programmatic sweep of every workflow for daemon-needing jobs on `ci-pool-*`
  runners found exactly two, both then fixed.

## Behavior changes (before/after)

| area | before | after |
|---|---|---|
| Labby → axon OAuth | DCR rejected with HTTP 422 | Registration succeeds; unrelated URIs still rejected |
| Render with dead remote Chrome | ~11 redials, then silent browserless HTTP crawl | Remote cleared; spider launches local Chrome, with an accurate warning |
| Render inside Docker | "probe failed" warning on every render | Probe skip is explicit; no spurious warning |
| Baseline graph upsert | SQLite error 275, degraded summary (zeros) | Succeeds — verified `4 nodes / 2 edges / 2 evidence` live |
| New `ProviderKind` variant | Silently unusable until it failed in production | Build fails until a widening migration ships |
| ci-runner-farm controller restart | Inherited whatever ambient scale-set eligibility existed | Reasserts last commanded value on start + every 5 min |
| axon Docker publish | Failed on every push to main since 08-23 | Succeeds on hosted runners; ghcr unblocked |

## Verification evidence

| command | expected | actual | status |
|---|---|---|---|
| `POST /register` with Labby callback | HTTP 200 | HTTP 200 with `client_id` | pass |
| `POST /register` with `evil.example.com` | rejected | HTTP 422 | pass |
| `cargo test -p axon-adapters --lib` | all pass | 789 passed | pass |
| `cargo test -p axon-jobs --lib` | all pass | 191 passed | pass |
| jobs test with 0009 unregistered | reproduces production error | `code: 275` CHECK failure at `Ledger` | pass |
| `mix test` (controller) | all pass | 149 passed | pass |
| `axon screenshot https://example.com` | captures via container | `captured art_screenshot_0bf8302552a2b0cf` | pass |
| `sqlite3 jobs.db` applied migrations | 0009 present | `jobs\|9\|0009_provider_scheduler_kind_registry` | pass |
| live `provider_reservations` CHECK | includes `graph` | all 20 kinds + `storage` present | pass |
| `axon <url> --scope page --wait true` | non-degraded graph | `Graph: 4 nodes 2 edges 2 evidence` | pass |
| `journalctl -u axon.service` post-deploy | no degraded/CHECK errors | 0 occurrences | pass |
| 18 runners online after fleet restore | 18/18 | 18/18 online | pass |
| `Docker image` workflow on main | success | `completed/success` at `48a8e7c02` | pass |

## Risks and rollback

- **Deployed binary is untagged.** `/usr/local/bin/axon` was built from main
  (`48a8e7c02`), not from a release artifact, because no `v7.2.23` tag exists.
  Rollback: `sudo install -m 0755 /usr/local/bin/axon.bak-pre-7223-20260824T223204Z
  /usr/local/bin/axon && sudo systemctl restart axon.service`.
- **Migration 0009 rebuilds a table.** It follows migration 0004's own
  append-only pattern and re-admits historical rows including legacy `storage`.
  It is applied and verified live; rolling the binary back does not roll back the
  schema, but the widened CHECK is a superset and remains compatible.
- **Docker builds are now slower and consume hosted minutes.** Cold builds
  measured 17 minutes versus 2.9 minutes warm on self-hosted. Rollback is
  reverting the two `runs-on` lines, but that restores the broken state until a
  pool can offer a Docker daemon.
- **tootie config carries new backup files only.** DinD is back to `false`, its
  pre-session value.

## Decisions not taken

- **Bypassing the privileged-runner gate.** Rejected — it would override a
  deliberate fail-closed control for a public repo whose `ci.yml` runs
  `pull_request` on self-hosted pools with no fork guard.
- **Restricting the runner group to private repos.** Viable but a much larger
  infrastructure change; noted in PR #585 as the alternative if hosted builds
  prove unsatisfactory.
- **Deleting the two empty `crf-scaleset-quarantine-*` runner groups.** Left
  alone; they are evidence of in-progress cutover work.
- **Fixing the `crf-*` JIT runner image.** It lacks Python ≥3.11 (`tomllib`
  missing). Out of scope and currently masked, so it was reported rather than fixed.
- **Fixing the auto-tag gap.** Filed as `axon_rust-urwjh` rather than fixed,
  since the correct remedy is not yet established.

## Open questions

- Why was `auto-tag` *skipped* entirely on `5c5de7fab` (#584), which did change
  CLI shipping paths? The skip reason was not captured.
- Should `v7.2.23` be tagged retroactively so the deployed binary corresponds to
  a release, or should the next bump supersede it?
- Is the ci-runner-farm scale-set cutover intended to continue? `POOL_BACKEND` is
  `scaleset` while `backend-transition.json` says `classic_active`, and jobs are
  again landing on `crf-*` JIT runners.
- The `crf-*` JIT runner image cannot run Python ≥3.11 tooling; unclear whether
  that blocks the cutover.

## Next steps

Unfinished from this session:

1. Decide the `v7.2.23` tagging question above; until then the running binary has
   no corresponding release.
2. Address `axon_rust-urwjh` (auto-tag bumped-but-untagged detection).

Follow-on, not started:

3. Fix the `crf-*` JIT runner image's missing Python ≥3.11 / `tomllib`.
4. Consider a fork-PR guard for axon's `ci.yml` so public fork code cannot run on
   self-hosted pools, mirroring `ci-runner-farm`'s own `lint.yml`.

Recommended immediate commands:

```bash
# confirm the deployed service is healthy and on the expected build
systemctl status axon.service --no-pager | head -5
/usr/local/bin/axon --version

# confirm Docker publishing stays green on the next push to main
gh run list --workflow docker-image.yml --branch main --limit 3
```
