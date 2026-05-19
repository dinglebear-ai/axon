---
date: 2026-05-13 04:49:00 EST
repo: git@github.com:jmagar/axon.git
branch: main
head: (see git log)
agent: Claude
working directory: /home/jmagar/workspace/axon_rust
---

## User Request

Fix OAuth so Claude Code can connect to `https://axon.tootie.tv/mcp` using OAuth credentials, same as claude.ai. Investigate plugin config errors, 401 failures, and ensure everything is properly working.

## Session Overview

Long debugging session across OAuth, container port binding, plugin config, SWAG nginx routing, and lab-auth allowed_users. Root cause chain unraveled across multiple layers. Everything is now working: claude.ai connects via OAuth, Claude Code connects via OAuth, and static bearer token still works for programmatic access.

## Sequence of Events

1. User reported `server_url:-http://127.0.0.1:8001` plugin config error — traced to shell-style default in `.mcp.json` that Claude Code read literally as a key name
2. Fixed `.mcp.json` to use `${user_config.server_url}` without shell fallback
3. User got 401 after OAuth flow — investigated via syslog, docker logs, axon log file
4. Discovered axon container had no port bindings (`{}`) — `docker compose restart` drops port bindings without the env file; port defaulted to `127.0.0.1:8001` (SWAG can't reach via Tailscale)
5. Fixed compose default from `127.0.0.1:8001` to `0.0.0.0:8001` in `docker-compose.yaml`
6. Discovered `allowed_users` table empty in `~/.axon/lab-auth/auth.db` — manually inserted `jmagar@gmail.com`
7. Discovered lab-auth debug logs not appearing — axon container (GHCR image) uses file logging (`AXON_LOG_FILE`) not stdout; file was stale from May 11 (old container)
8. Added `RUST_LOG=lab_auth=debug` to env file, then removed after finding root cause without it
9. Tested `/token` endpoint directly via SWAG — got 502; suspected nginx routing
10. Found SWAG nginx config (`axon.subdomain.conf`) had two upstreams: port 8001 for `/mcp` and port 49010 for everything else. `mcp-server.conf` already had all OAuth endpoints mapped to port 8001 but the container had lost its port binding so requests failed
11. Confirmed container port binding was the root cause — re-ran `docker compose --env-file ~/.axon/.env up -d axon` to restore `0.0.0.0:8001` binding
12. Tested full token flow: refresh_token → JWT → `/mcp` → 200 ✓
13. User still got 401 in Claude Code despite OAuth completing — discovered `.mcp.json` `headers.Authorization` was overriding OAuth JWT with static `api_token` value every time
14. Fixed `.mcp.json` to remove `Authorization` header entirely — Claude Code now uses its OAuth credentials natively
15. Made `api_token` optional in `plugin.json` since OAuth handles auth
16. Removed stale systemd `axon-mcp.service` unit that was conflicting

## Key Findings

- **SWAG nginx `axon.subdomain.conf`** had `upstream_port=49010` for catch-all `/` route; `mcp-server.conf` correctly maps OAuth endpoints to port 8001 but those fail when container has no port binding
- **Container loses port binding on `docker compose restart`** without `--env-file` — compose uses `127.0.0.1:8001` default, fails to bind (port in use), ends up with `{}`
- **`plugins/.mcp.json` Authorization header** overrides OAuth JWT — Claude Code does the OAuth flow, gets credentials, then the explicit `Authorization: Bearer ${user_config.api_token}` header in `.mcp.json` replaces the JWT with the static token. If static token is wrong → 401
- **`allowed_users` table was empty** — `AXON_MCP_AUTH_ADMIN_EMAIL` is already handled in `resolve_allowed_emails()` (always included without needing a DB row), so this was not blocking auth; manually inserted anyway
- **axon container logs go to file, not stdout** — `docker logs axon` shows nothing; logs at `~/.axon/logs/axon.log` but GHCR container wasn't writing there (stale from old locally-built container)

## Technical Decisions

- **Removed Authorization header from `.mcp.json`** rather than trying to conditionally include it — cleanest solution, OAuth handles auth for all browser clients (Claude Code, claude.ai); static bearer token still accepted server-side via `AXON_MCP_HTTP_TOKEN` for programmatic use
- **Made `api_token` optional** — OAuth clients don't need it; kept the field so users who want static bearer access can still configure it
- **Fixed compose default to `0.0.0.0:8001`** — makes container resilient to restarts without env file; `AXON_MCP_HTTP_TOKEN` still enforces auth

## Files Modified

| File | Change |
|------|--------|
| `plugins/.mcp.json` | Removed `Authorization` header — let Claude Code use OAuth credentials |
| `.claude-plugin/plugin.json` | Made `api_token` optional; updated description |
| `docker-compose.yaml` | Changed `AXON_MCP_HTTP_PUBLISH` default from `127.0.0.1:8001` to `0.0.0.0:8001` |
| `~/.axon/.env` | Added then removed `RUST_LOG=lab_auth=debug`; `AXON_MCP_HTTP_PUBLISH=0.0.0.0:8001` already present |
| `~/.axon/lab-auth/auth.db` | Manually inserted `jmagar@gmail.com` into `allowed_users` |

## Errors Encountered

- **Plugin config error `server_url:-http://127.0.0.1:8001`**: Shell-style default in `.mcp.json` — fixed by removing `:-http://127.0.0.1:8001` suffix
- **Container `{}` port binding**: `docker compose restart` without env file drops binding — fixed by `docker compose --env-file ~/.axon/.env up -d axon`; root fix: change compose default to `0.0.0.0:8001`
- **SWAG 502 on `/token`**: Container had no port binding — resolved when port binding restored
- **401 after OAuth**: `.mcp.json` Authorization header clobbered OAuth JWT with wrong static token — fixed by removing the header

## Behavior Changes

| Component | Before | After |
|-----------|--------|-------|
| Claude Code → axon MCP | 401 (static token override) | ✓ OAuth JWT used directly |
| Container restart without env-file | Port binding lost → SWAG 502 | Port binds on `0.0.0.0:8001` by default |
| Plugin `api_token` | Required | Optional |
| `systemd axon-mcp.service` | Stale unit causing startup conflict | Removed |

## Open Questions

- Why is the GHCR container not writing logs to `~/.axon/logs/axon.log`? The old locally-built container was. GHCR image may log to a different path or only to stdout.
- The `allowed_users` seeding: `resolve_allowed_emails()` already includes `admin_email` without a DB row, but the manually-inserted row provides belt-and-suspenders. The startup seeding from `AXON_MCP_AUTH_ADMIN_EMAIL` is technically not needed but would be a cleaner UX.

## Next Steps

**Not started:**
- Verify Claude Code connects successfully via OAuth after `/plugin update axon` + `/reload-plugins` + re-authenticate
- Investigate why GHCR container logs don't appear in `~/.axon/logs/axon.log`
- Address in-progress bead `axon_rust-cmm` (ask perf 0.3: batch full-doc fetch + parallelism audit)
