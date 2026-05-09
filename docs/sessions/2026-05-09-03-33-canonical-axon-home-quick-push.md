---
date: 2026-05-09 03:33:31 EST
repo: git@github.com:jmagar/axon.git
branch: chore/canonical-axon-home
head: 1dba9b43cdf5312e79b83ca3449c33299c6ab8d9
agent: Codex
working directory: /home/jmagar/workspace/axon_rust
worktree: /home/jmagar/workspace/axon_rust
---

# Canonical Axon Home Quick Push

## User Request

Make `~/.axon` the canonical directory for all Axon config and runtime state, update Docker Compose, plugin hooks, scripts, and docs, run simplification/review passes, then quick-push and create a PR.

## Session Overview

- Moved Compose, plugin setup, CI, helper scripts, docs, and tests toward a single canonical appdata root: `~/.axon`.
- Preserved repo `.env` only as an explicit local development fallback.
- Ran code simplification plus Lavra review lanes and applied their feedback before committing.
- Bumped the crate version from `1.8.4` to `1.9.0`.

## Sequence of Events

1. Audited active docs, scripts, plugin files, CI, Compose, and tests for stale `.env`, `services.env`, old compose path, and `./data` references.
2. Updated Compose and setup paths so host-side appdata defaults to `${HOME}/.axon`, while the container sees `/home/axon/.axon`.
3. Updated plugin setup to write the canonical env file and preserve existing non-plugin-managed keys.
4. Dispatched `code_simplifier` and Lavra review agents for simplicity, security, architecture, performance, and Rust review.
5. Applied review findings for loopback-only publish, token generation, container `AXON_HOME`, mcporter path expansion, and removed legacy full-stack env seeding.
6. Ran focused verification, committed, pushed `chore/canonical-axon-home`, and prepared this session note.

## Key Findings

- Compose `env_file` can read the host `~/.axon/.env`, but container runtime must override `AXON_HOME` to `/home/axon/.axon` to avoid host-path leakage inside the container.
- Compose full-stack startup needs `AXON_MCP_HTTP_TOKEN` when the container binds `AXON_MCP_HTTP_HOST=0.0.0.0`; setup now backfills that token.
- `config/mcporter.json` must compute `$HOME/.axon` in shell, not store literal `${HOME}` values in JSON env.
- `scripts/dev-setup.sh` should use `AXON_HOME` as the relocation knob and set `AXON_DATA_DIR=$AXON_HOME` to keep CLI and Compose aligned.

## Technical Decisions

- Kept `AXON_HOME` as the Docker/host bind-mount knob and `AXON_DATA_DIR` as the process data-root value, but setup aligns them by default.
- Kept Compose MCP HTTP published on `127.0.0.1:8001` by default via `AXON_MCP_HTTP_PUBLISH`, with non-loopback publish as an explicit opt-in.
- Preserved repo `.env` fallback in local helper paths for development only.
- Added a Dockerfile guard to fail builds if generated `apps/web/out` contains obvious secret/token patterns before Rust embeds static assets.

## Files Modified

- `docker-compose.yaml`, `config/Dockerfile`, `.dockerignore`, `.env.example`: canonical Compose/appdata wiring and build-context guard.
- `scripts/plugin-setup.sh`, `scripts/dev-setup.sh`, `scripts/axon`, helper scripts: canonical env loading and setup behavior.
- `config/mcporter.json`, `scripts/test-mcp-tools-mcporter.sh`: canonical env loading for MCP smoke tooling.
- `tests/compose_env_contract.rs`: contract coverage for Compose, CI, plugin setup, mcporter, and setup invariants.
- `README.md`, `docs/**`, `plugins/**`: documentation and skill updates for the new path contract.
- `src/cli/commands/status*`, `src/jobs/lite/*`: status display and local endpoint snapshot hardening already present in the dirty tree and included in the pushed commit.
- `Cargo.toml`, `Cargo.lock`, `CHANGELOG.md`: version bump and changelog entry.

## Commands Executed

- `cargo check --locked` passed after the version bump.
- `cargo test --locked --test compose_env_contract` passed with 7 tests.
- `cargo test --locked display_embed_input` passed.
- `cargo test --locked lite_config_snapshot_does_not_serialize_process_local_endpoint_urls` passed.
- `docker compose --env-file "$HOME/.axon/.env" -f docker-compose.yaml config` passed.
- `bash -n` passed for touched shell scripts.
- `python3 -m py_compile` passed for touched Python helpers.
- Pre-commit hooks passed during commit and amend, including clippy and tests.

## Errors Encountered

- Initial `cargo test compose_env_contract` filtered by binary name and ran zero tests; reran with `--test compose_env_contract`.
- A Rust assertion message containing `${HOME}` was parsed as a format placeholder; escaped it as `${{HOME}}`.
- Plugin setup smoke exposed an unbound `MCP_HOST` echo after refactoring; fixed it by reading effective host/port from the written env file.
- First patch attempt failed because simplifier edits changed context; reapplied smaller patches against current files.

## Behavior Changes

| Before | After |
|---|---|
| Compose read repo `.env` and used mixed host data roots | Compose reads `~/.axon/.env` and binds `${AXON_HOME:-$HOME/.axon}` |
| Container could inherit host `AXON_HOME` from env file | Container overrides `AXON_HOME=/home/axon/.axon` |
| Full Compose `axon` service could fail without MCP token | `dev-setup.sh` backfills `AXON_MCP_HTTP_TOKEN` |
| Plugin setup rewrote plugin-private env | Plugin setup updates canonical `~/.axon/.env` while preserving existing values where plugin options are omitted |
| `services-up` ambiguity after root Compose switch | `services-up` remains infra-only: Qdrant, TEI, Chrome |

## Verification Evidence

| Command | Expected | Actual | Status |
|---|---|---|---|
| `cargo check --locked` | Build checks with version `1.9.0` | Passed | Pass |
| `cargo test --locked --test compose_env_contract` | Canonical path contracts pass | 7 passed | Pass |
| `cargo test --locked display_embed_input` | Status display tests pass | 2 passed | Pass |
| `cargo test --locked lite_config_snapshot_does_not_serialize_process_local_endpoint_urls` | Snapshot local endpoint test passes | 1 passed | Pass |
| `docker compose --env-file "$HOME/.axon/.env" -f docker-compose.yaml config` | Compose renders | Passed | Pass |
| Pre-commit via `git commit`/`git commit --amend` | Hooks pass | Passed | Pass |

## Risks and Rollback

- Existing users with non-default `AXON_DATA_DIR` should align `AXON_HOME` as well, or run setup to write both values consistently.
- Rollback is a normal git revert of commit `1dba9b43`; machine-local `~/.axon/.env` changes made by setup are not automatically reverted by git.

## Decisions Not Taken

- Did not make host CLI and container share paths through bind-mounted host paths in API responses; artifact/job handles remain the cleaner long-term server/client boundary.
- Did not force full-stack Compose into a profile; instead, setup now seeds a token and Compose publishes loopback-only by default.

## Open Questions

- `src/services/setup/assets.rs` and `src/services/setup/deploy.rs` still reference older deployment env concepts according to architecture review; they were outside the changed-file scope and may need a follow-up cleanup.

## Next Steps

- Create a PR for `chore/canonical-axon-home`.
- Consider a follow-up issue for the remaining remote setup/deploy asset references to `services.env` / old data-root defaults.
