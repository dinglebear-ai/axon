# Claude Hot-Reload — claude-session + claude-watcher
Last Modified: 2026-02-27

The `axon-web` container runs a persistent Claude Code session alongside the Next.js
dev server as an s6 service. The web app communicates with this session so users always
get Claude with the latest agents, skills, hooks, and project config — no container
restart required.

---

## Table of Contents

1. [Architecture](#architecture)
2. [Watched Paths](#watched-paths)
3. [Setup](#setup)
4. [Verification](#verification)
5. [Introspection & Logs](#introspection--logs)
6. [Troubleshooting](#troubleshooting)
7. [Design Decisions](#design-decisions)

---

## Architecture

Two s6 `longrun` services work together:

```
┌──────────────────────────────────────────────────────────┐
│ axon-web container                                       │
│                                                          │
│  pnpm-dev          claude-session  ←─── claude-watcher  │
│  (Next.js dev)     (persistent      (inotifywait         │
│       │             claude process)  one-shot loop)      │
│       │                  │                │              │
│       │                  │        watches whitelisted    │
│       ↓                  │        paths in ~/.claude/   │
│  browser (Pulse UI) ←────┘        and /workspace/       │
└──────────────────────────────────────────────────────────┘
```

**`claude-session`** runs `claude --continue --fork-session --dangerously-skip-permissions`
as the `node` user from `$AXON_WORKSPACE`, inside a `script -q -e` pseudo-TTY so it
stays in interactive REPL mode. It loads all config (agents, skills, hooks, settings)
at startup.

**`claude-watcher`** blocks on `inotifywait` watching a whitelist of config paths. On
any change it sleeps 1 second (debounce), then sends `s6-svc -r` to restart
`claude-session`. s6 immediately restarts the watcher, making the watch continuous.

**Why `--fork-session`**: `--continue` alone reuses the session ID. If s6 restarts the
process before the previous session fully exits, a 409 conflict occurs. `--fork-session`
creates a new session ID branching from the conversation tree — context preserved,
no collision.

---

## Watched Paths

Whitelist approach — only paths that affect Claude's runtime behavior are watched.
Runtime-written dirs (`projects/`, `statsig/`) are intentionally excluded to prevent
restart loops while Claude is running.

### User-level (`~/.claude/`)

| Path | Type | Contains |
|------|------|----------|
| `~/.claude/agents/` | dir (recursive) | User-level custom agents |
| `~/.claude/commands/` | dir (recursive) | User-level slash commands |
| `~/.claude/hooks/` | dir (recursive) | User-level hooks |
| `~/.claude/output-styles/` | dir (recursive) | Output style definitions |
| `~/.claude/plugins/` | dir (recursive) | Plugins |
| `~/.claude/skills/` | dir (recursive) | User-level skills |
| `~/.claude/settings.json` | file | Global settings |
| `~/.claude/settings.local.json` | file | Local global settings overrides |
| ~~`~/.claude.json`~~ | excluded | Claude writes auth/token state here at runtime — watching it causes a restart loop |

### Project-level (`$AXON_WORKSPACE/.claude/`)

| Path | Type | Contains |
|------|------|----------|
| `.claude/agents/` | dir (recursive) | Project-specific agents |
| `.claude/commands/` | dir (recursive) | Project-specific slash commands |
| `.claude/hooks/` | dir (recursive) | Project-specific hooks |
| `.claude/output-styles/` | dir (recursive) | Project output style definitions |
| `.claude/settings.json` | file | Project settings |
| `.claude/settings.local.json` | file | Local project settings (gitignored) |
| `CLAUDE.md` | file | Project instructions |
| `.mcp.json` | file | MCP server config |

Paths that don't exist at watcher startup are silently skipped and not watched.
Create the dir/file and restart the container once to begin watching it.

---

## Setup

### Required `.env` variables

```bash
# Your home directory on the host machine
HOST_HOME=/home/yourname

# Host workspace parent dir — mounted at /workspace in axon-web
# Example: AXON_WORKSPACE=/home/yourname/workspace
AXON_WORKSPACE=/home/yourname/workspace
```

### Volume mounts (docker-compose.yaml — axon-web)

These are already configured in `docker-compose.yaml`:

```yaml
volumes:
  - ${HOST_HOME:-${HOME}}/.claude:/home/node/.claude
  - ${HOST_HOME:-${HOME}}/.claude.json:/home/node/.claude.json
  - ${AXON_WORKSPACE:-${HOME}/workspace}:/workspace
```

### Environment (docker-compose.yaml — axon-web)

```yaml
environment:
  HOME: /home/node
  AXON_WORKSPACE: /workspace
```

### bypassPermissions

`claude-session` passes `--dangerously-skip-permissions` directly so it starts without
a workspace trust prompt or permission dialogs. s6 services run without a TTY — Claude
would otherwise refuse to start in interactive mode.

`${AXON_WORKSPACE}/.claude/settings.local.json` can also set these at the project level
if you want them for interactive sessions in the same workspace:

```json
{
  "skipDangerousModePermissionPrompt": true,
  "permissions": {
    "defaultMode": "bypassPermissions"
  }
}
```

This file is gitignored (`.claude/` wildcard at `.gitignore:53`). It is only active
when Claude runs with this project as its working directory.

---

## Verification

After `just up`:

```bash
# Confirm all three services are running
docker exec axon-web s6-svstat /run/service/pnpm-dev
docker exec axon-web s6-svstat /run/service/claude-session
docker exec axon-web s6-svstat /run/service/claude-watcher

# Check what paths the watcher is currently monitoring
docker exec axon-web tail -20 /var/log/axon/claude-watcher/current

# Trigger a hot-reload manually
touch ~/.claude/settings.json
# → watcher log should show: "change detected: ... — restarting claude-session"
# → claude-session log should show: "[claude-session] restarted by watcher (SIGTERM)"
```

---

## Introspection & Logs

```bash
# Live log stream — claude-session
docker exec axon-web tail -f /var/log/axon/claude-session/current

# Live log stream — claude-watcher
docker exec axon-web tail -f /var/log/axon/claude-watcher/current

# Live log stream — pnpm-dev (Next.js)
docker exec axon-web tail -f /var/log/axon/pnpm-dev/current

# Restart count / uptime
docker exec axon-web s6-svstat /run/service/claude-session
docker exec axon-web s6-svstat /run/service/claude-watcher

# Manual restart (forces config reload without touching a file)
docker exec axon-web s6-svc -r /run/service/claude-session

# Kill watcher (s6 restarts it immediately — useful to reset watch state)
docker exec axon-web s6-svc -r /run/service/claude-watcher

# Open a shell as node user
docker exec -it -u node axon-web bash
```

---

## Troubleshooting

### `claude-session` crash-loops immediately (code=1)

**Cause A:** `claude` binary not found on `PATH`.
**Fix:** Rebuild the image — `just up`. The Dockerfile installs Claude via
`https://claude.ai/install.sh` and dereferences the symlink to `/usr/local/bin/claude`.

**Cause B:** `script` not installed in the image.
**Fix:** Rebuild the image — `script` (util-linux) is installed in the base `node:24-slim`
image and should always be present.

### `claude-session` starts but exits cleanly (code=0, signal=-1)

**Cause:** `--continue` found no previous session to resume; Claude exited normally.
**Fix:** Start an initial session manually: `docker exec -it -u node axon-web claude`.
Once a session exists, `--continue --fork-session` will resume it on next restart.

### `claude-watcher` logs "no watchable paths found — retrying in 30s"

**Cause:** Neither `$AXON_WORKSPACE/.claude/` nor `$AXON_WORKSPACE/CLAUDE.md` nor
`$HOME/.claude/` exist at the mounted path.
**Fix:** Confirm `AXON_WORKSPACE` and `HOST_HOME` are set correctly in `.env`.
Check mounts: `docker exec axon-web ls /workspace/.claude /home/node/.claude`.

### Changes not triggering restarts

**Cause:** The changed file/dir is not in the whitelist, or it didn't exist when the
watcher started (skipped at startup).
**Fix:** Ensure the path is in the whitelist in `docker/web/s6-rc.d/claude-watcher/run`.
If the dir was just created, restart the container once to pick it up.

### Restart loop (session restarting constantly)

**Cause:** A path in the whitelist is being written by Claude itself during the session.
**Symptom:** `claude-session` restarts every few seconds.
**Fix:** Check watcher logs to identify which path is triggering. Remove it from the
whitelist in `claude-watcher/run`.

### 1-second debounce not enough

**Symptom:** Session loads a half-written agent/skill file.
**Fix:** Increase `sleep 1` to `sleep 2` in `docker/web/s6-rc.d/claude-watcher/run`.

---

## Design Decisions

| Decision | Rationale |
|----------|-----------|
| Whitelist over recursive + exclude | Recursive `~/.claude/` with exclusions is fragile — Claude can add new runtime dirs anytime. Whitelist is explicit and never accidentally watches write-heavy paths. |
| `--fork-session` | Prevents 409 session ID collision on rapid restarts. Creates a new ID branching from the conversation tree — context preserved. |
| `inotifywait` one-shot (no `-m`) | s6's restart loop replaces the inner `while read` loop. Simpler, no subprocess management, `finish` script correctly reflects exit vs error. |
| 1-second debounce | Saving a file triggers both `modify` and `moved_to` events. Debounce absorbs bursts so the session restarts once, not twice. |
| `settings.local.json` not `settings.json` | Environment-specific; gitignored. Devs running locally don't inherit container-only `bypassPermissions`. |
| `claude` binary dereferenced to `/usr/local/bin/` | The installer drops a symlink into `/root/.local/bin/` pointing to the real binary under `/root/.local/share/claude/versions/`. `/root/.local/` is mode 700 — the `axon` user (non-root) cannot traverse it. Copying the dereferenced binary to a world-executable path fixes the EACCES. |
