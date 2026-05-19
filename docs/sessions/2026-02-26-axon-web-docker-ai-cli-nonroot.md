# Session: axon-web Docker — AI CLI Install + Non-Root + AXON_WORKSPACE

**Date:** 2026-02-26
**Branch:** feat/crawl-download-pack
**Commit:** 6f8f7c7

---

## Session Overview

Diagnosed and fixed a 502 Bad Gateway error on `axon.tootie.tv` where Pulse chat (`/api/pulse/chat`) was failing. Root cause was a missing `claude` binary in the `axon-web` container. Refactored the entire `axon-web` Docker setup: switched base image to `node:24-slim`, installed Claude/Codex/Gemini CLIs inside the image, dropped to non-root `node` user, added `~/.ssh` mount, and introduced `AXON_WORKSPACE` for host workspace bind-mount. Fixed two pre-commit hook failures (rustfmt + clippy) blocking the commit.

---

## Timeline

1. **502 Debug** — Traced Cloudflare 502 → Next.js `POST /api/pulse/chat` → subprocess spawn of `claude` binary → binary missing in `node:24-alpine` container
2. **glibc/musl fix** — Switched from `node:24-alpine` to `node:24-slim` (Debian/glibc)
3. **MCP hang fix** — Added `--strict-mcp-config` to claude subprocess args; `~/.claude.json` had 7 MCP servers that all tried to initialize and hung
4. **CLI install in image** — Moved from bind-mounting binary to installing Claude/Codex/Gemini inside the Dockerfile
5. **Non-root user** — Switched to `USER node`; fixed anonymous volume permissions; updated mount paths to `/home/node/`
6. **SSH mount** — Added `~/.ssh:/home/node/.ssh:ro` + `openssh-client` in apt-get
7. **AXON_WORKSPACE** — Added env var + bind-mount: `${AXON_WORKSPACE:-${HOME}/workspace}:/workspace`
8. **Pre-commit fixes** — `cargo fmt` (line too long in `processor.rs:346`), restored all three `_with_pool` re-exports in `refresh/mod.rs` (used by `crates/cli/commands/refresh/schedule.rs`)
9. **Commit + push** — SHA `6f8f7c7` pushed to `feat/crawl-download-pack`

---

## Key Findings

- **Root cause of 502**: `claude` is a glibc ELF binary; `node:24-alpine` uses musl libc → binary silently missing at runtime
- **MCP initialization hang**: Even with binary present, `~/.claude.json` has 7 configured MCP servers; subprocess tried to initialize all before responding; fixed with `--strict-mcp-config` at `apps/web/app/api/pulse/chat/route.ts`
- **`@openai/codex@latest` broken**: Version `0.106.0-linux-x64` is a platform-specific binary package with no `bin` field; `0.106.0` tarball returned 404 from npm registry. Pinned to `0.105.0`.
- **Anonymous volume ownership**: Previous root-owned `/app/.next` and `/app/node_modules` anonymous volumes prevented `node` user from writing. Fix: pre-create dirs with `chown node:node` in Dockerfile, delete stale volumes.
- **`refresh/mod.rs` false clippy alarm**: All three `pub(crate)` re-exports (`claim_due_refresh_schedules_with_pool`, `mark_refresh_schedule_ran_with_pool`, `start_refresh_job_with_pool`) are genuinely used — by `crates/cli/commands/refresh/schedule.rs:9-11` and `worker.rs:2`. Previous session incorrectly identified them as unused.

---

## Technical Decisions

| Decision | Rationale |
|---|---|
| Install CLIs inside image vs bind-mount binary | Eliminates glibc/musl mismatch; image is self-contained; version pinned in Dockerfile |
| `node:24-slim` (Debian) over `node:24-alpine` | `claude` binary requires glibc; alpine uses musl; slim is the smallest glibc-based official image |
| `--strict-mcp-config` in claude subprocess | Prevents subprocess from hanging waiting for unreachable MCP servers in container network |
| Pinned `@openai/codex@0.105.0` | `@latest` resolves to broken `0.106.0`; `0.105.0` is last known-good version |
| `AXON_WORKSPACE` as both env var and mount | Container gets clean `/workspace` path regardless of host dir; user-configurable via `.env` |
| Keep all three `_with_pool` re-exports in `refresh/mod.rs` | All three are used by `crates/cli/commands/refresh/schedule.rs`; removing any breaks compilation |

---

## Files Modified

| File | Change |
|---|---|
| `docker/web/Dockerfile` | Switch to `node:24-slim`, install claude/codex/gemini, `USER node`, pre-create dirs |
| `docker-compose.yaml` | Remove binary mount, update paths to `/home/node/`, add SSH mount, add AXON_WORKSPACE |
| `apps/web/app/api/pulse/chat/route.ts` | Add `--strict-mcp-config` to claude subprocess args |
| `.env.example` | Document `AXON_WORKSPACE` env var |
| `CHANGELOG.md` | Add commit entries for current session |
| `crates/jobs/refresh/processor.rs` | `cargo fmt` fixed line-length at line 346 |
| `crates/jobs/refresh/mod.rs` | Restored full `pub(crate) use schedule::{ ... }` block (was unchanged net result) |

---

## Commands Executed

```bash
# Identified 502 source
docker compose logs axon-web --tail=50

# Confirmed claude binary missing
docker compose exec axon-web which claude   # → not found

# Fixed Dockerfile and rebuilt
docker compose up -d --build axon-web

# Tested SSH from container
docker compose exec axon-web ssh dookie   # refused (host issue)
docker compose exec axon-web ssh tootie   # OK

# Fixed pre-commit failures
cargo fmt
cargo clippy  # → 0 errors
cargo fmt --check  # → clean

# Committed and pushed
git add . && git commit -m "feat(docker): ..."
git push
# → 6f8f7c7 pushed to feat/crawl-download-pack
```

---

## Behavior Changes (Before/After)

| Area | Before | After |
|---|---|---|
| Pulse chat (`/api/pulse/chat`) | 502 Bad Gateway — claude binary missing | Works; claude installed in image |
| Container user | root | `node` (UID 1001) |
| Base image | `node:24-alpine` (musl) | `node:24-slim` (Debian/glibc) |
| CLI installation | Binary bind-mounted from host | Installed in image layer |
| MCP init on chat | Hung 60s+ waiting for 7 MCP servers | Returns immediately with `--strict-mcp-config` |
| Host workspace access | None | Mounted at `/workspace` via `AXON_WORKSPACE` |
| SSH access from container | Not available | `~/.ssh` mounted ro, `openssh-client` installed |
| Agent config mounts | `:ro` on `.claude`, `.gemini`, `.codex` | Read-write (no `:ro`) |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---|---|---|---|
| `cargo fmt --check` | No changes | No output | ✅ |
| `cargo clippy` | 0 errors | 0 errors | ✅ |
| `cargo check` | Compiles | No errors | ✅ |
| `lefthook pre-commit` | All checks pass | monolith ✔, rustfmt ✔, clippy ✔ | ✅ |
| `git push` | Pushed to remote | `f5eb415..6f8f7c7` | ✅ |

---

## Source IDs + Collections Touched

None — this session was infrastructure/Docker/Rust fixes. No Axon embed/retrieve operations performed during the session.

---

## Risks and Rollback

- **Codex pinned to 0.105.0**: If a newer working version is released, update `docker/web/Dockerfile` line with `@openai/codex@NEW_VERSION`
- **Anonymous volumes**: If container is recreated and `/app/.next`/`/app/node_modules` volumes are root-owned (from a previous run), delete them: `docker compose down -v axon-web && docker volume rm <vol>` then `docker compose up -d axon-web`
- **`--strict-mcp-config` disables all MCPs in Pulse chat subprocess**: This is intentional — the subprocess runs in a container where no MCP hosts are reachable. If MCP access inside the subprocess is needed later, this flag must be removed and the MCPs configured for container-accessible endpoints.
- **Rollback**: `git revert 6f8f7c7` reverts all Docker/route changes in one commit

---

## Decisions Not Taken

- **Keep `node:24-alpine` + patch libc**: Would require installing glibc-compat layer on alpine; fragile and non-standard
- **Bind-mount `claude` binary from host**: Works but ties container to host binary version and path; breaks on host upgrades
- **Mount `.claude`, `.gemini`, `.codex` as `:ro`**: User explicitly required read-write so CLIs can write session data / conversation history
- **Remove all three `_with_pool` re-exports**: Would break `crates/cli/commands/refresh/schedule.rs` compilation

---

## Open Questions

- Is `@openai/codex@0.105.0` the correct latest stable? The npm `@latest` tag was pointing to `0.106.0` which had a broken tarball. Worth monitoring for a working `0.106.x` release.
- `dookie` SSH host refused connection — unknown if that's expected (host down, wrong hostname, firewall) or a new issue.
- `gemini-cli@latest` was installed without pinning — could break on future release. Consider pinning once a stable version is identified.

---

## Next Steps

- Rebuild `axon-web` container in production and verify Pulse chat end-to-end
- Monitor `@openai/codex` releases; update pin in Dockerfile when a working `0.106.x` lands
- Consider pinning `@google/gemini-cli` to a specific version
- Research pre-built dev container images with AI CLIs (was interrupted by `/quick-push`)
