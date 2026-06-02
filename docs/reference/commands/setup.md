# axon setup / preflight / compose / smoke
Last Modified: 2026-06-01

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
axon setup plugin-hook [--no-setup] [--json]
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
| `smoke` | Prewarm TEI, crawl `example.com`, and run a simple `ask` proof. |
| `setup targets` | List concrete SSH aliases from `~/.ssh/config`. |
| `setup plugin-hook` | Hook-safe path used by Claude Code SessionStart. Probes `/readyz` first and exits silently when the stack is already healthy; only runs preflight + compose when the stack is down. Use `--no-setup` for check-only mode. |

## `setup plugin-hook` Behavior

Run by the plugin's SessionStart hook on every session start. To avoid redeploying
(or emitting noise on) an already-running host, it short-circuits:

1. Refresh the user's `~/.local/bin/axon` copy and apply plugin env options.
2. **Probe `http://127.0.0.1:8001/readyz` once (3s timeout).** `/readyz` itself
   asserts qdrant + tei readiness, so a 200 means the whole stack is up. When it
   succeeds the hook exits `0` immediately — **no preflight, no `compose pull`/`up`,
   and no stdout** in human mode (`--json` prints `{"stack":"already_healthy",...}`).
3. **If `/readyz` is unreachable**, fall through to the normal path: run preflight
   checks, and if prerequisites pass but the stack isn't up, run `compose pull` +
   `up -d` followed by readiness checks.

This means a missing host prerequisite (e.g. `nvidia-smi`) no longer forces a
redeploy when axon is plainly serving. For an explicit deploy/restart, use
`axon compose up` directly or the `/axon-deploy` plugin slash command.

## `setup init` Options

| Option | Env key | Purpose |
|--------|---------|---------|
| `--mcp-host <host>` | `AXON_MCP_HTTP_HOST` | MCP HTTP bind host. |
| `--mcp-port <port>` | `AXON_MCP_HTTP_PORT` | MCP HTTP bind port. |
| `--auth-mode bearer\|oauth` | `AXON_MCP_AUTH_MODE` | Auth mode. Defaults to `bearer`. |
| `--mcp-token <token>` | `AXON_MCP_HTTP_TOKEN` | Static bearer token. Generated when bearer mode is selected and no token exists. |
| `--oauth-public-url <url>` | `AXON_MCP_PUBLIC_URL` | Required for OAuth mode. |
| `--google-client-id <id>` | `AXON_MCP_GOOGLE_CLIENT_ID` | Required for OAuth mode. |
| `--google-client-secret <secret>` | `AXON_MCP_GOOGLE_CLIENT_SECRET` | Required for OAuth mode. |
| `--auth-admin-email <email>` | `AXON_MCP_AUTH_ADMIN_EMAIL` | Required for OAuth mode. |
| `--tavily-api-key <key>` | `TAVILY_API_KEY` | Enables search/research. |
| `--github-token <token>` | `GITHUB_TOKEN` | Raises GitHub ingest rate limits. |
| `--reddit-client-id <id>` | `REDDIT_CLIENT_ID` | Required for Reddit ingest. |
| `--reddit-client-secret <secret>` | `REDDIT_CLIENT_SECRET` | Required for Reddit ingest. |

## Minimum Configuration

For local bearer-token operation, no manual env values are required. `setup init`
creates the local home, defaults to loopback MCP HTTP, writes
`AXON_MCP_AUTH_MODE=bearer`, and generates `AXON_MCP_HTTP_TOKEN`.

Optional features need their own credentials:

| Feature | Required outside Axon |
|---------|-----------------------|
| LLM features (`ask`, `evaluate`, `suggest`, LLM fallback extract, research synthesis) | Gemini CLI authenticated under `~/.gemini`. |
| Web search / research | `TAVILY_API_KEY`. |
| GitHub ingest with higher rate limits | `GITHUB_TOKEN`. |
| Reddit ingest | `REDDIT_CLIENT_ID` and `REDDIT_CLIENT_SECRET`. |
| OAuth MCP auth | `AXON_MCP_PUBLIC_URL`, `AXON_MCP_GOOGLE_CLIENT_ID`, `AXON_MCP_GOOGLE_CLIENT_SECRET`, and `AXON_MCP_AUTH_ADMIN_EMAIL`. |

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
axon setup plugin-hook --no-setup
```
