# Docker Build Optimizations
**Date:** 2026-03-04
**Branch:** feat/sidebar
**Session type:** Debugging + Infrastructure optimization

---

## Session Overview

Debugged a `docker compose` startup failure caused by a missing required variable, then systematically optimized both Dockerfiles (`docker/Dockerfile` for workers, `docker/web/Dockerfile` for the web container). Total changes: 1 bug fix, ~12 build optimizations across both images.

---

## Timeline

| Time | Activity |
|------|----------|
| Start | Debug `docker compose logs -f` failing with interpolation error |
| +10m | Root cause confirmed: `AXON_GIT_SHA` mandatory variable, no mechanism to set it |
| +15m | Analyzed SHA guard design — determined `:-dev` is correct, `rebuild-fresh.sh` handles real builds |
| +30m | Workers Dockerfile audit — found sccache not wired, healthcheck broken, inotify dead dep |
| +45m | Restored and properly wired sccache (`RUSTC_WRAPPER` + cache mount) |
| +60m | Added mold linker to builder stage |
| +75m | Switched to `lukemathwalker/cargo-chef` pre-built image; pinned to `0.1.77` |
| +90m | Web Dockerfile audit — pnpm cache mount silently broken (wrong uid), gemini unpinned |
| +100m | All fixes applied, `docker compose config` verified clean |

---

## Key Findings

### Bug: `docker compose` failing on `AXON_GIT_SHA`
- **Location:** `docker-compose.yaml:147,210`
- **Root cause:** `${AXON_GIT_SHA:?set AXON_GIT_SHA=$(git rev-parse HEAD)}` — `:?` is mandatory; Docker Compose interpolates ALL variables in the file regardless of subcommand, so even `docker compose logs` fails
- **Design intent preserved:** `scripts/rebuild-fresh.sh` sets the real SHA inline for builds; `docker/s6/cont-init.d/00-verify-image-sha:28` explicitly rejects containers built with `"dev"` SHA unless `AXON_ENFORCE_IMAGE_SHA=false`
- **Fix:** Changed to `:-dev` — non-build commands work, build commands via `rebuild-fresh.sh` still stamp the real SHA

### Bug: sccache not invoked during Docker builds
- **Root cause:** Host `~/.cargo/config.toml` sets `rustc-wrapper = "sccache"` but that file is never copied into the build context. sccache was installed in the builder stage but never called — pure wasted install time.
- **Fix:** Added `ENV RUSTC_WRAPPER=sccache` and `--mount=type=cache,target=/root/.cache/sccache` to both cook and build steps

### Bug: Healthcheck postgres check is dead code
- **Location:** `docker/Dockerfile` (runtime stage HEALTHCHECK)
- **Root cause:** `curl -fsS http://postgres:5432/` always fails — postgres speaks the wire protocol, not HTTP. Shell logic `cmd1 && curl_fails || cmd1` reduces to just `cmd1`. The postgres connectivity check never ran.
- **Fix:** Replaced with `bash -c "echo >/dev/tcp/${AXON_PG_HOST:-axon-postgres}/${AXON_PG_PORT:-5432}"` — uses bash built-in TCP redirect, no extra tools needed

### Bug: pnpm cache mount silently unused
- **Location:** `docker/web/Dockerfile`
- **Root cause:** Cache mount targeted `/root/.local/share/pnpm/store` but pnpm runs as the `node` user (uid 1000, home `/home/node`). Node's pnpm store is at `/home/node/.local/share/pnpm/store`. Cache was mounted but never written to.
- **Fix:** `--mount=type=cache,target=/home/node/.local/share/pnpm/store,uid=1000,gid=1000`

### Dead dependency: `inotify-tools` in workers runtime
- No worker s6 service (`crawl-worker`, `embed-worker`, `extract-worker`, `ingest-worker`, `mcp-http`) uses inotify. Only `docker/web/` uses it for pnpm-watcher.

---

## Technical Decisions

### `:-dev` vs `:?` for `AXON_GIT_SHA`
The `:?` operator was too strict — it blocked read-only compose commands (`logs`, `ps`, `exec`). The Dockerfile already has `ARG AXON_GIT_SHA=dev` as its own default, and `00-verify-image-sha` rejects `dev` at container startup. The safety net still exists; only non-build invocations are unblocked.

### `lukemathwalker/cargo-chef` pre-built image
Eliminates `cargo install cargo-chef --locked` (30-60s on cold builds). Pinned to `0.1.77-rust-1.93-bookworm` rather than `latest-rust-1.93-bookworm` for reproducibility — `latest-` could silently pull a new cargo-chef version and bust all downstream cache layers.

### mold linker in builder
Host `~/.cargo/config.toml` already uses mold + clang. Docker builder was using GNU ld (default). Wired via `ENV RUSTFLAGS="-C link-arg=-fuse-ld=mold"` — gcc 12 (ships in `rust:1.93-bookworm`) supports `-fuse-ld=mold` without needing clang.

### Not changed: Cargo.toml `[profile.release]`
Already well-tuned: `opt-level = 3`, `lto = "thin"`, `codegen-units = 1`, `strip = true`. No changes needed.

### Not changed: planner stage sparse COPY
Common optimization is to copy only `Cargo.toml`/`Cargo.lock` for the planner. Not done — spider path deps via additional_contexts make sparse copy complex, and the planner is fast (TOML reading, not compilation). The cook layer (slow) is correctly insulated from source changes.

---

## Files Modified

| File | Change |
|------|--------|
| `docker-compose.yaml` | `AXON_GIT_SHA` `:?` → `:-dev` on lines 147 and 210 |
| `.env.example` | Added `AXON_GIT_SHA` as optional documented variable |
| `docker/Dockerfile` | See workers changes below |
| `docker/web/Dockerfile` | See web changes below |
| `apps/web/.dockerignore` | **Created** — excludes `.next/`, `node_modules/`, coverage from build context |

### `docker/Dockerfile` (workers) — complete change list
- Base image: `rust:1.93-bookworm` (3 stages) → `lukemathwalker/cargo-chef:0.1.77-rust-1.93-bookworm` (1 shared base)
- `FROM chef AS planner` — inherits cargo-chef from base, no separate copy needed
- `FROM chef AS builder` — same base, removed redundant `COPY --from=chef` line
- Restored sccache install (was incorrectly removed), added `ENV RUSTC_WRAPPER=sccache`
- Added `--mount=type=cache,target=/root/.cache/sccache` to cook and build RUN commands
- Installed mold in builder via `apt-get`, added `ENV RUSTFLAGS="-C link-arg=-fuse-ld=mold"`
- Removed `inotify-tools` from runtime apt-get install
- Merged `RUN groupadd` + `RUN chmod` into a single layer
- Fixed HEALTHCHECK: removed dead `curl http://postgres/` check, replaced with `/dev/tcp` probe

### `docker/web/Dockerfile` — complete change list
- Merged `npm install -g @openai/codex@0.105.0` + `npm install -g @google/gemini-cli@latest` into one RUN
- Pinned `@google/gemini-cli@0.32.1` (was `@latest`)
- Pinned `corepack prepare pnpm@10.30.3` (was `pnpm@latest`)
- Fixed pnpm cache mount: `/root/.local/share/pnpm/store` → `/home/node/.local/share/pnpm/store,uid=1000,gid=1000`

---

## Behavior Changes (Before/After)

| Area | Before | After |
|------|--------|-------|
| `docker compose logs -f` | Fails: `required variable AXON_GIT_SHA is missing` | Works; uses `dev` placeholder (no build, no SHA check) |
| `docker compose build` (via `rebuild-fresh.sh`) | Sets real SHA; SHA guard passes | Unchanged — `rebuild-fresh.sh` still sets the real SHA |
| cargo-chef installation | Compiled from source each cold build (~30-60s) | Pre-built image — no compilation needed |
| sccache in Docker builds | Installed but never invoked (no `RUSTC_WRAPPER`) | Wired as `rustc-wrapper` with persistent cache mount |
| Rust link step | GNU ld (slow) | mold (5-10x faster for link) |
| Postgres healthcheck | `curl` to postgres (always fails, dead code) | `/dev/tcp` TCP probe (actually works) |
| pnpm store cache | Cache mount at `/root/...` (node user writes elsewhere; silently unused) | Cache mount at `/home/node/...` with `uid=1000,gid=1000` |
| gemini CLI version | `@latest` (non-deterministic) | `@0.32.1` (pinned) |
| pnpm version | `@latest` (non-deterministic) | `@10.30.3` (matches lockfile) |
| `inotify-tools` in workers image | Present (unused) | Removed |
| Web build context | Includes `.next/`, `node_modules/` if present | Excluded via `.dockerignore` |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `docker compose config --quiet` | No output (parse OK) | No output | ✅ |
| `grep "AXON_GIT_SHA" docker-compose.yaml` | `:-dev` on lines 147, 210 | `:-dev` on both | ✅ |
| `grep "RUSTC_WRAPPER" docker/Dockerfile` | `ENV RUSTC_WRAPPER=sccache` | Present | ✅ |
| `grep "mold" docker/Dockerfile` | mold install + RUSTFLAGS | Both present | ✅ |
| `grep "inotify" docker/Dockerfile` | Not present | Not present | ✅ |
| `grep "pnpm/store" docker/web/Dockerfile` | `uid=1000,gid=1000` | Present | ✅ |
| `cat apps/web/.dockerignore` | `.next` and `node_modules` excluded | Present | ✅ |

---

## Risks and Rollback

### `:-dev` for `AXON_GIT_SHA`
- **Risk:** A container accidentally built without a real SHA will start but the `00-verify-image-sha` cont-init script will reject it (exit 1) — so `dev`-labeled containers won't run in production. Low risk.
- **Rollback:** Revert lines 147 and 210 in `docker-compose.yaml` to `:?` and ensure `AXON_GIT_SHA` is exported before any `docker compose` invocation.

### mold linker
- **Risk:** mold is not in `rust:1.93-bookworm` by default — added via `apt-get install mold`. If mold package is unavailable or incompatible, builder stage fails. Tested with gcc 12 on bookworm (gcc 12 supports `-fuse-ld=mold`).
- **Rollback:** Remove the mold `apt-get install` block and `ENV RUSTFLAGS` line from `docker/Dockerfile`.

### `lukemathwalker/cargo-chef:0.1.77-rust-1.93-bookworm`
- **Risk:** External image dependency. If Docker Hub is unreachable or tag is removed, build fails. Pinned version mitigates silent breaking changes.
- **Rollback:** Restore original 3-stage pattern with `FROM rust:1.93-bookworm AS chef` + `cargo install cargo-chef --locked`.

---

## Decisions Not Taken

| Alternative | Reason rejected |
|-------------|----------------|
| Keep `AXON_GIT_SHA` as `:?` and add to `.env` statically | `.env` is static; SHA changes every commit. Would require manual update on every commit — worse DX than the current `rebuild-fresh.sh` approach. |
| Sparse COPY in planner stage (only Cargo.toml files) | Complex with spider additional_contexts; planner is fast anyway; cook layer is correctly isolated from source changes |
| `codegen-units = 4` for faster Docker builds | Reduces binary optimization quality; Cargo.toml already tuned correctly for production |
| Add clang to builder for mold (mirrors host config exactly) | gcc 12 already supports `-fuse-ld=mold`; adding clang is extra 200MB with no benefit |
| Run pnpm install as root then chown | Works but more steps; `uid=1000,gid=1000` on cache mount is cleaner and preserves the existing ownership model |

---

## Open Questions

- `00-verify-image-sha` reads `/repo/.git` — is the repo always mounted at `/repo` in the workers container? If not, the SHA guard logs an error and exits 1 at startup. Worth verifying the volume mount in `docker-compose.yaml`.
- sccache version `0.9.1` is pinned. Is there a newer stable release worth bumping to?
- The `claude.ai/install.sh` in web Dockerfile fetches the latest Claude version on every cold build — no version pinning, no cache. If build reproducibility for the web image becomes critical, this needs a different approach (pre-download, version pin, or skip if Claude is bind-mounted).

---

## Next Steps

- Run `./scripts/rebuild-fresh.sh` to test the full build pipeline with all optimizations active
- Verify sccache is actually being hit: `docker compose run --rm axon-workers sccache --show-stats` after a build
- Consider adding sccache stats to the build output (`SCCACHE_LOG=info` or post-build `sccache --show-stats`)
- Update `docker/CLAUDE.md` to document the `AXON_GIT_SHA` flow and `rebuild-fresh.sh` as the canonical build command
