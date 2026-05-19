# Claude Container Config Hardening
**Date:** 2026-02-27
**Branch:** feat/crawl-download-pack

---

## Session Overview

Replaced the `axon-web` container's Claude Code configuration with a clean, project-owned setup. The previous approach bind-mounted the dev machine's `~/.claude` (messy, full of host-specific state). The new approach mounts a dedicated `AXON_DATA_DIR/axon/claude` directory that contains only what Pulse needs: skills, commands, plugins, and an explicit `mcp.json`. Removed the `claude-session` and `claude-watcher` s6 services (interactive-terminal use case, no terminal in the web container). Wired the `axon-mcp` server into the container via SSH stdio tunnel over `host.docker.internal`. Fixed two bugs discovered during verification: missing `git` in the image and `axon-mcp` starting without `.env` sourced.

---

## Timeline

1. **Plan implementation** — Executed the pre-approved "Claude Code Container Config Hardening" plan verbatim.
2. **docker-compose.yaml** — Replaced `HOST_HOME/.claude` mount with `AXON_DATA_DIR/axon/claude`; kept `.claude.json` (auth file) on its own separate mount; uncommented SSH keys mount.
3. **claude-stream-types.ts** — Added `--mcp-config`, `--dangerously-skip-permissions`, `--include-partial-messages`, `--effort medium`, `--plugin-dir` to `buildClaudeArgs`.
4. **s6 service deletion** — Removed `claude-session/` and `claude-watcher/` directories and their `user/contents.d/` entries.
5. **Dockerfile cleanup** — Removed stale log dir mkdirs for the deleted services; updated comments.
6. **Bootstrap** — Created `/home/jmagar/appdata/axon/claude` with `sudo mkdir -p` + `chown jmagar`.
7. **Image rebuild** — `docker compose build --no-cache axon-web` → clean build, only `pnpm-dev` s6 service.
8. **Stack recreate** — `docker compose up -d --force-recreate axon-web` → all 7 services healthy.
9. **Skills + commands population** — Copied `quick-push.md`, `save-to-md.md` from `~/claude-homelab/commands/`; copied `~/.claude/commands/axon/` (20 commands) and `~/.claude/skills/axon/`.
10. **Plugin mirror** — Copied entire `~/.claude/plugins/` to container dir; rewrote `/home/jmagar/.claude` → `/home/node/.claude` in `installed_plugins.json` and `known_marketplaces.json` via `sed`.
11. **mcp.json creation** — Wrote container-appropriate MCP config; removed `exa` on user request.
12. **SSH keys** — Uncommented SSH mount in docker-compose; recreated container.
13. **axon MCP via SSH** — Added `axon` server to `mcp.json` using `ssh` as the command, tunneling stdio to host `axon-mcp` binary.
14. **Bug fix: .env not sourced** — Updated SSH command to `bash -c 'set -a; source .env; set +a; exec axon-mcp'`.
15. **Bug fix: git missing** — Added `git` to Dockerfile apt-get install; rebuilt image.

---

## Key Findings

- **`--strict-mcp-config` alone = no MCPs** — Without `--mcp-config <file>`, `--strict-mcp-config` blocks all MCP loading. Both flags are required to load a specific set of servers. (`claude-stream-types.ts:68-72`)
- **Docker cache didn't bust on context deletion** — First build after deleting claude-session/watcher s6 dirs served step 15 from cache. `--no-cache` required to get a clean image.
- **`axon-mcp` needs `.env` at runtime** — The binary calls `normalize_local_service_url()` to rewrite container DNS to `127.0.0.1:PORT`, but it still needs the env vars present to build those URLs. Sourcing `.env` in the SSH command fixes this.
- **known_hosts is read-only** — SSH mount is `:ro`, so `StrictHostKeyChecking=accept-new` silently fails to write. Switched to `StrictHostKeyChecking=no` + `UserKnownHostsFile=/dev/null`.
- **`s6-rc -da list` shows only DOWN services** — A clean `-da list` output (only `s6rc-fdholder`) means all services are UP, not that they're missing.
- **Plugin marketplaces require `git`** — Claude Code's `/plugin` command tries to `git clone` marketplace repos. `node:24-slim` has no `git` by default.

---

## Technical Decisions

- **No `CLAUDE_CONFIG_DIR`** — Previously needed to isolate from the host's messy `~/.claude`. With a dedicated mount that IS the clean dir, the env var is redundant.
- **SSH stdio tunnel for axon MCP** — `axon-mcp` is a Rust binary that only exists in `axon-workers`. Rather than copying the binary or exposing an HTTP MCP transport, SSH pipes the MCP wire protocol through `host.docker.internal`. The host runs the binary with full infra access via localhost-mapped ports.
- **`StrictHostKeyChecking=no` + `/dev/null` known_hosts** — Internal Docker bridge to `host.docker.internal` is a trusted path; the SSH session is used for process execution only, not general network access.
- **`neo4j-memory` disabled in mcp.json** — `uvx` (uv package runner) is not in the container image. Marked `disabled: true` rather than removing, so it can be enabled later by adding `uv` to the Dockerfile.
- **Path rewrite via `sed`** — `installed_plugins.json` and `known_marketplaces.json` embed absolute host paths. `sed -i 's|/home/jmagar/.claude|/home/node/.claude|g'` rewrites them in place after `cp -r`.
- **`exa` removed from mcp.json** — User requested removal. The `exa` MCP was an HTTP server; removing it reduces cold-start latency for every Pulse query.

---

## Files Modified

| File | Change |
|------|--------|
| `docker-compose.yaml` | Replace `HOST_HOME/.claude` mount with `AXON_DATA_DIR/axon/claude`; uncomment SSH keys mount |
| `apps/web/app/api/pulse/chat/claude-stream-types.ts` | Add `--mcp-config`, `--dangerously-skip-permissions`, `--include-partial-messages`, `--effort medium`, `--plugin-dir` to `buildClaudeArgs` |
| `docker/web/Dockerfile` | Add `git` to apt-get; remove stale claude-session/watcher log dir mkdirs; update comments |
| `docker/web/s6-rc.d/claude-session/` | **Deleted** |
| `docker/web/s6-rc.d/claude-watcher/` | **Deleted** |
| `docker/web/s6-rc.d/user/contents.d/claude-session` | **Deleted** |
| `docker/web/s6-rc.d/user/contents.d/claude-watcher` | **Deleted** |
| `.env` | Added bootstrap comment for claude config dir mount |
| `.env.example` | Added bootstrap comment + documentation for claude config dir mount |

**Files Created (host, outside repo):**

| Path | Purpose |
|------|---------|
| `/home/jmagar/appdata/axon/claude/mcp.json` | MCP server config for Pulse subprocesses |
| `/home/jmagar/appdata/axon/claude/commands/quick-push.md` | Slash command (from claude-homelab) |
| `/home/jmagar/appdata/axon/claude/commands/save-to-md.md` | Slash command (from claude-homelab) |
| `/home/jmagar/appdata/axon/claude/commands/axon/` | 20 axon slash commands |
| `/home/jmagar/appdata/axon/claude/skills/axon/` | axon skill + routing cheatsheet |
| `/home/jmagar/appdata/axon/claude/plugins/` | 11 marketplaces, 36 installed plugins (path-corrected) |

---

## Commands Executed

```bash
# Bootstrap host dir
sudo mkdir -p /home/jmagar/appdata/axon/claude
sudo chown jmagar:jmagar /home/jmagar/appdata/axon/claude

# Clean rebuild
docker compose build --no-cache axon-web
docker compose up -d --force-recreate axon-web

# Copy skills + commands
cp ~/claude-homelab/commands/quick-push.md /home/jmagar/appdata/axon/claude/commands/
cp ~/claude-homelab/commands/save-to-md.md /home/jmagar/appdata/axon/claude/commands/
cp -r ~/.claude/commands/axon /home/jmagar/appdata/axon/claude/commands/
cp -r ~/.claude/skills/axon /home/jmagar/appdata/axon/claude/skills/

# Mirror plugins with path correction
cp -r ~/.claude/plugins /home/jmagar/appdata/axon/claude/
sed -i 's|/home/jmagar/\.claude|/home/node/.claude|g' \
  /home/jmagar/appdata/axon/claude/plugins/installed_plugins.json \
  /home/jmagar/appdata/axon/claude/plugins/known_marketplaces.json

# SSH tunnel MCP test
echo '{"jsonrpc":"2.0","id":1,"method":"initialize",...}' | \
  docker exec -i -u node axon-web /usr/bin/ssh -T \
  -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null \
  -i /home/node/.ssh/id_ed25519 jmagar@host.docker.internal \
  "bash -c 'set -a; source /home/jmagar/workspace/axon_rust/.env; set +a; exec /home/jmagar/workspace/axon_rust/target/debug/axon-mcp'"
# → {"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2024-11-05","serverInfo":{"name":"rmcp","version":"0.16.0"},...}}
```

---

## Behavior Changes (Before / After)

| Aspect | Before | After |
|--------|--------|-------|
| `~/.claude` mount source | `${HOST_HOME}/.claude` (dev machine's global dir) | `${AXON_DATA_DIR}/axon/claude` (clean project-owned dir) |
| s6 services in axon-web | `pnpm-dev`, `claude-session`, `claude-watcher` | `pnpm-dev` only |
| Pulse CLI flags | `-p`, `--output-format stream-json`, `--verbose`, `--system-prompt`, `--strict-mcp-config` | + `--mcp-config /home/node/.claude/mcp.json`, `--dangerously-skip-permissions`, `--include-partial-messages`, `--effort medium`, `--plugin-dir /home/node/.claude/plugins` |
| MCP servers in Pulse | None (strict-mcp-config blocked all) | `swag`, `chrome-devtools`, `codex`, `axon` (via SSH tunnel) |
| Plugin marketplace | Unusable (no git in image) | Functional (`git` 2.39.5 installed) |
| SSH keys | Not mounted | `${HOST_HOME}/.ssh:/home/node/.ssh:ro` |
| `axon` MCP availability | Host-only (binary not in web container) | Available in Pulse via SSH stdio tunnel |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `docker exec axon-web ls /home/node/.claude/` | `commands plugins skills` | `commands plugins skills` | ✅ |
| `docker exec axon-web /command/s6-rc -da list` | empty (all up) | `s6rc-fdholder` only | ✅ |
| `docker exec axon-web /command/s6-svstat /run/service/pnpm-dev` | up | `up (pid 39 pgid 39)` | ✅ |
| `curl -sf http://localhost:49010 -w "%{http_code}"` | `200` | `200` | ✅ |
| `docker exec axon-web git --version` | git installed | `git version 2.39.5` | ✅ |
| MCP initialize over SSH tunnel | valid JSON response | `{"serverInfo":{"name":"rmcp","version":"0.16.0"},...}` | ✅ |
| `docker exec axon-web ls /home/node/.ssh/` | key files visible | `id_ed25519 id_ed25519.pub config known_hosts ...` | ✅ |
| Plugin count in installed_plugins.json | >0 with correct paths | `36 plugins, paths: /home/node/.claude/...` | ✅ |
| Compose stack health | all 7 healthy | all 7 healthy | ✅ |

---

## Source IDs + Collections Touched

No axon embed/retrieve operations were performed during this session (infrastructure and tooling work only).

---

## Risks and Rollback

- **SSH tunnel reliability** — If `host.docker.internal` is unreachable or the SSH key is revoked, the `axon` MCP will fail to start. Pulse queries still work; they just lack the axon tool. Rollback: remove `axon` entry from `mcp.json`.
- **Plugin path mismatch** — If `installed_plugins.json` paths drift from actual cache locations (e.g., after a plugin update on the host), Claude will fail to load those plugins. Fix: re-run the `cp -r` + `sed` mirror. Rollback: delete `plugins/` dir entirely; Claude degrades gracefully to no plugins.
- **Stale plugin cache** — The mirrored plugins are a point-in-time snapshot of the host's cache. Plugin updates on the host don't auto-propagate. A future `docker exec` or bind-mount of the full plugins dir would fix this permanently.
- **axon-mcp debug binary** — SSH tunnel targets `target/debug/axon-mcp`. Debug binaries are slower and may not exist if the workspace is cleaned. Switch to `target/release/axon-mcp` for production use.

---

## Decisions Not Taken

- **`CLAUDE_CONFIG_DIR` env var** — Originally used to redirect Claude config to a custom path. Unnecessary now that the mount itself points to the clean dir. Removed to reduce confusion.
- **Copying `axon-mcp` binary into the web image** — Would require multi-stage build changes and re-sync on every binary update. SSH tunnel is simpler and always uses the current build.
- **HTTP/SSE MCP transport for axon** — Would require adding an HTTP server mode to `axon-mcp`. SSH stdio reuse is zero-cost given the SSH client is already in the image.
- **`uv` in the Dockerfile for neo4j-memory** — Would add Python toolchain complexity. Disabled the server instead; easy to re-enable later.
- **Mounting full host `~/.claude` as plugins source** — Would keep plugins in sync automatically but would re-introduce the original problem (host-specific paths, state leakage).

---

## Open Questions

- Does `--effort medium` have a meaningful effect in `stream-json` non-interactive mode, or is it silently ignored? Needs testing against a real Pulse query.
- The `axon · ✘ failed` error the user saw — root cause confirmed as missing `.env` + wrong known_hosts behavior, but the exact Claude Code error message was not captured for verification.
- Plugin marketplace auto-update (`"autoUpdate": true`) will try to `git clone` on refresh. Will it respect the container's `HOME=/home/node` and write to the correct path, or will it write to a root-owned location?
- `neo4j-memory` MCP is disabled. Adding `uv` to the Dockerfile would enable it — worth doing if graph memory is needed in Pulse queries.

---

## Next Steps

- Switch `axon-mcp` SSH target from `target/debug` to `target/release` once a release build is current.
- Test a live Pulse query to confirm `axon` MCP tool appears and executes correctly.
- Consider automating plugin mirror sync (script or cron) so host plugin updates propagate to the container config dir.
- Evaluate adding `uv` to the Dockerfile to enable the `neo4j-memory` MCP server.
- Update `docker/CLAUDE.md` to remove stale references to `claude-session`/`claude-watcher` and document the new SSH MCP tunnel pattern.
