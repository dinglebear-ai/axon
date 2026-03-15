# axon_cli — Axon CLI (Rust + Spider.rs)
Last Modified: 2026-03-10

Web crawl, scrape, extract, embed, and query — all in one binary backed by a self-hosted RAG stack.

## Quick Start

> **Local dev mode**: Workers and the web frontend run as local processes, not in Docker. Only infrastructure (Postgres, Redis, RabbitMQ, Qdrant, Chrome) runs via Docker Compose.

```bash
# 1. Start infrastructure only
docker compose up -d axon-postgres axon-redis axon-rabbitmq axon-qdrant axon-chrome

# 2. Recommended: use the wrapper script (auto-sources .env)
./scripts/axon doctor
./scripts/axon scrape https://example.com --wait true

# 3. Run workers locally (each in a separate terminal or tmux pane)
cargo run --bin axon -- crawl worker
cargo run --bin axon -- embed worker
cargo run --bin axon -- extract worker
cargo run --bin axon -- graph worker

# 4. Run the web frontend locally
cd apps/web && pnpm dev    # → http://localhost:49010

# MCP server via CLI subcommand
./scripts/axon mcp

# Or build and run the binary directly
cargo build --release --bin axon
./target/release/axon --help

# Or build + run in one shot (does NOT auto-source .env)
cargo run --bin axon -- scrape https://example.com --wait true
```

> **Note:** The binary is named `axon`. Build with `cargo build --bin axon`.

## MCP Server (`axon mcp`)

Axon ships an MCP server subcommand that exposes a single tool (`axon`) with `action`/`subaction` routing for crawl/extract/embed/ingest/RAG/discovery/ops workflows.

```bash
cargo build --release --bin axon
./target/release/axon mcp
```

MCP docs:
- `docs/MCP.md` (runtime/design guide)
- `docs/MCP-TOOL-SCHEMA.md` (wire contract schema source of truth)

## Commands

| Command | Purpose | Async? |
|---------|---------|--------|
| `scrape <url>...` | Scrape one or more URLs to markdown | No |
| `crawl <url>...` | Full site crawl for one or more start URLs | Yes (default) |
| `map <url>` | Discover all URLs without scraping | No |
| `extract <urls...>` | LLM-powered structured data extraction | Yes (default) |
| `search <query>` | Web search via Tavily, auto-queues crawl jobs for results | No |
| `research <query>` | Web research via Tavily AI search with LLM synthesis | No |
| `embed [input]` | Embed file/dir/URL into Qdrant | Yes (default) |
| `query <text>` | Semantic vector search | No |
| `retrieve <url>` | Fetch stored document chunks from Qdrant | No |
| `ask <question>` | RAG: search + LLM answer. Use `--graph` to inject Neo4j graph context when configured. | No |
| `evaluate <question>` | RAG vs baseline + independent LLM judge (accuracy, relevance, completeness, specificity, verdict) | No |
| `suggest [focus]` | Suggest new docs URLs to crawl | No |
| `ingest <target>` | Ingest external source (GitHub repo, Reddit subreddit/thread, YouTube video/playlist/channel) — auto-detects source type from target. GitHub: source code indexed by default with tree-sitter AST chunking; use `--no-source` to skip. | Yes (default) |
| `sessions [format]` | Ingest AI session exports (Claude/Codex/Gemini) into Qdrant | No |
| `sources` | List all indexed URLs + chunk counts | No |
| `domains` | List indexed domains + stats | No |
| `stats` | Qdrant collection stats | No |
| `status` | Show async job queue status | No |
| `doctor` | Diagnose service connectivity | No |
| `debug` | Run doctor + LLM-assisted troubleshooting | No |
| `mcp` | Start MCP stdio server | No |
| `refresh <url>` | Periodic URL re-indexing (schedule, status, cancel, list). Supports `github:owner/repo` schedules with `pushed_at` gating. | Yes (default) |
| `graph <sub>` | Knowledge graph operations: `build`, `status`, `explore`, `stats`, `worker`. Requires `AXON_NEO4J_URL`. | Depends |
| `serve` | Start web UI server (axum + WebSocket + Docker stats) | No |

### Job Subcommands (for crawl / extract / embed / refresh)

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

### Global Flags Reference

All flags are `--global` (usable with any subcommand).

#### Core Behavior

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--wait <bool>` | bool | `false` | Run synchronously and block until completion. Without this, async commands enqueue and return immediately. |
| `--yes` | flag | `false` | Skip confirmation prompts (non-interactive mode). |
| `--json` | flag | `false` | Machine-readable JSON output on stdout. |
| `--graph` | flag | `false` | Enable graph-enhanced retrieval for `ask` (requires Neo4j). |

#### Crawl & Scrape

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--max-pages <n>` | u32 | `0` | Page cap for crawl (0 = uncapped, default). |
| `--max-depth <n>` | usize | `5` | Maximum crawl depth from start URL. |
| `--render-mode <mode>` | enum | `auto-switch` | `http`, `chrome`, or `auto-switch`. Auto-switch tries HTTP first, falls back to Chrome if >60% thin pages. |
| `--format <fmt>` | enum | `markdown` | Output format: `markdown`, `html`, `rawHtml`, `json`. |
| `--include-subdomains <bool>` | bool | `false` | Crawl all subdomains of the start URL's parent domain. Disabled by default — enable with `--include-subdomains true`. |
| `--respect-robots <bool>` | bool | `false` | Respect `robots.txt` directives. **Note:** defaults `false` — legal/ethical implications. |
| `--discover-sitemaps <bool>` | bool | `true` | Discover and backfill URLs from sitemap.xml after crawl. |
| `--max-sitemaps <n>` | usize | `512` | Maximum sitemap URLs to backfill per crawl. |
| `--sitemap-since-days <n>` | u32 | `0` | Only backfill sitemap URLs with `<lastmod>` within the last N days (0 = no filter). URLs without `<lastmod>` are always included. |
| `--min-markdown-chars <n>` | usize | `200` | Minimum markdown character count; pages below this are flagged as "thin". |
| `--drop-thin-markdown <bool>` | bool | `true` | Skip thin pages — do not save or embed them. |
| `--delay-ms <ms>` | u64 | `0` | Delay between requests in milliseconds. Useful for polite crawling. |
| `--header <HEADER>` | string | — | Custom HTTP header in `Key: Value` format. Repeatable (`--header "Auth: Bearer ..." --header "X-Custom: val"`). Applied to crawl, scrape, extract, and Chrome re-fetch paths. |

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
| `--concurrency-limit <n>` | usize | — | Override all three concurrency limits (crawl, sitemap, backfill) at once. |
| `--crawl-concurrency-limit <n>` | usize | *profile* | Override crawl concurrency (profile default: CPUs x multiplier). |
| `--sitemap-concurrency-limit <n>` | usize | *profile* | Override sitemap backfill concurrency. |
| `--backfill-concurrency-limit <n>` | usize | *profile* | Override backfill concurrency. |
| `--request-timeout-ms <ms>` | u64 | *profile* | Per-request timeout in milliseconds. |
| `--fetch-retries <n>` | usize | *profile* | Number of retries on failed fetches. |
| `--retry-backoff-ms <ms>` | u64 | *profile* | Backoff between retries in milliseconds. |

#### Service URLs (override env vars)

| Flag | Type | Env Var | Fallback |
|------|------|---------|----------|
| `--pg-url <url>` | string | `AXON_PG_URL` | `postgresql://axon:postgres@127.0.0.1:53432/axon` |
| `--redis-url <url>` | string | `AXON_REDIS_URL` | `redis://127.0.0.1:53379` |
| `--amqp-url <url>` | string | `AXON_AMQP_URL` | `amqp://axon:axonrabbit@127.0.0.1:45535/%2f` |
| `--qdrant-url <url>` | string | `QDRANT_URL` | `http://127.0.0.1:53333` |
| `--tei-url <url>` | string | `TEI_URL` | *(empty)* |
| `--openai-base-url <url>` | string | `OPENAI_BASE_URL` | *(empty)* |
| `--openai-api-key <key>` | string | `OPENAI_API_KEY` | *(empty)* |
| `--openai-model <name>` | string | `OPENAI_MODEL` | *(empty)* |

#### Queue Configuration

| Flag | Type | Env Var | Default |
|------|------|---------|---------|
| `--shared-queue <bool>` | bool | — | `true` |
| `--crawl-queue <name>` | string | `AXON_CRAWL_QUEUE` | `axon.crawl.jobs` |
| `--extract-queue <name>` | string | `AXON_EXTRACT_QUEUE` | `axon.extract.jobs` |
| `--embed-queue <name>` | string | `AXON_EMBED_QUEUE` | `axon.embed.jobs` |

## Architecture

Canonical architecture and data-flow diagrams live in `docs/ARCHITECTURE.md`.

High-level subsystem map:

- Entrypoint and dispatch:
  - `main.rs` loads environment and calls `axon::run()`
  - `lib.rs` owns `run`/`run_once` and command dispatch
- Command + config:
  - `crates/cli/*` command handlers
  - `crates/core/config/{cli,parse,types}.rs` flag/env parsing and runtime config resolution
- Crawl + content:
  - `crates/crawl/engine.rs`
  - `crates/core/http.rs` and `crates/core/content.rs`
- Async jobs:
  - `crates/jobs/crawl/` (manifest, processor, repo, sitemap, watchdog, worker, runtime)
  - `crates/jobs/{extract,embed}/` modules, `crates/jobs/ingest.rs`
  - `crates/jobs/common/*` and `crates/jobs/worker_lane.rs`
  - job states in `crates/jobs/status.rs`
- Vector + RAG:
  - `crates/vector/ops/*` (TEI embedding, Qdrant upsert/search, ask/evaluate/query)
- Services layer (services-first contract):
  - `crates/services/` — typed entry points consumed by both CLI handlers and MCP/web routes
  - CLI commands call `crates/services::{query,retrieve,ask,sources,domains,stats,system}` — **not** raw `run_*_native()` functions (those are removed)
  - Each service function returns a typed `ServiceResult` struct (defined in `crates/services/types/service.rs`) — no raw JSON printing or stdout side-effects
  - MCP handlers and web routes call the same service functions, mapping typed results to wire format
  - ACP orchestration lives in `crates/services/acp/` (session lifecycle, permission bridge, adapter subprocess)
- MCP server:
  - `crates/mcp/` (schema, server routing, handler modules, config)
  - Single `axon` tool with `action`/`subaction` routing
- Web runtimes:
  - WebSocket execution bridge: `crates/web.rs`
  - Active UI: `apps/web/` (Next.js — omnibox, Pulse workspace, port 49010)

## Infrastructure

### Docker Services

| Service | Image | Exposed Port | Purpose |
|---------|-------|-------------|---------|
| `axon-postgres` | postgres:17-alpine | `53432` | Job persistence |
| `axon-redis` | redis:8.2-alpine | `53379` | Queue state / caching |
| `axon-rabbitmq` | rabbitmq:4.0-management | `45535` | AMQP job queue |
| `axon-qdrant` | qdrant/qdrant:v1.13.1 | `53333`, `53334` (gRPC) | Vector store |
| `axon-chrome` | built from docker/chrome/Dockerfile | `6000` (management), `9222` (CDP proxy) | headless_browser + chrome-headless-shell |
| `axon-workers` | **local process** | — | Run with `cargo run --bin axon -- crawl worker` (and equivalent for extract/embed/ingest). Not in Docker for local dev. |
| `axon-web` | **local process** | `49010` | Run with `cd apps/web && pnpm dev`. Not in Docker for local dev. |

Infrastructure services (postgres, redis, rabbitmq, qdrant, chrome) live on the `axon` bridge network. Data volumes use `${AXON_DATA_DIR:-./data}/axon/...` (override with `AXON_DATA_DIR` in `.env`).

```bash
# Start infrastructure only (no workers or web)
docker compose up -d axon-postgres axon-redis axon-rabbitmq axon-qdrant axon-chrome

# Run workers locally (each in its own terminal or via a process manager)
cargo run --bin axon -- crawl worker
cargo run --bin axon -- embed worker
cargo run --bin axon -- extract worker

# Run the frontend locally
cd apps/web && pnpm dev    # → http://localhost:49010

# Check infra health
docker compose ps

# Tail infra logs
docker compose logs -f axon-postgres axon-redis axon-rabbitmq axon-qdrant
```

### External Service: TEI (Text Embeddings Inference)

TEI is **not** in docker-compose — it's an external self-hosted service. Set `TEI_URL` in `.env`.

```bash
TEI_URL=http://YOUR_TEI_HOST:52000
```

## Environment Variables

Copy `.env.example` → `.env` and fill in values:

```bash
# Compose persistent data root on host
AXON_DATA_DIR=/home/yourname/appdata

# Postgres
AXON_PG_URL=postgresql://axon:postgres@axon-postgres:5432/axon

# Redis
AXON_REDIS_URL=redis://:CHANGE_ME@axon-redis:6379

# RabbitMQ
AXON_AMQP_URL=amqp://axon:CHANGE_ME@axon-rabbitmq:5672

# Qdrant
QDRANT_URL=http://axon-qdrant:6333

# TEI embeddings (external — required for embed/query/ask)
TEI_URL=http://REPLACE_WITH_TEI_HOST:52000

# LLM (required for extract and ask commands)
OPENAI_BASE_URL=http://YOUR_LLM_HOST/v1
OPENAI_API_KEY=your-key-or-empty
OPENAI_MODEL=your-model-name

# CDP endpoint for headless_browser (axon-chrome management API)
AXON_CHROME_REMOTE_URL=http://axon-chrome:6000

# Optional queue name overrides
AXON_CRAWL_QUEUE=axon.crawl.jobs
AXON_EXTRACT_QUEUE=axon.extract.jobs
AXON_EMBED_QUEUE=axon.embed.jobs
AXON_INGEST_QUEUE=axon.ingest.jobs
AXON_GRAPH_QUEUE=axon.graph.jobs
AXON_COLLECTION=cortex              # Qdrant collection (default: cortex)

# Neo4j / GraphRAG (optional — graph features are disabled when AXON_NEO4J_URL is empty)
AXON_NEO4J_URL=bolt://localhost:7687
AXON_NEO4J_USER=neo4j
AXON_NEO4J_PASSWORD=
AXON_GRAPH_CONCURRENCY=4
AXON_GRAPH_LLM_URL=http://localhost:11434
AXON_GRAPH_LLM_MODEL=qwen3.5:2b
AXON_GRAPH_SIMILARITY_THRESHOLD=0.75
AXON_GRAPH_SIMILARITY_LIMIT=20
AXON_GRAPH_CONTEXT_MAX_CHARS=2000
AXON_GRAPH_TAXONOMY_PATH=

# Search and research (required for search/research commands)
TAVILY_API_KEY=your-tavily-api-key

# Ingest credentials (Reddit required; GitHub optional for higher rate limits)
GITHUB_TOKEN=                       # optional — raises GitHub rate limits
REDDIT_CLIENT_ID=                   # required for Reddit ingest targets
REDDIT_CLIENT_SECRET=               # required for Reddit ingest targets

# Worker tuning (optional, defaults shown)
AXON_INGEST_LANES=2                 # parallel ingest worker lanes
AXON_EMBED_DOC_TIMEOUT_SECS=300     # per-document embed timeout
AXON_EMBED_STRICT_PREDELETE=true    # delete existing points before re-embedding
AXON_JOB_STALE_TIMEOUT_SECS=300    # seconds before a running job is considered stale
AXON_JOB_STALE_CONFIRM_SECS=60     # additional grace period before stale reclaim
```

### Web App Security Env (`apps/web`)

Three auth tokens cover two surfaces (`/api/*` and `/ws`):

| Token | Scope | Required |
|-------|-------|----------|
| `AXON_WEB_API_TOKEN` | Primary token. **Server-only** — do NOT expose to the browser. Gates both `/api/*` (proxy.ts) and `/ws` (Rust WS gate via `?token=`). The `?token=` query param is a necessary limitation: WebSocket upgrade requests cannot carry custom headers. | Yes |
| `AXON_WEB_BROWSER_API_TOKEN` | Optional second-tier token for `/api/*` routes only. Does **not** gate `/ws`. If unset, `AXON_WEB_API_TOKEN` is used for all `/api/*` routes. Use this to keep the browser-exposed token separate from the primary WS gate token. | No |
| `NEXT_PUBLIC_AXON_API_TOKEN` | Browser-exposed token. `apiFetch()` sends it as `x-api-key` on `/api/*`; `use-axon-ws.ts` appends it as `?token=` on the WS URL. **Must equal `AXON_WEB_BROWSER_API_TOKEN`** when that is set, or `AXON_WEB_API_TOKEN` otherwise. Do not set this to `AXON_WEB_API_TOKEN` when `AXON_WEB_BROWSER_API_TOKEN` is configured. | Yes (when `AXON_WEB_API_TOKEN` is set) |

MCP OAuth (`atk_` tokens) is a separate auth system for MCP clients only — it does not touch `/ws` or `/api/*`.

```bash
# Primary token — gates both /api/* (proxy.ts) and /ws (Rust WS gate)
AXON_WEB_API_TOKEN=CHANGE_ME

# Optional second-tier token — gates /api/* only (does NOT gate /ws).
# If unset, AXON_WEB_API_TOKEN is used for all routes.
AXON_WEB_BROWSER_API_TOKEN=

# Browser-exposed token — must equal AXON_WEB_BROWSER_API_TOKEN when set, else AXON_WEB_API_TOKEN
# apiFetch() sends it as x-api-key on /api/*; use-axon-ws.ts sends it as ?token= on /ws
NEXT_PUBLIC_AXON_API_TOKEN=

AXON_WEB_ALLOWED_ORIGINS=
AXON_WEB_ALLOW_INSECURE_DEV=false

# Optional shell websocket auth/origin overrides
AXON_SHELL_WS_TOKEN=
AXON_SHELL_ALLOWED_ORIGINS=

# Optional client-side shell websocket token
NEXT_PUBLIC_SHELL_WS_TOKEN=

# Optional allowlist for Pulse chat --betas values
AXON_ALLOWED_CLAUDE_BETAS=interleaved-thinking
```

### Dev vs Container URL Resolution

The CLI auto-detects whether it's running inside Docker:
- **Inside Docker** (`/.dockerenv` exists): uses container-internal DNS (`axon-postgres:5432`, etc.)
- **Outside Docker** (local dev): rewrites to localhost with mapped ports (`127.0.0.1:53432`, etc.)

**So `.env` can use container DNS** — `normalize_local_service_url()` in `config.rs` handles translation transparently.

## Gotchas

### `--wait false` (default) = fire-and-forget
By default, `crawl`, `extract`, `embed`, and `ingest` enqueue jobs and return immediately. Use `--wait true` to block until completion. Without workers running, enqueued jobs will pend forever.

### `render-mode auto-switch`
The default mode. Runs an HTTP crawl first; if >60% of pages are thin (<200 chars) or total coverage is too low, automatically retries with Chrome. Chrome requires a running Chrome instance — if none is available, the HTTP result is kept.

### `crawl_raw()` vs `crawl()`
When Chrome feature is compiled in, `crawl()` expects a Chrome instance. `crawl_raw()` is pure HTTP and always works. `engine.rs` calls `crawl_raw()` for `RenderMode::Http` and `crawl()` for Chrome/AutoSwitch.

### `ask` LLM call pattern
`ask` constructs the URL as: `{OPENAI_BASE_URL}/chat/completions`
- **Correct:** `OPENAI_BASE_URL=http://host/v1`
- **Wrong:** `OPENAI_BASE_URL=http://host/v1/chat/completions` — double path

### TEI batch size / 413 handling
`tei_embed()` in `vector/ops/tei.rs` auto-splits batches on HTTP 413 (Payload Too Large). Set `TEI_MAX_CLIENT_BATCH_SIZE` env var to control default chunk size (default: 64, max: 128).

### TEI 429 / rate limiting
On HTTP 429 or 503, `tei_embed()` retries up to 5 times (default) with exponential backoff starting at 1s (1s, 2s, 4s, 8s, 16s) plus jitter. Override with `TEI_MAX_RETRIES` env var. Worst-case retry budget (~181s) fits inside the 300s doc timeout.

### Locale path prefix matching
`--exclude-path-prefix` (and the default locale list) treats both `/` and `-` as word boundaries. This means `/ja` blocks both `/ja/docs` and `/ja-jp/docs`. Pass `none` to disable all locale filtering.

### Text chunking
`chunk_text()` splits at 2000 chars with 200-char overlap. Each chunk = one Qdrant point. Very long pages produce many points.

### Thin page filtering
Pages with fewer than `--min-markdown-chars` (default: 200) are flagged as thin. If `--drop-thin-markdown true` (default), thin pages are skipped — not saved to disk or embedded.

### `readability: false` — do NOT change
`build_transform_config()` in `crates/core/content.rs` sets `readability: false`. Changing this to `true` causes Mozilla Readability to score VitePress/sidebar doc layouts as low-quality and strip them to just the page title — produces ~97% thin pages on most documentation sites. `main_content: true` handles structural extraction without the scoring penalty. This setting is the result of a confirmed production regression; do not "improve" it.

### Collection must exist before upsert
`ensure_collection()` does a GET first; only issues PUT on 404 (collection not found). This means it's safe on existing collections — no 409 Conflict. Safe to call on every embed.

### Sitemap backfill
After a crawl, `append_sitemap_backfill()` discovers URLs via sitemap.xml that the crawler missed and fetches them individually. Respects `--max-sitemaps` (default: 512) and `--include-subdomains`. Use `--sitemap-since-days N` to restrict backfill to URLs whose `<lastmod>` falls within the last N days; URLs without `<lastmod>` are always included.

### Docker build context
The `Dockerfile` builds from `docker/Dockerfile`. The build command inside the container is:

```bash
cargo build --release --bin axon
```

`docker-compose.yaml` sets `context: .` — run `docker compose build` from this directory, not from a parent workspace.

### `spider_agent` path dep (CI / fresh environments)

`Cargo.toml` uses `spider_agent = { path = "../spider/spider_agent", ... }` for local dev with a sibling `spider/` checkout. In CI or any environment without that sibling repo, switch to the registry version:

```toml
spider = { version = "2", default-features = false, features = [
    "basic", "chrome", "regex", "sitemap", "adblock",
    "chrome_stealth", "chrome_screenshot", "chrome_store_page",
    "chrome_headless_new", "chrome_simd",
    "simd", "inline-more", "cache_mem",
    "ua_generator", "headers", "glob", "time", "control",
    "firewall",
] }
spider_agent = { version = "2.45", default-features = false, features = ["search_tavily", "openai"] }
```

### Spider feature flags with observable behavior
- **`firewall`**: Blocks known-bad domains (malware, phishing, spam) before fetch via `spider_firewall` crate. Some URLs may be rejected that weren't before — this is defense-in-depth on top of `validate_url()`.
- **`chrome_headless_new`**: Uses `--headless=new` instead of legacy headless. Better DOM fidelity but slightly different rendering behavior on some sites.
- **`balance`**: NOT enabled — silently throttles concurrency with zero logging. We manage concurrency explicitly via performance profiles.
- Full flag inventory: [`docs/spider-feature-flags.md`](docs/spider-feature-flags.md)

### Subprocess stdout vs stderr
CLI commands output JSON data to stdout and progress/logs to stderr (Spinner via indicatif, tracing via `log_info`/`log_done`). The web UI streams both: stdout as `"type": "output"`, stderr as `"type": "log"`. ANSI codes stripped via `console::strip_ansi_codes()`.

### AMQP reconnect backoff
When a worker's AMQP channel dies (broker restart, consumer_timeout, network blip), the lane reconnects automatically with exponential backoff: starts at 2s, doubles each attempt, capped at 60s. On successful reconnect, the backoff resets to 2s **only if the connection was alive for >=60 seconds** (`ran_for_secs >= AMQP_RECONNECT_MAX_SECS` in `worker_lane.rs`). Short-lived connections that reconnect quickly retain their current backoff value. This prevents rapid reconnect loops from hammering the broker after a transient failure. The current job is not lost — it holds no AMQP reference and completes normally before the reconnect loop fires.

**Note:** The crawl worker's reconnect loop in `crawl/runtime/worker/loops.rs` has different semantics: it resets backoff to `RECONNECT_BACKOFF_INITIAL_SECS` (2s) on **every** successful reconnect (i.e., when `run_amqp_worker_lane` returns `Ok(())`), regardless of how long the connection was alive.

### Adding fields to `Config` struct
When adding a new non-`Option` field to `Config` in `crates/core/config.rs`, you **must** also update the inline `Config { .. }` struct literals used in test helpers:
- `crates/cli/commands/research.rs`
- `crates/cli/commands/search.rs`
- Any `make_test_config()` helpers in `crates/jobs/common/`

These are struct literals — the compiler will fail if a new field is missing, but only at test compilation time, not `cargo check`.

## Performance Profiles

Concurrency tuned relative to available CPU cores:

| Profile | Crawl concurrency | Sitemap concurrency | Backfill concurrency | Timeout | Retries | Backoff |
|---------|------------------|---------------------|----------------------|---------|---------|---------|
| `high-stable` (default) | CPUs×8 (64–192) | CPUs×12 (64–256) | CPUs×6 (32–128) | 20s | 2 | 250ms |
| `balanced` | CPUs×4 (32–96) | CPUs×6 (32–128) | CPUs×3 (16–64) | 30s | 2 | 300ms |
| `extreme` | CPUs×16 (128–384) | CPUs×20 (128–512) | CPUs×10 (64–256) | 15s | 1 | 100ms |
| `max` | CPUs×24 (256–1024) | CPUs×32 (256–1536) | CPUs×20 (128–1024) | 12s | 1 | 50ms |

## Development

### Build

```bash
cargo build --bin axon                          # debug
cargo build --release --bin axon                # release
cargo check                                     # fast type check
```

### Test

```bash
cargo test                    # run all tests
cargo test http               # SSRF / URL validation tests (21)
cargo test engine             # crawl engine tests (8)
cargo test chunk_text         # text chunking tests (7)
cargo test -- --nocapture     # show println! output
```

### Lint

```bash
cargo clippy
cargo fmt --check
```

### just (Recommended)

```bash
just verify      # fmt-check + clippy + check + test (pre-PR gate)
just fix         # cargo fmt + clippy --fix (auto-repair)
just precommit   # full pre-commit: monolith check + verify
just watch-check # cargo watch: check + test-lib on every file save
just rebuild     # check + test + docker-build (pre-deploy gate)
just up          # docker compose up -d --build
just down        # docker compose down
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

### Monolith Policy

Changed `.rs` files are enforced at CI and via lefthook pre-commit:
- File size: ≤ 500 lines (hard fail)
- Function size: warn at 80 lines, hard fail at 120 lines
- Exempt: `tests/**`, `benches/**`, `config/**`, `**/config.rs`
- Exceptions: add to `.monolith-allowlist`

```bash
./scripts/install-git-hooks.sh  # install lefthook once
```

### Diagnose service connectivity

```bash
axon doctor
```

Checks: Postgres, Redis, RabbitMQ, Qdrant, TEI, LLM endpoint reachability.

## Database Schema

Tables are auto-created via `ensure_schema()` in each `*_jobs.rs`. Full column detail: [`docs/schema.md`](docs/schema.md).

| Table | Key columns |
|-------|-------------|
| `axon_crawl_jobs` | `id`, `url`, `status`, `config_json`, `result_json` — index on `status` |
| `axon_extract_jobs` | `id`, `status`, `urls_json`, `config_json`, `result_json` |
| `axon_embed_jobs` | `id`, `status`, `input_text`, `config_json`, `result_json` |
| `axon_ingest_jobs` | `id`, `source_type`, `target`, `status`, `config_json`, `result_json` — partial index on pending |

All tables share: `created_at`, `updated_at`, `started_at`, `finished_at` (TIMESTAMPTZ), `error_text` (TEXT).

`axon_ingest_jobs` differs from the others: it uses `source_type` (`github`/`reddit`/`youtube`) + `target` instead of `url` or `urls_json` to identify the ingest target.

## Code Style

- Rust standard style — run `cargo fmt` before committing
- `cargo clippy` clean before committing
- Errors bubble via `Box<dyn Error>` at command boundaries; internal helpers return typed errors
- Structured log output via `log_info` / `log_warn` (not `println!` in library code)
- `--json` flag enables machine-readable output on all commands that print results

### Module Layout — Modern Rust Convention (ENFORCED)

**Never use `mod.rs`.** Use the Rust 2018+ file-per-module layout:

```plaintext
# WRONG — do not do this
foo/
└── mod.rs      ← forbidden

# CORRECT
foo.rs          ← module root lives here
foo/
├── bar.rs      ← submodule
└── baz.rs      ← submodule
```

- Module root always lives in `foo.rs`, never `foo/mod.rs`
- Submodules live in `foo/bar.rs`, declared with `mod bar;` inside `foo.rs`
- When splitting an existing `foo/mod.rs`: copy it to `foo.rs`, delete `foo/mod.rs` — the submodule files stay in `foo/` unchanged
- This applies everywhere: `crates/`, `crates/*/`, nested modules — no exceptions
