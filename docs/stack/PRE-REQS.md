# Prerequisites -- Axon

Required tools and versions before developing or deploying.

## Required tools

| Tool | Version | Install | Purpose |
|------|---------|---------|---------|
| Rust | 1.94+ | [rustup.rs](https://rustup.rs/) | Compiler (pinned via `rust-toolchain.toml`) |
| Docker | 24+ | [docs.docker.com](https://docs.docker.com/get-docker/) | Infrastructure services |
| Docker Compose | v2+ | Bundled with Docker | Service orchestration |
| just | latest | `cargo install just` | Task runner |
| Node.js | 22+ | [nodejs.org](https://nodejs.org/) | Web UI runtime |
| pnpm | 9+ | `corepack enable` | Web UI package manager |
| jq | 1.6+ | System package manager | JSON parsing in scripts |
| curl | any | System package manager | HTTP testing |
| Python | 3.10+ | System package manager | Scripts and analysis tools |

### Verify

```bash
rustc --version          # rustc 1.94.x
cargo --version          # cargo 1.94.x
docker --version         # Docker 24+
docker compose version   # Docker Compose v2+
just --version           # just x.y.z
node --version           # v22.x.x
pnpm --version           # 9.x.x
jq --version             # jq-1.6+
python3 --version        # Python 3.10+
```

## Automated setup

The setup script installs everything:

```bash
just setup
# or directly:
./scripts/dev-setup.sh
```

This installs: Rust toolchain components, cargo tools, pnpm, and verifies all prerequisites. If `just` is not yet installed, `dev-setup.sh` installs it.

## Optional tools (recommended)

| Tool | Install | Purpose |
|------|---------|---------|
| sccache | `cargo install sccache` | Compilation cache (auto-detected by Justfile) |
| mold | System package manager | Fast linker (auto-detected by Justfile) |
| cargo-nextest | `cargo install cargo-nextest` | Faster parallel test runner |
| cargo-deny | `cargo install cargo-deny` | Dependency auditing |
| cargo-llvm-cov | `cargo install cargo-llvm-cov` | Code coverage |
| lefthook | `cargo install lefthook` | Git hooks |
| gh | [cli.github.com](https://cli.github.com/) | GitHub CLI for PRs and issues |

## Infrastructure services

Infrastructure runs via Docker Compose. No manual installation needed:

| Service | Docker image | Host port | Purpose |
|---------|-------------|-----------|---------|
| Qdrant | qdrant/qdrant:v1.13.1 | 53333 | Vector store |
| TEI | ghcr.io/huggingface/text-embeddings-inference | 52000 | Embedding generation |
| Chrome | Custom (`config/chrome/`) | 6000 | Headless browser |

Start all infrastructure:

```bash
just services-up
```

### GPU requirements (optional)

TEI benefits from GPU acceleration for embedding generation:
- NVIDIA GPU with CUDA drivers
- `docker compose --env-file ~/.axon/.env -f docker-compose.yaml up -d`
- CPU-only: use an external `TEI_URL` or override the TEI image/settings to remove
  the NVIDIA device reservation before starting the default services compose file.

### System resources

Recommended minimums for local development:

| Resource | Minimum | Recommended |
|----------|---------|-------------|
| RAM | 8 GB | 16+ GB |
| Disk | 20 GB | 50+ GB (for Qdrant data and TEI models) |
| CPU | 4 cores | 8+ cores (concurrency scales with cores) |

## Quick start

```bash
git clone https://github.com/jmagar/axon.git ~/workspace/axon_rust
cd ~/workspace/axon_rust
just setup               # Install all tools
mkdir -m 700 -p ~/.axon
cp .env.example ~/.axon/.env && chmod 600 ~/.axon/.env
# Edit ~/.axon/.env with credentials
just services-up          # Start infrastructure
just dev                  # Build + run full stack
```

## See also

- [../SETUP.md](../SETUP.md) -- detailed setup guide
- [TECH.md](TECH.md) -- technology stack details
- [../repo/RECIPES.md](../repo/RECIPES.md) -- Justfile recipes
