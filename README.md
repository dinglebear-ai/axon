# Axon

Version: 5.1.1

Axon is a self-hosted RAG stack for crawling, scraping, ingesting, embedding, searching, and asking questions over indexed content. The production release is Docker Compose first: one Axon server container, Qdrant, Hugging Face TEI with `Qwen/Qwen3-Embedding-0.6B`, and Chrome for JS-heavy pages.

## Production Contract

Supported production runtime:

- Docker Compose only.
- Qdrant only for vector storage.
- Hugging Face TEI only for embeddings.
- `Qwen/Qwen3-Embedding-0.6B` as the production embedding model.
- Gemini CLI is the default LLM synthesis path; OpenAI-compatible endpoints
  such as llama.cpp (`AXON_LLM_BACKEND=openai-compat`) and the OpenAI Codex CLI
  (`AXON_LLM_BACKEND=codex-app-server`) are also supported. Save named provider
  profiles in `config.toml` and switch with `axon config provider use <name>`.
- Local NVIDIA RTX 4070 target with NVIDIA Container Toolkit.
- CLI and MCP run all actions in-process; deploy the `axon serve` container only when you need HTTP API access.
- One shared config home: `~/.axon/.env`, `~/.axon/config.toml`, `~/.axon/jobs.db`, `~/.axon/output`, `~/.axon/logs`, `~/.axon/artifacts`, `~/.axon/screenshots`, `~/.axon/qdrant`, and `~/.axon/tei`.

Not supported in the production path:

- systemd deployment of the Axon binary.
- Postgres, Redis, RabbitMQ, AMQP, or external worker services.
- Non-Gemini first-run LLM configuration. Configure
  `AXON_LLM_BACKEND=openai-compat` (llama.cpp / OpenAI-compatible
  `/v1/chat/completions`) or `AXON_LLM_BACKEND=codex-app-server` (OpenAI Codex
  CLI) manually after setup, or save and activate a provider profile.
- Neo4j or graph retrieval.
- Multiple competing `.env` or `config.toml` locations.

## Install

Prerequisites:

- Linux x86_64.
- Docker and Docker Compose.
- NVIDIA driver, `nvidia-smi`, and NVIDIA Container Toolkit.
- Gemini CLI installed and already authenticated, unless using a configured
  OpenAI-compatible endpoint for LLM synthesis.
- `curl`, `sha256sum`, and `install`.

One-line installer:

```bash
curl -fsSL https://raw.githubusercontent.com/jmagar/axon/main/install.sh | sh
```

The installer verifies the release checksum before installing the host `axon` binary to `~/.local/bin`, then delegates setup to:

```bash
axon setup
```

Useful installer controls:

```bash
AXON_INSTALL_DRY_RUN=1 ./install.sh
AXON_INSTALL_PREFIX=/opt/axon ./install.sh
AXON_VERSION=v4.10.0 ./install.sh
AXON_INSTALL_SKIP_SETUP=1 ./install.sh
```

Claude Code plugin install:

```bash
claude plugin install <path-to-this-repo>
```

The plugin uses the same Docker setup and `~/.axon` files. Its SessionStart hook is a thin adapter around `axon setup plugin-hook`, which is **probe-only and never deploys**: it checks `/readyz` and exits silently when the stack is up, or prints a one-line `run /axon-deploy` advisory when it is down. Provisioning is the `/axon-deploy` slash command (or `axon setup` / `axon compose up`). It does not create a systemd unit and does not symlink a plugin-cache binary into `~/.local/bin`.

## Setup Flow

`axon setup` is the convenience bootstrap path. It is idempotent and safe to rerun. It:

1. Creates or refreshes `~/.axon`.
2. Creates or preserves `~/.axon/config.toml`.
3. Creates or preserves `~/.axon/.env`, filling only missing runtime values and preserving secrets.
4. Writes Docker Compose assets under `~/.axon/compose`.
5. Checks Docker, Docker Compose, `nvidia-smi`, Gemini CLI auth, and OAuth config when requested.
6. Pulls and starts the Compose stack.
7. Waits for Qdrant, TEI, Chrome, and Axon server health.

Focused commands:

```bash
axon setup          # init + compose up + preflight
axon setup init     # create ~/.axon, config.toml, .env, and compose assets
axon preflight      # check prerequisites, auth config, and service readiness
axon compose up       # pull/start services, then follow logs until Ctrl-C
axon compose down     # stop services
axon compose restart  # restart services
axon compose rebuild  # rebuild the Axon image and start services
axon smoke          # TEI prewarm + crawl/ask proof
axon setup plugin-hook  # probe-only SessionStart path (never deploys; advises /axon-deploy when down)
axon setup targets  # list SSH aliases discovered from ~/.ssh/config (informational)
```

For local bearer-token operation, no manual env values are required. `setup init`
defaults to loopback MCP HTTP, writes `AXON_MCP_AUTH_MODE=bearer`, and generates
`AXON_MCP_HTTP_TOKEN`. Optional features need credentials: Gemini auth under
`~/.gemini` for default LLM features, `AXON_LLM_BACKEND=openai-compat` plus
`AXON_OPENAI_BASE_URL` and `AXON_OPENAI_MODEL` for OpenAI-compatible synthesis,
or `AXON_LLM_BACKEND=codex-app-server` plus `AXON_CODEX_CMD`/`AXON_CODEX_HOME`
for the OpenAI Codex CLI, `TAVILY_API_KEY` for search/research, `GITHUB_TOKEN` for higher-rate GitHub
ingest, and `REDDIT_CLIENT_ID` plus `REDDIT_CLIENT_SECRET` for Reddit ingest.
OAuth mode also requires
`AXON_MCP_PUBLIC_URL`, `AXON_MCP_GOOGLE_CLIENT_ID`,
`AXON_MCP_GOOGLE_CLIENT_SECRET`, and `AXON_MCP_AUTH_ADMIN_EMAIL`.

The warm-path setup goal is under 2 minutes once images and model weights are cached. Cold starts that pull images and model weights can take longer; target-hardware timing still needs to be measured against published release artifacts.

## Docker Stack

The production compose file starts:

| Service | Purpose | Host bind |
| --- | --- | --- |
| `axon` | HTTP server, web panel, MCP HTTP, action API, in-process workers | `127.0.0.1:8001` |
| `axon-qdrant` | vector storage | `127.0.0.1:53333`, `127.0.0.1:53334` |
| `axon-tei` | Qwen3 embeddings through TEI | `127.0.0.1:52000` |
| `axon-chrome` | browser rendering and CDP proxy | `127.0.0.1:6000`, `127.0.0.1:9222`, `127.0.0.1:9223` |

Start manually:

```bash
docker compose --env-file ~/.axon/.env -f ~/.axon/compose/docker-compose.yaml up -d
```

Check:

```bash
docker compose --env-file ~/.axon/.env -f ~/.axon/compose/docker-compose.yaml ps
axon preflight
axon doctor
```

Stop:

```bash
docker compose --env-file ~/.axon/.env -f ~/.axon/compose/docker-compose.yaml down
```

Development stack:

```bash
cargo build --bin axon
docker compose --env-file .env.example -f docker-compose.yaml build axon
docker compose --env-file ~/.axon/.env -f docker-compose.yaml up -d axon
```

The development stack uses the production infrastructure definitions but runs
`axon` from the bind-mounted local debug binary under `target/debug`, inside the
newer `axon:dev-runtime` image.

## Configuration

Axon has two config layers:

| File | Purpose |
| --- | --- |
| `~/.axon/.env` | URLs, secrets, auth, Docker interpolation, runtime bootstrap values |
| `~/.axon/config.toml` | non-secret tuning and behavior |

Precedence:

```text
CLI flags > environment variables > ~/.axon/config.toml > built-in defaults
```

Keep in `.env`:

- URLs: `QDRANT_URL`, `TEI_URL`, `AXON_CHROME_REMOTE_URL`.
- Secrets: `AXON_MCP_HTTP_TOKEN`, `TAVILY_API_KEY`, `GITHUB_TOKEN`, Reddit credentials, OAuth credentials, `HF_TOKEN`.
- Docker/runtime bootstrap: `AXON_HOME`, `AXON_DATA_DIR`, `AXON_IMAGE`, `AXON_MCP_HTTP_PUBLISH`, `TEI_HTTP_PORT`, GPU device values.
- LLM runtime pointers when needed: `AXON_HEADLESS_GEMINI_CMD`,
  `AXON_HEADLESS_GEMINI_HOME`, `AXON_LLM_BACKEND`, `AXON_OPENAI_BASE_URL`,
  `AXON_OPENAI_MODEL`, optional `AXON_OPENAI_API_KEY`, the Codex backend's
  `AXON_CODEX_CMD`/`AXON_CODEX_MODEL`/`AXON_CODEX_HOME`, and `AXON_PROVIDER` to
  select a saved provider profile.

Put in `config.toml`:

- collection/search/ask tuning.
- worker and job limits.
- TEI client tuning.
- Qdrant batch sizing.
- logging behavior that is not a process-launch concern.
- UI/output behavior.

`~/.axon/.env` under the config home is never loaded through a symlink.

## First Run

After setup:

```bash
axon doctor
axon crawl https://example.com --wait true
axon ask "What did we crawl?"
```

CLI and MCP commands always run in-process against Qdrant and TEI. `axon serve` exposes the same operations over HTTP (`/v1/*`, MCP-over-HTTP) for clients that want API access to a deployed instance.

## CLI Map

Core:

- `scrape <url>...`
- `crawl <url>...`
- `map <url>`
- `extract <url>...`
- `embed [input]`
- `query <text>`
- `retrieve <url>`
- `ask <question>`
- `summarize <url>...`
- `evaluate <question>`

Discovery and ingest:

- `search <query>`
- `research <query>`
- `suggest [focus]`
- `ingest <target>`
- `sessions`

Operations:

- `setup`
- `doctor`
- `debug`
- `serve`
- `mcp`
- `status`
- `sources`
- `domains`
- `stats`
- `watch`
- `dedupe`
- `migrate`
- `screenshot`
- `config`
- `completions`

Use command-specific help:

```bash
axon --help
axon setup --help
axon crawl --help
axon mcp --help
```

Graph flags are not part of the production CLI, MCP, or `/v1/ask` request contract.

## MCP And Auth

Axon exposes one MCP tool named `axon`; actions are routed by `action` and optional `subaction`.

Examples:

```json
{ "action": "doctor" }
{ "action": "scrape", "url": "https://example.com" }
{ "action": "ask", "query": "How does setup work?" }
{ "action": "summarize", "url": "https://example.com" }
{ "action": "crawl", "subaction": "status", "job_id": "<uuid>" }
```

HTTP auth modes:

- Static bearer token with `AXON_MCP_HTTP_TOKEN`.
- OAuth/lab-auth with `AXON_MCP_AUTH_MODE=oauth`.

`/mcp`, `/v1/actions`, and `/v1/ask` use the same auth policy. Tokenless HTTP is only for loopback development binds.

## Development

Build:

```bash
cargo build --bin axon
cargo build --release --bin axon
```

Test and lint:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets --features test-helpers
cargo clippy --workspace --all-targets --features test-helpers -- -D warnings
cargo test --workspace --features test-helpers
```

Common focused checks:

```bash
cargo test --test cli_help_contract -- --nocapture
cargo test parse_setup -- --nocapture
cargo test env_file_ -- --nocapture
python3 scripts/generate_mcp_schema_doc.py --check
docker compose --env-file .env.example -f docker-compose.prod.yaml config --quiet
```

Module layout policy:

- Do not add `mod.rs`.
- Rust module roots live in `foo.rs`.
- Submodules live in `foo/bar.rs`.

## Release Gates

Required before production release:

- CI fmt/check/clippy/test.
- MCP schema doc sync.
- CLI help contract tests.
- Docker Compose config validation.
- Docker image build and GHCR publish workflow.
- Compose smoke workflow.
- Self-hosted RTX 4070 smoke for Qwen3 TEI cold/warm setup timing.

`cargo machete` should be run when installed; it is not vendored in this repo.

## Troubleshooting

Fast checks:

```bash
axon preflight
axon doctor
docker compose --env-file ~/.axon/.env -f ~/.axon/compose/docker-compose.yaml ps
docker compose --env-file ~/.axon/.env -f ~/.axon/compose/docker-compose.yaml logs --tail=100 axon axon-tei axon-qdrant axon-chrome
```

Important paths:

- `~/.axon/.env`
- `~/.axon/config.toml`
- `~/.axon/jobs.db`
- `~/.axon/logs`
- `~/.axon/artifacts`
- `~/.axon/output`
- `~/.axon/tei`
- `~/.axon/qdrant`

Common failures:

- Docker missing: install Docker and Docker Compose, then rerun `axon setup`.
- GPU unavailable: verify `nvidia-smi` and NVIDIA Container Toolkit.
- Gemini unauthenticated: run Gemini CLI login outside Axon, then rerun setup,
  or configure `AXON_LLM_BACKEND=openai-compat` for an OpenAI-compatible
  endpoint, or `AXON_LLM_BACKEND=codex-app-server` for the OpenAI Codex CLI.
- TEI slow on first boot: model download/cache warmup is the cold path.
- Auth failures: make sure Claude/plugin config uses the same token as `AXON_MCP_HTTP_TOKEN` in `~/.axon/.env`.

## Related Files

- `install.sh` — verified one-line installer bootstrapper.
- `docker-compose.prod.yaml` — production Compose stack.
- `docker-compose.yaml` — local development stack.
- `.env.example` — production environment template.
- `config.example.toml` — non-secret tuning template.
- `.claude-plugin/plugin.json` — Claude plugin manifest.
- `scripts/plugin-setup.sh` — plugin hook delegating to shared setup.
- `docs/reference/mcp/tool-schema.md` — generated MCP wire contract.

## License

MIT
