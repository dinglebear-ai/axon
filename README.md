# ⚡ **Axon**
Last Modified: 2026-03-20

Self-hosted web crawling and RAG pipeline powered by Spider.rs. Single binary (`axon`) backed by a local Docker stack.

## CI Status

[![CI](https://github.com/jmagar/axon_rust/actions/workflows/ci.yml/badge.svg)](https://github.com/jmagar/axon_rust/actions/workflows/ci.yml)

- `mcp-smoke`: runs as a dedicated job in the `CI` workflow and executes `./scripts/test-mcp-tools-mcporter.sh` in both full (`AXON_LITE=0`) and lite (`AXON_LITE=1`) modes.
- `test-infra`: manual lane (`workflow_dispatch`) for ignored infra-backed tests; use Actions -> CI -> Run workflow -> `run_infra_tests=true`.

## Overview

Axon is a single CLI for crawl/scrape/extract plus local vector retrieval and Q&A. It runs on a local Docker stack (Postgres, Redis, RabbitMQ, Qdrant) and external model endpoints (TEI and OpenAI-compatible API).

## Features

- Commands: `scrape`, `crawl`, `watch`, `refresh`, `map`, `search`, `research`, `extract`, `embed`, `query`, `retrieve`, `ask`, `evaluate`, `suggest`, `graph`, `ingest`, `sessions`, `screenshot`, `completions`, `sources`, `domains`, `stats`, `status`, `doctor`, `dedupe`, `debug`, `mcp`, `serve`
- Async queue-backed jobs for `crawl`/`extract`/`embed`/`refresh`/ingest
- **Surgical Incremental Crawling**: SHA-256 content hashing, Reflink/Hardlink storage reuse, and smart embedding skips for unchanged pages.
- TEI embeddings + Qdrant vector storage
- ACP-backed extraction and answer generation (with `OPENAI_MODEL` compatibility override)
- Chrome CDP rendering for dynamic sites
- Automation-friendly JSON mode via `--json`
- `axon serve` hosts the core Axon WebSocket/download/output bridge used by `apps/web`; only the old static page UX is deprecated — see `docs/SERVE.md`
- Next.js web app (`apps/web`) with keyboard-first omnibox (`/` focus, `@mode` switching, `@file` context mentions)
- MCP server via `axon mcp` exposing a single `axon` tool (`action`/`subaction`) for crawler/RAG integration

## Architecture

- Canonical architecture and end-to-end data flow: `docs/ARCHITECTURE.md`
- Graph-specific reference: `docs/GRAPH.md`
- Runtime entrypoint: `main.rs` -> `lib.rs` (`run`/`run_once`)
- Core subsystems:
  - `crates/cli`: command handlers and routing
  - `crates/core`: config parsing, HTTP/content pipeline, logging
  - `crates/crawl`: crawl engine and sitemap backfill
  - `crates/jobs`: queue-backed workers and job lifecycle
  - `crates/vector`: TEI embedding + Qdrant RAG operations
  - `crates/web.rs` + `crates/web/*`: axum `/ws` runtime, shell bridge, and artifact download/output routes used by `apps/web`
  - `apps/web`: active Next.js UI (omnibox, pulse workspace, API routes)

For infra topology (Docker services, ports, persistence), see the Infrastructure and Environment sections below.

## Module READMEs

- [crates index](crates/README.md)
- [crates/cli](crates/cli/README.md)
- [crates/core](crates/core/README.md)
- [crates/crawl](crates/crawl/README.md)
- [crates/ingest](crates/ingest/README.md)
- [crates/jobs](crates/jobs/README.md)
- [crates/mcp](crates/mcp/README.md)
- [crates/vector](crates/vector/README.md)
- [crates/web](crates/web/README.md)
- [apps/web](apps/web/README.md)
- [docker](docker/README.md)
- [docs index](docs/README.md)
- [testing guide](docs/TESTING.md)

## MCP Server

Axon includes an MCP server command:

```bash
cargo build --release --bin axon
./target/release/axon mcp
```

Documentation:
- Design/runtime guide: `docs/MCP.md`
- Wire contract/schema source of truth: `docs/MCP-TOOL-SCHEMA.md`

Transport modes:
- `axon mcp` defaults to HTTP transport on `/mcp`.
- `axon mcp --transport stdio` starts stdio transport only.
- `axon mcp --transport both` starts stdio and HTTP concurrently.
- `AXON_MCP_TRANSPORT=stdio|http|both` provides the equivalent env override.

ACP MCP server store (used by Pulse ACP and Web UI `/api/mcp`):
- `${AXON_DATA_DIR}/axon/mcp.json` when `AXON_DATA_DIR` is set
- `~/.config/axon/mcp.json` fallback
- ACP hot-reloads MCP config changes from this file and reapplies them on the next turn.

MCP defaults are context-safe:
- Artifact-first responses (`response_mode=path`) written to `.cache/axon-mcp/` inside the running process/container (override with `AXON_MCP_ARTIFACT_DIR`; in Docker this is typically bind-mounted to `${AXON_DATA_DIR}/axon/artifacts`)
- Inline responses are optional (`response_mode=inline|both`) and capped
- Resource: `axon://schema/mcp-tool`
- Local mcporter config: `config/mcporter.json` registers the server as `axon`
- Primary MCP test path: `./scripts/test-mcp-tools-mcporter.sh` runs schema/help parity plus smoke coverage in both full and lite modes

## Quick Start

```bash
# 1) from repo root
cp .env.example .env
# edit .env — set AXON_DATA_DIR, POSTGRES_PASSWORD, REDIS_PASSWORD, RABBITMQ_PASS, TEI_URL, OPENAI_*

# 2) start infrastructure only (workers + web run locally, not in Docker)
docker compose up -d axon-postgres axon-redis axon-rabbitmq axon-qdrant axon-chrome
docker compose ps
```

```bash
# 3) run CLI (wrapper loads .env automatically)
./scripts/axon doctor
./scripts/axon scrape https://example.com --wait true
./scripts/axon crawl https://docs.rs/spider --wait false
./scripts/axon status
```

```bash
# Optional local alias
alias axon='./scripts/axon'

axon doctor
axon query "spider crawler"
axon ask "what does spider.rs support?"   # requires AXON_ACP_ADAPTER_CMD
```

## Environment

Copy `.env.example` to `.env`. At minimum set the `[REQUIRED]` vars:

### Required

| Variable | Description |
|----------|-------------|
| `POSTGRES_USER` / `POSTGRES_PASSWORD` / `POSTGRES_DB` | Docker Compose Postgres credentials |
| `REDIS_PASSWORD` | Docker Compose Redis password |
| `RABBITMQ_USER` / `RABBITMQ_PASS` | Docker Compose RabbitMQ credentials |
| `AXON_DATA_DIR` | Host path root for persistent compose data volumes (e.g. `/home/you/appdata`) |
| `AXON_PG_URL` | PostgreSQL DSN for CLI/workers |
| `AXON_REDIS_URL` | Redis DSN for health checks and cancel flags |
| `AXON_AMQP_URL` | AMQP DSN for queue-backed jobs |
| `QDRANT_URL` | Qdrant base URL |
| `TEI_URL` | TEI embeddings base URL (external — not in compose) |
| `AXON_ACP_ADAPTER_CMD` | ACP adapter command required for ask/evaluate/suggest/debug/research (and extract fallback) |
| `OPENAI_MODEL` | Model override used by ACP-backed completions (kept for compatibility) |
| `OPENAI_BASE_URL` / `OPENAI_API_KEY` | Legacy OpenAI-compatible settings kept for compatibility paths |

Migration note:
- `ask`, `evaluate`, `suggest`, extract fallback, `debug`, and research synthesis now execute via ACP (`AXON_ACP_ADAPTER_CMD`).
- `OPENAI_MODEL` remains the model override knob for those ACP calls.
- `OPENAI_BASE_URL` and `OPENAI_API_KEY` are retained for compatibility and transitional paths.

### Optional Queue and Collection

| Variable | Default | Description |
|----------|---------|-------------|
| `AXON_CRAWL_QUEUE` | `axon.crawl.jobs` | Crawl job queue name |
| `AXON_REFRESH_QUEUE` | `axon.refresh.jobs` | Refresh job queue name |
| `AXON_EXTRACT_QUEUE` | `axon.extract.jobs` | Extract job queue name |
| `AXON_EMBED_QUEUE` | `axon.embed.jobs` | Embed job queue name |
| `AXON_INGEST_QUEUE` | `axon.ingest.jobs` | Ingest job queue name (github/reddit/youtube) |
| `AXON_INGEST_LANES` | `2` | Number of parallel ingest worker lanes |
| `AXON_COLLECTION` | `cortex` | Qdrant collection name |

### Optional Graph / Neo4j

| Variable | Default | Description |
|----------|---------|-------------|
| `AXON_NEO4J_URL` | `` | Neo4j HTTP base URL. Empty disables graph features. Example: `http://127.0.0.1:7474` |
| `AXON_NEO4J_USER` | `neo4j` | Neo4j username |
| `AXON_NEO4J_PASSWORD` | `` | Neo4j password |
| `AXON_GRAPH_QUEUE` | `axon.graph.jobs` | AMQP queue for graph jobs |
| `AXON_GRAPH_CONCURRENCY` | `4` | Worker parallelism for graph extraction |
| `AXON_GRAPH_LLM_URL` | `http://localhost:11434` | LLM endpoint for ambiguous graph extraction |
| `AXON_GRAPH_LLM_MODEL` | `qwen3.5:4b` | LLM model used for graph extraction |
| `AXON_GRAPH_SIMILARITY_THRESHOLD` | `0.75` | Minimum score for graph similarity edges |
| `AXON_GRAPH_SIMILARITY_LIMIT` | `20` | Maximum similar URLs considered per source URL |
| `AXON_GRAPH_CONTEXT_MAX_CHARS` | `2000` | Max chars injected into `ask --graph` context |
| `AXON_GRAPH_TAXONOMY_PATH` | `` | Optional taxonomy file override path |

### Optional Ingest Credentials

| Variable | Description |
|----------|-------------|
| `GITHUB_TOKEN` | Personal access token for private repos and higher GitHub API rate limits (optional — public repos work without it) |
| `REDDIT_CLIENT_ID` | OAuth2 client ID from `https://www.reddit.com/prefs/apps` (required for `reddit` command) |
| `REDDIT_CLIENT_SECRET` | OAuth2 client secret (required for `reddit` command) |
| `TAVILY_API_KEY` | Tavily AI Search API key (required for `search` and `research` commands) |

### Optional Browser / Chrome

| Variable | Default | Description |
|----------|---------|-------------|
| `AXON_CHROME_REMOTE_URL` | — | Remote Chrome DevTools endpoint |
| `CHROME_URL` | — | spider-rs native CDP env var (alternative to `AXON_CHROME_REMOTE_URL`) |
| `AXON_CHROME_PROXY` | — | Proxy URL for Chrome requests |
| `AXON_CHROME_USER_AGENT` | — | User-Agent override for Chrome |
| `AXON_CHROME_DIAGNOSTICS` | `false` | Enable browser diagnostics artifact collection |
| `AXON_CHROME_DIAGNOSTICS_SCREENSHOT` | — | Save diagnostic screenshots to disk when set |
| `AXON_CHROME_DIAGNOSTICS_EVENTS` | — | Log raw CDP events when set |
| `AXON_CHROME_DIAGNOSTICS_DIR` | — | Directory for diagnostics output (default: temp dir) |

### Optional Worker / Watchdog

| Variable | Default | Description |
|----------|---------|-------------|
| `AXON_EMBED_DOC_TIMEOUT_SECS` | `300` | Per-document embed timeout in seconds before failing the embed job |
| `AXON_EMBED_STRICT_PREDELETE` | `true` | Require successful per-document Qdrant pre-delete before upsert (`false` = warn and continue) |
| `AXON_JOB_STALE_TIMEOUT_SECS` | `300` | Seconds before a running job is considered stale |
| `AXON_JOB_STALE_CONFIRM_SECS` | `60` | Seconds to confirm stale status before reclaiming |
| `AXON_NO_WIPE` | — | Prevent destructive cache wipes when set |

### Optional Output / Misc

| Variable | Default | Description |
|----------|---------|-------------|
| `AXON_NO_COLOR` | — | Disable ANSI color output when set |
| `AXON_DOMAINS_DETAILED` | — | Enable detailed `domains` command output |
| `AXON_EXTRACT_EST_COST_PER_1K_TOKENS` | — | Override extract cost estimate (USD/1K tokens) |
| `AXON_MCP_ARTIFACT_DIR` | `.cache/axon-mcp` | MCP artifact output root in the runtime filesystem for `response_mode=path` (container path in Docker) |
| `AXON_LOG_FILE` | `logs/axon.log` | Structured log file path (always on) |
| `AXON_LOG_MAX_BYTES` | `10485760` | Max bytes per log file before rotation (10MB) |
| `AXON_LOG_MAX_FILES` | `3` | Total log files to keep (`axon.log`, `.1`, `.2`) |

### Optional Cache/Build Guardrails

| Variable | Default | Description |
|----------|---------|-------------|
| `AXON_TARGET_MAX_GB` | `30` | `scripts/cache-guard.sh` threshold for local `target/` size before pruning |
| `AXON_BUILDKIT_MAX_GB` | `120` | `scripts/cache-guard.sh` threshold for Docker BuildKit cache before `docker builder prune -af` |
| `AXON_AUTO_CACHE_GUARD` | `true` | Run cache guard automatically in `scripts/rebuild-fresh.sh` |
| `AXON_ENFORCE_DOCKER_CONTEXT_PROBE` | `true` | Run Docker context-size probe automatically in `scripts/rebuild-fresh.sh` |
| `AXON_WORKERS_CONTEXT_MAX_MB` | `500` | Max allowed workers build context during probe |
| `AXON_WEB_CONTEXT_MAX_MB` | `100` | Max allowed web build context during probe |
| `AXON_CONTEXT_PROBE_TIMEOUT_SECS` | `30` | Timeout per service probe build |

### Web App Security (`apps/web`)

Auth is enforced externally by an OAuth gateway + SWAG reverse proxy. Token required in `Authorization: Bearer <token>` or `x-api-key` header for `/api/*` routes.

| Variable | Description |
|----------|-------------|
| `AXON_WEB_API_TOKEN` | **Required.** API bearer/x-api-key token enforced by `apps/web/proxy.ts` |
| `NEXT_PUBLIC_AXON_API_TOKEN` | **Required when `AXON_WEB_API_TOKEN` is set.** Must match — used by `apiFetch()` to attach `x-api-key` to all client-side `/api/*` calls |
| `AXON_WEB_ALLOWED_ORIGINS` | Comma-separated origin allowlist for `app/api/*` and shell websocket fallback |
| `AXON_WEB_ALLOW_INSECURE_DEV` | Localhost-only development bypass (`true` only for local dev — skips token check) |
| `AXON_SHELL_WS_TOKEN` | Optional token specifically for `/ws/shell` auth (falls back to `AXON_WEB_API_TOKEN`) |
| `AXON_SHELL_ALLOWED_ORIGINS` | Optional shell websocket origin allowlist |
| `NEXT_PUBLIC_SHELL_WS_TOKEN` | Optional client token for shell websocket auth |
| `AXON_ALLOWED_CLAUDE_BETAS` | Comma-separated allowlist for Pulse chat `--betas` values |

### ask RAG Tuning

The `ask` command retrieves chunks from Qdrant, reranks them, and builds a context window before calling the LLM. The following env vars tune that pipeline:

| Variable | Default | Clamp | Description |
|----------|---------|-------|-------------|
| `AXON_ASK_MIN_RELEVANCE_SCORE` | `0.45` | `-1.0`–`2.0` | Minimum Qdrant similarity score for a chunk to enter the context. Raise to tighten relevance; lower if you get "no candidates" errors on sparse collections. |
| `AXON_ASK_CANDIDATE_LIMIT` | `64` | `8`–`200` | Chunks retrieved from Qdrant before reranking. Higher values improve recall at the cost of reranking time. |
| `AXON_ASK_CHUNK_LIMIT` | `10` | `3`–`40` | Maximum chunks included in the LLM prompt after reranking. |
| `AXON_ASK_FULL_DOCS` | `4` | `1`–`20` | Number of top-scoring documents for which a full-doc backfill is attempted (fetches more chunks from the same doc). |
| `AXON_ASK_BACKFILL_CHUNKS` | `3` | `0`–`20` | Extra chunks added per full-doc backfill pass. Set to `0` to disable backfill. |
| `AXON_ASK_DOC_FETCH_CONCURRENCY` | `4` | `1`–`16` | Concurrent Qdrant fetches during full-doc backfill. |
| `AXON_ASK_DOC_CHUNK_LIMIT` | `192` | `8`–`2000` | Maximum chunks fetched per document during backfill. |
| `AXON_ASK_MAX_CONTEXT_CHARS` | `120000` | `20000`–`400000` | Total characters of context passed to the LLM. Raise for large-context models; lower to reduce token cost. |
| `AXON_ASK_AUTHORITATIVE_DOMAINS` | `` | — | Optional comma-separated domain list to boost during reranking (exact host or suffix match). Example: `docs.claude.com,developers.openai.com`. |
| `AXON_ASK_AUTHORITATIVE_BOOST` | `0.0` | `0.0`–`0.5` | Extra rerank score added when a candidate matches `AXON_ASK_AUTHORITATIVE_DOMAINS`. |
| `AXON_ASK_AUTHORITATIVE_ALLOWLIST` | `` | — | Optional comma-separated strict domain allowlist. When set, ask retrieval excludes candidates outside these domains. |
| `AXON_ASK_MIN_CITATIONS_NONTRIVIAL` | `2` | `1`–`5` | Minimum unique citations required for non-trivial answers; if not met, `ask` returns structured insufficient-evidence output. |

Notes:
- Container runtime uses service DNS names (`axon-postgres`, `axon-redis`, etc.).
- Local runtime rewrites those to mapped localhost ports automatically.
- Both `./scripts/axon` and `cargo run --bin axon -- ...` load `.env` (the binary calls `dotenvy` at startup).
- The wrapper additionally pre-exports `.env` in shell before invoking Cargo.
- `ask` now enforces citation-quality gates:
  - Non-trivial responses require multiple unique citations.
  - Procedural queries require at least one official-docs citation.
  - Config/schema queries require at least one exact-page citation.
  - If gates fail, output is forced to structured insufficient-evidence format.

## Worker Model (Local Processes)

Workers run as local processes, not in Docker. Start each in its own terminal or tmux pane:

```bash
cargo run --bin axon -- crawl worker
cargo run --bin axon -- embed worker
cargo run --bin axon -- extract worker
cargo run --bin axon -- ingest worker
```

Worker behavior notes:
- Workers run startup stale-job reclaim sweeps plus periodic stale sweeps.
- Stale timeout and confirmation window are tunable via `AXON_JOB_STALE_TIMEOUT_SECS` / `AXON_JOB_STALE_CONFIRM_SECS`.
- Use `./scripts/axon` wrapper to auto-source `.env` before launching workers.

## Surgical Incremental Crawling

Axon implements a multi-layered incremental crawl mechanism to minimize network traffic, disk I/O, and expensive AI embedding operations.

1.  **Network Level**: Enabled via `--cache true`. Uses standard HTTP caching headers (`ETag`, `Last-Modified`) to perform conditional GET requests.
2.  **Content Level**: Every crawled page is SHA-256 hashed. If the Markdown content hasn't changed since the last hunt, Axon identifies it as "unchanged."
3.  **Storage Level**: Uses **Reflinks** (Copy-on-Write) on supported filesystems (XFS, Btrfs, APFS) or hardlinks to reuse previous Markdown files on disk without taking extra space.
4.  **Intelligence Level**: The embedder reads the "changed" status from the manifest and automatically skips re-embedding unchanged pages in Qdrant, drastically reducing TEI/LLM load.
5.  **Job Level**: A 24-hour TTL protects recently conquered domains. If a crawl was completed within the last 24 hours, the worker skips the traversal entirely and returns the cached result.

## Commands

| Command | Purpose | Async? |
|---------|---------|--------|
| `scrape <url>...` | Scrape one or more URLs to markdown | No |
| `crawl <url>...` | Full site crawl for one or more start URLs | Yes (default) |
| `refresh <url>...` | Revalidate known URLs and update stored content/embeddings | Yes (default) |
| `watch <subcommand>` | Define and run top-level recurring watches (currently refresh task dispatch) | No |
| `map <url>` | Discover all URLs without scraping | No |
| `extract <urls...>` | LLM-powered structured data extraction | Yes (default) |
| `search <query>` | Web search via Tavily, auto-queues crawl jobs for results | No |
| `research <query>` | Web research via Tavily AI search with LLM synthesis | No |
| `embed [input]` | Embed file/dir/URL into Qdrant | Yes (default) |
| `export` | Export backup manifest (seeds + settings + integrity; optional history) to JSON | No |
| `query <text>` | Semantic vector search | No |
| `retrieve <url>` | Fetch stored document chunks from Qdrant | No |
| `ask <question>` | RAG: search + LLM answer | No |
| `evaluate <question>` | RAG vs baseline + LLM judge (accuracy · relevance · completeness · verdict) | No |
| `suggest [focus]` | Suggest complementary docs URLs not already indexed | No |
| `ingest <target>` | Ingest GitHub, Reddit, or YouTube — source type auto-detected from target (slug, URL, @handle, r/name) | Yes (default) |
| `sessions [--claude] [--codex] [--gemini] [--project <name>]` | Ingest AI session exports (Claude/Codex/Gemini) into Qdrant | No |
| `screenshot <url>...` | Capture page screenshot(s) via Chrome | No |
| `completions <bash|zsh|fish>` | Generate shell completion scripts | No |
| `graph <sub>` | Knowledge graph operations: `build`, `status`, `explore`, `stats`, `worker`. Requires `AXON_NEO4J_URL`. | Depends |
| `migrate --from <src> --to <dst>` | Migrate an unnamed-vector collection to named-mode (enables hybrid RRF search). No re-embedding needed. | No |
| `sources` | List all indexed URLs + chunk counts | No |
| `domains` | List indexed domains + stats | No |
| `stats` | Qdrant collection stats | No |
| `status` | Show async job queue status | No |
| `doctor` | Diagnose service connectivity | No |
| `debug` | Run doctor + LLM-assisted troubleshooting | No |
| `dedupe` | Remove duplicate vectors from Qdrant collection | No |
| `mcp` | Start MCP server runtime (`http`, `stdio`, or `both`) | No |
| `serve` | Start web UI server (axum + WebSocket + Docker stats) | No |

### Shell Completions

```bash
axon completions bash > ~/.local/share/bash-completion/completions/axon
axon completions zsh > ~/.zfunc/_axon
axon completions fish > ~/.config/fish/completions/axon.fish
```

See `docs/SHELL-COMPLETIONS.md` and `docs/commands/completions.md` for installation details.

### Freshness Strategy (Tiered Refresh + Discovery Crawl)

Use `refresh` for ongoing freshness of known URLs, and reserve `crawl` for discovery of newly added URLs.

Recommended tiered refresh cadence:

| Tier | Interval (seconds) | Typical content |
|------|--------------------|-----------------|
| `high` | `1800` | Fast-changing docs, changelogs, release pages |
| `medium` | `21600` | Standard documentation and guides |
| `low` | `86400` | Stable reference pages and archives |

Recommended production pattern:
- Add refresh schedules by content volatility (`high`/`medium`/`low`) and run the scheduler continuously.
- Run `refresh schedule worker` as the scheduler loop that enqueues due refresh jobs.
- Run `refresh worker` as the refresh job consumer that executes queued refresh jobs.
- Run `crawl` on an infrequent cadence (daily or weekly) for discovery only, then let refresh maintain known URLs.

### Job Subcommands (for crawl / extract / embed)

```bash
axon crawl status <job_id>
axon crawl cancel <job_id>
axon crawl errors <job_id>
axon crawl list
axon crawl cleanup
axon crawl clear
axon crawl recover    # reclaim stale/interrupted jobs
axon crawl worker     # run a worker inline
```

### Job Subcommands (for refresh)

```bash
axon refresh status <job_id>
axon refresh cancel <job_id>
axon refresh errors <job_id>
axon refresh list
axon refresh cleanup
axon refresh clear
axon refresh recover
axon refresh worker

# schedule management
axon refresh schedule add <name> [seed_url] [--every-seconds <n>|--tier <high|medium|low>] [--urls <csv>]
axon refresh schedule list
axon refresh schedule enable <name>
axon refresh schedule disable <name>
axon refresh schedule delete <name>
axon refresh schedule run-due [--batch <n>]
```

`refresh schedule` is a compatibility interface backed by top-level watch definitions (`task_type=refresh`). Existing automation can keep using `axon refresh schedule ...` while new workflows can use `axon watch ...` directly.

### Top-level Watch Subcommands

```bash
axon watch create <name> --task-type <type> --every-seconds <n> [--task-payload <json>]
axon watch list
axon watch run-now <watch_id>
axon watch history <watch_id> [--limit <n>]
```

Current dispatch support in the worker loop is `task_type=refresh` (payload shape: `{"urls":["https://..."]}`).

### Job Subcommands (for ingest)

```bash
axon ingest status <job_id>
axon ingest cancel <job_id>
axon ingest errors <job_id>
axon ingest list
axon ingest cleanup
axon ingest clear
axon ingest recover    # reclaim stale/interrupted jobs
axon ingest worker     # run an ingest worker inline
```

### Global Flags Reference

All flags are global (usable with any subcommand).

#### Core Behavior

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--wait <bool>` | bool | `false` | Run synchronously and block until completion. Without this, async commands enqueue and return immediately. |
| `--yes` | flag | `false` | Skip confirmation prompts (non-interactive mode). |
| `--json` | flag | `false` | Machine-readable JSON output on stdout. |
| `--graph` | flag | `false` | Enable graph-enhanced retrieval for `ask` (requires Neo4j via `AXON_NEO4J_URL`). |
| `--reclaimed` | flag | `false` | `status` mode: show only watchdog-reclaimed jobs. Default `status` hides reclaimed jobs. |
| `--active` | flag | `false` | List/status mode: show only active jobs (running/pending). |
| `--recent` | flag | `false` | List/status mode: show active + completed jobs (hide failed/canceled). |
| `--lite` | flag | `false` | Run without Postgres/Redis/RabbitMQ. Uses SQLite + in-process workers. |
| `--sqlite-path <path>` | path | — | Path to the SQLite jobs database (lite mode only). |

#### Crawl & Scrape

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--max-pages <n>` | u32 | `0` | Page cap for crawl (0 = uncapped). |
| `--max-depth <n>` | usize | `5` | Maximum crawl depth from start URL. |
| `--render-mode <mode>` | enum | `auto-switch` | `http`, `chrome`, or `auto-switch`. Auto-switch tries HTTP first, falls back to Chrome if >60% thin pages. |
| `--format <fmt>` | enum | `markdown` | Output format: `markdown`, `html`, `rawHtml`, `json`. |
| `--include-subdomains <bool>` | bool | `false` | Crawl subdomains of the start URL's parent domain. Disabled by default because spider root-domain scoping can widen traversal (for example, `code.claude.com` -> `*.claude.com`) and cause crawl drift. Enable explicitly with `--include-subdomains true` when intended. |
| `--respect-robots <bool>` | bool | `false` | Respect `robots.txt` directives. **Note:** defaults `false` — consider legal/ethical implications. |
| `--discover-sitemaps <bool>` | bool | `true` | Discover and backfill URLs from sitemap.xml after crawl. |
| `--sitemap-since-days <n>` | u32 | `0` | Only backfill sitemap URLs with `<lastmod>` within the last N days (0 = no filter). URLs without `<lastmod>` are always included. |
| `--min-markdown-chars <n>` | usize | `200` | Minimum markdown character count; pages below this are flagged as "thin". |
| `--drop-thin-markdown <bool>` | bool | `true` | Skip thin pages — do not save or embed them. |
| `--delay-ms <ms>` | u64 | `0` | Delay between requests in milliseconds. |
| `--exclude-path-prefix <prefixes>` | csv | *(locale list)* | Comma-separated URL path prefixes to exclude (e.g. `/fr,/de`). Defaults to a broad locale-prefix list. |
| `--url-glob <patterns>` | csv | — | Comma-separated brace-glob patterns for URL expansion. |
| `--start-url <url>` | string | `https://example.com` | Seed URL override. |
| `--url-whitelist <patterns>` | repeatable | — | Only crawl URLs matching these regex patterns. Default: none (crawl all in-scope URLs). |
| `--block-assets <bool>` | bool | `false` | Block image/CSS/font downloads during crawl. |
| `--max-page-bytes <n>` | u64 | `0` | Maximum response size per page in bytes (0 = unlimited). |
| `--redirect-policy-strict <bool>` | bool | `false` | Only follow same-origin redirects. |
| `--sitemap-only` | flag | `false` | Skip the crawl phase; only run sitemap backfill. |
| `--normalize <bool>` | bool | `false` | Deduplicate trailing-slash URL variants during crawl. |
| `--auto-switch-thin-ratio <ratio>` | f64 | `0.60` | Thin-page ratio (0.0–1.0) that triggers auto-switch to Chrome. |
| `--auto-switch-min-pages <n>` | usize | `10` | Minimum pages crawled before auto-switch eligibility check fires. |
| `--root-selector <css>` | string | — | CSS selector to scope content extraction (e.g. `article, main`). |
| `--exclude-selector <css>` | string | — | CSS selector to exclude elements from extraction (e.g. `.sidebar`). |
| `--since <date>` | string | — | Lower temporal bound for query/ask results. Formats: `7d`, `30d`, `YYYY-MM-DD`, RFC3339. |
| `--before <date>` | string | — | Upper temporal bound for query/ask results. Same formats as `--since`. |

#### Browser / Chrome

| Flag | Type | Default | Env Var | Description |
|------|------|---------|---------|-------------|
| `--chrome-remote-url <url>` | string | — | `AXON_CHROME_REMOTE_URL` | Remote Chrome DevTools endpoint. |
| `--chrome-proxy <url>` | string | — | `AXON_CHROME_PROXY` | Proxy URL for Chrome requests. |
| `--chrome-user-agent <ua>` | string | — | `AXON_CHROME_USER_AGENT` | User-Agent override for Chrome. |
| `--chrome-headless <bool>` | bool | `true` | — | Run Chrome in headless mode. |
| `--chrome-anti-bot <bool>` | bool | `true` | — | Enable anti-bot evasion in Chrome. |
| `--chrome-intercept <bool>` | bool | `true` | — | Enable request interception in Chrome. |
| `--chrome-stealth <bool>` | bool | `true` | — | Enable stealth mode in Chrome. |
| `--chrome-bootstrap <bool>` | bool | `true` | — | Enable Chrome bootstrap. |
| `--chrome-bootstrap-timeout-ms <ms>` | u64 | `3000` | — | Bootstrap timeout in ms. |
| `--chrome-bootstrap-retries <n>` | usize | `2` | — | Bootstrap retry count. |
| `--chrome-network-idle-timeout <s>` | u64 | `15` | — | Seconds to wait for Chrome network idle before page capture. |
| `--chrome-wait-for-selector <css>` | string | — | — | CSS selector to wait for before Chrome captures the page. |
| `--chrome-screenshot <bool>` | bool | `false` | — | Capture full-page PNG screenshots during Chrome crawl. |
| `--bypass-csp <bool>` | bool | `false` | — | Bypass Content Security Policy in Chrome. |
| `--accept-invalid-certs <bool>` | bool | `false` | — | Accept invalid or self-signed TLS certificates. |
| `--screenshot-full-page <bool>` | bool | `true` | — | Capture full scrollable page (`true`) or viewport only (`false`). |
| `--viewport <WxH>` | string | `1920x1080` | — | Viewport dimensions for Chrome rendering. |
| `--header <HEADER>` | repeatable | — | — | Custom HTTP header in `Key: Value` format. Repeatable. Applied to crawl, scrape, extract, and Chrome re-fetch paths. |

#### Caching

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--cache <bool>` | bool | `true` | Enable response caching, content hashing, and file reuse. |
| `--cache-skip-browser <bool>` | bool | `false` | Skip cache for browser-rendered pages. |

#### Output

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--output-dir <dir>` | path | `.cache/axon-rust/output` | Directory for saved markdown/HTML output files. |
| `--output <path>` | path | — | Explicit output file path (overrides `--output-dir` for single-file commands). |

#### Vector & Embedding

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--collection <name>` | string | `cortex` | Qdrant collection name. Also settable via `AXON_COLLECTION` env var. |
| `--embed <bool>` | bool | `true` | Auto-embed scraped content into Qdrant. |
| `--limit <n>` | usize | `10` | Result limit for search/query commands. |
| `--query <text>` | string | — | Query text (alternative to positional argument for some commands). |
| `--urls <csv>` | string | — | Comma-separated URL list (alternative to positional arguments). |

#### Performance Tuning

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--performance-profile <p>` | enum | `high-stable` | `high-stable`, `extreme`, `balanced`, `max`. Sets defaults for concurrency, timeouts, retries. |
| `--batch-concurrency <n>` | usize | `16` | Concurrent connections for batch operations (clamped 1–512). |
| `--concurrency-limit <n>` | usize | — | Override both crawl and backfill concurrency limits at once. |
| `--crawl-concurrency-limit <n>` | usize | *profile* | Override crawl concurrency. |
| `--backfill-concurrency-limit <n>` | usize | *profile* | Override backfill concurrency. |
| `--request-timeout-ms <ms>` | u64 | *profile* | Per-request timeout in milliseconds. |
| `--fetch-retries <n>` | usize | *profile* | Number of retries on failed fetches. |
| `--retry-backoff-ms <ms>` | u64 | *profile* | Backoff between retries in milliseconds. |

#### Scheduled / Cron

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--cron-every-seconds <n>` | u64 | — | Repeat a command every N seconds. |
| `--cron-max-runs <n>` | usize | — | Maximum number of cron repetitions (unset = unlimited). |

#### Watchdog

| Flag | Type | Default | Env Var | Description |
|------|------|---------|---------|-------------|
| `--watchdog-stale-timeout-secs <n>` | i64 | `300` | `AXON_JOB_STALE_TIMEOUT_SECS` | Seconds before a running job is considered stale. |
| `--watchdog-confirm-secs <n>` | i64 | `60` | `AXON_JOB_STALE_CONFIRM_SECS` | Seconds to confirm stale status before reclaiming. |

#### Service URLs (override env vars)

| Flag | Env Var | Fallback |
|------|---------|----------|
| `--pg-url <url>` | `AXON_PG_URL` | local Postgres endpoint (rewritten to localhost outside Docker) |
| `--redis-url <url>` | `AXON_REDIS_URL` | local Redis endpoint (rewritten to localhost outside Docker) |
| `--amqp-url <url>` | `AXON_AMQP_URL` | local RabbitMQ endpoint (rewritten to localhost outside Docker) |
| `--qdrant-url <url>` | `QDRANT_URL` | `http://127.0.0.1:53333` |
| `--tei-url <url>` | `TEI_URL` | *(empty)* |
| `--openai-base-url <url>` | `OPENAI_BASE_URL` | *(empty)* |
| `--openai-api-key <key>` | `OPENAI_API_KEY` | *(empty)* |
| `--openai-model <name>` | `OPENAI_MODEL` | *(empty)* |

#### Queue Configuration

| Flag | Env Var | Default |
|------|---------|---------|
| `--shared-queue <bool>` | — | `true` |
| `--crawl-queue <name>` | `AXON_CRAWL_QUEUE` | `axon.crawl.jobs` |
| `--refresh-queue <name>` | `AXON_REFRESH_QUEUE` | `axon.refresh.jobs` |
| `--extract-queue <name>` | `AXON_EXTRACT_QUEUE` | `axon.extract.jobs` |
| `--embed-queue <name>` | `AXON_EMBED_QUEUE` | `axon.embed.jobs` |
| `--ingest-queue <name>` | `AXON_INGEST_QUEUE` | `axon.ingest.jobs` |

## Performance Profiles

Concurrency tuned relative to available CPU cores:

| Profile | Crawl concurrency | Backfill concurrency | Timeout | Retries | Backoff |
|---------|------------------|----------------------|---------|---------|---------|
| `high-stable` (default) | CPUs×8 (64–192) | CPUs×6 (32–128) | 20s | 2 | 250ms |
| `balanced` | CPUs×4 (32–96) | CPUs×3 (16–64) | 30s | 2 | 300ms |
| `extreme` | CPUs×16 (128–384) | CPUs×10 (64–256) | 15s | 1 | 100ms |
| `max` | CPUs×24 (256–1024) | CPUs×20 (128–1024) | 12s | 1 | 50ms |

## Troubleshooting

- `axon doctor` for service reachability (Postgres/Redis/AMQP/Qdrant/TEI/LLM)
- `axon debug` to run doctor + LLM-assisted troubleshooting through ACP (`AXON_ACP_ADAPTER_CMD`)
- Check your worker terminal output for failures (workers run locally, not in Docker)
- Jobs stuck in pending: ensure worker processes are running and AMQP/Redis are reachable (`./scripts/axon doctor`)
- Manually reclaim stale jobs if needed:
  - `axon crawl recover`
  - `axon extract recover`
  - `axon embed recover`
- `ask`/`evaluate`/`suggest`/`debug`/`research` failures: verify `AXON_ACP_ADAPTER_CMD` is configured and runnable
- `embed`/`query` failures: verify `TEI_URL` and `QDRANT_URL`
- Browser fallback failures: verify `AXON_CHROME_REMOTE_URL` points to a live Chrome management endpoint (e.g. `http://127.0.0.1:6000`). The `axon-chrome` compose service exposes this at `127.0.0.1:6000` (management) and `127.0.0.1:9222` (CDP proxy) when running.

## Monolith Guardrails

Axon enforces a ratcheting monolith policy on changed code:

- File size limit (changed Rust files): `500` lines
- Rust function size limit (changed functions): `80` lines
- Only Rust source files (`*.rs`) are checked for file size
- Test/config paths are exempt (`tests/**`, `**/*_test.*`, `**/*.test.*`, `**/*.spec.*`, `benches/**`, `config/**`, `**/config/**`, `**/config.rs`)
- Temporary file-level exceptions can be added to `.monolith-allowlist`

Axon also enforces a legacy symbol deny-list in hooks/CI to prevent reintroducing removed v1 paths.

Install local pre-commit enforcement (lefthook):

```bash
# install lefthook once (choose one)
brew install lefthook
# or
cargo install --locked lefthook

# install git hooks for this repo
./scripts/install-git-hooks.sh
```

The same policy runs in CI on pull requests and pushes.

Detailed policy and exception workflow: `docs/LIVE-TEST-SCRIPTS.md`.

## Database Schema

Tables are auto-created on first worker/command start via `CREATE TABLE IF NOT EXISTS` in each `*_jobs.rs` file's `ensure_schema()` function.

### axon_crawl_jobs

| Column | Type | Nullable | Default | Description |
|--------|------|----------|---------|-------------|
| `id` | UUID | NOT NULL | — | Primary key, job identifier |
| `url` | TEXT | NOT NULL | — | Target URL for the crawl |
| `status` | TEXT | NOT NULL | — | `pending` / `running` / `completed` / `failed` / `canceled` |
| `created_at` | TIMESTAMPTZ | NOT NULL | `NOW()` | Job creation timestamp |
| `updated_at` | TIMESTAMPTZ | NOT NULL | `NOW()` | Last status change |
| `started_at` | TIMESTAMPTZ | NULL | — | When worker began processing |
| `finished_at` | TIMESTAMPTZ | NULL | — | When job completed/failed/canceled |
| `error_text` | TEXT | NULL | — | Error message on failure |
| `result_json` | JSONB | NULL | — | Crawl results (pages found, stats) |
| `config_json` | JSONB | NOT NULL | — | Serialized job configuration |

**Index:** `idx_axon_crawl_jobs_status` on `status`.

### axon_extract_jobs

| Column | Type | Nullable | Default | Description |
|--------|------|----------|---------|-------------|
| `id` | UUID | NOT NULL | — | Primary key |
| `status` | TEXT | NOT NULL | — | `pending` / `running` / `completed` / `failed` / `canceled` |
| `created_at` | TIMESTAMPTZ | NOT NULL | `NOW()` | Job creation timestamp |
| `updated_at` | TIMESTAMPTZ | NOT NULL | `NOW()` | Last status change |
| `started_at` | TIMESTAMPTZ | NULL | — | When worker began processing |
| `finished_at` | TIMESTAMPTZ | NULL | — | When job completed/failed/canceled |
| `error_text` | TEXT | NULL | — | Error message on failure |
| `urls_json` | JSONB | NOT NULL | — | Array of URLs for LLM extraction |
| `result_json` | JSONB | NULL | — | Extracted structured data |
| `config_json` | JSONB | NOT NULL | — | Serialized job configuration |

### axon_embed_jobs

| Column | Type | Nullable | Default | Description |
|--------|------|----------|---------|-------------|
| `id` | UUID | NOT NULL | — | Primary key |
| `status` | TEXT | NOT NULL | — | `pending` / `running` / `completed` / `failed` / `canceled` |
| `created_at` | TIMESTAMPTZ | NOT NULL | `NOW()` | Job creation timestamp |
| `updated_at` | TIMESTAMPTZ | NOT NULL | `NOW()` | Last status change |
| `started_at` | TIMESTAMPTZ | NULL | — | When worker began processing |
| `finished_at` | TIMESTAMPTZ | NULL | — | When job completed/failed/canceled |
| `error_text` | TEXT | NULL | — | Error message on failure |
| `input_text` | TEXT | NOT NULL | — | Input path, URL, or text to embed |
| `result_json` | JSONB | NULL | — | Embedding results (chunk count, point IDs) |
| `config_json` | JSONB | NOT NULL | — | Serialized job configuration |

### axon_ingest_jobs

Differs from the other three tables: uses `source_type` + `target` instead of `url` or `urls_json` to identify the ingest target, and has no `urls_json` or `input_text` column.

| Column | Type | Nullable | Default | Description |
|--------|------|----------|---------|-------------|
| `id` | UUID | NOT NULL | — | Primary key |
| `status` | TEXT | NOT NULL | — | `pending` / `running` / `completed` / `failed` / `canceled` |
| `source_type` | TEXT | NOT NULL | — | Discriminator: `github`, `reddit`, or `youtube` |
| `target` | TEXT | NOT NULL | — | Ingest target: repo name (`owner/repo`), subreddit, or YouTube URL |
| `created_at` | TIMESTAMPTZ | NOT NULL | `NOW()` | Job creation timestamp |
| `updated_at` | TIMESTAMPTZ | NOT NULL | `NOW()` | Last status change |
| `started_at` | TIMESTAMPTZ | NULL | — | When worker began processing |
| `finished_at` | TIMESTAMPTZ | NULL | — | When job completed/failed/canceled |
| `error_text` | TEXT | NULL | — | Error message on failure |
| `result_json` | JSONB | NULL | — | Ingest results (`{"chunks_embedded": N}`) |
| `config_json` | JSONB | NOT NULL | — | Serialized `IngestJobConfig` (source variant + collection) |

**Index:** `idx_axon_ingest_jobs_pending` — partial index on `created_at ASC WHERE status = 'pending'` for efficient FIFO queue polling.

### axon_refresh_jobs

| Column | Type | Nullable | Default | Description |
|--------|------|----------|---------|-------------|
| `id` | UUID | NOT NULL | — | Primary key |
| `status` | TEXT | NOT NULL | — | `pending` / `running` / `completed` / `failed` / `canceled` |
| `urls_json` | JSONB | NOT NULL | — | Array of URLs to refresh |
| `created_at` | TIMESTAMPTZ | NOT NULL | `NOW()` | Job creation timestamp |
| `updated_at` | TIMESTAMPTZ | NOT NULL | `NOW()` | Last status change |
| `started_at` | TIMESTAMPTZ | NULL | — | When worker began processing |
| `finished_at` | TIMESTAMPTZ | NULL | — | When job completed/failed/canceled |
| `error_text` | TEXT | NULL | — | Error message on failure |
| `result_json` | JSONB | NULL | — | Refresh results |
| `config_json` | JSONB | NOT NULL | — | Serialized job configuration |

### axon_refresh_targets

Tracks per-URL HTTP cache metadata for conditional GET refresh (ETag / Last-Modified / content hash).

| Column | Type | Nullable | Description |
|--------|------|----------|-------------|
| `url` | TEXT | NOT NULL (PK) | Target URL |
| `etag` | TEXT | NULL | Last ETag seen |
| `last_modified` | TEXT | NULL | Last-Modified header value |
| `content_hash` | TEXT | NULL | SHA-256 of last markdown content |
| `markdown_chars` | INTEGER | NULL | Character count of last content |
| `last_status` | INTEGER | NULL | HTTP status code of last fetch |
| `last_checked_at` | TIMESTAMPTZ | NOT NULL | Last fetch attempt timestamp |
| `last_changed_at` | TIMESTAMPTZ | NULL | Last time content actually changed |
| `error_text` | TEXT | NULL | Error on last failed fetch |

### axon_refresh_schedules

Named recurring refresh schedules. Backed by `axon refresh schedule ...` commands. Each schedule maps a name to a set of URLs and a recurrence interval.

| Column | Type | Nullable | Description |
|--------|------|----------|-------------|
| `id` | UUID | NOT NULL (PK) | Primary key |
| `name` | TEXT | NOT NULL (UNIQUE) | Schedule name |
| `seed_url` | TEXT | NULL | Optional seed URL |
| `urls_json` | JSONB | NULL | Array of URLs to refresh on schedule |
| `every_seconds` | BIGINT | NOT NULL | Recurrence interval in seconds |
| `enabled` | BOOLEAN | NOT NULL | Whether schedule is active |
| `next_run_at` | TIMESTAMPTZ | NOT NULL | Next scheduled run time |
| `last_run_at` | TIMESTAMPTZ | NULL | Last time the schedule ran |
| `created_at` | TIMESTAMPTZ | NOT NULL | Creation timestamp |
| `updated_at` | TIMESTAMPTZ | NOT NULL | Last update timestamp |

### axon_graph_jobs

| Column | Type | Nullable | Description |
|--------|------|----------|-------------|
| `id` | UUID | NOT NULL (PK) | Primary key |
| `url` | TEXT | NOT NULL | Source URL for graph extraction |
| `status` | TEXT | NOT NULL | `pending` / `running` / `completed` / `failed` / `canceled` |
| `chunk_count` | INTEGER | NULL | Number of chunks processed |
| `entity_count` | INTEGER | NULL | Extracted entity count |
| `relation_count` | INTEGER | NULL | Extracted relation count |
| `result_json` | JSONB | NULL | Extraction results |
| `config_json` | JSONB | NULL | Serialized job configuration |
| `error_text` | TEXT | NULL | Error message on failure |
| `created_at` | TIMESTAMPTZ | NULL | Job creation timestamp |
| `updated_at` | TIMESTAMPTZ | NULL | Last status change |
| `started_at` | TIMESTAMPTZ | NULL | When worker began processing |
| `finished_at` | TIMESTAMPTZ | NULL | When job completed/failed/canceled |

### axon_watch_defs

Top-level recurring watch definitions (e.g. `task_type=refresh`). Managed by `axon watch ...` commands.

| Column | Type | Nullable | Description |
|--------|------|----------|-------------|
| `id` | UUID | NOT NULL (PK) | Primary key |
| `name` | TEXT | NOT NULL (UNIQUE) | Watch name |
| `task_type` | TEXT | NOT NULL | Task type (currently: `refresh`) |
| `task_payload` | JSONB | NOT NULL | Task payload (e.g. `{"urls":["https://..."]}`) |
| `every_seconds` | BIGINT | NOT NULL | Recurrence interval in seconds |
| `enabled` | BOOLEAN | NOT NULL | Whether watch is active |
| `next_run_at` | TIMESTAMPTZ | NOT NULL | Next scheduled run time |
| `lease_expires_at` | TIMESTAMPTZ | NULL | Worker lease expiry for concurrency safety |
| `last_run_at` | TIMESTAMPTZ | NULL | Last time the watch fired |
| `created_at` | TIMESTAMPTZ | NOT NULL | Creation timestamp |
| `updated_at` | TIMESTAMPTZ | NOT NULL | Last update timestamp |

**Index:** `idx_axon_watch_defs_due` — partial index on `next_run_at ASC WHERE enabled = TRUE` for efficient due-watch polling.

### axon_watch_runs

Execution history for each `axon_watch_defs` trigger.

| Column | Type | Nullable | Description |
|--------|------|----------|-------------|
| `id` | UUID | NOT NULL (PK) | Primary key |
| `watch_id` | UUID | NOT NULL (FK → axon_watch_defs) | Parent watch definition |
| `status` | TEXT | NOT NULL | `pending` / `running` / `completed` / `failed` / `canceled` |
| `dispatched_job_id` | UUID | NULL | Downstream job dispatched by this run |
| `error_text` | TEXT | NULL | Error on failure |
| `result_json` | JSONB | NULL | Run results |
| `started_at` | TIMESTAMPTZ | NULL | Run start time |
| `finished_at` | TIMESTAMPTZ | NULL | Run completion time |
| `created_at` | TIMESTAMPTZ | NOT NULL | Creation timestamp |
| `updated_at` | TIMESTAMPTZ | NOT NULL | Last update timestamp |

### axon_watch_run_artifacts

Artifacts produced by a watch run (e.g. output files, diffs).

| Column | Type | Nullable | Description |
|--------|------|----------|-------------|
| `id` | BIGSERIAL | NOT NULL (PK) | Auto-increment primary key |
| `watch_run_id` | UUID | NOT NULL (FK → axon_watch_runs) | Parent run |
| `kind` | TEXT | NOT NULL | Artifact kind discriminator |
| `path` | TEXT | NULL | Filesystem path of the artifact |
| `payload` | JSONB | NULL | Artifact payload data |
| `created_at` | TIMESTAMPTZ | NOT NULL | Creation timestamp |

## Gotchas

### `--wait false` (default) = fire-and-forget
By default, `crawl`, `refresh`, `extract`, `embed`, and `ingest` enqueue jobs and return immediately. Use `--wait true` to block until completion. Without workers running, enqueued jobs will pend forever.

### Armory Structure: Domain-Grouped
Axon now organizes its spoils by domain to make the armory more browseable.
- **Atomic Hunts**: Every job is isolated in `domains/<domain>/<job-id>/`.
- **Latest View**: A zero-cost **Reflink** (or hardlink) provides a stable view of the most current successful hunt at `domains/<domain>/latest/`.

### Incremental Crawl Synchronization
When caching is enabled, Axon uses a "Recycling Bin" pattern. It moves existing markdown to a temporary location, surgically reflinks unchanged content during the crawl, and then purges any files that were not rediscovered. This ensures that the `domains/<domain>/latest/` directory is a perfect mirror of the live site.

### `render-mode auto-switch`
The default mode. Runs an HTTP crawl first; if >60% of pages are thin (<200 chars) or total coverage is too low, automatically retries with Chrome. Chrome requires `axon-chrome` running — if unreachable, the HTTP result is kept.

### `crawl_raw()` vs `crawl()`
When Chrome feature is compiled in, `crawl()` expects a Chrome instance. `crawl_raw()` is pure HTTP and always works. `engine.rs` calls `crawl_raw()` for `RenderMode::Http` and `crawl()` for Chrome/AutoSwitch.

### ACP completion path
`ask`, `evaluate`, `suggest`, extract fallback, `debug`, and research synthesis route through ACP.
- **Required:** `AXON_ACP_ADAPTER_CMD=<adapter-binary>`
- **Optional model override:** `OPENAI_MODEL=<model-name>`

### TEI batch size / 413 handling
`tei_embed()` in `vector/ops/tei.rs` auto-splits batches on HTTP 413 (Payload Too Large). Set `TEI_MAX_CLIENT_BATCH_SIZE` env var to control default chunk size (default: 128, max: 128).

### TEI retries / timeouts

`tei_embed()` retries transient failures (transport errors, HTTP 429, and HTTP 5xx) with exponential backoff + jitter.
- `TEI_MAX_RETRIES` controls max attempts per TEI request (default: 5, max: 20).
- `TEI_REQUEST_TIMEOUT_MS` controls per-attempt request timeout (default: 30000, clamped 100..600000).
- Hard client errors (HTTP 4xx except 429) fail fast (no retry storm).
- HTTP 413 still triggers batch-splitting logic before retrying smaller chunks.

### Locale path prefix matching
`--exclude-path-prefix` treats both `/` and `-` as word boundaries. Specifying `/ja` blocks both `/ja/docs` and `/ja-jp/docs` (IETF BCP 47 region subtags). Pass `none` to disable all locale filtering.

### Text chunking
`chunk_text()` splits at 2000 chars with 200-char overlap. Each chunk = one Qdrant point. Very long pages produce many points.

### Thin page filtering
Pages with fewer than `--min-markdown-chars` (default: 200) are flagged as thin. If `--drop-thin-markdown true` (default), thin pages are skipped — not saved to disk or embedded.

### ensure_collection() auto-creates collections
`ensure_collection()` does a GET first; only issues a PUT on 404 (collection not found). Safe to call on every embed — no 409 Conflict on existing collections. New collections are created with named dense + bm42 sparse vectors (hybrid mode). Existing unnamed-vector collections are left unchanged (hybrid search disabled for those; use `axon migrate` to upgrade).

### Default collection name
The default Qdrant collection is `cortex` (set via `AXON_COLLECTION` or `--collection`). If you previously used an older build that defaulted to `spider_rust`, pass `--collection spider_rust` explicitly.

### Sitemap backfill
After a crawl, `append_sitemap_backfill()` discovers URLs via sitemap.xml that the crawler missed and fetches them individually. It currently uses an internal cap of 512 parsed sitemap entries and respects `--include-subdomains`. Use `--sitemap-since-days N` to restrict backfill to URLs whose `<lastmod>` falls within the last N days; URLs without `<lastmod>` are always included.

### Docker build context
The `Dockerfile` is at `docker/Dockerfile`. Run `docker compose build` from this directory (not a parent workspace). The binary built inside the container is `axon`.

## Development

### Git Hooks (required)

Install lefthook pre-commit hooks before making any commits. The hooks enforce the monolith policy (file size, function size, deny-list) that CI also checks:

```bash
# install lefthook once (choose one method)
brew install lefthook
# or
cargo install --locked lefthook

# install git hooks for this repo (required)
./scripts/install-git-hooks.sh
```

Without this, you will not get local feedback before commits fail CI.

### Build

```bash
cargo build --bin axon                        # debug
cargo build --release --bin axon              # release
cargo check                                   # fast type check
```

### Local Dev Workflow (Optimized)

Use `just` targets for a faster Rust loop:

```bash
# install optional local tools
just nextest-install
just llvm-cov-install

# local default test lane (nextest when available, skips worker_e2e)
just test

# fastest inner loop (unit/lib focused)
just test-fast

# explicit infra-dependent tests (worker_e2e)
just test-infra

# watch mode: check + test type-check + fast lib tests
just watch-check

# branch-level coverage report (lcov)
just coverage-branch

# inspect/trim local build caches
just cache-status
just cache-prune

# enforce live Docker build-context size thresholds
just docker-context-probe
```

Notes:
- `just` auto-enables `sccache` and `mold` if installed (`RUSTC_WRAPPER=sccache`, `-fuse-ld=mold`).
- Worker E2E tests are marked `#[ignore]` and intended to run explicitly via `just test-infra`.
- Build/test/check/clippy commands in local and CI paths are lockfile-strict (`--locked`).
- `scripts/rebuild-fresh.sh` runs two guardrails by default before rebuilding:
  - cache guard (`scripts/cache-guard.sh`)
  - live Docker context probe (`scripts/check_docker_context_size.sh`)

### Manual CI Infra Lane

Use the optional GitHub Actions lane when you want CI to run ignored infra-backed worker tests:

1. Open `Actions` -> `CI` -> `Run workflow`.
2. Set `run_infra_tests` to `true`.
3. Start the run; the `test-infra` job will execute `just test-infra` with Postgres/Redis/RabbitMQ services.

This lane is manual-only (`workflow_dispatch`) so normal PR/push CI stays fast.

### Lint

```bash
cargo clippy
cargo fmt --check
```

### Run directly

```bash
# Debug binary
./target/debug/axon scrape https://example.com

# With env overrides
QDRANT_URL=http://localhost:53333 \
TEI_URL=http://myserver:52000 \
./target/release/axon query "embedding pipeline" --collection my_col
```

### Diagnose service connectivity

```bash
axon doctor
```

Checks: Postgres, Redis, RabbitMQ, Qdrant, TEI, LLM endpoint reachability.

## Code Style

- Rust standard style — run `cargo fmt` before committing
- `cargo clippy` clean before committing
- Errors bubble via `Box<dyn Error>` at command boundaries; internal helpers return typed errors
- Structured log output via `log_info` / `log_warn` (not `println!` in library code)
- `--json` flag enables machine-readable output on all commands that print results
