---
date: 2026-08-24 21:59:38 EST
repo: git@github.com:dinglebear-ai/axon.git
branch: claude/systematic-debugging-issue-cb9d95
head: 61fbb7916
working directory: /home/jmagar/workspace/axon/.claude/worktrees/systematic-debugging-issue-cb9d95
worktree: /home/jmagar/workspace/axon/.claude/worktrees/systematic-debugging-issue-cb9d95
pr: |
  axon #582 fix(render): fall back to local Chrome when the remote CDP probe fails — https://github.com/dinglebear-ai/axon/pull/582
  axon #583 fix(jobs): widen provider_kind CHECK to the full ProviderKind registry — https://github.com/dinglebear-ai/axon/pull/583
  axon #585 ci: build Docker jobs on hosted runners + fill in the 7.2.23 changelog — https://github.com/dinglebear-ai/axon/pull/585
  axon #586 docs: save session log — https://github.com/dinglebear-ai/axon/pull/586
  axon #587 fix(release): stop main from sitting bumped-but-untagged — https://github.com/dinglebear-ai/axon/pull/587
  ci-runner-farm #82 fix(scaleset): reassert commanded eligibility across restarts — https://github.com/dinglebear-ai/ci-runner-farm/pull/82
  ci-runner-farm #92 fix(release): assert anonymous pullability instead of mutating visibility — https://github.com/dinglebear-ai/ci-runner-farm/pull/92
beads: axon_rust-nkh6y (closed), axon_rust-a0qd7 (closed), axon_rust-urwjh (created, closed)
---

# Chrome fallback, provider_kind CHECK, CI runner recovery, and release-pipeline repair

Supersedes `2026-08-24-chrome-fallback-provider-kind-and-ci-runner-recovery.md`, which
was written mid-session and is now stale: its "Risks" section says the deployed
binary is untagged and its "Next steps" list tagging v7.2.23 and deploying
ci-runner-farm#82 as open. All three are done. That file is left in place as the
record of what was true when written.

## User request

Began with a screenshot: Labby's "Edit server" dialog failing against
`axon.dinglebear.ai/mcp` with `HTTP 422 ... redirect URI must target a loopback
host, match the native callback endpoint, or match an allowed redirect pattern`,
and the instruction to debug it systematically. The session then extended through
successive requests: fix the Chrome fallback trap, fix a `provider_kind` CHECK
failure, merge everything, investigate stuck CI runs, fix the broken Docker
publish, deploy, and finally close two release-pipeline gaps.

## Session overview

Seven distinct problems were diagnosed; all seven were fixed and merged. Seven
pull requests across two repositories landed, two production deploys were made
and verified against live data, a self-inflicted CI outage was diagnosed and
recovered, and v7.2.23 was tagged and released.

Three diagnoses were wrong before they were right, and all three corrections were
driven by evidence rather than reasoning: the Docker failure was blamed on PR
#578's pool routing (disproven by run history), `DIND=false` was called an
accidental regression (it is required by a fail-closed gate), and the runner pool
was reported as "thinning" (its floor had been deliberately reduced). Each is
recorded below rather than smoothed over.

## Sequence of events

1. **Labby OAuth 422.** Reproduced with a direct `POST /register`, traced to the
   vendored lab-auth allowlist, fixed by adding Labby's callback to
   `AXON_ALLOWED_REDIRECT_URIS`, verified 200 and that unrelated URIs still 422.
2. **Chrome audit.** Found the `axon` Incus container stopped, axon running
   host-native, and `AXON_CHROME_REMOTE_URL` pointed at a port owned by an
   unrelated Codex scratch browser.
3. **Container-vs-fallback research.** A workflow (3 research agents, 8
   adversarial verifiers) compared the Chrome container against the Spider local
   launcher and surfaced the real defect: a *set-but-dead* remote never falls back.
4. **axon#582.** Fixed the dead-remote degradation; built and started the real
   `axon-chrome` container; repointed axon at its management endpoint.
5. **axon#583.** Fixed the `provider_kind` CHECK degrading baseline graph upserts.
6. **CI runner investigation.** A 3-agent evidence sweep explained the queue
   blackhole; fixed live, then made durable via ci-runner-farm#82.
7. **Docker publish.** Diagnosed twice wrongly, attempted a DinD restore that
   tripped a fail-closed gate and **took the fleet down**, restored service, then
   fixed properly by moving both Docker jobs to hosted runners (axon#585).
8. **Deploy + release.** Built from main, installed, restarted, verified live;
   tagged and released v7.2.23 manually after finding auto-tag had not fired.
9. **Pipeline gaps.** Investigated both with a workflow, then fixed: axon#587
   (auto-tag reconciler) and ci-runner-farm#92 (anonymous-pull assertion).

## Key findings

- **Dead remote Chrome never fell back.** `bootstrap_chrome_runtime` logged
  "falling back to local Chrome launcher" but never cleared
  `cfg.chrome_remote_url` (`crates/axon-adapters/src/web_engine/chrome_bootstrap.rs`),
  so spider redialled ~11 times then degraded to a **browserless HTTP crawl**. A
  true local launch only ever happened when the variable was *unset*.
  `configure_spider_browser` (`crates/axon-adapters/src/web_engine/browser.rs:86`)
  had the same defect with no probe at all.
- **`provider_kind` CHECK lagged the enum by 9 variants.** Migration 0004 allowed
  11 kinds; `ProviderKind` (`crates/axon-api/src/source/enums/runtime.rs:112`) had
  20. The `sqlite-graph` scheduler
  (`crates/axon-services/src/context/target_runtime/schedulers.rs:97`) reserves as
  `ProviderKind::Graph`, so every baseline graph upsert failed with SQLite 275.
- **CI queue starvation was dual-backend routing.** `ci-pool-*` labels were served
  by classic listeners *and* a scale set in the same runner group (id 4). Jobs
  routed to an unhealthy scale-set session showed `runner_id: 0` forever; plain
  cancel could not land, only force-cancel.
- **Docker publish: DinD is forbidden, not accidentally off.** ci-runner-farm
  1.11.1 added a fail-closed gate refusing privileged runners while org-scoped
  targeting cannot prove the group excludes public repos. axon is public, so
  `DIND=true` cannot start — proven empirically by trying it and taking the fleet
  down. No self-hosted pool can offer these jobs a daemon.
- **auto-tag had three stacked gates and no escape hatch.** The plan generator is
  tag-relative but the gate running it was push-relative (`v7.2.22..48a8e7c02` =
  113 shipping files vs `5c5de7fab..48a8e7c02` = 0); CI was red on the bump
  commit; and the next commit was docs-only so no CI run fired at all.
  `workflow_dispatch` could not help — both sides require `event == 'push'`.
- **`publish-distributed-release` never reached its curl.** It exits on a guard
  for `secrets.UNRAID_BOT_GITHUB_ADMIN_TOKEN`, which does not exist in that repo.
  Adding it would not help: `PATCH /orgs/{org}/packages/container/{name}` is not a
  routed endpoint (404 regardless of scope). Its failure **skipped
  `verify-publication`**, so v1.13.0 shipped a bundle the pipeline never verified.

## Technical decisions

- **Tri-state probe result rather than clear-on-failure.** `resolve_cdp_ws_url`
  returns `None` both for a dead remote and for the normal in-Docker path, so a
  naive clear would break Docker deployments. Added `remote_unreachable`, set only
  when the probe ran and exhausted retries.
- **Compiler-enforced enum/migration coupling.** Beyond widening the CHECK, a
  wildcard-free `match` over `ProviderKind` breaks the build if a variant ships
  without a widening migration — chosen over a runtime assertion so drift cannot
  recur silently.
- **Controller-scoped eligibility fix.** The chosen option was framed as "derive
  from `backend-transition.json`", but the controller has no path to that
  tootie-local file. Implemented the equivalent guarantee — persist and reassert
  the controller's own commanded intent — and stated the scope adaptation in the PR.
- **Hosted runners over bypassing the security gate.** Overriding
  `CRF_I_ACCEPT_UNRESTRICTED_PRIVILEGED_RUNNER_HOST_ROOT_RISK` for a public repo
  would undermine a deliberate first-party control. Both Docker jobs moved to
  `ubuntu-latest`, timeout 30 → 60 based on a measured 17-minute cold build.
- **Assert availability rather than mutate it.** For ci-runner-farm#92, deleting
  the job was cheaper and also correct, but the assertion keeps the anonymous-pull
  guarantee explicit — deletion would let a future GHCR login silently destroy it.
- **One matrix assignment, not two.** The reconciler initially added a second
  `jq` selector; a repo test blocks that because a broader selector could tag
  release-please-owned components. Restructured so both paths only *produce*
  `release-plan.json` and one validate step owns the selector.

## Files changed

| status | path | previous path | purpose | evidence |
|---|---|---|---|---|
| modified | `crates/axon-adapters/src/web_engine/chrome_bootstrap.rs` | — | tri-state probe outcome + `apply_bootstrap_outcome` | axon#582 |
| created | `crates/axon-adapters/src/web_engine/chrome_bootstrap_tests.rs` | — | 6 probe/persist tests | axon#582 |
| modified | `crates/axon-adapters/src/web_engine/browser.rs` | — | skip connection on confirmed-dead remote | axon#582 |
| modified | `crates/axon-adapters/src/web_engine/browser_tests.rs` | — | ws passthrough + dead-remote tests | axon#582 |
| modified | `crates/axon-adapters/src/providers/chrome_render.rs` | — | use shared outcome helper | axon#582 |
| modified | `crates/axon-adapters/src/web_engine/engine.rs`, `engine/runtime.rs` | — | export `cdp_probe_skipped_in_docker` | axon#582 |
| created | `crates/axon-jobs/src/migrations/0009_provider_scheduler_kind_registry.sql` | — | widen CHECK to 20 kinds + legacy `storage` | axon#583 |
| modified | `crates/axon-jobs/src/migrations.rs`, `migration-checksums.txt` | — | register + pin migration 9 | axon#583 |
| modified | `crates/axon-jobs/src/scheduler_tests.rs` | — | registry round-trip + exhaustiveness witness | axon#583 |
| modified | `xtask/src/schemas/database_defs_tests.rs` | — | expect nine jobs migrations | axon#583 |
| modified | generated schema contracts (`docs/reference/runtime/*`, `generated/memory.md`, `source-input-manifest.json`, xtask database snapshot) | — | regenerated | `cargo xtask generated-contracts refresh` |
| modified | `.github/workflows/docker-image.yml` | — | build job → `ubuntu-latest`, timeout 30→60 | axon#585 |
| modified | `.github/workflows/compose-smoke.yml` | — | `image-build-smoke` → `ubuntu-latest` | axon#585 |
| modified | `CHANGELOG.md` | — | fill empty `[7.2.23]` section | axon#585 |
| created | `docs/sessions/2026-08-24-…-ci-runner-recovery.md` | — | first session log | axon#586 |
| modified | `.github/workflows/auto-tag.yml` | — | schedule + dispatch reconciler; `target_sha` | axon#587 |
| modified | `.github/workflows/ci.yml` | — | drop push-relative plan gate | axon#587 |
| modified | `tests/workflow_shapes.rs` | — | `target_sha` rename in 2 harnesses + 1 assertion | axon#587 |
| created | `controller/lib/crf_controller/scaleset_eligibility.ex` (ci-runner-farm) | — | durable eligibility state | crf#82 |
| created | `controller/test/scaleset_eligibility_test.exs` (ci-runner-farm) | — | persistence tests | crf#82 |
| modified | `controller/lib/crf_controller/scaleset_client.ex` (ci-runner-farm) | — | reassert on start + periodic retry | crf#82 |
| modified | `controller/test/scaleset_client_test.exs` (ci-runner-farm) | — | reassert + retry tests | crf#82 |
| modified | `.github/workflows/publish-distributed-release.yml` (ci-runner-farm) | — | anonymous-pull assertion replaces PATCH | crf#92 |
| modified | `tests/distributed-publication-workflow.sh` (ci-runner-farm) | — | assert the property, forbid any secret | crf#92 |
| created | `docs/sessions/2026-08-24-…-ci-runner-recovery-v2.md` | — | this note | this commit |

Non-repo changes: `/home/jmagar/.axon/.env` (`AXON_ALLOWED_REDIRECT_URIS`,
`AXON_CHROME_REMOTE_URL`); `/usr/local/bin/axon` replaced (backup
`axon.bak-pre-7223-20260824T223204Z`); `/opt/ci-runner-farm/current` flipped
1.11.1 → 1.13.0; tootie's `ci-runner-farm.cfg` DinD toggled true then reverted
(backup `…bak-pre-dind-restore-20260824T160938Z`); stale GitHub runner
registration `tootie-ci-runner-ops-4` (id 9075) deleted.

## Beads activity

| id | title | actions | final status | why it mattered |
|---|---|---|---|---|
| `axon_rust-nkh6y` | Dead remote Chrome degrades renders to raw HTTP instead of local launcher | created, claimed, closed | closed | Tracked axon#582 |
| `axon_rust-a0qd7` | provider_kind CHECK rejects graph (and other current enum kinds) | created, claimed, closed | closed | Tracked axon#583; found still `in_progress` after merge during a wrap-up sweep and closed |
| `axon_rust-urwjh` | auto-tag leaves main bumped-but-untagged when no CLI shipping paths change | created, closed | closed | Filed when the release gap was found during deploy; closed after axon#587 merged |

No beads were created in ci-runner-farm; that work is tracked by PRs #82 and #92.

## Repository maintenance

**Plans.** `docs/plans/` holds 16 non-complete plans; none relate to this session
and none were completed by it, so none were moved. Evidence: `ls docs/plans/*.md`;
no plan file was read or written.

**Beads.** Three beads, all now closed (table above).

**Worktrees and branches.** Verified merges by **PR state**, not ancestry —
squash merges make branch commits non-ancestors of main, and an ancestry check
wrongly reported `fix/auto-tag-reconciler` and the session-log branch as
unmerged. Removed `.worktrees/auto-tag-fix`, `.worktrees/bump-7223`, and
ci-runner-farm's `.worktrees/visibility-fix`; deleted local
`fix/auto-tag-reconciler`, `session-log/2026-08-24-…`, `deploy-main`,
`fix/release-visibility-assertion`, and (earlier) `claude/fix-provider-kind-check`
and `claude/bump-7.2.23`. Remote branches were auto-deleted by GitHub on merge.

Deliberately left alone:
- `.claude/worktrees/systematic-debugging-issue-cb9d95` — this live session's
  worktree; cannot remove from within itself.
- `.claude/worktrees/docs-asana-adapter-guide` — **locked by another running
  Claude session** (pid 1715306).
- `.claude/worktrees/connected-tools-not-exposed-0e199c`,
  `pr-581-review-08c523`, both `.worktrees/codex/*`, and all ci-runner-farm
  `codex/*` and `/tmp/*` worktrees — other sessions' work, unknown ownership.
- Stale `codex/live-source-benchmark`, `codex/release-v7.2.20`,
  `codex/speed-ci-contracts` — pre-existing, not this session's to reap.

**Stale docs.** Searched `docs/` and `CLAUDE.md` for `ci-pool-system`: zero hits,
so the Docker runner change made nothing stale. The **first session log is stale**
in its Risks/Next-steps; this v2 supersedes it and says so at the top rather than
rewriting history. Two agent memories were corrected (deployed axon build; the
ci-runner-farm drift note now records the 1.13.0 deploy).

**Transparency.** The DinD restore attempt took the tootie runner fleet down for
roughly six minutes (18 runners deregistered; `cmd_start` blocked by the security
gate). Restored by reverting `DIND=false` and running `runner-farm.sh start`; 18/18
returned online and a dispatched `repository-contract` run completed in ~36s.

## Tools and skills used

- **Skills.** `superpowers:systematic-debugging` (root-cause-first discipline on
  every bug); `vibin:save-to-md` (both session logs).
- **Workflow tool.** Three fan-outs: Chrome container-vs-fallback (3 research + 8
  adversarial verify agents, ~1.5M tokens), CI runner-fleet evidence sweep (3
  agents), and pipeline-gap analysis (2 agents). One verifier correctly refuted a
  claim about the AutoSwitch thin-page path, which was corrected before reporting.
- **Shell.** Extensive `git`, `gh`, `cargo`, `mix`, `sqlite3`, `systemctl`,
  `journalctl`, `docker`, `curl`, `ssh` to tootie, and the ci-runner-farm plugin CLI.
- **Monitor / background tasks.** CI watches and long builds. One monitor produced
  a **false green** (see Errors) and was replaced with one that watches the run
  object rather than the check list.
- **MCP.** `ccd_session` `spawn_task` / `dismiss_task` for out-of-scope findings.
  Several MCP servers disconnected and reconnected mid-session; none were needed
  at those moments. Many plugin MCP servers remain unauthenticated and unused.
- **Issues encountered.** `gh api …/packages/…` returned 403 (token lacks
  `read:packages`), so ghcr staleness was inferred from run history.
  `bd list --status=open` did not surface a newly created bead; `bd search` did.

## Commands executed

| command | result |
|---|---|
| `curl -X POST https://axon.dinglebear.ai/register …` | 422 before fix; 200 with `client_id` after |
| `cargo test -p axon-adapters --lib` | 789 passed |
| `cargo test -p axon-jobs --lib` | 191 passed |
| `cargo test --test workflow_shapes` | 56 passed |
| `mix test` (ci-runner-farm controller) | 149 passed |
| `bash tests/distributed-publication-workflow.sh` | contract passed |
| `runner-farm.sh restart` | stop OK; **start failed** on the privileged-runner gate |
| `runner-farm.sh start` (after reverting DinD) | `fleet up: 18 runner(s)` |
| `scripts/verify-distributed-bundle.sh <v1.13.0 bundle>` | verification passed |
| `install.sh` (v1.13.0 distributed bundle) | `current` → 1.13.0 |
| `axon https://example.com --scope page --wait true` | `Graph: 4 nodes 2 edges 2 evidence` |

## Errors encountered

- **Fleet outage during the DinD restore.** `restart` stopped and deregistered all
  18 runners, then `cmd_start` refused to start privileged runners for a public-repo
  org group. Restored by reverting DinD and starting; verified 18/18 plus a green run.
- **CI caught a Windows-portability bug in my own test.** `/tmp/eligibility.json`
  is `:volumerelative` on Windows, so the path check fired before the identity
  check under test. Fixed with `System.tmp_dir!()`. Production code unaffected.
- **A monitor reported a false green.** It checked for *pending rows* in
  `gh pr checks`; a workflow that has not started produces no rows, so the required
  `ci-gate` was absent rather than pending. Replaced with run-object polling.
- **A stale queued CI run blocked its successor for over an hour.** After a
  force-push the superseded run stayed `queued` holding ci.yml's concurrency group
  (`cancel-in-progress` does not reap queued runs) and plain cancel could not land.
- **A commit silently did not happen.** The pre-commit hook was killed at its 60s
  budget on a cold worktree, so git rejected the commit — but the subsequent push
  succeeded in publishing the *unchanged* branch, which looked like success. Caught
  by `git diff-tree`. Prebuilding xtask dropped hooks to ~1.5s.
- **Incomplete first Docker fix.** Moving only `docker-image.yml` left
  `image-build-smoke` failing identically; a programmatic sweep found exactly two
  daemon-needing jobs on `ci-pool-*` runners, both then fixed.
- **An invented action SHA.** Pinned `dtolnay/rust-toolchain@b3b07ba…`, which
  appears nowhere in the repo; corrected to the repo's `29eef336…`.
- **A step appended into the wrong job.** Appending YAML at end-of-file placed the
  validate step inside `release` rather than `plan`; the file still parsed, so only
  inspecting the job graph revealed it.

## Behavior changes (before/after)

| area | before | after |
|---|---|---|
| Labby → axon OAuth | DCR rejected 422 | Registration succeeds; unrelated URIs still rejected |
| Render with dead remote Chrome | ~11 redials, then silent browserless HTTP crawl | Remote cleared; local Chrome launches, with an accurate warning |
| Render inside Docker | "probe failed" warning every render | Probe skip explicit; no spurious warning |
| Baseline graph upsert | SQLite 275, degraded summary | Succeeds — verified `4 nodes / 2 edges / 2 evidence` live |
| New `ProviderKind` variant | Silently unusable until production failure | Build fails until a widening migration ships |
| Controller restart | Inherited ambient scale-set eligibility | Reasserts last commanded value on start + every 5 min |
| axon Docker publish | Failed every push since 08-23 | Succeeds on hosted runners |
| Bumped-but-untagged main | Could persist indefinitely, no override | Daily reconciler + `workflow_dispatch`, still gated on green CI |
| Release publication verification | Skipped whenever the visibility step failed | Visibility step can pass; verification always runs |

## Verification evidence

| command | expected | actual | status |
|---|---|---|---|
| `POST /register` with Labby callback | 200 | 200 with `client_id` | pass |
| `POST /register` with `evil.example.com` | rejected | 422 | pass |
| jobs test with 0009 unregistered | reproduces production error | `code: 275` at `Ledger` | pass |
| `sqlite3 jobs.db` applied migrations | 0009 present | `jobs\|9\|0009_provider_scheduler_kind_registry` | pass |
| live `provider_reservations` CHECK | includes `graph` | all 20 kinds + `storage` | pass |
| `axon <url> --scope page --wait true` | non-degraded graph | `4 nodes 2 edges 2 evidence` | pass |
| `journalctl -u axon.service` post-deploy | no degraded/CHECK errors | 0 occurrences | pass |
| runners online after fleet restore | 18/18 | 18/18 | pass |
| `Docker image` workflow on main | success | success at `48a8e7c02` | pass |
| v7.2.23 release binary | contains both fixes | migration 0009 + unreachable-remote string present | pass |
| anonymous ghcr manifest fetch (crf#92 logic) | HTTP 200 | 200 for `sha256:0ae7d4ac…` | pass |
| `Code.ensure_loaded?(CrfController.ScaleSetEligibility)` on the live controller | true | true | pass |
| dispatched `repository-contract` after controller upgrade | success | success on `tootie-ci-runner-ops-2` | pass |

## Risks and rollback

- **Deployed axon binary was built from main**, not from the release, because no
  tag existed at deploy time. The subsequently published v7.2.23 Linux artifact was
  verified to contain the same fixes. Rollback:
  `sudo install -m 0755 /usr/local/bin/axon.bak-pre-7223-20260824T223204Z /usr/local/bin/axon && sudo systemctl restart axon.service`.
- **Controller upgraded 1.11.1 → 1.13.0 — a 33-commit jump**, mostly unrelated
  in-flight work, accepted deliberately because #82 cannot ship alone. Rollback is
  a symlink flip to the 1.11.1 release dir, still on disk.
- **The v1.13.0 bundle targets ubuntu-24.04 while dookie is 26.04.** It runs
  (older glibc is forward-compatible) but diverges from the prior 26.04 convention.
- **Docker builds are slower and consume hosted minutes** — 17m cold vs 2.9m warm
  self-hosted.
- **v7.2.23 points at `e0c345a76`**, a docs-only commit, not the bump commit
  `5c5de7fab` — an artifact of manual tagging.

## Decisions not taken

- **Bypassing the privileged-runner gate** — would override a deliberate control
  for a public repo whose `ci.yml` runs `pull_request` on self-hosted pools.
- **Restricting the runner group to private repos** — viable but a far larger
  infrastructure change; noted in axon#585 as the alternative.
- **Making `rust-contracts` unconditional on main** — closes a fourth narrow
  auto-tag hole but adds a 14–186 min job to every main push; the reconciler covers
  it daily instead.
- **Deleting the visibility job outright** (crf#92) — cheaper and correct, but the
  assertion keeps the anonymous-pull guarantee explicit.
- **Relaxing the "exactly one matrix assignment" test** — the first instinct, and
  wrong; restructuring to a single selector was correct.
- **Fixing the `crf-*` JIT runner image** (missing Python ≥3.11 / `tomllib`) —
  out of scope and currently masked; reported, not fixed.

## Open questions

- Why was `auto-tag` *skipped* entirely on `5c5de7fab` beyond the red-CI gate? The
  skip reason was inferred from the workflow's `if:`, not read from a log line.
- Should `v7.2.23` be re-pointed at the actual bump commit, or is tagging main's
  head (what the new reconciler also does) the intended semantics?
- Is the ci-runner-farm scale-set cutover meant to continue? `POOL_BACKEND` is
  `scaleset` while `backend-transition.json` says `classic_active`, and jobs are
  again landing on `crf-*` JIT runners.
- Does the `crf-*` runner image's missing Python ≥3.11 block that cutover?

## Next steps

Unfinished from this session: none — all seven problems are merged, and both
deploys are verified live.

Follow-on, not started:

1. Fix the `crf-*` JIT runner image's missing Python ≥3.11 / `tomllib`.
2. Consider a fork-PR guard for axon's `ci.yml` so public fork code cannot run on
   self-hosted pools, mirroring ci-runner-farm's own `lint.yml`.
3. Resolve the v7.2.23 tag-target question above if it matters for provenance.

Recommended immediate commands:

```bash
# confirm the deployed service and controller are healthy
systemctl status axon.service --no-pager | head -5
systemctl status ci-runner-farm-controller.service --no-pager | head -5

# confirm the new reconciler is registered and can be driven by hand
gh workflow view auto-tag.yml -R dinglebear-ai/axon
```
