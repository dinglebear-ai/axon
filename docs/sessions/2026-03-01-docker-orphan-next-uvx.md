# Session: Docker Orphan next-server Fix + uvx Install

**Date:** 2026-03-01
**Branch:** feat/crawl-download-pack
**Commits:** `a5dc786c`, `a31a58ea`

---

## Session Overview

Continuation of the Docker/Next.js stability work. The EADDRINUSE crash loop returned despite the earlier `sleep 3` fix in pnpm-watcher — diagnosed as orphan turbopack worker processes surviving after pnpm exits. Fixed with an s6 `finish` script for `pnpm-dev`. Also installed `uvx` system-wide to enable the `neo4j-memory` stdio MCP server.

---

## Timeline

| Time | Activity |
|------|----------|
| Session start | User reported EADDRINUSE crash loop returning — same symptom as before |
| ~T+5m | Confirmed `sleep 3` in pnpm-watcher was NOT the issue — identified the real cause: `pnpm run dev` spawns `next dev` as a child, which spawns turbopack workers; when s6 kills pnpm, workers become orphans holding port 49010 |
| ~T+10m | Created `docker/web/s6-rc.d/pnpm-dev/finish` with `pkill -9 -f "next" && sleep 2` |
| ~T+12m | Rebuilt image but didn't recreate container — crash loop continued (user correctly called this out) |
| ~T+15m | Used `nsenter -t $CPID -n ss -tlnp` from host to identify orphan: `next-server (v1` pid 3026425 |
| ~T+18m | Killed orphan via `sudo kill -9 3026425`; server recovered |
| ~T+20m | Discovered `docker cp` of finish script had `#\!/bin/sh` (mangled shebang) — `s6-supervise: warning: unable to spawn ./finish: Exec format error` |
| ~T+22m | Fixed via `docker cp` (not printf/heredoc) to copy the proper file — shebang intact |
| ~T+25m | Rebuilt + properly recreated container (`stop → rm → up`) — clean startup, finish script present in `/run/service/pnpm-dev/finish` |
| ~T+30m | User asked about `[mcp-status] checkStdioServer error for neo4j-memory Command failed: which uvx` |
| ~T+32m | Identified: neo4j-memory MCP server uses `uvx mcp-neo4j-memory` (from `/home/node/.claude/mcp.json`) |
| ~T+35m | Added `uv` installer to Dockerfile: `curl -LsSf https://astral.sh/uv/install.sh | UV_INSTALL_DIR=/usr/local/bin sh` |
| ~T+40m | Built, recreated, verified: `uvx 0.10.7` at `/usr/local/bin/uvx`, MCP status logs clean |
| ~T+42m | Committed `a31a58ea` and pushed both fixes |

---

## Key Findings

- **Root cause of EADDRINUSE**: `pnpm run dev` forks `next dev` which forks turbopack worker processes. s6 only kills the direct child (pnpm). `next dev` and its workers become orphan processes that retain the `:49010` LISTEN socket — surviving across s6 restarts indefinitely.
- **nsenter technique**: `CPID=$(docker inspect --format '{{.State.Pid}}' axon-web) && sudo nsenter -t $CPID -n ss -tlnp 'sport = :49010'` — the only reliable way to find port holders inside a container when `ss`/`ps` aren't on the container's PATH.
- **Orphan confirmed**: `next-server (v1` pid 3026425 was holding `:49010` with UID 1000 (node), even though no pnpm-dev service was running.
- **Shebang mangling**: Writing scripts via `printf "#!/bin/sh\n..."` inside `docker exec sh -c` mangled `!` to `\!` on some shells, producing `Exec format error`. Always use `docker cp` from a host file.
- **s6 finish script placement**: Hot-copying to `/etc/s6-overlay/s6-rc.d/pnpm-dev/finish` has NO effect on a running container — s6-rc compiles service definitions at boot into `/run/service/`. Must copy to `/run/service/pnpm-dev/finish` for immediate effect, or rebuild + recreate for permanent fix.
- **uvx location**: `/home/node/.claude/mcp.json` → `neo4j-memory` server uses `"command": "uvx"` with `["--with", "fastmcp<3", "mcp-neo4j-memory"]`. The `disabled: true` flag means it's registered but not auto-started.

---

## Technical Decisions

| Decision | Rationale | Alternative Rejected |
|----------|-----------|---------------------|
| `pkill -9 -f "next"` in finish script | Kills all processes with "next" in cmdline — catches both `next dev` and any turbopack workers regardless of PID | `pkill -P $PPID` — can't get parent PID reliably in finish script |
| `UV_INSTALL_DIR=/usr/local/bin` | System-wide, on PATH for all users including `node` (s6-setuidgid node) | `~/.local/bin` — not on PATH for node user in s6 context |
| `finish` script vs wrapping `run` in a process group | finish script is the s6-native lifecycle hook for cleanup; no run script changes needed | `setsid` wrapper + SIGPGRP in pnpm-watcher — more complex, brittle |

---

## Files Modified

| File | Change | Purpose |
|------|--------|---------|
| `docker/web/Dockerfile:20-21` | Added `uv` install via official Astral installer to `/usr/local/bin` | Provides `uvx` for neo4j-memory MCP server |
| `docker/web/s6-rc.d/pnpm-dev/finish` | **Created** — `pkill -9 -f "next" && sleep 2` | Kills orphan next/turbopack processes before s6 restarts pnpm-dev |

---

## Commands Executed

```bash
# Find what holds port 49010 inside container (when ss not on container PATH)
CPID=$(docker inspect --format '{{.State.Pid}}' axon-web)
sudo nsenter -t $CPID -n ss -tlnp 'sport = :49010'
# → next-server (v1, pid=3026425, fd=19)

# Kill the orphan from host
sudo kill -9 3026425

# Verify finish script shebang after docker cp (not printf)
docker exec axon-web cat /run/service/pnpm-dev/finish

# Verify uvx after rebuild
docker exec axon-web which uvx    # → /usr/local/bin/uvx
docker exec axon-web uvx --version # → uvx 0.10.7

# Correct rebuild+recreate sequence
docker compose build axon-web
docker stop axon-web && docker rm axon-web && docker compose up -d axon-web
```

---

## Behavior Changes (Before/After)

| Symptom | Before | After |
|---------|--------|-------|
| pnpm-watcher restart | EADDRINUSE crash loop (orphan next-server holds port) | Clean restart, finish script kills orphans first |
| `s6-supervise pnpm-dev: warning` | `unable to spawn ./finish: Exec format error` | Finish script executes cleanly |
| MCP neo4j-memory status | `Command failed: which uvx` on every `/api/mcp/status` poll | `uvx` found at `/usr/local/bin/uvx` |
| `pnpm-dev/finish` in image | Not present | Present, executable, `pkill -9 -f "next" && sleep 2` |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `sudo nsenter -t $CPID -n ss -tlnp 'sport = :49010'` | Orphan process identified | `next-server (v1` pid 3026425 | ✅ |
| `docker exec axon-web which uvx` | `/usr/local/bin/uvx` | `/usr/local/bin/uvx` | ✅ |
| `docker exec axon-web uvx --version` | `uvx 0.x.x` | `uvx 0.10.7` | ✅ |
| `docker exec axon-web ls /run/service/pnpm-dev/finish` | file present | `-rwxr-xr-x finish` | ✅ |
| `docker compose logs --tail=3 axon-web` | `200` responses, no errors | `GET /api/mcp/status 200` etc. | ✅ |
| `git push` | `a5dc786c..a31a58ea` | Confirmed | ✅ |

---

## Source IDs + Collections Touched

| Source ID | Collection | Operation | Outcome |
|-----------|-----------|-----------|---------|
| `docs/sessions/2026-03-01-docker-inotify-port-race-ownership.md` | `cortex` | embed (prior session) | ✅ confirmed |
| `docs/sessions/2026-03-01-docker-orphan-next-uvx.md` | `cortex` | embed (this session) | See Axon embed below |

---

## Risks and Rollback

| Risk | Rollback |
|------|----------|
| `pkill -9 -f "next"` is broad — would kill any process with "next" in cmdline | Narrow to `pkill -9 -f "next dev"` or `pkill -9 -f "next-server"` if collateral kills occur |
| `uv` installer fetches from `astral.sh` at build time — build fails if network unavailable | Pin to a specific uv version release URL as fallback |

---

## Decisions Not Taken

- **`setsid` in run script + kill process group** — would create a new session, allowing `kill -TERM -$PGID`. More precise but more complex than the finish script approach.
- **`npx kill-port 49010`** — an npm package for this exact problem. Rejected: adds a dependency for a problem that's better solved at the process supervision layer.
- **Increase `sleep` in pnpm-watcher further** — sleep 3 was already in place; the real fix is the finish script, not longer sleep.

---

## Open Questions

- `neo4j-memory` MCP server has `"disabled": true` in mcp.json — confirm whether this means Claude won't auto-start it or if it's fully excluded from the MCP registry.
- `esbuild@0.27.3` and `msw@2.12.10` build scripts are still blocked by pnpm's security policy — should run `pnpm approve-builds` inside the container to resolve.
- `ELIFECYCLE Command failed with exit code 1` still appears in logs on every pnpm-watcher restart cycle — harmless (it's pnpm reporting the `dev` script was killed), but noisy. No suppression mechanism without patching pnpm.

---

## Next Steps

- Run `docker exec axon-web sh -c "cd /app && s6-setuidgid node pnpm approve-builds"` to approve esbuild/msw build scripts.
- Confirm `neo4j-memory` MCP server starts correctly after `disabled: true` is removed or the server is explicitly enabled.
- Consider pinning the `uv` installer URL to a specific version for reproducible builds.
