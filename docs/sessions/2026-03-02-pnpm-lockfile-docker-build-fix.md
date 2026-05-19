# Session: pnpm Lockfile / Docker Build Fix

**Date:** 2026-03-02
**Branch:** feat/sidebar
**Duration:** ~10 minutes

---

## Session Overview

Diagnosed and fixed a Docker build failure for `axon-web`. The `pnpm install --frozen-lockfile` step in `docker/web/Dockerfile:77` exited with code 1 because `apps/web/pnpm-lock.yaml` was out of sync with `apps/web/package.json`. Fixed by regenerating the lockfile on the host, then verified the image built successfully.

---

## Timeline

1. **Build failure presented** — `docker compose up` failed with `exit code: 1` on Dockerfile line 77
2. **Read Dockerfile** — Confirmed line 77: `RUN chown -R node:node /app && su node -s /bin/sh -c "pnpm install --frozen-lockfile"`
3. **Ran pnpm install locally** — `cd apps/web && pnpm install --frozen-lockfile` reproduced the exact error message: `ERR_PNPM_OUTDATED_LOCKFILE`
4. **Identified root cause** — Lockfile `dependencies` section contained `@faker-js/faker` and `@types/pg` that were no longer in `package.json`
5. **Fixed** — Ran `pnpm install` (no `--frozen-lockfile`) in `apps/web`; lockfile regenerated
6. **Verified** — `pnpm install --frozen-lockfile` passed; `docker compose build axon-web` succeeded

---

## Key Findings

- **Root cause:** `package.json` was modified (packages removed) without running `pnpm install` to regenerate `pnpm-lock.yaml`. Stale entries: `@faker-js/faker@10.3.0` and `@types/pg@8.18.0` in lockfile `dependencies` but absent from `package.json`.
- **`--frozen-lockfile` is intentional** — It's a build-time invariant in the Dockerfile ensuring the image always has a deterministic, verified dependency set. It cannot and should not auto-fix drift.
- **Auto-sync gap** — The `pnpm-watcher` + `cont-init.d/20-pnpm-install` mechanisms keep `node_modules` in sync *inside a running container*. They cannot regenerate a stale lockfile during `docker build`.
- **Correct workflow** — When editing `package.json` directly: run `pnpm install` on host → commit lockfile → then build.

---

## Technical Decisions

- **No change to `--frozen-lockfile` flag** — The flag provides a meaningful safety gate; removing it would silently allow stale builds. The correct fix is maintaining the lockfile, not loosening the gate.
- **Regenerate, don't skip** — Used `pnpm install` (not `--no-frozen-lockfile`) to properly resolve the dependency tree rather than papering over the mismatch.

---

## Files Modified

| File | Change |
|------|--------|
| `apps/web/pnpm-lock.yaml` | Regenerated to match `package.json`; removed stale `@faker-js/faker` and `@types/pg` entries from `dependencies` |

---

## Commands Executed

```bash
# Reproduced the error
cd apps/web && pnpm install --frozen-lockfile
# → ERR_PNPM_OUTDATED_LOCKFILE

# Fixed lockfile
cd apps/web && pnpm install
# → Done in 3s

# Verified lockfile now passes frozen check
pnpm install --frozen-lockfile
# → Lockfile is up to date, Already up to date, Done in 852ms

# Verified image builds
docker compose build axon-web
# → Image axon-axon-web Built ✓
```

---

## Behavior Changes (Before/After)

| Aspect | Before | After |
|--------|--------|-------|
| `docker compose up` / build | Failed: `exit code: 1` at pnpm install step | Succeeds: image builds cleanly |
| `pnpm install --frozen-lockfile` in `apps/web` | `ERR_PNPM_OUTDATED_LOCKFILE` | Passes: "Lockfile is up to date" |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `pnpm install --frozen-lockfile` (apps/web) | Exits 0 | "Lockfile is up to date, Already up to date" | ✅ |
| `docker compose build axon-web` | Image builds | "Image axon-axon-web Built" | ✅ |

---

## Risks and Rollback

- **Risk:** None — lockfile regeneration is a safe, deterministic operation. The only observable change is the lockfile file itself.
- **Rollback:** `git checkout apps/web/pnpm-lock.yaml` to restore previous lockfile (but this would re-introduce the build failure).

---

## Decisions Not Taken

- **Remove `--frozen-lockfile`** — Would eliminate the error but lose build determinism and the early-warning signal for lockfile drift. Rejected.
- **Use `--no-frozen-lockfile` in the Dockerfile** — Same problem as above; the Dockerfile should enforce reproducibility.

---

## Open Questions

- What specific change to `package.json` introduced the drift (which PR/commit removed `@faker-js/faker` and `@types/pg` from `dependencies`)? Not investigated — not blocking.
- Should a pre-commit or CI hook verify lockfile consistency (`pnpm install --frozen-lockfile --dry-run`)? Currently no such gate exists.

---

## Next Steps

- Consider adding a CI check: `pnpm install --frozen-lockfile` as a lint step so lockfile drift is caught before `docker build` fails.
- The three parallel images (`axon-web`, `axon-workers`, `axon-chrome`) were all building simultaneously; verify `axon-workers` and `axon-chrome` also completed successfully with `docker compose ps`.
