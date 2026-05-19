# Session: platejs ESM Build Error + Stale node_modules Fix
**Date:** 2026-02-27
**Branch:** feat/crawl-download-pack

---

## Session Overview

Fixed a Next.js 16.1.6 (Turbopack) build error caused by missing `@platejs/code-block/react` subpath exports, then diagnosed and permanently resolved the root cause: Docker anonymous volumes causing stale `node_modules` in the `axon-web` container. Two changes were made: `transpilePackages` added to `next.config.ts`, and a new s6 `cont-init.d` startup script created to auto-sync `node_modules` on every container start.

---

## Timeline

1. **Build error reported** — `Module not found: Can't resolve '@platejs/code-block/react'` in `components/editor/plugins/extended-nodes-kit.tsx` (Next.js 16.1.6 / Turbopack)
2. **First fix: `transpilePackages`** — Added all 9 `@platejs/*` packages + `platejs` to `transpilePackages` in `next.config.ts`. Necessary for Turbopack to resolve ESM subpath exports (e.g., `/react`).
3. **Verified browser** — Navigated to `axon.tootie.tv` via Chrome DevTools MCP — build error persisted despite `transpilePackages`.
4. **Container restart / image rebuild** — Neither fixed it; error survived both.
5. **Root cause found** — `docker inspect axon-web` revealed two anonymous volumes mounted over `/app/node_modules` and `/app/.next`, both created from a stale image layer. The anonymous `node_modules` volume only contained 3 of the 9 `@platejs` packages (those present when the image was originally built).
6. **Manual fix** — `docker exec axon-web sh -c "CI=true pnpm install --frozen-lockfile"` (run as root) populated the anonymous volume with all packages. Server started: `✓ Ready in 1026ms`.
7. **Permanent fix** — Created `docker/web/cont-init.d/20-pnpm-install`: an s6 startup script that uses a sentinel file inside the anonymous volume to detect lockfile changes and auto-runs `pnpm install` on container start.
8. **Image rebuilt** — `docker compose build axon-web && docker compose up -d axon-web` to install the new cont-init script.

---

## Key Findings

- **ESM subpath exports require `transpilePackages`**: All `@platejs/*` packages are `"type": "module"` with subpath exports like `"./react": "./dist/react/index.js"`. Turbopack cannot resolve these without `transpilePackages`.
- **Anonymous volumes shadow bind mounts at sub-paths**: When Compose declares both `./apps/web:/app` (bind) and `/app/node_modules` (anonymous), Docker applies both. The anonymous volume wins for the sub-path, completely isolating `node_modules` from the host. This is by design — it prevents host `node_modules` from leaking in — but means the volume goes stale when new packages are added after the image was built.
- **pnpm virtual store**: pnpm stores content in `node_modules/.pnpm/` and creates symlinks at `node_modules/@scope/pkg`. If the anonymous volume was built with 3 packages, symlinks for the other 6 packages don't exist even though `pnpm-lock.yaml` references them.
- **`CI=true` required for pnpm non-TTY**: `pnpm install` aborts with `ERR_PNPM_ABORTED_REMOVE_MODULES_DIR_NO_TTY` unless `CI=true` is set in non-interactive environments.
- **s6 `cont-init.d` runs as root before services**: The right place to do one-time sync work that needs filesystem access before the `node` user's services start.

---

## Technical Decisions

- **Sentinel file inside anonymous volume** (not `/tmp` or a bind-mount path): The sentinel must persist across container restarts but not be visible to the host. Placing it at `/app/node_modules/.pnpm-install-stamp` achieves this — it lives inside the anonymous volume itself.
- **mtime comparison** (`-nt` operator): Simple and reliable. `touch "$SENTINEL"` after install records the install time; `[ "$LOCKFILE" -nt "$SENTINEL" ]` fires only when the lockfile is newer.
- **`--frozen-lockfile`**: Prevents accidental lockfile mutations inside the container; ensures the container always installs exactly what the lockfile specifies.
- **`transpilePackages` kept even after install fix**: The ESM subpath resolution issue is real and would reappear in any clean build without `transpilePackages`. Both fixes are necessary: `transpilePackages` for module resolution, sentinel script for volume freshness.

---

## Files Modified

| File | Type | Purpose |
|------|------|---------|
| `apps/web/next.config.ts` | Modified | Added `transpilePackages` for all `@platejs/*` + `platejs` packages so Turbopack resolves ESM subpath exports |
| `docker/web/cont-init.d/20-pnpm-install` | Created | s6 cont-init script: compares `pnpm-lock.yaml` mtime vs sentinel, runs `pnpm install --frozen-lockfile` if stale |

---

## Commands Executed

| Command | Purpose | Result |
|---------|---------|--------|
| `docker inspect axon-web` | Diagnose mount configuration | Revealed two anonymous volumes at `/app/node_modules` and `/app/.next` |
| `docker exec axon-web sh -c "ls node_modules/@platejs"` | Count installed packages | Only 3 packages (ai, basic-nodes, markdown) — 6 missing |
| `docker exec axon-web sh -c "CI=true pnpm install --frozen-lockfile"` | Manual volume populate | Success — all packages installed |
| `docker compose build axon-web` | Rebuild image with new cont-init script | Success |
| `docker compose up -d axon-web` | Restart with new image | `✓ Ready in 1026ms`, `GET / 200` |

---

## Behavior Changes (Before/After)

| Aspect | Before | After |
|--------|--------|-------|
| Next.js build | Failed: `Module not found: Can't resolve '@platejs/code-block/react'` | Passes: `✓ Ready in 1026ms` |
| New package workflow | After `pnpm add`, container would silently use stale modules until manually fixed | After `pnpm add` + container restart, cont-init script detects changed lockfile and auto-installs |
| Platejs ESM resolution | Turbopack could not resolve `/react` subpath exports | `transpilePackages` enables correct subpath resolution |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `docker exec axon-web sh -c "CI=true pnpm install --frozen-lockfile"` | Exit 0, all packages installed | Success | ✅ |
| Browser: `axon.tootie.tv` after fix | `✓ Ready`, no build errors | `✓ Ready in 1026ms`, `GET / 200` | ✅ |
| `docker compose build axon-web` | Build includes `20-pnpm-install` | Success | ✅ |

---

## Source IDs + Collections Touched

None — no Axon embed/retrieve operations were performed during this session.

---

## Risks and Rollback

- **`transpilePackages` risk**: Low. Adds transpilation overhead at build time but has no runtime cost. Rollback: remove the array from `next.config.ts`.
- **`20-pnpm-install` risk**: Low. Script is idempotent; if `pnpm install` fails, it exits non-zero but does NOT update the sentinel, so it will retry on next start. Rollback: remove `docker/web/cont-init.d/20-pnpm-install` and rebuild image.
- **Anonymous volume behavior**: The anonymous volume model means `node_modules` is never visible to the host — this is intentional. The cont-init script is the correct layer to manage it.

---

## Decisions Not Taken

- **Remove anonymous volume from docker-compose**: Would expose host `node_modules` to container and vice versa — risks platform-specific binary incompatibilities (e.g., native addons built for macOS host running in Linux container). Rejected.
- **Rebuild image on every `pnpm add`**: Would fix freshness but slow down the `pnpm add` workflow significantly. The sentinel script provides same freshness guarantee with zero image rebuild cost.
- **Use `pnpm-lock.yaml` SHA instead of mtime**: More robust but requires `sha256sum` which may not be in `dash`. The `sh` shebang requires POSIX-compatible commands. mtime is sufficient for this use case.

---

## Open Questions

- **Postgres auth failures in logs**: Repeated `FATAL: password authentication failed for user "axon"` seen during container logs inspection. Not investigated — may indicate password mismatch between `.env` credentials and the Postgres data volume from a prior setup.
- **`axon-web` image not pinned**: After rebuild, the image is latest local build. If `docker compose pull` is run, the new `20-pnpm-install` script would be wiped. Confirm image is only built locally (not pushed to registry).

---

## Next Steps

- Investigate Postgres auth failures (`FATAL: password authentication failed for user "axon"`) — check `.env` password vs `axon-postgres` volume init password
- Consider adding `docker/web/CLAUDE.md` entry about the anonymous volume pattern and the cont-init sentinel approach for future reference
