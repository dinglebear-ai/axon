---
date: 2026-05-10 01:30:25 EST
repo: git@github.com:jmagar/axon.git
branch: fix/empty-env-and-docker-cache
head: 870aeb34
agent: Codex
session id: unknown
transcript: unavailable
working directory: /home/jmagar/workspace/axon_rust
worktree: /home/jmagar/workspace/axon_rust  870aeb34 [fix/empty-env-and-docker-cache]
---

# Session: Empty Env and Docker Cache Fix

## User Request

Investigate why `axon crawl https://help.getzep.com/overview` failed with `--output-dir` requiring a value, then quick-push the resulting fixes.

## Session Overview

- Fixed blank optional path env handling for `AXON_OUTPUT_DIR`, `AXON_SQLITE_PATH`, and `AXON_LOG_DIR`.
- Repointed the local `axon` command away from a stale plugin-cache binary and rebuilt the runtime image.
- Updated Docker builder base image to match the pinned Rust `1.94.0` toolchain.
- Bumped the project version from `1.9.0` to `1.9.1` and pushed branch `fix/empty-env-and-docker-cache`.

## Sequence of Events

1. Reproduced the original CLI parse failure and found `AXON_OUTPUT_DIR=` in `~/.axon/.env`.
2. Confirmed the installed `/home/jmagar/.local/bin/axon` pointed at a stale plugin-cache binary reporting `axon 1.8.4`.
3. Moved `AXON_OUTPUT_DIR` parsing out of clap env parsing and into config resolution so blank values fall through.
4. Fixed the same blank-env pattern for `AXON_LOG_DIR` and `AXON_SQLITE_PATH`.
5. Rebuilt and restarted the Axon container, corrected tailnet publish/origin env, and verified server-mode crawl submission.
6. Investigated slow Docker rebuilds and `sccache` warnings, then aligned the Docker builder image to `rust:1.94.0-bookworm`.
7. Versioned, committed, and pushed the fix branch.

## Key Findings

- `AXON_OUTPUT_DIR=` caused clap to treat `--output-dir` as present with no value.
- `AXON_LOG_DIR=` caused the logger to try creating an appender in an empty path.
- `AXON_SQLITE_PATH=` caused the container to open the wrong SQLite path, leading to missing job-table startup errors.
- Docker was using `rust:1.83-bookworm` while `rust-toolchain.toml` pins `1.94.0`, forcing rustup work inside the build.
- `sccache.service` was OOM-killed on May 9 after peaking at 11.2G memory; current stats showed it running with a 68.75% Rust hit rate.

## Technical Decisions

- Removed clap-level `env = "AXON_OUTPUT_DIR"` for `output_dir` and resolved the env var through the existing trimmed `read_env()` helper.
- Kept CLI flag priority over env by only consulting `AXON_OUTPUT_DIR` when the flag/default value is still unchanged.
- Derived default output from `axon_data_base_dir().join("output")` to honor the canonical `~/.axon` layout.
- Left the server plaintext guard intact and used explicit machine-local env opt-ins for the trusted tailnet endpoint.
- Chose the low-risk Docker cache fix first: exact Rust builder image alignment instead of introducing cargo-chef in this patch.

## Files Modified

- `src/core/config/cli.rs` / `src/core/config/cli/global_args.rs`: expose and use a shared default output-dir constant.
- `src/core/config/parse/build_config.rs`: ignore blank `AXON_OUTPUT_DIR` and `AXON_SQLITE_PATH`.
- `src/core/config/parse/build_config/post_init.rs`: derive output dir from canonical Axon data root.
- `src/core/config/parse/build_config/tests.rs`: add coverage for blank output/sqlite env vars and env/flag priority.
- `src/core/logging.rs`: ignore blank `AXON_LOG_DIR` and `AXON_LOG_FILE`.
- `config/Dockerfile`: use `rust:1.94.0-bookworm`.
- `Cargo.toml`, `Cargo.lock`, `apps/web/package.json`, `README.md`, `CHANGELOG.md`: version `1.9.1`.

## Commands Executed

- `axon --version`: found stale `axon 1.8.4` before repointing the symlink.
- `cargo test -q output_dir -- --test-threads=1`: passed.
- `cargo test -q sqlite_path -- --test-threads=1`: passed.
- `cargo test -q version_bearing_files_stay_in_sync`: passed.
- `cargo check -q`: passed.
- `docker build --target builder -f config/Dockerfile . --progress=plain`: verified builder cache behavior.
- `axon crawl https://help.getzep.com/overview --max-pages 1 --wait false --json`: succeeded through server mode and created job `547924c2-9589-4741-be47-2f04a37b3a57`.

## Errors Encountered

- Initial CLI parse failure: blank `AXON_OUTPUT_DIR` was interpreted as a missing `--output-dir` value.
- Container crash loop: blank `AXON_SQLITE_PATH` led to missing SQLite job tables.
- Tailnet server-mode failure: container was published only to loopback; corrected machine-local publish/origin env.
- Host conflict on port 8001: a stale plugin-cache `axon serve mcp` process held the port; killed it so the container could own the canonical port.
- Slow Docker builds: base image/toolchain mismatch forced rustup downloads; fixed by using the exact pinned Rust image.

## Behavior Changes

| Before | After |
|---|---|
| Blank `AXON_OUTPUT_DIR=` could abort CLI parsing. | Blank `AXON_OUTPUT_DIR` falls through to canonical output defaults. |
| Blank `AXON_SQLITE_PATH=` could point SQLite at an empty path. | Blank `AXON_SQLITE_PATH` falls through to `~/.axon/jobs.db`. |
| Blank `AXON_LOG_DIR=` caused appender creation warnings. | Blank log path env values fall through to `~/.axon/logs/axon.log`. |
| Docker builder started from Rust 1.83 then rustup-installed 1.94.0. | Docker builder starts from Rust 1.94.0. |

## Verification Evidence

| command | expected | actual | status |
|---|---|---|---|
| `cargo check -q` | Rust check passes | Passed | pass |
| `cargo test -q output_dir -- --test-threads=1` | output-dir env tests pass | 4 passed | pass |
| `cargo test -q sqlite_path -- --test-threads=1` | sqlite-path env test passes | 1 passed | pass |
| `cargo test -q version_bearing_files_stay_in_sync` | version files in sync | Passed | pass |
| `docker build --target builder -f config/Dockerfile . --progress=plain` | second build uses cached builder steps | completed in 1.517s on immediate repeat | pass |
| `axon crawl https://help.getzep.com/overview --max-pages 1 --wait false --json` | submits crawl via server mode | returned job `547924c2-9589-4741-be47-2f04a37b3a57` | pass |
| pre-commit hook | monolith, rustfmt, env guard, clippy, tests pass | all passed | pass |

## Risks and Rollback

- Risk: Docker builder image tag `rust:1.94.0-bookworm` must remain available upstream. Roll back `config/Dockerfile` to the prior builder image if unavailable.
- Risk: machine-local `~/.axon/.env` was adjusted during validation but is not tracked in this commit.
- Rollback path: revert commit `870aeb34` and restore the previous `~/.axon/.env` values if server publication needs to return to loopback only.

## Decisions Not Taken

- Did not add `cargo-chef`; exact toolchain alignment gave a low-risk cache improvement and verified immediate repeat builds at 1.517s.
- Did not weaken the plaintext bearer-token guard; used explicit `AXON_SERVER_INSECURE=1` only for the trusted tailnet endpoint.

## Open Questions

- Whether `sccache.service` should get a memory limit, lower job concurrency, or a monitoring alert after the observed OOM kill.
- Whether Docker build cache should be further improved with dependency-layer planning such as cargo-chef.

## Next Steps

- Open a PR for `fix/empty-env-and-docker-cache`.
- Consider a separate follow-up to harden the user `sccache` service against OOM restarts.
