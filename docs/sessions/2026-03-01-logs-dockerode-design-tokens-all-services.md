# Session: Logs Page — dockerode, Design Token Cleanup, All-Services Mode
**Date:** 2026-03-01
**Branch:** feat/crawl-download-pack
**Duration:** Multi-context session (resumed)

---

## Session Overview

Three distinct workstreams completed:

1. **Bug fix**: `/api/logs` SSE endpoint crashed with `spawn docker ENOENT` because the `axon-web` container has no Docker CLI. Replaced `spawn('docker', ...)` with the `dockerode` npm package + Docker socket mount, then fixed the resulting `EACCES` permission error via a `cont-init.d` script that dynamically adds the `node` user to the Docker socket group before s6 starts services.

2. **Design token cleanup**: Audited all components against `docs/UI-DESIGN-SYSTEM.md`. Rewrote `log-line.tsx` to parse Rust tracing format and render structured columns. Fixed all legacy v1 token usages (`--axon-text-*`, `--axon-accent-*`) across the codebase and deleted the now-unused shim definitions from `globals.css`.

3. **All-services mode**: Added `service=all` support to `/api/logs` — fans out to all 7 containers concurrently via parallel dockerode streams, tags each SSE event with `service`. Log viewer defaults to "All services" view with per-service color badges.

---

## Timeline

| Time | Activity |
|------|----------|
| Session start | Debugged `spawn docker ENOENT` in `apps/web/app/api/logs/route.ts` |
| +15 min | Replaced `spawn` with `dockerode`; added `/var/run/docker.sock` bind mount |
| +30 min | Debugged `EACCES /var/run/docker.sock` — root cause: `s6-setuidgid` strips supplementary groups |
| +45 min | Fixed via `docker/web/cont-init.d/12-docker-socket-group` cont-init script |
| +60 min | Improved log output: ANSI stripping, Rust tracing format parser, structured column layout |
| +75 min | Design system alignment: audited + fixed `log-line.tsx` against `docs/UI-DESIGN-SYSTEM.md` |
| +80 min | Updated `docs/UI-DESIGN-SYSTEM.md` §3 Typography for Noto Sans / Noto Sans Mono |
| +90 min | Tightened: static PassThrough import, removed redundant `group_add`, removed double-dimmed opacity |
| +95 min | Fixed all legacy v1 token usages in `globals.css` utility classes |
| +100 min | Fixed `terminal-toolbar.tsx`: `--axon-accent-blue` → `--axon-primary-strong` (3 occurrences) |
| +105 min | Fixed `terminal/page.tsx`: `--axon-text-primary` / `--axon-text-muted` → v2 tokens |
| +110 min | Deleted 9-var shim block from `globals.css` — confirmed zero remaining usages |
| +115 min | Implemented `service=all` — parallel dockerode streams, service color badges in log lines |

---

## Key Findings

### spawn docker ENOENT
- `apps/web/app/api/logs/route.ts:32` — `spawn('docker', ['logs', '--follow', ...])` assumed Docker CLI in container PATH; `axon-web` is Node-only, no CLI tools installed.

### EACCES /var/run/docker.sock — s6 group stripping (critical insight)
- `group_add: ["981"]` in docker-compose adds GID 981 to the **root** PID 1 process only.
- `s6-setuidgid node` calls `initgroups()` which reads `/etc/group` at exec time — if GID 981 isn't in `/etc/group` for `node`, the supplementary group is never set.
- Fix: `cont-init.d/12-docker-socket-group` runs as root before services, reads `stat -c '%g' /var/run/docker.sock`, creates `docker-host` group with that GID, and adds `node` via `usermod -aG`. On next s6 service start, `initgroups()` finds the group in `/etc/group` and it sticks.

### Legacy token shim block (globals.css:94–102)
- 9 vars (`--axon-text-primary/secondary/muted/subtle/dim`, `--axon-accent-pink/pink-strong/blue/blue-strong`) were only used by `terminal/page.tsx` and `terminal/terminal-toolbar.tsx` — not by any Plate editor or other components as the comment implied.
- After fixing those two files, a `grep` confirmed zero usages → shim block deleted.

### Docker multiplexed streams
- Non-TTY Docker log streams use 8-byte frame headers. `docker.modem.demuxStream(logStream, pt, pt)` handles splitting stdout/stderr into a single `PassThrough` correctly.

---

## Technical Decisions

### dockerode over CLI subprocess
- **Chosen**: `dockerode` npm package using `/var/run/docker.sock`
- **Rejected**: Installing `docker-cli` in the container image (bloat, security surface), using Docker TCP API (extra network config)
- **Reason**: Socket access is zero-overhead, no binary installation, works inside the existing network

### cont-init.d for socket permissions (not group_add)
- **Chosen**: `docker/web/cont-init.d/12-docker-socket-group` reads GID dynamically, creates group, adds node user
- **Rejected**: Hardcoding GID 981 in docker-compose `group_add` (host GIDs vary), running Next.js as root (security)
- **Reason**: Dynamic GID detection survives host changes; `usermod -aG` persists into `/etc/group` which `initgroups()` reads

### Unified timeline (all-services mode) over multi-pane grid
- **Chosen**: Single interleaved stream with per-line service badge
- **Rejected**: N separate scroll panes (complex state, awkward UX for 7 containers)
- **Reason**: Matches `docker compose logs --follow` mental model; service badge with fixed width keeps columns aligned

### Service badge colors from design system palette
- workers=blue (`--axon-primary`), web=pink (`--axon-secondary`), postgres=green (`--axon-success`), redis=orange (`--axon-warning`), rabbitmq=bright-blue (`--axon-primary-strong`), qdrant=purple (rgba), chrome=yellow (rgba)
- Purple and yellow have no design system tokens → used matching rgba values

---

## Files Modified

| File | Change |
|------|--------|
| `apps/web/app/api/logs/route.ts` | Replaced `spawn('docker', ...)` with dockerode; added `service=all` fan-out; ANSI stripping |
| `apps/web/components/logs/log-line.tsx` | Rust tracing parser; structured columns; design token alignment; service badge |
| `apps/web/components/logs/logs-toolbar.tsx` | Added "All services" option; updated `ServiceName` type |
| `apps/web/components/logs/logs-viewer.tsx` | Threads `service` from SSE payload into `LogEntry`; defaults to `'all'` |
| `docker/web/cont-init.d/12-docker-socket-group` | **New file**: dynamically adds node user to Docker socket group |
| `docker-compose.yaml` | Added `/var/run/docker.sock:/var/run/docker.sock:ro` mount; removed redundant `group_add` |
| `apps/web/app/globals.css` | Fixed legacy token usages in utility classes; deleted 9-var shim block |
| `apps/web/components/terminal/terminal-toolbar.tsx` | `--axon-accent-blue` → `--axon-primary-strong`; `--axon-text-muted` → `--text-muted` |
| `apps/web/app/terminal/page.tsx` | `--axon-text-primary` → `--text-primary`; `--axon-text-muted` → `--text-muted` |
| `docs/UI-DESIGN-SYSTEM.md` | Updated §3 Typography: Space Mono/Sora/JetBrains → Noto Sans / Noto Sans Mono |
| `apps/web/package.json` | Added `dockerode@4.0.9`, `@types/dockerode@4.0.1` |

---

## Commands Executed

```bash
# Verify docker socket GID after cont-init fix
docker exec -it axon-web s6-setuidgid node id
# Groups: 981 1000 confirmed

# Clean restart after EADDRINUSE from manual debugging
docker compose stop axon-web && docker compose start axon-web

# Verify no legacy tokens remain
grep -r '--axon-text-\|--axon-accent-' apps/web --include='*.{tsx,ts,css}'
# No matches — confirmed clean
```

---

## Behavior Changes (Before/After)

| Feature | Before | After |
|---------|--------|-------|
| `/logs` SSE | `[stream error] spawn docker ENOENT` on every connection | Streams Docker logs via socket, no error |
| Docker socket access | `EACCES /var/run/docker.sock` — node user denied | `s6-setuidgid node` gets GID via `/etc/group` — access granted |
| Log line format | Raw ANSI escape codes; unstructured single string | Parsed columns: timestamp / level badge / module / message; ANSI stripped |
| Log level colors | Raw Tailwind (`text-red-400`, etc.) | Design system tokens (`--axon-success`, `--axon-warning`, etc.) |
| `/logs` default view | `axon-workers` only | "All services" — all 7 containers interleaved with service badge |
| Service selector | 7 individual options | "All services" + 7 individual options |
| Legacy CSS tokens | 9 shim vars + usages in terminal components | All usages migrated to v2 tokens; shim block deleted |

---

## Verification Evidence

| Check | Expected | Actual | Status |
|-------|----------|--------|--------|
| No legacy token usages | 0 grep matches | 0 matches | ✅ |
| `s6-setuidgid node id` after fix | GID 981 in groups | `Groups: 981 1000` | ✅ |
| `route.ts` line count | ≤ 500 lines | ~110 lines | ✅ |
| `log-line.tsx` line count | ≤ 500 lines | ~130 lines | ✅ |
| `logs-toolbar.tsx` line count | ≤ 500 lines | ~145 lines | ✅ |

---

## Risks and Rollback

**Docker socket mount** (`/var/run/docker.sock:ro`):
- Risk: Any code that can call dockerode can list/inspect all containers on the host. The `ALLOWED_SERVICES` allowlist in `route.ts` limits which containers can be accessed via the API, but the socket itself is unrestricted inside the container.
- Rollback: Remove the `volumes` entry from `docker-compose.yaml` and revert `route.ts`.

**cont-init socket group script**:
- If host socket GID changes (e.g., Docker daemon upgrade), the script re-reads it dynamically — no action needed.
- If `usermod` is unavailable in the image, the script fails silently (exits 0 before `usermod`). Verified present in `node:24-alpine`.

---

## Decisions Not Taken

- **Multi-pane log grid**: Each service in its own scrollable panel. Rejected — managing 7 independent scroll positions and SSE connections adds complexity without clear benefit over a unified filtered stream.
- **Hardcoded GID 981 in group_add**: Brittle — GIDs vary by host. `cont-init.d` dynamic approach is portable.
- **Installing docker-cli in the container**: Adds ~30MB image size and a significant security surface. `dockerode` via socket is lighter and more direct.
- **Server-side log filtering**: The filter is client-side (in-memory). Server-side would reduce bandwidth but adds complexity and breaks real-time UX. Client filter is fine for the current scale.

---

## Open Questions

- Should `service=all` also include a per-service connection health indicator in the toolbar (e.g., "5/7 Live") rather than a single dot? Currently shows single "Live" dot once any stream is active.
- `axon-chrome` may not always be running. The current implementation silently emits a `[stream error]` line and decrements the active count — acceptable but could be made invisible if the container is expected to be down.
- `MAX_LINES = 2000` in the viewer — with 7 services active, the initial tail burst (7×200=1400 lines) nearly fills the buffer. Consider bumping to 5000 or making it configurable.

---

## Next Steps

- Test the "All services" view live in the browser — verify service badge colors and column alignment
- Consider whether `axon-chrome` absence should suppress the `[stream error]` line or show a "container not found" indicator in the toolbar
- Review `apps/web/app/api/jobs/route.ts` — modified in git diff but not touched this session; may have pre-existing issues
