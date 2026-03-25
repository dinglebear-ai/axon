# axon serve
Last Modified: 2026-03-03

Start Axon's axum WebSocket execution bridge backend used by `apps/web`.

## Synopsis

```bash
axon serve [FLAGS]
```

## Flags

All global flags apply. Key flags for this command:

| Flag | Default | Description |
|------|---------|-------------|
| `--port <n>` | `49000` | Port for the serve backend. Env: `AXON_SERVE_PORT`. |

Host binding is controlled by `AXON_SERVE_HOST` (default `127.0.0.1`).

## Endpoints

- `GET /ws` - command execution WebSocket bridge
- `GET /ws/shell` - shell WebSocket (loopback-only)
- `GET /output/{*path}` - serve generated output files
- `GET /download/{job_id}/pack.md` - download job output as markdown pack
- `GET /download/{job_id}/pack.xml` - download job output as XML pack
- `GET /download/{job_id}/archive.zip` - download job output as zip archive
- `GET /download/{job_id}/file/{*path}` - download individual job artifact file

## Examples

```bash
# Default localhost bind on :49000
axon serve

# Custom port
axon serve --port 8080

# Bind all interfaces (for reverse proxy/container use)
AXON_SERVE_HOST=0.0.0.0 axon serve --port 49000
```

## Notes

- `serve` does not host the Next.js frontend itself; it provides backend WS/HTTP routes.
- `/ws/shell` rejects non-loopback clients with HTTP 403.
- See `docs/SERVE.md` for full protocol and architecture details.
