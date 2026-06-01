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
| npm | bundled with Node.js | Web panel package manager |

Optional but recommended:

| Tool | Purpose |
|------|---------|
| sccache | Compilation cache (auto-detected by Justfile) |
| mold | Fast linker (auto-detected by Justfile) |
| cargo-nextest | Faster parallel test runner |

See [stack/PRE-REQS.md](../architecture/stack/pre-reqs.md) for detailed installation instructions.

## 1. Clone the repository

```bash
git clone https://github.com/jmagar/axon.git ~/workspace/axon
cd ~/workspace/axon
```

## 2. Initialize local Axon state

```bash
./scripts/axon setup init
```

This creates or refreshes `~/.axon`, `~/.axon/config.toml`, `~/.axon/.env`, and
Compose assets. It is non-destructive and safe to re-run.

For local bearer-token operation, no manual env values are required. `setup init`
defaults to loopback MCP HTTP, writes `AXON_MCP_AUTH_MODE=bearer`, and generates
`AXON_MCP_HTTP_TOKEN`.

Optional features need their own credentials:

| Feature | Required outside Axon |
|---------|-----------------------|
| LLM features (`ask`, `evaluate`, `suggest`, LLM fallback extract, research synthesis) | Gemini CLI authenticated under `~/.gemini`. |
| Web search / research | `TAVILY_API_KEY`. |
| GitHub ingest with higher rate limits | `GITHUB_TOKEN`. |
| Reddit ingest | `REDDIT_CLIENT_ID` and `REDDIT_CLIENT_SECRET`. |
| OAuth MCP auth | `AXON_MCP_PUBLIC_URL`, `AXON_MCP_GOOGLE_CLIENT_ID`, `AXON_MCP_GOOGLE_CLIENT_SECRET`, and `AXON_MCP_AUTH_ADMIN_EMAIL`. |

## 3. Inspect configuration

```bash
./scripts/axon config path
./scripts/axon config list
```

The main generated files are:

- `~/.axon/.env` for URLs, secrets, auth, Docker interpolation, and runtime bootstrap values.
- `~/.axon/config.toml` for non-secret tuning.

Optional client/server mode for host CLI calls:

```bash
# Server listens on the MCP/action HTTP port.
AXON_SERVER_URL=http://127.0.0.1:8001

# For non-loopback published servers, use OAuth mode or set the same token on server and client.
AXON_MCP_HTTP_TOKEN=
```

See [CONFIG.md](configuration.md) for the full variable reference.

## 4. Start the stack

```bash
./scripts/axon compose up
```

This pulls images, starts the Docker stack from `~/.axon/compose` with
`docker compose up -d`, then follows compose logs. Press Ctrl-C when you are
done watching startup; the services keep running.

## 5. Run axon

```bash
./scripts/axon scrape https://example.com --wait true
```

Axon uses SQLite-backed jobs and in-process workers. Qdrant and TEI are the only external services needed.

## 6. Verify

```bash
# Check prerequisites, auth config, and service readiness
./scripts/axon preflight

# Check service connectivity diagnostics
./scripts/axon doctor

# Test a scrape
./scripts/axon scrape https://example.com --wait true

# Run TEI prewarm + crawl/ask proof
./scripts/axon smoke

# Test host CLI against a running axon serve process
AXON_SERVER_URL=http://127.0.0.1:8001 ./scripts/axon status --json

# Check the web panel
# Open http://localhost:8001
```

## Job runtime

Axon uses SQLite for job storage and runs workers in-process. Qdrant and TEI are required for embeddings.

## Docker deployment

For production or containerized deployment:

```bash
./scripts/axon setup       # init + compose up + preflight
./scripts/axon compose up    # start services
./scripts/axon compose down  # stop services
```

See [mcp/DEPLOY.md](../reference/mcp/deploy.md) for detailed Docker deployment patterns.

## Troubleshooting

### "doctor" reports service unreachable

- Confirm infrastructure is running: `docker compose --env-file ~/.axon/.env -f docker-compose.prod.yaml ps`
- Check that `~/.axon/.env` has the expected service URLs and token values
- For local dev, Qdrant URLs auto-normalize to localhost ports

### Build fails fetching crates

`spider`/`spider_agent` come from crates.io; `lab-auth` is vendored locally via
`[patch]` → `vendor/lab-auth` (no network needed for it). A fresh checkout builds
without any local-path setup. If a build fails fetching crates, run `cargo fetch`
and confirm crates.io connectivity.

### TEI container exits immediately

- TEI requires a GPU with NVIDIA drivers for the default image
- CPU-only hosts: override the TEI image/settings or point `TEI_URL` at an external CPU endpoint
- Check model download: `docker compose --env-file ~/.axon/.env -f docker-compose.prod.yaml logs axon-tei`

### Web panel shows "connection refused"

- Verify `axon serve` or the Compose `axon` service is running
- Check port 8001 is not in use: `lsof -i :8001`
