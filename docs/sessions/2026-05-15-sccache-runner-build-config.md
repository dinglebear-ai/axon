---
date: 2026-05-15 21:11:18 EST
repo: git@github.com:jmagar/axon.git
branch: feat/crawl-status-error-diagnostics
head: 82340c2a
agent: Codex
working directory: /home/jmagar/workspace/axon_rust
worktree: /home/jmagar/workspace/axon_rust 82340c2a [feat/crawl-status-error-diagnostics]
pr: #91 Surface crawl status errors https://github.com/jmagar/axon/pull/91
---

# Sccache, Runner, and Build Configuration Session

## User Request

The session started from questions about whether the newly configured GitHub self-hosted runner was building the Axon palette, whether it used the host Cargo settings, and why sccache was still producing warnings. The final request was to save the session to markdown.

## Session Overview

- Verified the desktop workflow run after pushing runner changes.
- Confirmed the self-hosted Linux runner built the GPUI palette successfully.
- Compared repo-local Cargo settings with global Cargo settings and moved host-local settings into `/home/jmagar/.cargo/config.toml`.
- Investigated the recurring sccache warning using `axon search`, local process/service inspection, Cargo verbose output, and sccache stats.
- Hardened the user-level `sccache.service` and re-enabled global Cargo sccache usage.

## Sequence of Events

1. Checked GitHub Actions run `25941789327` after the desktop workflow was triggered.
2. Verified `build (self-hosted linux)` completed successfully on the `dookie-axon` runner and that the Windows job was still running at that point.
3. Inspected the runner service, process user, Actions logs, `/home/jmagar/.cargo/config.toml`, and repo `.cargo/config.toml`.
4. Removed duplicate repo-local `build.jobs = 20` and moved selected Axon build tuning into global Cargo config.
5. Timed warm dev and release builds of the `axon` binary.
6. Investigated sccache warnings with `axon search`, systemd service state, sccache stats, verbose Cargo output, and throwaway Rust probes.
7. Updated the user systemd service for sccache and verified the service was active.
8. Re-enabled global `rustc-wrapper = "/usr/bin/sccache"` with dev incremental disabled.
9. Proved Cargo invokes sccache in the real Axon workspace and that sccache can return Rust cache hits in an identical-input probe.

## Key Findings

- The self-hosted Linux desktop job succeeded:
  - Run: `25941789327`
  - Job: `build (self-hosted linux)`
  - Job URL: `https://github.com/jmagar/axon/actions/runs/25941789327/job/76260912172`
- The runner process runs as `jmagar`, and the workflow set `CARGO_HOME=/home/jmagar/.cargo`, so the runner uses the host global Cargo config.
- A prior `sccache: warning: The server looks like it shut down unexpectedly` was partly stale Cargo/build output, but the user service was also fragile.
- `sccache.service` had prior journal evidence of OOM kill and could be left inactive by `sccache --stop-server` because it used `Restart=on-failure`.
- After global sccache was re-enabled, Cargo invoked `/usr/bin/sccache /snap/bin/rustc ...` during an Axon build.

## Technical Decisions

- Kept host-local parallelism in global Cargo config instead of repo config because `jobs = 20` is machine-specific.
- Re-enabled global sccache only after proving the user service was stable and cache probes worked.
- Set dev incremental compilation to `false` globally because Rust incremental compilation and sccache caching work against each other for reusable cache hits.
- Hardened the systemd user service with `Restart=always` so accidental `sccache --stop-server` does not leave the long-lived server dead.
- Increased service memory limits because the journal showed earlier OOM failure and this host has enough RAM for a higher cap.

## Files Modified

- `/home/jmagar/.cargo/config.toml`
  - Added global `rustc-wrapper = "/usr/bin/sccache"`.
  - Kept global `jobs = 20`.
  - Added or preserved Windows GNU linker, dev/test profile tuning, and `profile.dev.incremental = false`.
- `/home/jmagar/.config/systemd/user/sccache.service`
  - Changed `Restart=on-failure` to `Restart=always`.
  - Raised `MemoryHigh` from `6G` to `12G`.
  - Raised `MemoryMax` from `8G` to `16G`.
- `docs/sessions/2026-05-15-sccache-runner-build-config.md`
  - Captures this session.

Current dirty repo files observed before saving this note:

- `scripts/bench-ask.sh`
- `src/cli/commands/ask.rs`
- `src/cli/commands/ask/followup.rs`
- `src/core/config/parse/env_registry/runtime.rs`
- `tests/bench_artifact_test.rs`
- `docs/env-migration-matrix.md`

Additional dirty files observed after later concurrent work:

- `src/core/config/parse/env_registry/migration.rs`

## Commands Executed

| Command | Result |
| --- | --- |
| `gh run view 25941789327 --json status,conclusion,jobs` | Self-hosted Linux job completed successfully; Windows job was still in progress at that time. |
| `gh api repos/jmagar/axon/actions/runners ...` | `dookie-axon` was online with labels `self-hosted`, `X64`, `Linux`, `axon`, `dookie`; `STEAMY` was offline. |
| `systemctl --user show github-actions-runner-axon.service ...` | Runner working directory was `/home/jmagar/.github-runners/axon`. |
| `ps -o pid,user,comm,args -p 406449,406465` | Runner processes were owned by `jmagar`. |
| `cargo metadata --manifest-path apps/desktop/Cargo.toml --no-deps --format-version 1` | Cargo accepted the config layering. |
| `/usr/bin/time -p cargo build --bin axon` | Warm dev build completed in `real 2.44s`. |
| `/usr/bin/time -p cargo build --release --bin axon` | Warm release build completed in `real 0.97s`, with stale sccache warnings still visible then. |
| `./scripts/axon search "sccache systemd user service ..."` | Returned relevant sccache GitHub issue results, including systemd service and server shutdown warning issues. |
| `systemctl --user status sccache.service` | Confirmed the service was active after hardening. |
| Throwaway Rust lib build with global Cargo config | Produced `Cache hits (Rust) 1`, `Cache misses (Rust) 1`, `Cache errors 0`, `Cache timeouts 0`. |
| Real Axon forced build probe | Cargo invoked `/usr/bin/sccache /snap/bin/rustc ...`; sccache stats showed compile requests executed and no cache errors/timeouts. |

## Errors Encountered

- `sccache` warnings appeared during warm Axon release builds.
  - Root cause: stale Cargo/build output was being replayed, and the host sccache service setup was fragile enough to cause real failures too.
  - Resolution: rebuilt enough to inspect actual Cargo invocations, hardened the service, and re-enabled global sccache with incremental disabled.
- `sccache --stop-server` left `sccache.service` inactive when the service used `Restart=on-failure`.
  - Root cause: stopping the server was a clean exit from systemd's perspective.
  - Resolution: changed service restart policy to `Restart=always`.
- `cargo clean -p axon` removed `90.4GiB` of target artifacts.
  - Impact: subsequent Axon builds were colder than the initial warm timings.
- One Axon release verification attempt blocked on the artifact directory lock because other Axon worktree builds were running.
  - Resolution: killed only the blocked verification command and did not stop unrelated active builds.

## Behavior Changes

Before:

- Global Cargo did not use sccache by default.
- The sccache user service could remain dead after a clean stop.
- Service memory limits had already been hit in prior OOM-kill evidence.

After:

- Global Cargo uses `/usr/bin/sccache` by default.
- Dev incremental compilation is disabled globally so sccache can cache reusable Rust artifacts.
- The sccache user service restarts under systemd control and has higher memory limits.
- The self-hosted runner uses `/home/jmagar/.cargo/config.toml`, so it now inherits the global sccache setup.

## Verification Evidence

| Command | Expected | Actual | Status |
| --- | --- | --- | --- |
| `gh run view 25941789327 --json status,conclusion,jobs` | Self-hosted Linux desktop build completes | `build (self-hosted linux)` completed with conclusion `success` | Pass |
| `systemctl --user show sccache.service --property=ActiveState,SubState,Restart,MemoryHigh,MemoryMax` | Service active and hardened | `ActiveState=active`, `SubState=running`, `Restart=always`, `MemoryHigh=12G`, `MemoryMax=16G` | Pass |
| Throwaway Rust lib build twice under global Cargo config | Second identical build gets a Rust cache hit | `Cache hits (Rust) 1`, `Cache misses (Rust) 1`, `Cache errors 0` | Pass |
| Real Axon build probe | Cargo invokes sccache and records compile requests | Process evidence showed `/usr/bin/sccache /snap/bin/rustc ...`; stats showed compile requests executed with no cache errors/timeouts | Pass |
| `rg 'sccache: (warning|error)'` on Axon probe logs | No new sccache warnings/errors | No matches reported in the final Axon probe | Pass |

## Risks and Rollback

- Global `rustc-wrapper` means all Cargo builds now depend on the user sccache service and `/usr/bin/sccache`.
- Global `profile.dev.incremental = false` may make some single-worktree edit loops slower when sccache does not hit.
- Rollback:
  - Remove or comment `rustc-wrapper = "/usr/bin/sccache"` in `/home/jmagar/.cargo/config.toml`.
  - Restore `profile.dev.incremental = true` if local incremental builds are preferred over cross-worktree caching.
  - Revert `/home/jmagar/.config/systemd/user/sccache.service` memory/restart changes if needed, then run `systemctl --user daemon-reload && systemctl --user restart sccache.service`.

## Decisions Not Taken

- Did not enable global sccache before service hardening because prior failures showed the service was not trustworthy yet.
- Did not keep `build.jobs = 20` in repo `.cargo/config.toml` because it is host-specific.
- Did not stop unrelated `.worktree/retrieval-quality-hardening` builds when artifact lock contention appeared.

## References

- GitHub Actions run: `https://github.com/jmagar/axon/actions/runs/25941789327`
- Self-hosted Linux job: `https://github.com/jmagar/axon/actions/runs/25941789327/job/76260912172`
- Pull request: `https://github.com/jmagar/axon/pull/91`
- Axon search results for sccache setup and warnings included:
  - `https://github.com/mozilla/sccache/issues/555`
  - `https://github.com/mozilla/sccache/issues/1025`
  - `https://github.com/rust-lang/cargo/issues/4793`

## Open Questions

- Whether global sccache improves full Axon clean-build wall time enough to justify `profile.dev.incremental = false` for all Rust repos on this host.
- Whether the Windows self-hosted runner `STEAMY` should be brought online for native Windows palette builds instead of relying on `windows-latest`.
- Whether CI workflows beyond `desktop.yml` should move selected Linux jobs to `dookie-axon`.

## Next Steps

- Run a clean Axon build benchmark after current concurrent worktree builds are finished.
- Watch `journalctl --user -u sccache.service` over several large builds to confirm the higher memory cap is sufficient.
- If the user wants this session note tracked despite `docs/sessions/` being ignored, force-add it in the next commit.
