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

The compose file creates the `axon` bridge network and reads repo-root `.env`
for `${VAR}` interpolation.

### GPU acceleration

For NVIDIA hosts with GPU-accelerated TEI:

```bash
docker compose -f config/docker-compose.services.yaml up -d
```

CPU-only hosts should override the TEI image/settings or point `TEI_URL` at an
external CPU embedding endpoint.

### Container architecture

The `axon-workers` container uses s6-overlay for process supervision:

| s6 service | Binary command | Purpose |
|------------|---------------|---------|
| `web-server` | `axon serve` | Backend bridge + MCP HTTP |
| `crawl-worker` | `axon crawl worker` | Crawl job processor |
| `embed-worker` | `axon embed worker` | Embedding pipeline |
| `extract-worker` | `axon extract worker` | LLM extraction |
| `ingest-worker` | `axon ingest worker` | Source ingestion |
| `graph-worker` | `axon graph worker` | Neo4j graph building |

All worker processes run as the `axon` user (UID 1001) via `s6-setuidgid`.

The `axon-web` container uses s6-overlay for:
- `pnpm-dev`: Next.js dev server
- `pnpm-watcher`: Polls lockfile for changes
- `claude-session`: Persistent Claude Code session
- `claude-watcher`: Hot-reload trigger

### Build

```bash
# Build Chrome image
docker compose -f config/docker-compose.services.yaml build axon-chrome
```

Run compose commands from the repo root. The services compose file lives under
`config/`, so paths inside it resolve relative to `config/`; its `env_file`
entry intentionally points back to the repo-root `services.env`.

### Health checks

```bash
# Infrastructure
docker compose -f config/docker-compose.services.yaml ps

# App containers
docker compose ps

# Service connectivity
docker exec axon-workers axon doctor
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
