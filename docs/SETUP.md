# Setup Guide -- Axon

Step-by-step instructions to get Axon running locally or via Docker.

## Prerequisites

| Dependency | Version | Purpose |
|------------|---------|---------|
| Rust | 1.94+ | Compiler and toolchain (see `rust-toolchain.toml`) |
| Docker | 24+ | Infrastructure services |
| Docker Compose | v2+ | Service orchestration |
| just | latest | Task runner |
| Node.js | 24+ | Web panel asset build |
| pnpm | 10+ | Web panel package manager |

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
mkdir -m 700 -p ~/.axon
cp .env.example ~/.axon/.env
chmod 600 ~/.axon/.env
```

Edit `~/.axon/.env` and set required values:

```bash
# Host appdata root (optional; default is ~/.axon)
AXON_HOME=/home/you/.axon
AXON_DATA_DIR=/home/you/.axon

# Qdrant and TEI
QDRANT_URL=http://axon-qdrant:6333
TEI_URL=http://axon-tei:80
```

Optional client/server mode for host CLI calls:

```bash
# Server listens on the MCP/action HTTP port.
AXON_SERVER_URL=http://127.0.0.1:8001

# For non-loopback published servers, set the same token on server and client.
AXON_MCP_HTTP_TOKEN=
```

See [CONFIG.md](CONFIG.md) for the full variable reference.

## 4. Start infrastructure

```bash
just services-up
```

This starts Qdrant, TEI, and Chrome via `docker-compose.yaml`.

For GPU-accelerated embeddings (NVIDIA):

```bash
docker compose --env-file ~/.axon/.env -f docker-compose.yaml up -d axon-tei
```

## 5. Run axon

```bash
./scripts/axon scrape https://example.com --wait true
```

Axon uses SQLite-backed jobs and in-process workers. Qdrant and TEI are the only external services needed.

## 6. Verify

```bash
# Check service connectivity
./scripts/axon doctor

# Test a scrape
./scripts/axon scrape https://example.com --wait true

# Test host CLI against a running axon serve process
AXON_SERVER_URL=http://127.0.0.1:8001 ./scripts/axon status --json

# Check the web panel
# Open http://localhost:8001
```

## Job runtime

Axon uses SQLite for job storage and runs workers in-process. Qdrant and TEI are required for embeddings. `AXON_LITE` / `--lite` are accepted only for backwards compatibility.

## Docker deployment

For production or containerized deployment:

```bash
# Start infrastructure
just services-up

# Build and start the full stack
docker compose --env-file ~/.axon/.env -f docker-compose.yaml up -d
```

See [mcp/DEPLOY.md](mcp/DEPLOY.md) for detailed Docker deployment patterns.

## Troubleshooting

### "doctor" reports service unreachable

- Confirm infrastructure is running: `docker compose --env-file ~/.axon/.env -f docker-compose.yaml ps`
- Check that `~/.axon/.env` has the expected service URLs and token values
- For local dev, Qdrant URLs auto-normalize to localhost ports

### Build fails with spider_agent path error

`Cargo.toml` references a local `spider_agent` path for development. In CI or fresh environments, switch to the crates.io version. See the `spider_agent` gotcha in the root `CLAUDE.md`.

### TEI container exits immediately

- TEI requires a GPU with NVIDIA drivers for the default image
- CPU-only hosts: override the TEI image/settings or point `TEI_URL` at an external CPU endpoint
- Check model download: `docker compose --env-file ~/.axon/.env -f docker-compose.yaml logs axon-tei`

### Web panel shows "connection refused"

- Verify `axon serve` or the Compose `axon` service is running
- Check port 8001 is not in use: `lsof -i :8001`
