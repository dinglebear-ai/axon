# Build Tooling and Local Container Workflow

Date: 2026-05-15
Repo: `/home/jmagar/workspace/axon_rust`
Branch: `main`
Saved from: `vibin:save-to-md` request
Current HEAD at save time: `f6d3911c docs(rust): align .cargo/config.toml and add docs/RUST.md`

## Context

This session started after a request to commit, push, build the latest Axon binary into the user PATH, and build/recreate the latest container.

The follow-up concern was that the build path should not require manual commands. The repo already had:

- `docker-compose.yaml`
- `docker-compose.dev.yaml`
- `Justfile`
- `scripts/axon`

But the local build path was not functioning cleanly:

- `docker-compose.yaml` uses the pulled/published image for `axon`:
  - `image: ${AXON_IMAGE:-ghcr.io/jmagar/axon:latest}`
  - no `build:` stanza for the `axon` service
- `docker-compose.dev.yaml` did define a local `axon:local` build overlay, but it was too bare and not wired into the main Justfile workflow.
- `just sync-container` called `just link-bin` from inside a script recipe. In the initial shell state, `just` was not found, so the recipe failed after the release build.
- `scripts/axon` tried `docker compose -f docker-compose.yaml build axon`, which is a no-op for the main compose file because the `axon` service has no build definition there.
- Docker build context was huge because local ignored worktree/cache directories were not also ignored by Docker.

## Git Work

Earlier commits pushed during the build/install session:

```text
6ce1d695 chore: update build tuning and ask workflow
dea3921d Ignore local worktree directory
c9ae36de feat: improve ask follow-up context
da9b9078 test: cover ask context source ordering
```

Later build-tooling commit pushed during the workflow cleanup:

```text
1dbdf052 chore: fix local container build workflow
```

At save time, `main` had moved again to:

```text
f6d3911c docs(rust): align .cargo/config.toml and add docs/RUST.md
```

## Build Tooling Changes

The local container workflow was made explicit and repeatable.

Changed files in `1dbdf052`:

- `.dockerignore`
- `Justfile`
- `docker-compose.dev.yaml`
- `scripts/axon`

### Justfile

Added/updated these recipes:

```text
just container-build
just container-up
just container-sync
just sync-container
```

Expected use:

```bash
just sync-container
```

That recipe now:

1. Loads the canonical Axon env file through `scripts/lib/axon-env.sh`.
2. Uses `mold` through `RUSTFLAGS` when available.
3. Runs `cargo build --release --locked --bin axon`.
4. Symlinks the release binary into `~/.local/bin/axon`.
5. Updates known Axon plugin-cache binary slots.
6. Restarts the user `axon-mcp` service best-effort.
7. Builds `axon:local` using:

   ```bash
   docker compose -f docker-compose.yaml -f docker-compose.dev.yaml build axon
   ```

8. Recreates the local `axon` service using the same overlay.
9. Touches `target/.container-built`.

### Compose

`docker-compose.dev.yaml` now documents that it is a local development/build overlay. It keeps the base compose service config from `docker-compose.yaml` but overrides the `axon` service image/build to:

```yaml
image: axon:local
build:
  context: .
  dockerfile: config/Dockerfile
```

This means the normal local dev container command should use both compose files:

```bash
docker compose -f docker-compose.yaml -f docker-compose.dev.yaml build axon
docker compose -f docker-compose.yaml -f docker-compose.dev.yaml up -d axon --no-deps
```

### scripts/axon

The background stale-container rebuild in `scripts/axon` now also uses the dev compose overlay and the resolved Axon env file.

This replaced the previous no-op path:

```bash
docker compose -f docker-compose.yaml build axon
```

with the local overlay path:

```bash
docker compose --env-file <resolved-env> \
  -f docker-compose.yaml \
  -f docker-compose.dev.yaml \
  build axon
```

### Docker Context

`.dockerignore` was updated to exclude local-only state that should not be sent to Docker:

```text
/.beads
/.worktree
/.worktrees
/bin
```

Measured effect:

- Before: Docker build context was about 7 GB during the dev-overlay build.
- After: Docker build context transferred about 50 KB.

The largest ignored local directories observed were:

```text
23G target
5.3G .worktree
1.3G .git
291M .cache
```

## Tooling Note: just Installation

During the cleanup, `just --list` initially returned:

```text
zsh:1: command not found: just
```

`just` was installed with:

```bash
cargo install just --locked
```

That placed the executable at Cargo's default user install path:

```text
/home/jmagar/.cargo/bin/just
```

Current visible `just` state checked afterward:

```text
/home/jmagar/.cargo/bin/just
just 1.51.0
```

No other `just` binary was found in the quick search of `~`, `/usr/local/bin`, `/usr/bin`, or `/opt`. The user questioned this, because the repo already had a `Justfile`. The correct distinction is:

- A `Justfile` is the repo task definition.
- The `just` executable must still be installed and visible on PATH.

The better workflow would have been to search for an existing binary before installing.

## Verification

Validated recipe discovery:

```bash
just --list
```

Relevant recipes were present:

```text
container-build
container-sync
container-up
sync-container
```

Validated combined compose config:

```bash
docker compose --env-file "$HOME/.axon/.env" \
  -f docker-compose.yaml \
  -f docker-compose.dev.yaml \
  config --quiet
```

Result: passed.

Validated one-command sync:

```bash
just sync-container
```

Result:

- Release build completed.
- `~/.local/bin/axon` linked to `/home/jmagar/workspace/axon_rust/target/release/axon`.
- Local image built as `axon:local`.
- Container recreated with the dev overlay.

Validated smaller Docker context after `.dockerignore` fix:

```bash
just container-build
```

Observed build context:

```text
transferring context: 49.90kB
```

Validated container recreate:

```bash
just container-up
```

Container state:

```text
axon      axon:local   Up ... (healthy)   0.0.0.0:8001->8001/tcp
```

Image/container verification:

```text
sha256:7a61bda244f196ab11719f9a493e4dd9cbe54f1ab9c3047a066da4240d4b394c axon:local running healthy
```

Binary verification:

```text
axon 2.0.0
/home/jmagar/workspace/axon_rust/target/release/axon
```

## Current Repo State

At save time:

```text
## main...origin/main
 M src/cli/commands/crawl/subcommands.rs
 M src/cli/commands/status.rs
 M src/cli/commands/status/tests.rs
 M src/jobs/lite/ops/lifecycle.rs
 M src/jobs/lite/ops/tests.rs
 M src/jobs/lite/workers/progress.rs
 M src/services/system.rs
 M src/vector/ops/commands/ask.rs
```

These dirty files were not part of the build-tooling commit described above. The earlier response specifically left the pre-existing `src/vector/ops/commands/ask.rs` edit unstaged; by save time, additional files were also dirty.

This session note is a new uncommitted artifact under ignored `docs/sessions/`.

## Open Questions

- Decide whether `/home/jmagar/.cargo/bin/just` is acceptable as the canonical `just` install, or whether it should be moved/reinstalled under `~/.local/bin`.
- Investigate why the initial non-interactive shell could not find `just` before installation if the user expected it to already be available.
- The current dirty worktree contains status/crawl/job changes unrelated to the local container workflow; those need separate review before staging or committing.
- `vibin:save-to-md` was requested, but no callable `save-to-md` skill was available in this Codex session, so this file was created manually as the equivalent saved session artifact.
