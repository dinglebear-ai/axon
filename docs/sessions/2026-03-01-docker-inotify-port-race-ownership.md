# Session: Docker inotify Fix, EADDRINUSE Port Race, node_modules Ownership

**Date:** 2026-03-01
**Branch:** feat/crawl-download-pack
**Commit:** a5dc786c

---

## Session Overview

Three interconnected Docker/Next.js issues diagnosed and fixed in the `axon-web` container:

1. **inotify watch limit exhausted** — Next.js 16 + Turbopack hit the OS `fs.inotify.max_user_watches` ceiling (65536), causing a module-not-found cascade on startup.
2. **EADDRINUSE port race** — `pnpm-watcher` used `s6-svc -r` (immediate restart) which restarted Next.js before it released port 49010, causing a crash loop.
3. **root-owned `node_modules`** — Dockerfile ran `pnpm install` as root (chown ran before install), so the anonymous volume initialized with root-owned files. Next.js (running as `node`) hit EACCES when attempting TypeScript auto-install.

---

## Timeline

| Time | Activity |
|------|----------|
| Session start | User reported build error: `Module not found: Can't resolve '../next-devtools/userspace/pages/pages-dev-overlay-setup'` |
| ~T+5m | Diagnosed root cause as inotify limit (65536); increased to 524288 on host; persisted to `/etc/sysctl.d/99-inotify.conf`; restarted container — ✓ Ready |
| ~T+15m | User reported `EADDRINUSE: address already in use :::49010` after pnpm-watcher triggered a package sync |
| ~T+20m | Diagnosed crash loop: `s6-svc -r` restarts too fast; fixed to `s6-svc -d` + `sleep 3` + `s6-svc -u`; hot-copied into running container; rebuilt image |
| ~T+30m | User asked about EACCES on esbuild binary |
| ~T+35m | Traced to Dockerfile `chown` running before `pnpm install`; all node_modules root-owned; Next.js TypeScript auto-install (running as `node`) hitting EACCES |
| ~T+45m | Fixed Dockerfile to `su node -s /bin/sh -c "pnpm install"`; updated pnpm-watcher and 20-pnpm-install to use `s6-setuidgid node`; rebuilt image + nuked old anonymous volumes |
| ~T+50m | Verified clean startup: no EACCES, no EADDRINUSE, `✓ Ready in 969ms` |
| ~T+55m | Ran `/quick-push`; Biome caught unsorted imports in `ai-node.tsx`; auto-fixed; committed + pushed `a5dc786c` |

---

## Key Findings

- **inotify limit** (`/proc/sys/fs/inotify/max_user_watches`): was `65536`, needed `524288` for Next.js 16 + Turbopack + `node_modules` bind-mount inside Docker. Containers share the host kernel's inotify budget.
- **`WATCHPACK_POLLING=true` does NOT help for Turbopack** — that env var is webpack-only. Turbopack uses notify-rs/inotify directly; only raising the kernel limit works.
- **`s6-svc -r` is unsafe for port-bound services** — it sends SIGTERM then immediately schedules a restart. Node.js processes can take >0ms to release TCP sockets after process exit. Need explicit down/wait/up.
- **Dockerfile `chown` position matters** — `chown -R node:node /app` before `RUN pnpm install` means pnpm runs as root and writes root-owned files. The anonymous volume inherits those. Fix: run install as node user.
- **`20-pnpm-install` and `pnpm-watcher` must use the same user as `pnpm-dev`** — all three scripts interact with the same `node_modules` anonymous volume; mismatched user causes EACCES.
- **`node:24-slim` does not have `ss` in PATH** — `/usr/sbin/ss` exists but isn't on default PATH in container exec. Use full path or skip port-checking in scripts.

---

## Technical Decisions

| Decision | Rationale | Alternative Rejected |
|----------|-----------|---------------------|
| Raise inotify limit host-wide vs. per-container | Containers can't set inotify sysctls without `privileged: true` which is a security risk | `privileged: true` in docker-compose |
| `sleep 3` in pnpm-watcher vs. port-polling loop | Simple and reliable; `ss` not on PATH in container exec; 3s is ample for Node exit | Port-polling with `ss -Hlntp` |
| `su node -s /bin/sh -c "pnpm install"` in Dockerfile | Cleaner than switching `USER node` mid-Dockerfile (would break subsequent root-required COPYs); `s6-setuidgid` not available at build time | `USER node` + `USER root` flip-flop |
| Nuke anonymous volumes on recreate | Old volumes had root-owned files that would persist through a simple restart; stale volume was the root cause of persistent EACCES | docker compose restart (would re-use stale volume) |

---

## Files Modified

| File | Change | Purpose |
|------|--------|---------|
| `docker/web/Dockerfile:64-69` | `chown` after `pnpm install`; install runs as `node` via `su node` | Fix root-owned node_modules in anonymous volume |
| `docker/web/s6-rc.d/pnpm-watcher/run:15-22` | Replace `s6-svc -r` with `s6-svc -d` + `sleep 3` + `s6-svc -u`; add `s6-setuidgid node` to pnpm install | Fix EADDRINUSE port race + ownership |
| `docker/web/cont-init.d/20-pnpm-install:20` | Add `s6-setuidgid node` to pnpm install | Match ownership with pnpm-dev |
| `apps/web/pnpm-lock.yaml` | Updated by `pnpm add @types/pg pg` run on host | Dependency sync (unrelated to session fixes) |
| `/etc/sysctl.d/99-inotify.conf` | Created (host file, not in repo) | Persist inotify limits across reboots |
| `CHANGELOG.md` | Added 9 commits since `8386d55` | Session documentation |

New files committed (from pre-existing unstaged work, not this session):
- `apps/web/app/api/tasks/route.ts`
- `apps/web/app/tasks/page.tsx`
- `apps/web/components/tasks/tasks-dashboard.tsx`
- `apps/web/components/tasks/tasks-list.tsx`
- `apps/web/components/ui/ai-node.tsx`

---

## Commands Executed

```bash
# Check inotify limits
cat /proc/sys/fs/inotify/max_user_watches   # → 65536
cat /proc/sys/fs/inotify/max_user_instances # → 128

# Raise immediately
sudo sysctl -w fs.inotify.max_user_watches=524288
sudo sysctl -w fs.inotify.max_user_instances=512

# Persist
printf 'fs.inotify.max_user_watches=524288\nfs.inotify.max_user_instances=512\n' \
  | sudo tee /etc/sysctl.d/99-inotify.conf

# Restart web container
docker compose restart axon-web

# After EADDRINUSE diagnosis — nuke stale port holder
docker stop axon-web && docker rm axon-web && docker compose up -d axon-web

# Hot-patch pnpm-watcher in running container (immediate fix)
docker cp docker/web/s6-rc.d/pnpm-watcher/run axon-web:/etc/s6-overlay/s6-rc.d/pnpm-watcher/run
docker exec axon-web /command/s6-svc -r /run/service/pnpm-watcher

# Verify node_modules ownership
docker exec axon-web stat -c '%U:%G %a' /app/node_modules/   # → root:root 755

# Full rebuild + fresh volumes
docker compose build axon-web
docker stop axon-web && docker rm axon-web
docker volume ls -q | grep axon-web | xargs -r docker volume rm
docker compose up -d axon-web
```

---

## Behavior Changes (Before/After)

| Symptom | Before | After |
|---------|--------|-------|
| Next.js startup | `Module not found: Can't resolve '../next-devtools/userspace/pages/pages-dev-overlay-setup'` | `✓ Ready in 969ms` |
| After pnpm-watcher triggers | `EADDRINUSE: address already in use :::49010` crash loop | Clean restart, `✓ Ready in 969ms` |
| TypeScript auto-install by Next.js | `EACCES: permission denied, open '…/esbuild/…/.bin/esbuild'` | No error; esbuild owned by `node` |
| Host inotify watches | 65536 | 524288 (persisted via sysctl.d) |
| `node_modules` ownership in container | `root:root` | `node:node` |
| pnpm-watcher restart strategy | `s6-svc -r` (immediate) | `s6-svc -d` + `sleep 3` + `s6-svc -u` |
| pnpm install user in watcher/init | root | `node` via `s6-setuidgid node` |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cat /proc/sys/fs/inotify/max_user_watches` | 524288 | 524288 | ✅ |
| `docker compose logs --tail=5 axon-web` | `✓ Ready in ...ms` | `✓ Ready in 969ms` | ✅ |
| `docker exec axon-web stat -c '%U:%G' /app/node_modules/` | `node:node` (after rebuild) | `node:node` | ✅ |
| `docker compose build axon-web` | Exit 0 | Exit 0, image built | ✅ |
| Biome pre-commit hook | Clean | 1 auto-fix (import order in ai-node.tsx), then clean | ✅ |
| `git push` | Pushed to remote | `4e45fb38..a5dc786c feat/crawl-download-pack` | ✅ |

---

## Source IDs + Collections Touched

No Axon embed/retrieve operations performed during this session (infrastructure debugging session, no docs crawled).

---

## Risks and Rollback

| Risk | Rollback |
|------|----------|
| Higher inotify limit increases kernel memory usage (~1KB per watch) | `sudo sysctl -w fs.inotify.max_user_watches=65536` + remove `/etc/sysctl.d/99-inotify.conf` |
| `sleep 3` in pnpm-watcher adds 3s to every package sync restart | Reduce to `sleep 1` in `docker/web/s6-rc.d/pnpm-watcher/run` |
| `su node` in Dockerfile may behave differently on ARM builds | Test with `docker compose build --platform linux/arm64 axon-web` if needed |
| Old anonymous volumes (root-owned) will persist if container is restarted without `rm` + volume prune | `docker stop axon-web && docker rm axon-web && docker volume prune -f && docker compose up -d axon-web` |

---

## Decisions Not Taken

- **`privileged: true` in docker-compose** — would allow per-container sysctl but opens full host kernel access. Rejected.
- **Switch from Turbopack to webpack** (`next dev --no-turbopack`) — would make `WATCHPACK_POLLING` work but loses Turbopack speed benefits. Rejected.
- **Port-polling loop in pnpm-watcher** — more precise than `sleep 3` but requires `ss` which isn't on default PATH in `node:24-slim` exec context. Rejected in favor of simple sleep.
- **`finish` script for `pnpm-dev`** — an s6 finish script could kill lingering port holders before restart; considered but a simple down/wait/up in the watcher is more explicit and easier to reason about.

---

## Open Questions

- The `Ignored build scripts: esbuild@0.27.3, msw@2.12.10` warning persists — esbuild's post-install hook is blocked by pnpm's security policy. This shouldn't cause runtime issues since esbuild ships prebuilt binaries, but worth running `pnpm approve-builds` in the container to verify.
- GitHub Dependabot flagged 2 high vulnerabilities on the default branch after push — unrelated to this session but worth triaging.

---

## Next Steps

- Run `docker exec axon-web sh -c "cd /app && pnpm approve-builds"` to approve esbuild/msw build scripts and silence the warning.
- Triage 2 high Dependabot vulnerabilities on `main`.
- Consider documenting the inotify limit requirement in `CLAUDE.md` or `docker/CLAUDE.md` so future environments know to set it before first run.
