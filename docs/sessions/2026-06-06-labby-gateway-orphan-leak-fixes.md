---
date: 2026-06-06 02:29:58 EST
repo: git@github.com:jmagar/axon.git
branch: android-design-implementation
head: 3413fa54
session id: ee666381-2f0e-496e-a39c-d14e1adc10b2
transcript: /home/jmagar/.claude/projects/-home-jmagar-workspace-axon/ee666381-2f0e-496e-a39c-d14e1adc10b2.jsonl
working directory: /home/jmagar/workspace/axon
beads: lab-yinnu, lab-nh4wf, lab-wr7fm (lab repo tracker, not axon)
---

# Labby gateway orphan-process leak: diagnosis and three fixes

> Note: this session ran from the axon checkout, but all code changes landed in `~/workspace/lab` (the labby gateway). Axon was the victim (TEI OOM-killed), lab was the culprit.

## User Request

`/lumen:doctor` health check, which uncovered a host-wide OOM. Follow-ups: "kill em" (orphan processes), "whats spawning these?", "kill now - fix it now" (the spawner + the leak), then `/lavra-quick` "both" (the two follow-up beads) with explicit choice of "Full fix now anyway" for the architectural half.

## Session Overview

Lumen doctor found TEI down (restart-loop, exit 137). Root cause: the host (dookie, 48 GiB) was OOM — ~1,450 orphaned MCP server processes (40.3 GiB RSS) leaked by `labby gateway code exec`, driven by an orphaned YouTube-download batch script plus a second live agent session. Fixed three bugs in the lab repo across two commits on `main`: (1) drain the upstream pool on normal CLI exit, (2) drain on SIGINT/SIGTERM too, (3) stop connecting the full upstream fleet per one-shot invocation via lazy seeding + a fingerprinted on-disk codemode catalog cache. All verified empirically, deployed to `~/.local/bin/labby` (0.22.2). TEI/Lumen healthy.

## Sequence of Events

1. **Doctor**: `health_check` → TEI unreachable; `index_status` → index fresh (1946 files / 30,437 chunks). `axon-tei` container restarting, exit 137, 52 restarts.
2. **Diagnosis**: GPU fine (89 MiB used); host RAM 45/48 GiB. `ps` census: 318 `npm exec mcp-*`, 245 `uv`/`python` pairs, total process RSS 40.3 GiB — orphaned MCP server trees (PPID 1).
3. **First sweep**: awk pid-table descendant walk killed 1,450 processes; memory 47→17 GiB used; TEI recovered; Lumen health OK.
4. **Spawner hunt**: fresh orphan pairs kept appearing every ~10 s. Caught parents at birth: `/tmp/ytdl-expanded-batch-20260605-201446.sh` (431-URL YouTube→mp3 batch from a prior session, at URL 201/431) looping `timeout 100s labby gateway code exec`; later also a second live session doing Android automation via the same command.
5. **Leak mechanism**: every one-shot `labby gateway` invocation eagerly spawns ALL configured stdio upstreams (`build_manager` → `discover_all_with_in_process_peers`), and the manager lives in a process-global (`install_gateway_manager`), so `UpstreamConnection::Drop` never runs at exit → child process groups orphaned (~7 trees/invocation).
6. **Fix 1 (bead lab-yinnu)**: split `run()` so all command arms funnel through `dispatch_command()`, then `drain_for_swap("gateway.cli.exit")` before exit. Verified: 36 descendants spawned during a run, 0 alive after exit. Committed `c6e0a64d` to lab main, installed to `~/.local/bin/labby`; 90 s watch with the other session active → 0 new orphans. Killed the batch script; swept 190 more leftovers.
7. **`/lavra-quick` both follow-ups**: scope-escalation fired on lab-wr7fm (codemode proxy generation reprobes every upstream — `manager.rs` `refresh_code_mode_catalog`); user chose "Full fix now anyway". Advisor consulted; key directives: cache must only ever make things slower never wrong, atomic writes, per-upstream partial caching (failures never cached), CLI-only gate, TTL+fingerprint, and re-use the descendant harness as the acceptance test.
8. **Fix 2 (lab-nh4wf)**: `run()` races `dispatch_command` against `shutdown_signal()` (SIGINT→130, SIGTERM→143); drain runs on both paths.
9. **Fix 3 (lab-wr7fm)**: `build_manager` seeds lazily (mirrors `serve`); new `catalog_cache.rs` persists per-upstream tool lists to `~/.lab/cache/codemode-catalog.json` (sha256 config fingerprint, 6 h TTL, temp+rename atomic writes, merge-on-store); `execute.rs` routes `CodeModeSurface::Cli` through `code_mode_catalog_tools_cached`; `refresh_code_mode_catalog` (serve path) keeps the cache warm.
10. **Verification round-trips**: initial checks were misleading twice (malformed callTool id; `LAB_LOG` default hides INFO so a grep for connect logs was vacuous; one "warm" run was accidentally cold because the SIGTERM test had deleted the cache). Final clean runs: warm proxy-only = 0 stdio connects; callTool = exactly 1 (target upstream); SIGTERM mid-cold-connect → `pool drain reason=gateway.cli.exit` logged, no new orphans.
11. **Branch mishap + repair**: the lab commit landed on `fix/synapse2-prod-mount` because a concurrent session had switched the shared checkout's branch. Repaired: `git push --force-with-lease` rewind of that branch, `git reset --keep` locally (other session's WIP preserved), cherry-pick to main via `.worktrees/main-hotfix` → `b66a1792` pushed.
12. **Closeout**: fmt/clippy/tests green (103 tests in touched modules), release built + installed (`labby 0.22.2`), 54 residual orphans from the old binary's signal path swept, beads commented and closed, `bd dolt push`, session log saved.

## Key Findings

- **Leak root cause**: `crates/lab/src/cli/gateway.rs` `build_manager` eagerly spawned all stdio upstreams per one-shot invocation; the manager is installed into a process-global so Drop-based cleanup (`UpstreamConnection::Drop` → `killpg`) never ran at process exit.
- **Second leak path**: `timeout 100s labby ...` SIGTERMs skip even an exit-path drain — the default signal disposition kills before any cleanup (explains 18+54 orphans accumulating after fix 1 was deployed, from the other session's old-binary/timeout calls).
- **Codemode proxy needs the full catalog**: `build_code_mode_proxy` → `code_mode_catalog_tools(allow_cold_connect=true)` → `refresh_code_mode_catalog` reprobes every enabled upstream (`manager.rs:2144` pre-change), so lazy seeding alone cannot fix `code exec` — hence the disk cache.
- **The lazy machinery already existed**: `seed_lazy_upstreams` / `ensure_tools_for_upstream` (`pool/ensure.rs`), used by `serve` (`cli/serve.rs:315`); the CLI was the only eager path.
- **`callTool` resolves live regardless of cache**: `resolve_code_mode_upstream_tool` ensures the single target upstream at call time — a stale cache can only mis-shape the `codemode.*` helper namespace, never execute against stale state.
- **GNU `timeout` exit 124 is expected** even with graceful TERM handling (no `--preserve-status`); drain proof must come from logs/orphan counts, not the exit code.
- **`LAB_LOG=info` required** to see `upstream connect start` / `pool drain` lines; the default filter hides INFO, which made one verification round vacuous.
- **Code Mode ids are `<upstream>::<tool>`** — a leading `upstream::` segment is rejected (`invalid_code_mode_id`).

## Technical Decisions

- **Drain explicitly instead of relying on Drop**: the global-static manager makes Drop unreachable at exit; `drain_for_swap` already existed for config reload and does the right kill work.
- **`tokio::select!` signal race, not a signal-hook atexit**: in-flight stdio connects stay covered by the existing `ProcessGroupGuard` (armed at spawn, drops when `select!` cancels the connect future) — no new guard machinery needed.
- **Disk cache over serve-delegation** for wr7fm: matches the bead's own suggestion, keeps the CLI standalone, and the failure mode is benign (cold refresh). Server delegation (proxying exec to a running `labby serve`) was rejected as a larger auth/semantics change.
- **Failures never cached**: a down upstream is omitted from the proxy and retried next run — caching a failure would suppress recovery.
- **Cache validity = sha256(config) fingerprint AND 6 h TTL**: fingerprint catches config edits; TTL catches upstream-side tool drift no config change reflects.
- **Atomic temp+rename writes, merge-on-store, parse-failure = miss**: concurrent one-shot invocations are exactly the workload that caused the incident; worst case for a lost race is a redundant refresh.
- **`reset --keep` (not `--hard`) for the branch repair**: preserves the other session's uncommitted WIP since my files didn't overlap theirs.

## Files Changed

All in `/home/jmagar/workspace/lab` (lab repo), commits `c6e0a64d` and `b66a1792` on `main`. No axon files changed except this session log.

| status | path | previous path | purpose | evidence |
|---|---|---|---|---|
| modified | `crates/lab/src/cli/gateway.rs` | — | `dispatch_command` split + exit drain (c6e0a64d); signal race + `shutdown_signal()` + lazy `seed_lazy_upstreams` in `build_manager` (b66a1792) | both commits |
| created | `crates/lab/src/dispatch/gateway/code_mode/catalog_cache.rs` | — | fingerprinted, TTL'd, atomically-written on-disk codemode catalog cache + 3 unit tests | `b66a1792` |
| modified | `crates/lab/src/dispatch/gateway/code_mode.rs` | — | `pub(crate) mod catalog_cache;` | `b66a1792` |
| modified | `crates/lab/src/dispatch/gateway/code_mode/execute.rs` | — | `CodeModeSurface::Cli` routes proxy generation through `code_mode_catalog_tools_cached` | `b66a1792` |
| modified | `crates/lab/src/dispatch/gateway/manager.rs` | — | new `code_mode_catalog_tools_cached`; `refresh_code_mode_catalog` warms the cache | `b66a1792` |
| modified | `crates/lab/src/dispatch/setup.rs` | — | `pub use client::lab_home;` | `b66a1792` |
| created | `docs/sessions/2026-06-06-labby-gateway-orphan-leak-fixes.md` (axon) | — | this session log | this commit |

Deployed artifact: `~/.local/bin/labby` reinstalled twice (after c6e0a64d, after b66a1792; final `labby 0.22.2`).

## Beads Activity

All in the **lab** repo tracker (`~/workspace/lab`), not axon's.

| bead | title | actions | final status | why it mattered |
|---|---|---|---|---|
| lab-yinnu | gateway CLI leaks stdio upstream child processes on exit | created, claimed, closed | closed | the incident bug — 1,450 leaked processes OOM'd dookie and killed axon-tei |
| lab-nh4wf | gateway CLI: drain pool on SIGTERM/SIGINT | created (as follow-up), claimed, LEARNED comment, closed | closed | `timeout`-killed invocations still leaked after fix 1 |
| lab-wr7fm | gateway CLI: lazy stdio upstream spawn for one-shot exec | created (as follow-up), claimed, DECISION + LEARNED + DEVIATION comments, closed | closed | per-invocation full-fleet spawn was the cost amplifier |

`bd dolt push` run twice (after lab-yinnu close, after final closes); both reported `Push complete.`

No axon beads were touched this session.

## Repository Maintenance

- **Plans (axon)**: no plan moves needed — the injected "active plan" (`2026-05-27-android-phase2-stubbed-modes.md`) already lives under `docs/plans/complete/`. Remaining non-complete plans under `docs/plans/` belong to other workstreams and were not assessed for completion (out of scope for this lab-focused session).
- **Beads**: lab beads fully closed out (see above); `bd list --status=in_progress` in lab returned "No issues found". Axon beads untouched.
- **Worktrees/branches (axon)**: 3 extra worktrees (`recursing-jemison-9a544a`, `lavra-review-fixes-tz85`, `palette-design-implementation`) and their branches left alone — they belong to other active sessions/PRs (evidence: `git worktree list`, dirty checkout on `android-design-implementation`). The lab repo's `.worktrees/main-hotfix` was created for the cherry-pick and removed afterward (`git worktree remove`).
- **Branch repair (lab)**: `fix/synapse2-prod-mount` was force-with-lease rewound exactly one commit (mine) to `7391e1de` and the other session's WIP preserved via `reset --keep`; their dirty files (`plugins/testing/skills/desktop-app-testing/*`) remain untouched in their tree.
- **Stale docs**: none identified as contradicted by this session. CLAUDE.md in lab was checked for version-bump rules (none found).
- **Process cleanup**: three orphan sweeps (1,450 + 190 + 54 processes) plus the ytdl batch script kill (`/tmp/ytdl-expanded-batch-20260605-201446.sh`, PID 1828768). Final state: 0 orphaned npm/uv processes.

## Tools and Skills Used

- **Lumen MCP** (`health_check`, `index_status`, `semantic_search`): doctor checks; one `semantic_search` against the lab repo failed (TEI HTTP 413 during fresh-index embed of the lab tree) — fell back to grep.
- **Skills**: `lumen:doctor` (entry point), `lavra:lavra-quick` (both follow-up beads; scope-escalation checkpoint exercised, user overrode to full fix).
- **Advisor tool**: consulted once before the catalog-cache implementation; its hardening directives (atomic writes, partial caching, CLI-only gate, empirical acceptance test, lab-not-axon conventions) were all applied.
- **AskUserQuestion**: one question (wr7fm scope) → "Full fix now anyway".
- **Bash + file tools (Read/Edit/Write)**: all diagnosis, process forensics (awk pid-table descendant walks), cargo build/test/clippy/fmt, git surgery. Issues: a `sleep 25` chain was blocked by the harness (switched to an `until` loop); `cargo check -p lab` failed (package is named `labby`); `git pull --rebase` failed on the other session's unstaged files (used `--autostash`, which is what later revealed the branch switch).
- **beads (`bd`)** in the lab repo: create/claim/comment/close/dolt push.
- No browser tools, no subagents, no workflow orchestration.

## Commands Executed

| command | result |
|---|---|
| `docker ps/logs/inspect axon-tei` | restart loop, exit 137, OOMKilled=false (host-level OOM), 52 restarts |
| awk pid-table descendant kill (3 sweeps) | 1,450 + 190 + 54 processes killed; 47→17 GiB used after first sweep |
| `kill 1828768` (+ sweep) | ytdl batch spawner stopped |
| `cargo check/build/test/clippy/fmt --bin labby` | green; 103 tests in touched modules; 1 new clippy warning fixed (`let_underscore_drop`) |
| descendant-tracking harness around `labby gateway code exec` | fix 1: 36 spawned / 0 alive after exit |
| `LAB_LOG=info` connect-count runs | warm proxy: 0 stdio connects; callTool: exactly 1 (searxng); cold: fleet + cache write |
| `LAB_LOG=info timeout -s TERM 4 labby gateway code exec ...` | `pool drain reason=gateway.cli.exit` logged; orphans before==after |
| `git push --force-with-lease ... 7391e1de:fix/synapse2-prod-mount` + `git reset --keep` + cherry-pick in `.worktrees/main-hotfix` | branch repaired; `b66a1792` on main |
| `install -m 755 target/release/labby ~/.local/bin/labby` | deployed; `labby 0.22.2` |

## Errors Encountered

- **TEI restart loop (exit 137)**: kernel OOM-killer reaping it during model load. Resolved by killing the leaked process trees; no TEI config change needed.
- **First descendant-kill script failed** (`ps --ppid` whitespace → "process ID list syntax error"); rewritten as a single awk pid-table walk.
- **`cargo check -p lab`** → no such package (binary crate is `labby`).
- **Vacuous verification, twice**: grep for connect logs without `LAB_LOG=info` (default filter hides INFO) and a "warm" run that was actually cold (prior test had deleted the cache). Both caught and re-verified properly.
- **Malformed Code Mode id** (`upstream::searxng::tool`) → `invalid_code_mode_id`; correct form is `searxng::tool`.
- **Commit landed on the wrong branch** (`fix/synapse2-prod-mount`): a concurrent session switched the shared lab checkout's branch mid-session; `git pull --rebase --autostash` masked it. Repaired without losing the other session's WIP (see maintenance).
- **lumen `semantic_search` on the lab repo** failed with TEI HTTP 413 during embed; used grep instead.

## Behavior Changes (Before/After)

| area | before | after |
|---|---|---|
| `labby gateway <cmd>` one-shot exit | orphaned ~7 stdio upstream trees per invocation (PPID 1 npm/uvx) | pool drained (`gateway.cli.exit`); 0 orphans |
| `labby gateway <cmd>` killed by SIGTERM/SIGINT | process died with no cleanup; children orphaned | drain runs; exits 128+signum |
| `labby gateway list/get/add/...` | spawned the entire upstream fleet eagerly | lazy seed; no upstream processes spawned |
| `labby gateway code exec` proxy generation | connected every enabled upstream per invocation | warm cache: 0 stdio connects; only stale/missing upstreams connect; failures retried next run |
| host stability (dookie) | 40 GiB of leaked MCP processes → OOM → axon-tei crashloop | 0 orphans; TEI + Lumen healthy |
| new artifact | — | `~/.lab/cache/codemode-catalog.json` (auto-managed, safe to delete) |

## Verification Evidence

| command | expected | actual | status |
|---|---|---|---|
| `mcp lumen health_check` (after sweep) | TEI healthy | `Status: OK` | pass |
| descendant harness, fix-1 binary | 0 children alive after exit | `descendants=36 alive_after=0` | pass |
| 90 s orphan watch with other session active (fix-1 binary installed) | 0 new orphans | `orphans after 90s watch: 0` | pass |
| `LAB_LOG=info` warm proxy-only exec | 0 stdio connects | 5 connect attempts, all dead-HTTP upstreams (no processes); 0 stdio | pass |
| `LAB_LOG=info` callTool exec (warm) | exactly 1 stdio connect | `upstream=searxng` only (+5 dead-HTTP retries) | pass |
| `timeout -s TERM 4` mid-cold-connect | drain log + no new orphans | `pool drain start/finish reason=gateway.cli.exit`; orphans 18→18 | pass |
| `cargo test --bin labby -- code_mode` / `cli::gateway` | green | 100 + 3 passed | pass |
| `cargo clippy --bin labby` | no new warnings | clean after `drop()` fix | pass |
| `git diff-tree` equivalent on lab main | only intended files | 6 files in `b66a1792` | pass |

## Risks and Rollback

- **Cache staleness window (≤6 h)**: a `codemode.*` helper may be offered for a tool the upstream no longer has (call fails live) or a new tool may be missing from helpers (callTool escape hatch works). Delete `~/.lab/cache/codemode-catalog.json` to force a cold refresh.
- **Behavior change in `gateway list`**: with lazy seeding the CLI no longer probes upstreams, so status reflects seeded-not-connected state (parity with `serve` startup). Use `gateway test <name>` for a live probe.
- **Rollback**: revert `b66a1792` (and `c6e0a64d` if needed) on lab main, reinstall previous binary. The cache file is inert without the code.
- **Concurrent-session git risk remains**: the shared lab checkout can be branch-switched by any session mid-operation. This session repaired one such collision; consider doing lab work from a dedicated worktree going forward.

## Decisions Not Taken

- **Delegating one-shot exec to the running `labby serve`**: real fix for fleet-spawn but changes auth/transport semantics; rejected for scope.
- **Caching failed upstream probes**: rejected — would suppress recovery of a temporarily-down upstream.
- **Escalating wr7fm to `/lavra-design`**: offered at the scope checkpoint; user chose full fix in-session.
- **Force-pushing nothing / leaving the stray commit on `fix/synapse2-prod-mount`**: rejected — a 451-line gateway feature does not belong in that PR branch.

## References

- lab commits: `c6e0a64d` (exit drain), `b66a1792` (signal drain + lazy seed + catalog cache) on `jmagar/lab` main
- lab beads: lab-yinnu, lab-nh4wf, lab-wr7fm (all closed, comments carry the LEARNED/DECISION/DEVIATION trail)
- Key code: `crates/lab/src/cli/gateway.rs` (`run`/`dispatch_command`/`shutdown_signal`/`build_manager`), `crates/lab/src/dispatch/gateway/code_mode/catalog_cache.rs`, `crates/lab/src/dispatch/gateway/manager.rs` (`code_mode_catalog_tools_cached`, `refresh_code_mode_catalog`), `crates/lab/src/dispatch/upstream/pool/ensure.rs` (pre-existing lazy machinery)

## Open Questions

- Why did the long-dead ytdl batch script's parent session never reap it? (The script itself was PPID-1-orphaned bash — origin session unidentified.)
- Should the labby serve Docker image be rebuilt to pick up the cache-warming path in `refresh_code_mode_catalog`? The container runs its own binary; the CLI fix works without it, but warm-cache freshness from serve won't apply until the image updates.
- `gateway list` cosmetic follow-up: with lazy seeding, in-process (virtual server) summaries show unregistered until first use — acceptable parity with serve, but worth a UX look.
- The lumen-on-lab-repo TEI 413 during embed (batch too large for `Payload too large` limits) — axon-side `TEI_MAX_CLIENT_BATCH_SIZE` handling exists; lumen's indexer may need the same split-on-413 logic.

## Next Steps

- **Done & shipped**: all three leak fixes merged to lab main and deployed locally; beads closed; no unfinished work from this session.
- **Recommended**: rebuild/redeploy the labby serve container so the serve path warms the catalog cache (`docker compose` in whatever deploys `lab.tootie.tv` — currently returning 502 per session-start hook, worth investigating separately).
- **Recommended**: file a lumen bead for 413-split batching when indexing large repos.
- **Watch for**: any reappearance of PPID-1 `npm`/`uv` orphans (`ps -eo pid,ppid,comm | awk '$2==1 && ($3=="npm" || $3=="uv")'`) — would indicate a leak path not covered (e.g. SIGKILL, which cannot be handled).
