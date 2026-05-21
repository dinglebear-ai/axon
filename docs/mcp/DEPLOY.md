# Deployment Guide -- Axon MCP

Deployment patterns for the Axon MCP server. Choose the method that fits your environment.

## Local development

### Full stack (recommended)

```bash
just dev
```

This starts infrastructure services, builds the binary, and launches `axon serve`
with the unified HTTP API, MCP HTTP endpoint, web panel, and in-process workers
on port 8001.

### MCP server only

```bash
# Start infrastructure
just services-up

# Run MCP server standalone (stdio)
axon mcp

# Or HTTP transport
axon serve mcp
```

### Minimal local runtime

```bash
axon mcp
```

Jobs use SQLite and in-process workers. Qdrant and TEI are still required for
embedding/search paths.

## Docker

### Infrastructure + app containers

```bash
# Start infrastructure (Qdrant, TEI, Chrome)
just services-up
```

### Docker Compose split

| File | Contents | Env file |
|------|----------|----------|
| `docker-compose.prod.yaml` | Axon server, Qdrant, TEI, Chrome | `~/.axon/.env` |

The compose file creates the `axon` bridge network. Pass `--env-file ~/.axon/.env`
when running it directly so the canonical appdata env file is used for `${VAR}`
interpolation.

### GPU acceleration

For NVIDIA hosts with GPU-accelerated TEI:

```bash
docker compose --env-file ~/.axon/.env -f docker-compose.prod.yaml up -d axon-qdrant axon-tei axon-chrome
```

CPU-only hosts should override the TEI image/settings or point `TEI_URL` at an
external CPU embedding endpoint.

### Local app runtime

The tracked compose file starts the Axon server plus Qdrant, TEI, and Chrome.
Run `axon serve` locally only when you want to bypass Compose and run the
unified HTTP API, MCP HTTP endpoint, web panel, and in-process workers directly.

### Build

```bash
# Build Chrome image
docker compose --env-file ~/.axon/.env -f docker-compose.prod.yaml build axon-chrome
```

Run compose commands from the repo root. Host-side state defaults to
`${HOME}/.axon` through `AXON_HOME`; the container sees that same appdata tree as
`/home/axon/.axon`.

### Health checks

```bash
# Infrastructure
docker compose --env-file ~/.axon/.env -f docker-compose.prod.yaml ps

# Service connectivity
./scripts/axon doctor
```

## Data volumes

Runtime data uses `${AXON_DATA_DIR:-~/.axon}/...`; Docker bind mounts use `${AXON_HOME:-$HOME/.axon}/...`. Keep them aligned unless relocating the entire Axon appdata tree.

| Volume | Content |
|--------|---------|
| `$AXON_DATA_DIR/jobs.db` | SQLite job database |
| `$AXON_DATA_DIR/qdrant` | Qdrant vector storage |
| `$AXON_DATA_DIR/tei` | TEI model cache |
| `$AXON_DATA_DIR/artifacts` | MCP response artifacts |
| `$AXON_DATA_DIR/output` | CLI output files |

## See also

- [TRANSPORT.md](TRANSPORT.md) -- transport configuration
- [CONNECT.md](CONNECT.md) -- client connection methods
- [ENV.md](ENV.md) -- environment variables
