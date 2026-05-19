# 2026-05-13 Axon MCP OAuth Proxy and Image Deploy

## Context

This session investigated a Claude `/mcp` reconnect failure:

```text
Failed to reconnect to plugin:axon:axon: HTTP 401 at https://axon.tootie.tv/mcp
```

The user could authenticate, but reconnect discovery failed after auth.

Working repo:

- Path: `/home/jmagar/workspace/axon_rust`
- Branch: `fix/unify-web-mcp-port-8001`
- Final observed git state: `fix/unify-web-mcp-port-8001...origin/fix/unify-web-mcp-port-8001 [ahead 7, behind 1]`

## Root Cause

There were two related discovery problems.

First, Axon generated a `WWW-Authenticate` challenge pointing at:

```text
https://axon.tootie.tv/mcp/.well-known/oauth-protected-resource
```

but the OAuth metadata router is mounted at:

```text
https://axon.tootie.tv/.well-known/oauth-protected-resource
```

The code fix in `src/mcp/auth.rs` makes `oauth_resource_url_from_parts()` return the public origin, not `origin + /mcp`.

Second, the SWAG reverse proxy on `squirts` routed protected-resource metadata paths through `/.well-known/oauth-protected-resource/mcp`, which Axon served as the embedded web app HTML. Exact `location = ...` blocks from `/mnt/appdata/swag/nginx/mcp-server.conf` overrode the broader regex block in `/mnt/appdata/swag/nginx/proxy-confs/axon.subdomain.conf`.

## Remote SWAG Changes

Host:

```text
squirts
```

Reviewed:

```text
/mnt/appdata/swag/nginx/proxy-confs/axon.subdomain.conf
/mnt/appdata/swag/nginx/mcp-server.conf
/mnt/appdata/swag/nginx/mcp-location.conf
```

Backup created:

```text
/mnt/appdata/swag/nginx/mcp-server.conf.bak-20260513153916
```

Changed `/mnt/appdata/swag/nginx/mcp-server.conf` so:

- `/.well-known/oauth-protected-resource` proxies to Axon unchanged.
- `/.well-known/oauth-protected-resource/mcp` rewrites back to `/.well-known/oauth-protected-resource`.
- `/mcp/.well-known/oauth-protected-resource` rewrites back to `/.well-known/oauth-protected-resource`.

Verification on `squirts`:

```text
docker exec swag nginx -t
```

passed, then SWAG was reloaded.

Public checks after reload returned JSON for:

```text
https://axon.tootie.tv/.well-known/oauth-protected-resource
https://axon.tootie.tv/.well-known/oauth-protected-resource/mcp
https://axon.tootie.tv/mcp/.well-known/oauth-protected-resource
https://axon.tootie.tv/.well-known/oauth-authorization-server
https://axon.tootie.tv/.well-known/oauth-authorization-server/mcp
```

## Axon Deploy

Built a new local Docker image from the current worktree:

```text
ghcr.io/jmagar/axon:latest
```

Release build completed successfully inside Docker.

The local `axon` service was recreated with:

```text
docker compose --env-file ~/.axon/.env up -d axon --no-deps --force-recreate
```

At deploy-time verification, the container was healthy and public `/mcp` returned:

```text
WWW-Authenticate: Bearer resource_metadata="https://axon.tootie.tv/.well-known/oauth-protected-resource"
```

Valid bearer requests reached the MCP handler and returned the expected raw GET response:

```text
400 Bad Request: Session ID is required
```

The image push initially failed due to GHCR auth, then a retry succeeded.

Published registry digest:

```text
ghcr.io/jmagar/axon@sha256:a1950774a8bad924c8133be856ac1e0e71368f29f6eecd7493ee3041b238ee8b
```

## Image Secret Check

Checked `ghcr.io/jmagar/axon:latest` for baked runtime env/secrets.

Image config env only contained:

```text
PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin
HOME=/home/axon
AXON_DATA_DIR=/home/axon/.axon
RUST_LOG=info
```

Other checks:

- No `.env`, `.env.*`, or `*.env` files in the runtime image.
- No exact secret values from `~/.axon/.env` found in runtime text files.
- No exact secret values from `~/.axon/.env` found in `/usr/local/bin/axon` strings.
- Docker history did not expose secret env values.
- `.dockerignore` excludes `.env` and `.env.*`, while allowing `.env.example`.

Expected finding: the compiled binary contains environment variable names such as `AXON_MCP_HTTP_TOKEN`, `TAVILY_API_KEY`, and `QDRANT_URL` because the app reads those variables at runtime. No actual secret values were found.

## Final Runtime Observation

Final check at `2026-05-13T19:10:20-04:00` showed the published image present locally:

```text
image_id=sha256:a1950774a8bad924c8133be856ac1e0e71368f29f6eecd7493ee3041b238ee8b
repo_digests=["ghcr.io/jmagar/axon@sha256:a1950774a8bad924c8133be856ac1e0e71368f29f6eecd7493ee3041b238ee8b"]
```

Final Docker state check showed supporting services running:

```text
axon-qdrant    Up 3 hours (healthy)
axon-tei       Up 3 hours (healthy)
axon-chrome    Up 2 days (healthy)
```

but the `axon` container itself was observed as:

```text
axon    ghcr.io/jmagar/axon:latest    Created
```

This differs from the earlier post-deploy healthy state and should be checked before assuming the public service is currently live.

## Open Questions

- Why was the local `axon` container in `Created` state at the final check after earlier healthy deployment verification?
- Whether the branch divergence (`ahead 7, behind 1`) is expected before the next push/merge.
- Whether the SWAG shared `mcp-server.conf` path-routing behavior should be generalized for other MCP services using root and path-based OAuth protected-resource metadata.
