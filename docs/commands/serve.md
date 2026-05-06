# axon serve
Last Modified: 2026-03-25

Start Axon's local stack supervisor.

## Synopsis

```bash
axon serve [FLAGS]
```

## Flags

All global flags apply. Key flags for this command:

| Flag | Default | Description |
|------|---------|-------------|
| `--port <n>` | `49000` | Port for the Rust websocket/download bridge. Env: `AXON_SERVE_PORT`. |

Host binding is controlled by `AXON_SERVE_HOST` (default `127.0.0.1`).

## Managed Processes

- Rust websocket/download bridge on `AXON_SERVE_PORT` (default `49000`)
- MCP HTTP server on `AXON_MCP_HTTP_PORT` (default `8001`)
- `apps/web/shell-server.mjs` on `SHELL_SERVER_PORT` (default `49011`)
- `apps/web` Next.js dev server on `AXON_WEB_DEV_PORT` (default `49010`)
- Full-mode workers: crawl, embed, extract, ingest, refresh, and graph when Neo4j is configured

## Container Preflight

`axon serve` fails fast if required infrastructure containers are not running and healthy.

Required infrastructure:

- `axon-qdrant`
- `axon-tei`
- `axon-chrome` (for Chrome render mode)

## Bridge Endpoints

- `GET /ws` - command execution WebSocket bridge
- `GET /ws/shell` - shell WebSocket (loopback-only)
- `GET /output/{*path}` - serve generated output files
- `GET /download/{job_id}/pack.md` - download job output as markdown pack
- `GET /download/{job_id}/pack.xml` - download job output as XML pack
- `GET /download/{job_id}/archive.zip` - download job output as zip archive
- `GET /download/{job_id}/file/{*path}` - download individual job artifact file

## Examples

```bash
# Default localhost bind on :49000 and supervise the local stack
axon serve

# Custom port
axon serve --port 8080

# Bind the Rust bridge on all interfaces
AXON_SERVE_HOST=0.0.0.0 axon serve --port 49000
```

## Notes

- `serve` is now the primary local dev entrypoint.
- `serve` restarts failed child processes with bounded exponential backoff.
- `serve` aborts the whole stack after repeated fast failures instead of crash-looping forever.
- `serve` does not auto-start Docker containers; it only checks them.
- `/ws/shell` rejects non-loopback clients with HTTP 403.
> Note: this doc was flagged for rewrite in the 2026-05-06 stale-docs audit — the description above predates the lite-mode simplification. The current `axon serve` runs the MCP HTTP server only (see `crates/cli/commands/serve.rs`).
