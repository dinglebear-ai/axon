---
date: 2026-05-20 21:38:42 EST
repo: git@github.com:jmagar/axon.git
branch: main
head: 98f1df00
working directory: /home/jmagar/workspace/axon_rust
worktree: /home/jmagar/workspace/axon_rust
---

# Dev Compose Workflow

## User Request

Rename the compose files, diagnose the restarting Axon container, convert the root compose file into a real development stack, tighten the workflow, and quick-push the result.

## Session Overview

- Renamed the production compose file to `docker-compose.prod.yaml`.
- Reworked `docker-compose.yaml` into the local development stack.
- Added a `dev-runtime` Dockerfile target that can run the host-built debug binary.
- Updated wrapper scripts, Justfile recipes, CI compose validation, docs, env registry, and contract tests.
- Pushed commit `98f1df00` to `origin/main`.

## Sequence of Events

1. Diagnosed the restart loop as a glibc mismatch: the mounted host binary required newer glibc than the old Debian bookworm runtime container provided.
2. Recreated the running service with production compose so the container stopped restarting.
3. Renamed compose files and updated production references.
4. Converted `docker-compose.yaml` from a thin overlay into a real dev stack that extends production service definitions.
5. Built and started `axon:dev-runtime`, then verified the live container is healthy.
6. Tightened guard scripts and tests around the new dev/prod compose split.
7. Fixed the package version mismatch in `apps/web/package.json`.
8. Committed and pushed the final changes.

## Key Findings

- The restart loop came from `/home/axon/.axon/dev/axon` requiring `GLIBC_2.38` and `GLIBC_2.39` while the previous runtime image had glibc `2.36`.
- `axon:dev-runtime` now uses Debian trixie via `node:24-trixie-slim`, observed with glibc `2.41`.
- The live `axon` container is running `axon:dev-runtime` with entrypoint `/home/axon/.axon/dev/axon`.

## Technical Decisions

- Keep production on `docker-compose.prod.yaml` and a production image path.
- Make `docker-compose.yaml` the default local development stack.
- Bind-mount the local debug target directory into the dev container instead of rebuilding the whole application image for normal Rust edits.
- Use `AXON_DEV_TARGET_DIR` as the explicit dev bind-mount source knob.

## Files Modified

- `docker-compose.prod.yaml`: production/full stack compose file.
- `docker-compose.yaml`: development stack extending production services and running the mounted debug binary.
- `config/Dockerfile`: added `dev-runtime`; preserved the prior web asset copy fix.
- `scripts/axon`: builds the debug binary and restarts the dev container only when needed.
- `Justfile`: updated dev container recipes and service helpers.
- `scripts/check_compose_port_bindings.py`: checks both production and dev compose files by default.
- `tests/compose_env_contract.rs`: added dev compose contract coverage and retargeted production assertions.
- `apps/web/package.json`: aligned version to `4.2.0`.
- Docs and CI files: updated compose naming and validation commands.

## Commands Executed

- `docker compose --env-file ~/.axon/.env -f docker-compose.yaml build axon`
- `docker compose --env-file ~/.axon/.env -f docker-compose.yaml up -d --no-deps axon`
- `cargo test -q --test compose_env_contract`
- `python3 scripts/check-env-config-boundary.py`
- `python3 scripts/check_compose_port_bindings.py`
- `git commit -m "Improve Axon dev compose workflow"`
- `git push`

## Errors Encountered

- Initial commit failed on `rustfmt` for pre-existing unformatted Rust files. Ran `cargo fmt --all`, restaged, and retried successfully.
- Full `compose_env_contract` initially failed because `apps/web/package.json` still declared version `3.0.1`; updated it to `4.2.0`.

## Behavior Changes

Before:
- Local dev compose used a thin overlay and copied a host binary into `~/.axon/dev`, which could fail in-container due glibc mismatch.
- Production and dev compose naming was ambiguous.

After:
- Production compose is explicit: `docker-compose.prod.yaml`.
- Development compose is explicit: `docker-compose.yaml`.
- The dev container runs the bind-mounted host debug binary from `target/debug` inside a newer runtime image.

## Verification Evidence

| Command | Expected | Actual | Status |
| --- | --- | --- | --- |
| `cargo test -q --test compose_env_contract` | all contract tests pass | `13 passed` | pass |
| `python3 scripts/check-env-config-boundary.py` | env matrix valid | `env/config boundary ok: 199 classified keys` | pass |
| `python3 scripts/check_compose_port_bindings.py` | no forbidden host/interface defaults | no output, exit 0 | pass |
| `docker compose --env-file ~/.axon/.env -f docker-compose.yaml ps axon` | dev container healthy | `axon:dev-runtime`, healthy | pass |
| pre-commit hook | all checks pass | rustfmt, clippy, and 2033 tests passed on retry | pass |

## Risks and Rollback

- Risk: `docker-compose.yaml` now depends on `docker-compose.prod.yaml` through Compose `extends`; moving either file requires updating the reference.
- Risk: dev runtime tracks `node:24-trixie-slim`; production remains on bookworm.
- Rollback: revert commit `98f1df00` or run production directly with `docker compose --env-file ~/.axon/.env -f docker-compose.prod.yaml up -d axon`.

## Next Steps

- No unfinished implementation tasks from this session.
- Optional follow-up: run GitHub checks after the push if this branch requires remote CI confirmation.
