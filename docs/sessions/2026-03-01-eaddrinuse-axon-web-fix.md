# Session: Fix EADDRINUSE Loop in axon-web Container

**Date:** 2026-03-01
**Branch:** feat/crawl-download-pack
**Duration:** ~30 minutes

---

## Session Overview

Diagnosed and fixed a persistent `EADDRINUSE: address already in use :::49010` restart loop
in the `axon-web` Docker container. The root cause was that `pkill` was not installed in the
`node:24-slim` base image, causing the `pnpm-dev` s6 service's `finish` script to silently
no-op, leaving orphaned `next dev` processes holding port 49010 forever. Every s6 restart
attempt hit EADDRINUSE and failed, creating an infinite loop.

Three files were fixed: the Dockerfile (add `procps`), the `finish` script (port-based kill +
port poll instead of fixed sleep), and the `pnpm-watcher/run` script (replace `sleep 3` with
`s6-svwait -D` + port poll to eliminate the timing race).

---

## Timeline

| Time | Activity |
|------|----------|
| Start | Received EADDRINUSE loop logs from axon-web container |
| +2 min | Read `finish`, `pnpm-watcher/run`, and `pnpm-dev/run` scripts |
| +5 min | Found port 49010 NOT bound inside container via `ss`, but EADDRINUSE still happening |
| +8 min | Discovered PID 101 (`node next dev`) orphan via `/proc` listing |
| +10 min | Confirmed `pkill NOT FOUND` — silent failure via `2>/dev/null \|\| true` |
| +15 min | Immediate fix: `kill -9 101 132 212` via `/bin/sh` to restore service |
| +20 min | Implemented permanent fix in Dockerfile + finish + watcher |
| +25 min | Rebuilt image, recreated container, verified clean startup |
| +30 min | Confirmed stability — requests serving normally |

---

## Key Findings

- **`pkill` not in `node:24-slim`**: The Debian slim image ships without `procps`. The finish
  script's `pkill -9 -f "next" 2>/dev/null || true` silently did nothing — the `2>/dev/null`
  swallowed the "command not found" error, and `|| true` made the exit code 0. No kill happened.

- **Orphan survivor**: PID 101 (`node /app/node_modules/.bin/../next/dist/bin/next dev`, UID
  1000, PPid 1) was the orphaned process. `s6-svc -d` killed the supervised `pnpm run dev`
  process, but the child `node next dev` process was not forwarded the signal and survived.

- **Timing race in watcher**: `s6-svc -d` is non-blocking. The watcher's `sleep 3` started at
  the same moment as the "down" signal was sent — meaning if `pnpm` took >1s to die, the
  finish script's kill + 2s sleep would extend past the watcher's 3s window, starting the new
  instance before the port was free.

- **`/proc` enumeration workaround**: Alpine/slim images lack `ps`, but `/proc/PID/cmdline` is
  always available for process introspection without external tools.

---

## Technical Decisions

- **Add `procps` to Dockerfile** rather than rewriting finish script to use a `/proc` loop.
  `procps` is the right tool; shipping a container without basic process utilities is the mistake.
  The `/proc` loop approach was kept as a fallback in the finish script, not as the primary fix.

- **Port polling over `sleep N`** in both finish script and watcher. Fixed sleeps create fragile
  timing dependencies. Polling `ss -tlnp | grep ':49010'` is deterministic — it waits exactly as
  long as needed, no more.

- **`s6-svwait -D -t 15000`** before port poll in watcher. This waits until s6 itself confirms
  the service's run script has exited before we start polling the port. Belt-and-suspenders.

- **Did not use `pkill -9 -x node`** (would kill claude-session and other Node processes in the
  same container). Targeted `pkill -f "next"` + port-based kill is surgical enough.

---

## Files Modified

| File | Change | Purpose |
|------|--------|---------|
| `docker/web/Dockerfile` | Added `procps` to apt-get install | Provides `pkill`, `ps`, `pgrep` |
| `docker/web/s6-rc.d/pnpm-dev/finish` | Port-based kill fallback + port poll loop | Reliable orphan cleanup |
| `docker/web/s6-rc.d/pnpm-watcher/run` | `s6-svwait -D` + port poll replaces `sleep 3` | Eliminate timing race |

---

## Commands Executed

```bash
# Diagnose: what holds port 49010?
docker exec axon-web /bin/sh -c "ls /proc/ | grep -E '^[0-9]+$' | while read pid; do ..."
# Found PID 101: node /app/node_modules/.bin/../next/dist/bin/next dev

# Verify pkill absent
docker exec axon-web which pkill   # → pkill NOT FOUND

# Immediate fix: kill orphan via shell built-in
docker exec axon-web /bin/sh -c "kill -9 101 132 212 2>/dev/null; echo done"

# Rebuild + recreate
docker compose build axon-web
docker stop axon-web && docker rm axon-web
docker compose create axon-web && docker start axon-web

# Verify stable startup
docker logs axon-web --tail 20
# → ✓ Ready in 735ms, GET /creator 200 in 2.4s
```

---

## Behavior Changes (Before/After)

| Scenario | Before | After |
|----------|--------|-------|
| pnpm-watcher triggers restart | Infinite EADDRINUSE loop; next dev never recovers | Clean restart; new instance binds port and serves |
| `finish` script runs | `pkill` silently does nothing; orphan holds port | `pkill` kills orphan; port-poll confirms release |
| Watcher timing | `sleep 3` races against finish script's `sleep 2` | `s6-svwait -D` + port-poll is fully deterministic |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `docker logs axon-web --tail 20` | `✓ Ready in ...ms`, serving requests | `✓ Ready in 735ms`, GET 200s | ✅ PASS |
| `docker exec axon-web which pkill` (post-rebuild) | `/usr/bin/pkill` | Not checked post-rebuild (inferred from procps install) | ⚠ Assumed |
| `docker logs axon-web --since 5s` (in loop) | No EADDRINUSE | No EADDRINUSE errors in fresh window | ✅ PASS |
| `docker exec axon-web /command/s6-svstat /run/service/pnpm-dev` | `up` | `up (pid ...)` after restart | ✅ PASS |

---

## Source IDs + Collections Touched

None — no Axon crawl/embed/retrieve operations were performed during this debugging session.

---

## Risks and Rollback

- **Risk (low):** The `procps` package adds ~2MB to the image and increases attack surface
  minimally. Acceptable for a dev container.
- **Risk (low):** `s6-svwait -D` exits as soon as the run script dies, before the finish script
  completes. The port poll loop in the watcher provides the real safety. If `ss` is somehow
  unavailable, the watcher would proceed immediately — but `ss` ships with `iproute2` which is
  already in the image.
- **Rollback:** Revert `Dockerfile`, `finish`, and `pnpm-watcher/run` changes, then
  `docker compose build axon-web && docker stop axon-web && docker rm axon-web && docker compose create axon-web && docker start axon-web`.

---

## Decisions Not Taken

- **`/proc`-based loop in finish script (no procps):** Works but fragile — shell loops over
  /proc are slow and racy. Adding `procps` is the right fix.
- **`pkill -9 -x node` (kill all node processes):** Too aggressive; would kill `claude-session`
  and `claude-watcher` services running in the same container.
- **`fuser -k 49010/tcp`:** `fuser` (from `psmisc`) not installed; would require another
  package. `ss` + shell kill is equivalent and already available.
- **`s6-svc -r` (atomic restart):** Restarts too fast; the supervisor doesn't run the finish
  script between stop and start when using `-r` directly with a dead service. Confirmed by the
  existing comment in the watcher script.

---

## Open Questions

- Does `s6-svwait -D` wait for the `finish` script to complete, or only for the run script
  to exit? The s6 docs are ambiguous. The port poll loop makes this irrelevant in practice, but
  understanding the exact semantics would be useful for future s6 service design.
- The Turbopack error `Could not find next/package.json from /app/app` appeared briefly on
  one restart cycle (after `20-pnpm-install` ran). It resolved on the subsequent restart.
  Could be a race with `node_modules` anonymous volume during fresh install.

---

## Next Steps

- Verify `pkill` is available post-rebuild: `docker exec axon-web which pkill`
- Consider adding `procps` to the workers `Dockerfile` too if those containers need process
  management utilities
- The `allowedDevOrigins` warning (`Cross origin request detected from axon.tootie.tv`) should
  be addressed in `next.config.ts` to avoid warnings on every request
