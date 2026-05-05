# Deployment Guide -- Axon MCP

Deployment patterns for the Axon MCP server. Choose the method that fits your environment.

## Local development

### Full stack (recommended)

```bash
just dev
```

This starts infrastructure services, builds the binary, and launches `axon serve` which supervises the MCP HTTP server (port 8001) alongside the backend, workers, and web UI.

### MCP server only

```bash
# Start infrastructure
just services-up

# Run MCP server standalone (stdio)
axon mcp

# Or HTTP transport
axon serve mcp
```

### Lite mode (zero infrastructure)

```bash
AXON_LITE=1 axon mcp
```

Runs with SQLite for job storage. Requires Qdrant and TEI for embedding/search. Does not support graph, refresh, or watch operations.

## Docker

### Infrastructure + app containers

```bash
# Start infrastructure (Qdrant, TEI, Chrome)
just services-up
```

### Docker Compose split

| File | Contents | Env file |
|------|----------|----------|
| `config/docker-compose.services.yaml` | Infrastructure (Qdrant, TEI, Chrome) | repo-root `services.env` |

The compose file creates the `axon` bridge network. Pass `--env-file .env` when
running it directly so repo-root `.env` is used for `${VAR}` interpolation.

### GPU acceleration

For NVIDIA hosts with GPU-accelerated TEI:

```bash
docker compose --env-file .env -f config/docker-compose.services.yaml up -d
```

CPU-only hosts should override the TEI image/settings or point `TEI_URL` at an
external CPU embedding endpoint.

### Local app runtime

The tracked compose file starts infrastructure only. Run `axon serve` locally to
supervise the MCP HTTP server, backend bridge, workers, shell server, and web UI.

### Build

```bash
# Build Chrome image
docker compose --env-file .env -f config/docker-compose.services.yaml build axon-chrome
```

Run compose commands from the repo root. The services compose file lives under
`config/`, so paths inside it resolve relative to `config/`; its `env_file`
entry intentionally points back to the repo-root `services.env`.

### Health checks

```bash
# Infrastructure
docker compose --env-file .env -f config/docker-compose.services.yaml ps

# Service connectivity
./scripts/axon doctor
```

## Data volumes

All persistent data uses `${AXON_DATA_DIR:-./data}/axon/...`:

| Volume | Content |
|--------|---------|
| `$AXON_DATA_DIR/axon/jobs.db` | SQLite job database |
| `$AXON_DATA_DIR/axon/qdrant` | Qdrant vector storage |
| `$AXON_DATA_DIR/axon/tei-data` | TEI model cache |
| `$AXON_DATA_DIR/axon/artifacts` | MCP response artifacts |
| `$AXON_DATA_DIR/axon/output` | CLI output files |

## See also

- [TRANSPORT.md](TRANSPORT.md) -- transport configuration
- [CONNECT.md](CONNECT.md) -- client connection methods
- [ENV.md](ENV.md) -- environment variables
