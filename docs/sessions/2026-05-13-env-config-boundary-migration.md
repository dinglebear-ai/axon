# Env Config Boundary Migration

**Date:** 2026-05-13 19:09:49 EDT  
**Repository:** `git@github.com:jmagar/axon.git`  
**Working directory:** `/home/jmagar/workspace/axon_rust/.worktrees/main`  
**Branch:** `main`  
**Merge commit:** `29f77319`  
**PR:** https://github.com/jmagar/axon/pull/85  
**Transcript:** Not available in the current Codex context.

## User Request

Merge the completed `env-config-boundary-migration` branch back into `main`, pull the latest, clean up temporary branch/worktree state, and save the session to markdown.

## Summary

Merged PR #85 into `main` after local verification. The branch moves Axon toward the intended config boundary: secrets, service URLs, and necessary runtime environment values stay in `.env`, while non-secret tuning and behavior knobs are represented in `config.toml`.

The merge introduced the env migration matrix, an automated boundary checker, split env registries, setup migration support, config parsing tests, and documentation/template updates. GitHub marked PR #85 as merged at `2026-05-13T23:09:16Z`.

## Major Changes Landed

- Added `docs/config/env-migration-matrix.toml` as the audited classification source for env keys.
- Added `scripts/check-env-config-boundary.py`, which passed with `193 classified keys`.
- Split env registry code into runtime, advanced, and migration modules.
- Added setup-time migration support under `src/services/setup/local/env_migration.rs`.
- Updated `.env.example`, `config.example.toml`, `README.md`, `docs/CONFIG.md`, setup docs, auth docs, and compose contracts.
- Added tests for config priority, TOML parsing, env boundary classification, compose env contract, and local migration behavior.

## Verification

Local verification run in `/home/jmagar/workspace/axon_rust/.worktrees/main`:

```text
cargo fmt --check
python3 scripts/check-env-config-boundary.py
cargo test
cargo clippy --all-targets -- -D warnings
```

Results:

- `cargo fmt --check` passed.
- `scripts/check-env-config-boundary.py` passed: `env/config boundary ok: 193 classified keys`.
- `cargo test` passed: `1584 passed; 0 failed; 5 ignored`.
- `cargo clippy --all-targets -- -D warnings` passed.

PR #85 checks before merge:

- `compose-config`: success.
- `image-build-smoke`: success.
- `GitGuardian Security Checks`: success.
- `CodeRabbit`: success.
- `cubic · AI code reviewer`: neutral.

## Merge And Cleanup

- Pulled `origin/main` in the temporary main worktree; it was already up to date before merge.
- Merged `env-config-boundary-migration` into `main` with merge commit `29f77319`.
- Pushed `main` to `origin/main`.
- Confirmed PR #85 state was `MERGED`.
- Removed `/home/jmagar/workspace/axon_rust/.worktrees/env-config-boundary-migration`.
- Deleted the local `env-config-boundary-migration` branch.
- Deleted the remote `origin/env-config-boundary-migration` branch.

The root checkout at `/home/jmagar/workspace/axon_rust` was intentionally left untouched because it was already on `fix/unify-web-mcp-port-8001` with unrelated dirty files.

## Notes

GitHub reported 13 existing Dependabot vulnerability alerts on pushes to the default branch: 7 high, 4 moderate, and 2 low. No dependency changes were made as part of the merge cleanup session.

## Open Questions

- None for the merge and cleanup flow.
