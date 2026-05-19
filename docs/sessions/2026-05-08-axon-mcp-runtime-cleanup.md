# Axon MCP Runtime Cleanup

Date: 2026-05-08

## Context

This session investigated the local `axon-mcp.service` user systemd unit after the service appeared to be restarting and after an unexpected OAuth drop-in was found at:

- `/home/jmagar/.config/systemd/user/axon-mcp.service.d/oauth.conf`

The goal changed from diagnosing the restart behavior to normalizing the runtime wiring:

- One canonical env file.
- No systemd drop-in override for Axon.
- Runtime binary behavior aligned with the `syslog-mcp` plugin pattern.
- Obsolete OAuth worktree cleaned up.

## Repo State

- Repository: `/home/jmagar/workspace/axon_rust`
- Branch: `main`
- HEAD: `6f5ff6d0` (`fix: plugin setup script`)
- Status at save time: clean relative to `origin/main`
- Worktrees at save time: only the main checkout remains

```text
worktree /home/jmagar/workspace/axon_rust
HEAD 6f5ff6d0eb8e3b6b6c21c0877d43f60d9006eb0c
branch refs/heads/main
```

## Starting State

`axon-mcp.service` was active, but it had a user systemd drop-in:

```text
/home/jmagar/.config/systemd/user/axon-mcp.service.d/oauth.conf
```

The base unit originally pointed at the plugin-cache path:

```text
/home/jmagar/.claude/plugins/cache/jmagar-lab/axon/575c090bcaf5/bin/axon serve mcp
```

The OAuth drop-in overrode `ExecStart` to an old feature worktree binary:

```text
/home/jmagar/workspace/axon_rust/.claude/worktrees/oauth/target/release/axon serve mcp
```

It also carried a separate OAuth env file. That meant the running service was not using the canonical plugin binary path and config was split across multiple files.

Systemd did not show a crash loop. `NRestarts=0`, and the repeated `worker quit with fatal: keep alive timeout after 300000ms` messages looked like MCP session churn rather than unit restarts.

## Investigation Findings

The unexpected OAuth drop-in was runtime drift from the OAuth feature work. It was not needed for the current merged service shape.

The Axon setup differed from `../syslog-mcp`:

- `syslog-mcp` keeps the systemd unit pointed at the installed plugin binary under plugin cache.
- `syslog-mcp` has `~/.local/bin/syslog` symlink to the plugin-cache binary.
- `syslog-mcp` renders `ExecStart=${CLAUDE_PLUGIN_ROOT}/bin/syslog serve mcp` from `scripts/plugin-setup.sh`.
- Its OAuth drop-in did not override the binary path.

The desired Axon state was therefore to keep the unit simple and make the plugin-cache binary the canonical runtime binary.

## Changes Made

Merged the OAuth-related env variables into the canonical Axon plugin env file:

```text
/home/jmagar/.claude/plugins/data/axon-jmagar-lab/axon.env
```

Removed the separate OAuth secrets env file after merging its values.

Removed the systemd drop-in so the service no longer depends on:

```text
/home/jmagar/.config/systemd/user/axon-mcp.service.d/oauth.conf
```

Removed the obsolete OAuth worktree:

```text
/home/jmagar/workspace/axon_rust/.claude/worktrees/oauth
```

Deleted the local branch:

```text
feat/oauth-auth
```

Built a release `axon` binary from the merged `origin/main` state in a temporary detached worktree:

```text
/tmp/axon-origin-main-build
```

That temporary worktree was used so the binary could be built from the exact merged main state without influence from the active checkout. It was removed after installation, along with its temporary target directory.

Installed the resulting release binary to:

```text
/home/jmagar/.claude/plugins/cache/jmagar-lab/axon/575c090bcaf5/bin/axon
```

Updated the local command symlink:

```text
~/.local/bin/axon -> /home/jmagar/.claude/plugins/cache/jmagar-lab/axon/575c090bcaf5/bin/axon
```

Patched `scripts/plugin-setup.sh` and installed the same script into the plugin cache. The setup script now:

- Defaults `MCP_HOST` to `0.0.0.0`.
- Preserves existing optional OAuth keys when rewriting the env file.
- Writes the env file with `umask 077`.

The preserved optional OAuth keys are:

```text
AXON_MCP_ALLOWED_ORIGINS
AXON_MCP_AUTH_MODE
AXON_MCP_PUBLIC_URL
AXON_MCP_GOOGLE_CLIENT_ID
AXON_MCP_GOOGLE_CLIENT_SECRET
AXON_MCP_AUTH_ADMIN_EMAIL
```

## Current Runtime State

The effective user systemd unit is now simple and has no drop-ins:

```ini
[Unit]
Description=axon MCP HTTP server
After=network.target

[Service]
ExecStart=/home/jmagar/.claude/plugins/cache/jmagar-lab/axon/575c090bcaf5/bin/axon serve mcp
EnvironmentFile=/home/jmagar/.claude/plugins/data/axon-jmagar-lab/axon.env
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
```

`systemctl --user show axon-mcp.service` reported:

```text
MainPID=2201934
ActiveState=active
SubState=running
DropInPaths=
```

The live process, plugin-cache binary, and local symlink all resolve to the same installed binary:

```text
/home/jmagar/.claude/plugins/cache/jmagar-lab/axon/575c090bcaf5/bin/axon
```

The live process executable and plugin binary had matching SHA-256:

```text
b3b0aca2ec96ef5b520d292313ada14c26ccc2f4eec02429aab346fdee7230e5
```

The service is listening on all interfaces at port `8001`:

```text
0.0.0.0:8001
```

The OAuth metadata endpoint responded successfully:

```text
http://127.0.0.1:8001/.well-known/oauth-authorization-server
```

## Env File State

The canonical env file contains the expected service, MCP, OAuth, and API keys:

```text
AXON_CHROME_REMOTE_URL
AXON_COLLECTION
AXON_MCP_ALLOWED_ORIGINS
AXON_MCP_AUTH_ADMIN_EMAIL
AXON_MCP_AUTH_MODE
AXON_MCP_GOOGLE_CLIENT_ID
AXON_MCP_GOOGLE_CLIENT_SECRET
AXON_MCP_HTTP_HOST
AXON_MCP_HTTP_PORT
AXON_MCP_HTTP_TOKEN
AXON_MCP_PUBLIC_URL
OPENAI_API_KEY
OPENAI_BASE_URL
OPENAI_MODEL
QDRANT_URL
TAVILY_API_KEY
TEI_URL
```

## Verification

Verification performed:

- Confirmed `axon-mcp.service` is active and running.
- Confirmed `DropInPaths=` is empty.
- Confirmed the effective `ExecStart` uses the plugin-cache binary.
- Confirmed the effective `EnvironmentFile` is the single canonical env file.
- Confirmed `~/.local/bin/axon`, `/proc/<pid>/exe`, and the plugin-cache binary resolve to the same path.
- Confirmed the live process binary hash matches the installed plugin-cache binary hash.
- Confirmed `axon` is listening on `0.0.0.0:8001`.
- Confirmed the OAuth metadata endpoint responds locally.
- Confirmed obsolete OAuth worktree removal.
- Confirmed local worktree inventory contains only the main checkout.
- Confirmed repo status is clean on `main` at `6f5ff6d0`.

No full Rust test suite was run during the save step. The meaningful verification for this session was release build plus live systemd/runtime smoke checks.

## Intentionally Left Alone

The plugin-cache directory name remained:

```text
575c090bcaf5
```

That directory is the installed plugin cache location currently referenced by the unit. The installed binary inside it was replaced with the release binary from merged main.

Secrets stayed in the machine-local plugin data env file and were not moved into tracked repo files.

## Open Questions

- Whether the plugin cache version directory should be regenerated through a formal plugin reinstall instead of keeping the existing cache directory and replacing its binary in place.
- Whether `scripts/plugin-setup.sh` should also be covered by an automated smoke test that verifies OAuth env preservation after setup reruns.
- Whether the OAuth metadata endpoint should be included in the standard `just` or plugin setup verification flow.
