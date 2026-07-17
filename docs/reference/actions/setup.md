# axon setup / preflight / compose / smoke
Last Modified: 2026-07-15

<!-- BEGIN GENERATED ACTION SURFACES -->
## Surfaces

| Surface | Entry point |
|---|---|
| CLI | `axon setup ...` |
| REST | Not inventoried |
| MCP | Not exposed as a dedicated MCP action. |
| Service | `Not inventoried` |

Parity notes: This action page is missing from docs/reference/api-parity.md.
<!-- END GENERATED ACTION SURFACES -->


Local service-stack bootstrap is split into focused commands.

## Synopsis

```bash
axon setup [--json]
axon setup init [options] [--json]
axon setup check [--json]
axon preflight [--json]
axon compose up [--json]
axon compose down [--json]
axon compose restart [--json]
axon compose rebuild [--json]
axon smoke [--json]
axon setup targets [--json]
axon setup plugin-hook [--json]
```

## Commands

| Command | Purpose |
|---------|---------|
| `setup` | Convenience wrapper: initialize local files/assets, start the service stack, then run preflight readiness checks. |
| `setup init` | Create or refresh `~/.axon`, `config.toml`, `.env`, and compose assets. Does not start services. |
| `setup check` | Alias for `preflight` — check local prerequisites and service readiness without mutating files or services. |
| `preflight` | Check local prerequisites, auth config, and running service readiness. Does not mutate files or start services. |
| `compose up` | Pull images, start the Docker service stack detached, then follow `docker compose logs -f` so startup is visible. Press Ctrl-C to stop watching logs; services keep running. |
| `compose down` | Stop the Docker service stack. |
| `compose restart` | Restart the Docker service stack. |
| `compose rebuild` | Rebuild the Axon image and start the Docker service stack. |
| `smoke` | Prewarm TEI, index `example.com` through the source pipeline, and run a simple `ask` proof. |
| `setup targets` | List concrete SSH aliases from `~/.ssh/config`. |
| `setup plugin-hook` | Probe-only path used by Claude Code SessionStart. Checks `/readyz`; exits silently when the stack is up, or advises `/axon-deploy` when it is down. Never deploys. |

## `setup plugin-hook` Behavior

Run by the plugin's SessionStart hook on every session start. **It never deploys** —
provisioning is the `/axon-deploy` slash command (or `axon setup` / `axon compose up`).
The hook only does:

1. Refresh the user's `~/.local/bin/axon` copy and apply plugin env options.
2. **Probe `/readyz` once (3s timeout)** at the configured bind (`AXON_HTTP_HOST`/`AXON_HTTP_PORT` from `~/.axon/.env`, default `127.0.0.1:8001`; bind-all hosts are probed over loopback). `/readyz` itself asserts qdrant + tei readiness, so a 200 means the whole stack is up.
   - **Up** → exit `0` immediately, **no stdout** in human mode (`--json` prints `{"stack":"already_healthy",...}`).
   - **Down** → print one line, `axon stack not reachable on /readyz — run /axon-deploy to start it`, and exit `0` (non-blocking advisory; `--json` prints `{"stack":"down","action":"run /axon-deploy",...}`).

The hook runs **no** preflight checks and **no** `docker compose`. To provision or
restart the stack, use the `/axon-deploy` plugin slash command, or `axon setup` /
`axon compose up|restart|rebuild` directly.

## `setup init` Options

| Option | Env key | Purpose |
|--------|---------|---------|
| `--mcp-host <host>` | `AXON_HTTP_HOST` | MCP HTTP bind host. |
| `--mcp-port <port>` | `AXON_HTTP_PORT` | MCP HTTP bind port. |
| `--auth-mode bearer\|oauth` | `AXON_AUTH_MODE` | Auth mode. Defaults to `bearer`. |
| `--mcp-token <token>` | `AXON_HTTP_TOKEN` | Static bearer token. Generated when bearer mode is selected and no token exists. |
| `--oauth-public-url <url>` | `AXON_PUBLIC_URL` | Required for OAuth mode. |
| `--google-client-id <id>` | `AXON_GOOGLE_CLIENT_ID` | Required for OAuth mode. |
| `--google-client-secret <secret>` | `AXON_GOOGLE_CLIENT_SECRET` | Required for OAuth mode. |
| `--auth-admin-email <email>` | `AXON_AUTH_ADMIN_EMAIL` | Required for OAuth mode. |
| `--tavily-api-key <key>` | `TAVILY_API_KEY` | Enables Tavily fallback search/research when SearXNG is not configured. |
| `--github-token <token>` | `GITHUB_TOKEN` | Raises GitHub source indexing rate limits. |
| `--reddit-client-id <id>` | `REDDIT_CLIENT_ID` | Required for Reddit source indexing. |
| `--reddit-client-secret <secret>` | `REDDIT_CLIENT_SECRET` | Required for Reddit source indexing. |

## Minimum Configuration

For local bearer-token operation, no manual env values are required. `setup init`
creates the local home, defaults to loopback MCP HTTP, writes
`AXON_AUTH_MODE=bearer`, and generates `AXON_HTTP_TOKEN`.

Optional features need their own credentials:

| Feature | Required outside Axon |
|---------|-----------------------|
| LLM features (`ask`, `evaluate`, `suggest`, LLM fallback extract, research synthesis) | Gemini CLI authenticated under `~/.gemini`. |
| Web search / research | `TAVILY_API_KEY`. |
| GitHub source indexing with higher rate limits | `GITHUB_TOKEN`. |
| Reddit source indexing | `REDDIT_CLIENT_ID` and `REDDIT_CLIENT_SECRET`. |
| OAuth MCP auth | `AXON_PUBLIC_URL`, `AXON_GOOGLE_CLIENT_ID`, `AXON_GOOGLE_CLIENT_SECRET`, and `AXON_AUTH_ADMIN_EMAIL`. |

## Examples

```bash
axon setup init
axon setup init --auth-mode oauth \
  --oauth-public-url https://axon.example.com \
  --google-client-id "$GOOGLE_CLIENT_ID" \
  --google-client-secret "$GOOGLE_CLIENT_SECRET" \
  --auth-admin-email you@example.com
axon compose up
axon preflight
axon smoke
axon setup plugin-hook
```
