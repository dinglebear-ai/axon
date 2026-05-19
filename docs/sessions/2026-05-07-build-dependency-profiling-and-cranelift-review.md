---
date: 2026-05-07 21:35:13 EDT
repo: git@github.com:jmagar/axon.git
branch: bd-work/retrieval-remediation-ug6
head: 64a2d670
agent: Codex
session id: 019e03eb-8621-7202-8fad-3ac565169780
transcript: /home/jmagar/.codex/sessions/2026/05/07/rollout-2026-05-07T15-30-27-019e03eb-8621-7202-8fad-3ac565169780.jsonl
working directory: /home/jmagar/workspace/axon_rust
worktree: /home/jmagar/workspace/axon_rust 64a2d670 [bd-work/retrieval-remediation-ug6]
pr: none observed via gh pr view
---

# Build Dependency Profiling And Cranelift Review

## User Request

The session started with a request to review Axon's dependencies and crates to identify what contributes most to long build times. Follow-up questions narrowed into Spider's Chrome-related crates, Lab's build setup, and specifically Lab's Cranelift fast-dev path.

## Session Overview

- Profiled Axon's current build behavior without cleaning `target/`.
- Identified the distinction between cold dependency cost and incremental root-crate cost.
- Reviewed Spider/Chrome dependency paths and counted the Chrome/CDP crates pulled through Spider.
- Inspected `../lab` for comparison, first at the crate/workspace level and then at the Cranelift-specific dev loop.
- Saved this session note as a local ignored markdown artifact under `docs/sessions/`.

## Sequence of Events

1. Read Axon guidance and the available cargo performance skill.
2. Inspected Axon's `Cargo.toml`, existing `target/` size, artifact sizes, dependency tree, duplicate crates, and Cargo timing outputs.
3. Ran `cargo build --timings --bin axon` and `cargo build --release --timings --bin axon` to capture fresh build timing evidence.
4. Queried Chrome-related dependency paths with `cargo tree`, `Cargo.lock`, and source searches.
5. Checked out `../lab` read-only, inspected workspace manifests and Justfile, and ran a timing check there.
6. Re-read Lab's Cranelift-specific wiring in `Justfile`, `.cargo/config.toml`, README, CLAUDE.md, and `docker-compose.dev.yml`.

## Key Findings

- Axon incremental debug build was dominated by the root crate, not dependency recompilation: `axon` lib took 54.0s, with 44.6s in frontend/type analysis, while the bin took 2.6s.
- Axon incremental release build was dominated by final optimized root crate work: `axon` lib took 169.4s and the bin took 199.8s with release `lto = "thin"` and `codegen-units = 1`.
- Axon's normal binary graph had 598 unique normal/build packages during the reviewed run.
- Spider's Chrome path was one integration path but four Chrome/CDP crates transitively: `chromey`, `spider_chromiumoxide_cdp`, `spider_chromiumoxide_pdl`, and `spider_chromiumoxide_types`.
- Lab's `just dev-debug` uses nightly Rust with `-Z codegen-backend=cranelift`, clears `RUSTC_WRAPPER`, keeps `mold` in `RUSTFLAGS`, installs `target/debug/labby` to `bin/labby`, and restarts a dev container that bind-mounts that binary.

## Technical Decisions

- Did not clean `target/`; the goal was to avoid destructive profiling and preserve current local build state.
- Treated Axon and Lab dirty worktrees as user-owned and made no code changes outside this ignored session note.
- Reported Cargo timing numbers as current incremental evidence, not full cold-build measurements.
- For Lab comparison, separated crate/workspace architecture from Cranelift dev-loop wiring after the user clarified the intended focus.

## Files Modified

- `docs/sessions/2026-05-07-build-dependency-profiling-and-cranelift-review.md` - saved markdown record of the build profiling and Cranelift review session.

## Commands Executed

| Command | Result |
| --- | --- |
| `git status --short --branch` | Axon was on `bd-work/retrieval-remediation-ug6` with dirty files unrelated to this note. |
| `du -sh target` | Axon `target/` was 25G. |
| `cargo build --timings --bin axon` | Finished in 1m 09s; timing report saved under `target/cargo-timings/`. |
| `cargo build --release --timings --bin axon` | Finished in 6m 09s; timing report saved under `target/cargo-timings/`. |
| `cargo tree ...` variants | Identified dependency paths for Spider, Chrome/CDP, reqwest, aws-lc, sqlite, rmcp, octocrab, and Lab crate graphs. |
| `cargo check --workspace --all-features --timings` in `../lab` | Finished in 49.08s; showed `lab-apis`, `labby` lib, and `labby` bin as the dirty units. |
| `rustup run nightly rustc -Vv` in `../lab` | Confirmed nightly rustc `1.97.0-nightly (67bcaa9c4 2026-05-01)`. |

## Errors Encountered

- Some `cargo tree` and package-cache operations printed lock waits because other Cargo jobs were active in Axon worktrees.
- `sccache` warned during Axon timing builds that the server appeared to shut down unexpectedly; the build continued locally.
- A shell glob for Claude transcript lookup failed because there was no matching Claude project transcript; Codex session transcripts were found under `~/.codex/sessions/`.

## Behavior Changes

No application behavior changed. The only change was this local session documentation file.

## Verification Evidence

| Command | Expected | Actual | Status |
| --- | --- | --- | --- |
| `cargo build --timings --bin axon` | Fresh timing evidence for debug build | `Finished dev profile ... in 1m 09s`; report `cargo-timing-20260507T193143.224639144Z.html` | PASS |
| `cargo build --release --timings --bin axon` | Fresh timing evidence for release build | `Finished release profile ... in 6m 09s`; report `cargo-timing-20260507T193356.465681836Z.html` | PASS |
| `cargo check --workspace --all-features --timings` in `../lab` | Fresh timing evidence for Lab all-features check | `Finished dev profile ... in 48.99s`; report `cargo-timing-20260507T222801.845091184Z.html` | PASS |
| `rustup run nightly rustc -Vv` in `../lab` | Confirm nightly toolchain for Cranelift path | `rustc 1.97.0-nightly (67bcaa9c4 2026-05-01)` | PASS |

## Risks And Rollback

- This note is under `docs/sessions/`, which is ignored in this repo. It will not be staged by plain `git add .`; use `git add -f` only if the note should be committed.
- Rollback is simply deleting this markdown file.

## Decisions Not Taken

- Did not implement Axon feature gates or Cranelift wiring yet; the session only reviewed and proposed the approach.
- Did not run a clean cold build because that would destroy useful local incremental state and take substantially longer.
- Did not modify Lab; it was inspected read-only as a reference implementation.

## References

- Axon timing reports:
  - `/home/jmagar/workspace/axon_rust/target/cargo-timings/cargo-timing-20260507T193143.224639144Z.html`
  - `/home/jmagar/workspace/axon_rust/target/cargo-timings/cargo-timing-20260507T193356.465681836Z.html`
- Lab timing report:
  - `/home/jmagar/workspace/lab/target/cargo-timings/cargo-timing-20260507T222801.845091184Z.html`
- Lab Cranelift wiring:
  - `/home/jmagar/workspace/lab/Justfile`
  - `/home/jmagar/workspace/lab/.cargo/config.toml`
  - `/home/jmagar/workspace/lab/docker-compose.dev.yml`

## Open Questions

- Whether Axon should copy Lab's `just dev-debug` pattern exactly, or add a container hot-swap path with `bin/axon`.
- Whether Axon's default feature set should be reduced so Spider Chrome/CDP is not compiled for non-crawl dev loops.
- Whether Cranelift should run with incremental enabled for Axon local dev, unlike Lab's repo-level `incremental = false` setup optimized for sccache.

## Next Steps

- Started but not completed: no implementation work was started in Axon.
- Follow-on: add an Axon `dev-debug` recipe using nightly + Cranelift and measure it against normal `cargo build --bin axon`.
- Follow-on: design Axon feature gates for `crawl-chrome`, `screenshot`, `github-ingest`, `mcp`, and other expensive subsystems.
- Follow-on: consider a root-crate split similar to Lab if root `axon` frontend/type analysis remains the dominant edit-loop cost.
