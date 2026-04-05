# Setup Guide -- Axon

Step-by-step instructions to get Axon running locally or via Docker.

## Prerequisites

| Dependency | Version | Purpose |
|------------|---------|---------|
| Rust | 1.94+ | Compiler and toolchain (see `rust-toolchain.toml`) |
| Docker | 24+ | Infrastructure services |
| Docker Compose | v2+ | Service orchestration |
| just | latest | Task runner |
| Node.js | 22+ | Web UI (Next.js) |
| pnpm | 9+ | Web UI package manager |

Optional but recommended:

| Tool | Purpose |
|------|---------|
| sccache | Compilation cache (auto-detected by Justfile) |
| mold | Fast linker (auto-detected by Justfile) |
| cargo-nextest | Faster parallel test runner |

See [stack/PRE-REQS.md](stack/PRE-REQS.md) for detailed installation instructions.

## 1. Clone the repository

```bash
git clone https://github.com/jmagar/axon.git ~/workspace/axon_rust
cd ~/workspace/axon_rust
```

## 2. Run the setup script

```bash
just setup
```

This installs Rust toolchain components, cargo tools, pnpm, and verifies all prerequisites. If `just` is not installed, run `./scripts/dev-setup.sh` directly -- it installs `just` for you.

## 3. Configure environment

```bash
cp .env.example .env
chmod 600 .env
cp .env.example services.env
chmod 600 services.env
```

Edit `.env` and set required values:

```bash
# Postgres, Redis, RabbitMQ credentials
POSTGRES_PASSWORD=your_secure_password
REDIS_PASSWORD=your_secure_password
RABBITMQ_PASS=your_secure_password

# Connection URLs (update passwords to match)
AXON_PG_URL=postgresql://axon:your_secure_password@axon-postgres:5432/axon
AXON_REDIS_URL=redis://:your_secure_password@axon-redis:6379
AXON_AMQP_URL=amqp://axon:your_secure_password@axon-rabbitmq:5672

# Host paths
AXON_DATA_DIR=/path/to/persistent/data
HOST_HOME=/home/yourname
```

See [CONFIG.md](CONFIG.md) for the full variable reference.

## 4. Start infrastructure

```bash
just services-up
```

This starts PostgreSQL, Redis, RabbitMQ, Qdrant, TEI, and Chrome via `docker-compose.services.yaml`.

For GPU-accelerated embeddings (NVIDIA):

```bash
docker compose -f docker-compose.services.yaml -f docker-compose.gpu.yaml up -d
```

## 5. Run the local app stack

```bash
just dev
```

This builds the binary, starts infrastructure (if not already running), and launches `axon serve` which supervises:
- Backend bridge (port 49000)
- MCP HTTP server (port 8001)
- All 6 worker types (crawl, embed, extract, ingest, refresh, graph)
- Shell WebSocket server
- Next.js dev server (port 49010)

## 6. Verify

```bash
# Check service connectivity
./scripts/axon doctor

# Test a scrape
./scripts/axon scrape https://example.com --wait true

# Check the web UI
# Open http://localhost:49010
```

## Lite mode (zero infrastructure)

For quick testing without Postgres, Redis, or RabbitMQ:

```bash
AXON_LITE=1 ./scripts/axon scrape https://example.com --wait true
```

Lite mode uses SQLite for job storage and runs workers in-process. Qdrant and TEI are still required for embeddings. See the `AXON_LITE` section in [CONFIG.md](CONFIG.md).

## Docker deployment

For production or containerized deployment:

```bash
# Start infrastructure
just services-up

# Build and start app containers (workers + web)
just up
```

See [mcp/DEPLOY.md](mcp/DEPLOY.md) for detailed Docker deployment patterns.

## Troubleshooting

### "doctor" reports service unreachable

- Confirm infrastructure is running: `docker compose -f docker-compose.services.yaml ps`
- Check that `.env` credentials match `services.env`
- For local dev, URLs auto-normalize to localhost ports (e.g., `axon-postgres:5432` becomes `127.0.0.1:53432`)

### Build fails with spider_agent path error

`Cargo.toml` references a local `spider_agent` path for development. In CI or fresh environments, switch to the crates.io version. See the `spider_agent` gotcha in the root `CLAUDE.md`.

### TEI container exits immediately

- TEI requires a GPU with NVIDIA drivers for the default image
- CPU-only hosts: use `docker-compose.services.yaml` without the GPU overlay
- Check model download: `docker compose -f docker-compose.services.yaml logs axon-tei`

### Web UI shows "connection refused"

- Verify `axon serve` is running (it starts Next.js as a supervised child)
- Check port 49010 is not in use: `lsof -i :49010`
- For Docker: ensure `axon-web` container is healthy
