# Session: pnpm-watcher, Docker tightening, PlateJS deps
Date: 2026-02-28
Branch: feat/crawl-download-pack
Commit: b2d8a74

## Session Overview

Diagnosed and resolved a build error caused by missing packages in the axon-web
Docker container, then built a permanent fix so packages are auto-installed when
`pnpm-lock.yaml` changes. Tightened three additional Docker infrastructure issues
discovered in the process.

## Timeline

1. **Build error** — `Module not found: Can't resolve '@tanstack/react-virtual'`
   surfaced from Next.js/Turbopack inside the container.
2. **Root cause identified** — anonymous Docker volume at `/app/node_modules`
   shadowed the image's freshly built `node_modules`; stale volume lacked the new
   package's hoisted symlink.
3. **Immediate fix** — manually created the missing pnpm symlink, cleared stale
   anonymous volumes, rebuilt image.
4. **Permanent fix** — added `pnpm-watcher` s6 service that polls `pnpm-lock.yaml`
   every 3 s and auto-runs `pnpm install` + restarts `pnpm-dev` on change.
5. **Tightening pass** — fixed three additional issues: chrome health check hang,
   chrome dependency cascade, missing pnpm-watcher log directory.
6. **Commit + push** — biome format auto-fixed 3 files; `b2d8a74` pushed to remote.

## Key Findings

- `node_modules` anonymous volume is created from the image at first container
  start. Subsequent rebuilds write a new image but the old volume is mounted over
  it — new packages are invisible until the volume is recreated or `pnpm install`
  runs inside the container.
- pnpm hoisting bug: when `@platejs/floating` was added as a direct dep it was
  already in the virtual store as a transitive dep. pnpm skipped creating the
  top-level symlink at `node_modules/@platejs/floating`, causing the module-not-found
  error even after a successful `pnpm install --frozen-lockfile`.
- `inotifywait` does NOT receive events for host-side writes through Docker bind
  mounts on this setup — same root cause as the `WATCHPACK_POLLING=true` note in
  `docker-compose.yaml`. Confirmed by inotifywait producing no output while host
  `touch pnpm-lock.yaml` changed the mtime visible inside the container via `stat`.
- `node_modules` directory is root-owned inside the container (created by root
  during image build). Running `pnpm install` with `s6-setuidgid node` fails with
  EACCES. Must run as root.
- `axon-chrome` health check curl had no `--max-time`, so it hung for >5 s (the
  timeout window) when Chrome's GPU process crashed, causing the health check to
  report "exceeded timeout" rather than a clean failure.
- `axon-workers` depending on `axon-chrome: service_healthy` caused a cascade:
  Chrome GPU crash → workers unhealthy → `docker compose up -d axon-web` failed
  with "dependency failed to start". Workers operate fine without Chrome (HTTP
  crawl mode), so the dependency should be `service_started`.

## Technical Decisions

| Decision | Rationale |
|---|---|
| Polling (3 s) over inotifywait | inotify events don't propagate from host through bind mounts; polling is 100% reliable |
| Run `pnpm install` as root | `node_modules` volume is root-owned; node user cannot write to it |
| `service_started` for chrome dep | Chrome health is not required for workers to serve HTTP crawls; hard dep caused false outages |
| `--max-time 4` on chrome curl | Curl must fail within the 5 s timeout window; without it the check exceeds timeout every time |
| Keep `--frozen-lockfile` | Prevents accidental lockfile mutation inside the container; host is the source of truth |

## Files Modified

| File | Change |
|---|---|
| `docker/web/s6-rc.d/pnpm-watcher/run` | **Created** — s6 longrun service: 3 s poll loop, pnpm install, pnpm-dev restart |
| `docker/web/s6-rc.d/pnpm-watcher/type` | **Created** — `longrun` |
| `docker/web/s6-rc.d/user/contents.d/pnpm-watcher` | **Created** — registers service in s6 user bundle |
| `docker/web/Dockerfile` | Added `/var/log/axon/pnpm-watcher` mkdir |
| `docker-compose.yaml` | chrome healthcheck: added `--max-time 4`; axon-chrome dep: `service_healthy` → `service_started` |
| `docker/CLAUDE.md` | Updated files tree, replaced "rebuild after adding deps" with auto-sync workflow docs |
| `CLAUDE.md` | Updated axon-web service table row |
| `.gitignore` | Added `/node_modules/`, `/package.json`, `/pnpm-lock.yaml` (root-level artifacts) |
| `apps/web/package.json` | Added `@tanstack/react-virtual`, `@platejs/floating` and related packages |
| `apps/web/pnpm-lock.yaml` | Updated lockfile |
| `CHANGELOG.md` | Added `8d85538`, `5dc43f1` entries |
| `docs/sessions/` (this file) | Session documentation |

## Commands Executed

```bash
# Check package in container node_modules
docker exec axon-web ls /app/node_modules/@platejs/

# Check pnpm virtual store — floating was there, symlink was missing
docker exec axon-web ls /app/node_modules/.pnpm/ | grep floating

# Create missing symlink manually (immediate fix)
STORE_DIR=$(ls /app/node_modules/.pnpm/ | grep '@platejs+floating')
ln -s "../.pnpm/${STORE_DIR}/node_modules/@platejs/floating" /app/node_modules/@platejs/floating

# Rebuild image and cycle container
docker compose build axon-web
docker stop axon-web && docker rm axon-web
docker compose create axon-web && docker start axon-web

# Verify polling watcher fired
touch /home/jmagar/workspace/axon_rust/apps/web/pnpm-lock.yaml
# → "[pnpm-watcher] pnpm-lock.yaml changed — running pnpm install"

# End-to-end test: add real package
pnpm add is-odd   # host
# → container: detected within 3 s, installed, pnpm-dev restarted
docker exec axon-web ls node_modules/is-odd   # confirmed present
pnpm remove is-odd   # cleaned up

# Verify docker compose up now works (chrome no longer blocks)
docker compose up -d axon-web   # → "Container axon-web Started" (no chrome failure)
```

## Behavior Changes (Before/After)

| Scenario | Before | After |
|---|---|---|
| `pnpm add <pkg>` on host | Package missing in container; must rebuild image + delete volumes | Detected within 3 s, installed automatically, Next.js restarted |
| `docker compose up -d axon-web` with chrome GPU crash | Fails: "dependency axon-chrome failed to start" | Starts normally; chrome health doesn't block web |
| Chrome health check when port hangs | `curl` hangs >5 s, reported as "exceeded timeout" | Fails cleanly within 4 s (`--max-time 4`) |
| pnpm-watcher logs | Only visible in `docker logs axon-web` | Also available at `/var/log/axon/pnpm-watcher/current` |
| Root-level node_modules in git | Showed as untracked `??` | Gitignored via `/node_modules/`, `/package.json`, `/pnpm-lock.yaml` |

## Verification Evidence

| Command | Expected | Actual | Status |
|---|---|---|---|
| `docker exec axon-web ls /var/log/axon/` | `pnpm-dev pnpm-watcher` | `pnpm-dev pnpm-watcher` | ✅ |
| `docker logs axon-web \| grep pnpm-watcher` | `polling /app/pnpm-lock.yaml every 3s` | `[pnpm-watcher] polling /app/pnpm-lock.yaml every 3s` | ✅ |
| `pnpm add is-odd` → 6 s → watcher logs | install + restart logged | `done — restarting pnpm-dev` | ✅ |
| `docker exec axon-web ls node_modules/is-odd` | package files present | `LICENSE README.md index.js package.json` | ✅ |
| `docker compose up -d axon-web` (chrome unhealthy) | Container starts | `Container axon-web Started` | ✅ |
| `docker logs axon-web \| grep Ready` | `✓ Ready in ...ms` | `✓ Ready in 1083ms` | ✅ |

## Source IDs + Collections Touched

_Populated after Axon embed completes below._

## Risks and Rollback

- **pnpm-watcher** restarts `pnpm-dev` mid-request if a package is added while
  someone is actively using the UI. Risk is low (dev-only container); rollback:
  remove `pnpm-watcher` from `user/contents.d/` and rebuild.
- **`service_started` for chrome** means workers can boot without a healthy Chrome.
  Chrome-mode crawls will fail at runtime rather than at startup. This is the
  correct behavior — workers should not be blocked by an optional dependency.
- **Root gitignore entries** (`/node_modules`, `/package.json`, `/pnpm-lock.yaml`)
  are intentionally scoped to the repo root with leading `/`. They do not affect
  `apps/web/` entries.

## Decisions Not Taken

| Alternative | Rejected because |
|---|---|
| inotifywait-based watcher | Doesn't receive host bind-mount write events on this setup; polling is simpler and reliable |
| Run pnpm as `node` user | `node_modules` volume is root-owned; would require chown pass on every install |
| Delete anonymous volumes on every rebuild | Loses the performance benefit of cached node_modules between normal restarts |
| `service_healthy` for chrome with longer timeout | Doesn't fix the root cause (GPU crash); `service_started` is semantically correct |

## Open Questions

- Root-level `package.json` and `pnpm-lock.yaml` — unclear how they appeared.
  Likely from a `pnpm install` run at the repo root. Now gitignored.
- GitHub Dependabot flagged 2 high vulnerabilities on the default branch (reported
  during push). Not investigated this session.
- Chrome GPU crash is a pre-existing issue (`GPU process isn't usable. Goodbye.`
  in logs). Root cause not investigated; axon-chrome container appears to recover.

## Next Steps

- Investigate and address the 2 Dependabot high vulnerabilities on main.
- Investigate Chrome GPU crash root cause (`docker logs axon-chrome` shows
  `GPU stall due to ReadPixels` before fatal exit).
- Address user request: editor only taking up ~half the available space in Pulse
  workspace (raised at end of session, not yet implemented).
