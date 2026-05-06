# Setup Guide -- Axon

Step-by-step instructions to get Axon running locally or via Docker.

## Prerequisites

| Dependency | Version | Purpose |
|------------|---------|---------|
| Rust | 1.94+ | Compiler and toolchain (see `rust-toolchain.toml`) |
| Docker | 24+ | Infrastructure services |
| Docker Compose | v2+ | Service orchestration |
| just | latest | Task runner |
| Node.js | 24+ | Web UI (Next.js) |
| pnpm | 10+ | Web UI package manager |

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
./scripts/dev-setup.sh
```

This installs the Rust toolchain, cargo tools, `just`, pnpm, and verifies all prerequisites. The script is idempotent — safe to re-run.

## 3. Configure environment

```bash
cp .env.example .env
chmod 600 .env
cp .env.example services.env
chmod 600 services.env
```

Edit `.env` and set required values:

```bash
# Host paths
AXON_DATA_DIR=/path/to/persistent/data

# Qdrant and TEI
QDRANT_URL=http://axon-qdrant:6333
TEI_URL=http://axon-tei:80
```

See [CONFIG.md](CONFIG.md) for the full variable reference.

## 4. Start infrastructure

```bash
just services-up
```

This starts Qdrant, TEI, and Chrome via `config/docker-compose.services.yaml`.

For GPU-accelerated embeddings (NVIDIA):

```bash
docker compose --env-file .env -f config/docker-compose.services.yaml up -d axon-tei
```

## 5. Run axon

```bash
./scripts/axon --lite scrape https://example.com --wait true
```

Axon runs in lite mode by default — workers run in-process, no external queue broker required. Qdrant and TEI are the only external services needed.

## 6. Verify

```bash
# Check service connectivity
./scripts/axon doctor

# Test a scrape
./scripts/axon scrape https://example.com --wait true

# Check the web UI
# Open http://localhost:49010
```

## Lite mode (default)

Axon uses SQLite for job storage and runs workers in-process by default. Qdrant and TEI are required for embeddings. See the `AXON_LITE` section in [CONFIG.md](CONFIG.md).

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

- Confirm infrastructure is running: `docker compose --env-file .env -f config/docker-compose.services.yaml ps`
- Check that `.env` credentials match `services.env`
- For local dev, Qdrant URLs auto-normalize to localhost ports

### Build fails with spider_agent path error

`Cargo.toml` references a local `spider_agent` path for development. In CI or fresh environments, switch to the crates.io version. See the `spider_agent` gotcha in the root `CLAUDE.md`.

### TEI container exits immediately

- TEI requires a GPU with NVIDIA drivers for the default image
- CPU-only hosts: override the TEI image/settings or point `TEI_URL` at an external CPU endpoint
- Check model download: `docker compose --env-file .env -f config/docker-compose.services.yaml logs axon-tei`

### Web UI shows "connection refused"

- Verify `axon serve` is running (it starts Next.js as a supervised child)
- Check port 49010 is not in use: `lsof -i :49010`
