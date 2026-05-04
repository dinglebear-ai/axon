# Operations Runbook
Last Modified: 2026-03-09

Version: 1.1.0
Last Updated: 00:00:00 | 03/09/2026 EST

## Table of Contents

1. Scope
2. Day 0 Prerequisites
3. Day 1 Startup
4. Health Checks
5. Daily Operations
6. Incident Playbooks
7. Job Queue Operations
8. Data and Storage Hygiene
9. Logs and Diagnostics
10. Safe Shutdown
11. Source Map

## Scope

This is the operator runbook for local/homelab operation of Axon.

## Day 0 Prerequisites

### Recommended: automated bootstrap

Run `scripts/dev-setup.sh` once. It handles all of steps 1–4 automatically:

```bash
./scripts/dev-setup.sh
```

What the script does: installs system tools and Rust toolchain, installs `just` and `lefthook`, sets up Node.js and pnpm, creates `.env` from `.env.example`, prompts for `AXON_DATA_DIR` (default `~/.local/share/axon`), pre-creates all volume-mount directories, and starts Docker infrastructure (Qdrant, TEI, Chrome).

After the script completes, edit `.env` and fill in any remaining `CHANGE_ME` values:

- `TEI_URL` — external text embedding service
- `OPENAI_BASE_URL`, `OPENAI_API_KEY`, `OPENAI_MODEL` — LLM endpoint
- `TAVILY_API_KEY` — required for `search` and `research` commands

See `docs/DEPLOYMENT.md` for a full description of all env vars including ACP and test infrastructure variables.

### Manual bootstrap (alternative)

1. Copy env template:

```bash
cp .env.example .env
```

2. Populate required values in `.env`:

- `QDRANT_URL`
- `TEI_URL`
- `OPENAI_BASE_URL`, `OPENAI_API_KEY`, `OPENAI_MODEL`
- `AXON_WEB_API_TOKEN` and `NEXT_PUBLIC_AXON_API_TOKEN` (must be equal)

3. Ensure Docker and Compose are healthy.

4. Pre-create data directories:

```bash
mkdir -p ~/.local/share/axon/axon/{postgres,redis,rabbitmq,qdrant,output,artifacts}
```

## Day 1 Startup

> **Local dev mode**: Workers and the web frontend run as local processes, not in Docker. Only infrastructure runs via Compose.

### Infrastructure (Docker)

```bash
docker compose -f config/docker-compose.services.yaml up -d axon-qdrant axon-tei axon-chrome
docker compose -f config/docker-compose.services.yaml ps
```

### Workers (local processes)

**All at once (recommended):**

```bash
just dev
```

This starts the local supervisor, including the MCP HTTP server and in-process workers.

**Or each in its own terminal or tmux pane:**

```bash
cargo run --bin axon -- crawl worker
cargo run --bin axon -- embed worker
cargo run --bin axon -- extract worker
cargo run --bin axon -- ingest worker
```

### Web frontend (local process)

```bash
cd apps/web && pnpm dev    # → http://localhost:49010
```

### Full dev stack (all-in-one)

```bash
just dev
```

Starts infra, all workers, the axum serve process, MCP server, shell server, and Next.js dev server. `Ctrl+C` cleanly stops all spawned processes.

Current `just dev` behavior:
- Runs `cargo build --locked --bin axon` first (hard gate).
- If build fails, none of serve/MCP/workers/web processes are started.
- On success, all Rust processes run from the built `target/debug/axon` binary to reduce Cargo lock contention and startup noise.

### Verify service reachability

```bash
./scripts/axon doctor
```

### Tail worker output

Workers run in-process locally — output goes to the terminal directly. For infra containers:

```bash
docker compose -f config/docker-compose.services.yaml logs -f axon-qdrant axon-tei axon-chrome
```

## Health Checks

Expected healthy Docker containers (infra only):

- `axon-qdrant`
- `axon-tei`
- `axon-chrome`

Workers and web frontend are local processes — verify they are running in their respective terminals (or via `just dev`).

Quick checks:

```bash
docker compose -f config/docker-compose.services.yaml ps
./scripts/axon status
```

If any are unhealthy, inspect logs before restart.

## Daily Operations

### Run crawl/scrape

```bash
./scripts/axon scrape https://example.com --wait true
./scripts/axon crawl https://docs.rs/spider --wait false
```

### Ingest sources

The `github`, `reddit`, and `youtube` ingest commands were unified into `axon ingest` in v0.12.0. The target type is auto-detected:

```bash
./scripts/axon ingest owner/repo            # GitHub repo (slug or github.com URL)
./scripts/axon ingest https://github.com/owner/repo
./scripts/axon ingest r/subreddit           # Reddit subreddit
./scripts/axon ingest https://reddit.com/r/subreddit
./scripts/axon ingest https://www.youtube.com/watch?v=VIDEO_ID   # YouTube video
./scripts/axon ingest https://www.youtube.com/playlist?list=...  # YouTube playlist
./scripts/axon ingest https://www.youtube.com/@channel           # YouTube channel
```

### Track async progress

```bash
./scripts/axon status
./scripts/axon crawl list
./scripts/axon crawl status <job_id>
./scripts/axon ingest list
./scripts/axon ingest status <job_id>
```

### Query/RAG

```bash
./scripts/axon query "vector search"
./scripts/axon ask "what did we index for X?"
```

## Incident Playbooks

### Jobs stuck in `pending`

1. Confirm worker processes are running (check your terminal/tmux pane or `just dev` output).

2. Confirm AMQP and DB reachable:

```bash
./scripts/axon doctor
```

3. Restart all workers — kill each process and re-run:

```bash
# All at once:
just dev

# Or individually (each in its own terminal):
cargo run --bin axon -- crawl worker
cargo run --bin axon -- embed worker
cargo run --bin axon -- extract worker
cargo run --bin axon -- ingest worker
```

### Jobs stuck in `running`

1. Trigger manual recover:

```bash
./scripts/axon crawl recover
./scripts/axon extract recover
./scripts/axon embed recover
./scripts/axon ingest recover
```

2. If repeated, inspect worker logs and watchdog configuration.

### Pulse/API returning 503

Cause: missing LLM env vars in web runtime.

Required:

- `OPENAI_BASE_URL`
- `OPENAI_API_KEY`

Also needed for retrieval features:

- `TEI_URL`
- `QDRANT_URL`

### ACP sessions not starting

Check:

- `AXON_ACP_MAX_CONCURRENT_SESSIONS` — if at limit (default 8), sessions will be rejected until existing ones complete
- Adapter resolution order is: `AXON_ACP_<AGENT>_ADAPTER_*` → `AXON_ACP_ADAPTER_*` → built-in defaults (`claude-agent-acp`, `codex-acp`, `gemini --experimental-acp`)
- Selected agent is strict: failures surface as errors; Axon does not silently switch to another agent.
- `AXON_ACP_AUTO_APPROVE` — if `false`, tool calls require explicit approval; unexpected for automated flows

## Job Queue Operations

Runbook commands:

```bash
./scripts/axon crawl list
./scripts/axon crawl errors <job_id>
./scripts/axon crawl cancel <job_id>
./scripts/axon crawl cleanup
```

Same pattern applies to `extract`, `embed`, and `ingest`.

## Data and Storage Hygiene

Persistent data roots are under `${AXON_DATA_DIR}/axon/...`:

- SQLite jobs database
- Qdrant storage
- Worker output and logs
- MCP artifacts (`${AXON_DATA_DIR}/axon/artifacts` when `AXON_MCP_ARTIFACT_DIR` is set)

Cleanup caution:

- `clear` and aggressive cleanup commands are destructive.
- Use `list` and `status` first.

### Test infrastructure data

Test infrastructure uses the repo test recipes and stores data in ephemeral volumes. Destroy test data with:

```bash
just test-infra         # run ignored worker e2e infrastructure tests
```

## Logs and Diagnostics

Primary logs:

```bash
docker compose -f config/docker-compose.services.yaml logs -f axon-qdrant
```

Structured app logs are written under mounted logs volume for workers.

Chrome diagnostics:

- controlled by `AXON_CHROME_DIAGNOSTICS*` env vars
- output directory defaults to configured diagnostics path

## Safe Shutdown

Graceful shutdown:

```bash
docker compose -f config/docker-compose.services.yaml down
```

If draining is needed first:

1. Stop new submissions.
2. Monitor active jobs with `status`.
3. Cancel remaining long-running jobs if required.
4. Bring stack down.

To stop local worker processes started with `just dev`, use `Ctrl+C` in the terminal running it — the EXIT trap kills spawned processes cleanly.

## Source Map

- `config/docker-compose.services.yaml`
- `docker-compose.gpu.yaml`
- `scripts/dev-setup.sh`
- `Justfile`
- `README.md`
- `crates/jobs/*`
- `crates/services/acp/`
- `crates/web.rs`
- `apps/web/app/api/*`
