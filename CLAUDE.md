# Axon CLI (Rust + Spider.rs)
Last Modified: 2026-05-16

Web crawl, scrape, extract, embed, and query — all in one binary backed by a self-hosted RAG stack.

## Quick Start

> **SQLite/in-process jobs are the runtime.** axon requires only Qdrant and TEI. Jobs are stored in SQLite and workers run in-process inside the same tokio runtime.

```bash
# Recommended: use the wrapper script (auto-sources .env)
./scripts/axon doctor
./scripts/axon scrape https://example.com --wait true

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
| `ask <question>` | RAG: search + LLM answer. | No |
| `summarize <url>...` | Scrape URL content and summarize it with the configured LLM | No |
| `diff <url-a> <url-b>` | Compare two URLs, show content/metadata/link changes | No |
| `brand <url>` | Extract brand identity: colors, fonts, logos, favicon | No |
| `evaluate <question>` | RAG vs baseline + independent LLM judge (accuracy, relevance, completeness, specificity, verdict) | No |
| `suggest [focus]` | Suggest new docs URLs to crawl | No |
| `ingest <target>` | Ingest external source (GitHub repo, GitLab project URL, Gitea/Forgejo repo, generic HTTPS Git repo, Reddit subreddit/thread, YouTube video/playlist/channel) — auto-detects source type from target where possible. Git providers: source code indexed by default; use `--no-source` to skip. | Yes (default) |
| `sessions [format]` | Ingest AI session exports (Claude/Codex/Gemini) into Qdrant | No |
| `sources` | List all indexed URLs + chunk counts | No |
| `domains` | List indexed domains + stats | No |
| `stats` | Qdrant collection stats | No |
| `status` | Show async job queue status | No |
| `doctor` | Diagnose service connectivity | No |
| `debug` | Run doctor + LLM-assisted troubleshooting | No |
| `mcp` | Start MCP stdio/HTTP server | No |
| `serve` | Start the unified HTTP server (web panel, MCP HTTP, `/v1/ask`, `/v1/actions`, in-process workers) | No |
| `setup` | First-run local setup wrapper plus SSH target helper | No |
| `screenshot <url>` | Capture a full-page screenshot via headless Chrome | No |
| `dedupe` | Deduplicate near-identical chunks within a Qdrant collection | No |
| `completions <shell>` | Emit shell completion scripts | No |
| `watch <sub>` | Scheduled task management. SQLite-backed implementations: `create`, `list`, `run-now`, `history`. Schema-defined but not yet implemented: `get`, `update`, `pause`, `resume`, `delete`, `artifacts`. | Depends |
| `migrate --from <src> --to <dst>` | Copy all points from an unnamed-vector collection to a new named-mode collection (dense + bm42 sparse), enabling RRF hybrid search. No re-embedding needed. | No |
| `config <sub>` | Read/write entries in `~/.axon/.env` and `~/.axon/config.toml`. Subcommands: `list`, `get`, `set`, `unset`, `path`. Auto-routes by key shape (UPPER_SNAKE → .env, dotted lowercase → config.toml) with `--env`/`--toml` overrides. Secrets are redacted by default; pass `--reveal` to show them. | No |

### Job Subcommands (for crawl / extract / embed / ingest / sessions)

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
| `--collection <name>` | string | `axon` | Qdrant collection name. Also settable via `AXON_COLLECTION` env var. |
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
| `--backfill-concurrency-limit <n>` | usize | *profile* | Override sitemap backfill concurrency. |
| `--request-timeout-ms <ms>` | u64 | *profile* | Per-request timeout in milliseconds. |
| `--fetch-retries <n>` | usize | *profile* | Number of retries on failed fetches. |
| `--retry-backoff-ms <ms>` | u64 | *profile* | Backoff between retries in milliseconds. |

#### Service URLs (override env vars)

| Flag | Type | Env Var | Fallback |
|------|------|---------|----------|
| `--qdrant-url <url>` | string | `QDRANT_URL` | `http://127.0.0.1:53333` |
| `--tei-url <url>` | string | `TEI_URL` | *(empty)* |

## Architecture

Canonical architecture and data-flow diagrams live in `docs/ARCHITECTURE.md`.

High-level subsystem map:

- Entrypoint and dispatch:
  - `main.rs` loads environment and calls `axon::run()`
  - `lib.rs` owns `run`/`run_once` and command dispatch
- Command + config:
  - `src/cli/*` command handlers
  - `src/core/config/{cli,parse,types}.rs` flag/env parsing and runtime config resolution
- Crawl + content:
  - `src/crawl/engine.rs` (collector pipeline runs antibot detect, structured-data pass, DOM ladder before commit)
  - `src/core/http.rs` and `src/core/content.rs` (including `extract_ladder.rs` retry strategy)
- Vertical extractors:
  - `src/extract/` — per-site extractor framework (registry + 13 verticals: github_repo, pypi, npm, crates_io, reddit, etc.) — see `src/extract/CLAUDE.md`
  - Auto-routed from `services::scrape::scrape` via `dispatch_by_url()` when `cfg.enable_verticals = true` (default on)
- Async jobs:
  - `src/jobs/runtime.rs` + `src/jobs/` (SQLite-backed enqueue/query/store/cancel)
  - `src/jobs/workers.rs` + `src/jobs/workers/runners/{crawl,embed,extract,ingest}.rs` (in-process worker lanes)
  - `src/jobs/{crawl,embed,extract,ingest}.rs` (per-family job payload + dispatch helpers)
  - `src/jobs/crawl/` (manifest, processor, repo, sitemap, watchdog support)
  - `src/jobs/watch.rs` (recurring task scheduler — list/create/run-now/history)
  - `src/jobs/backend.rs` (`JobBackend` trait + `SqliteJobBackend` only)
  - job states in `src/jobs/status.rs`
- Vector + RAG:
  - `src/vector/ops/*` (TEI embedding, Qdrant upsert/search, ask/evaluate/query)
  - Hybrid search: new collections use named `dense` + `bm42` sparse vectors with Reciprocal Rank Fusion (RRF) via Qdrant `/query` when hybrid search is active; falls back to dense-only when the sparse query is empty or hybrid is disabled. Legacy collections use dense-only. See `src/vector/CLAUDE.md`.
- Services layer (services-first contract) — see `src/services/CLAUDE.md`:
  - `src/services/` — typed entry points consumed by both CLI handlers and MCP/web routes
  - CLI commands call `src/services::{query,retrieve,ask,summarize,sources,domains,stats,system}` — **not** raw `run_*_native()` functions (those public call-site entry points are removed from the API surface; callers must go through the services layer)
  - Each service function returns a typed result struct (defined in `src/services/types/service.rs`) — no raw JSON printing or stdout side-effects
  - MCP handlers and web routes call the same service functions, mapping typed results to wire format
  - Gemini headless LLM completions live in `src/services/llm_backend/` — used by ask synthesis, summarize, research, evaluate, suggest, debug, and extract fallback
- MCP server:
  - `src/mcp/` (schema, server routing, handler modules, config)
  - Single `axon` tool with `action`/`subaction` routing

## Infrastructure

### Docker Compose

The production stack and local development stack are split:

| File | Contents | Env file |
|------|----------|----------|
| `docker-compose.prod.yaml` | Axon server, Qdrant, Chrome, TEI | `~/.axon/.env` |
| `docker-compose.yaml` | Local dev stack; extends production services and runs `axon` from the bind-mounted local debug binary in `target/debug` | `~/.axon/.env` |

**GPU acceleration:** On NVIDIA hosts, `docker-compose.prod.yaml` includes NVIDIA reservations for `axon-tei`.

```bash
docker compose --env-file ~/.axon/.env -f docker-compose.prod.yaml up -d
```

For local dev:

```bash
cargo build --bin axon
docker compose --env-file ~/.axon/.env -f docker-compose.yaml up -d axon
```

CPU-only hosts should override the TEI image/settings or run an external TEI endpoint.

### Infrastructure Services (`docker-compose.prod.yaml`)

| Service | Image | Exposed Port | Purpose |
|---------|-------|-------------|---------|
| `axon-qdrant` | qdrant/qdrant:v1.13.1 | `53333`, `53334` (gRPC) | Vector store |
| `axon-tei` | ghcr.io/huggingface/text-embeddings-inference:latest | `52000` | Embedding generation (GPU, NVIDIA) |
| `axon-chrome` | built from config/chrome/Dockerfile | `6000` (management), `9222` (CDP proxy) | headless_browser + chrome-headless-shell |

```bash
# Start infrastructure (qdrant, tei, chrome)
just services-up
# or: docker compose --env-file ~/.axon/.env -f docker-compose.yaml up -d axon-qdrant axon-tei axon-chrome

# Check infra health
docker compose --env-file ~/.axon/.env ps

# Stop everything
just services-down
```

## Configuration (Two-Layer System)

Axon uses two configuration layers, both rooted under `~/.axon/`:

| Layer | File | Purpose | Secrets? |
|-------|------|---------|---------|
| Tuning knobs | `~/.axon/config.toml` | Search params, worker limits, TEI settings (also settable via env vars — env wins) | No — safe to commit |
| URLs + secrets | `~/.axon/.env` (auto-loaded) or repo `.env` | Service URLs, API keys, passwords | Yes — never commit |

**Priority:** CLI flags > env vars > `~/.axon/config.toml` > built-in defaults.

`~/.axon/` is the canonical home for axon's persistent data — `jobs.db`, `output/`, `logs/`, `artifacts/`, `screenshots/`, and `chrome-diagnostics/` all live flat under it. `AXON_DATA_DIR` defaults to `~/.axon` (no nested `axon/` subdirectory). See `docs/CONFIG.md` for the full directory tree.

**Migration from `~/.local/share/axon`:** axon does NOT auto-migrate. Either move the directory yourself (`mv ~/.local/share/axon ~/.axon`) or set `AXON_DATA_DIR=~/.local/share` to pin the old location. Tuning knobs that were previously env-only are now also accepted in `~/.axon/config.toml`.

```bash
# Set up config.toml (optional — defaults are sensible)
mkdir -m 700 ~/.axon
cp config.example.toml ~/.axon/config.toml
chmod 600 ~/.axon/config.toml

# Override config path
AXON_CONFIG_PATH=/path/to/config.toml axon ask "..."

# Malformed config.toml = hard fail with file path + line number
# Missing config.toml = silent, uses defaults
```

See `config.example.toml` at the repo root for all supported keys with defaults and docs. See `docs/CONFIG.md` for the full environment variable reference.

## Environment Variables

`.env` is primarily for service URLs, API keys, and secrets. Tuning params (search, TEI, workers) can live in either `~/.axon/config.toml` **or** as env vars — env vars always win over TOML.

Copy `.env.example` → `.env`, then fill in values:

```bash
# Data root on host
AXON_DATA_DIR=

# Qdrant
QDRANT_URL=http://axon-qdrant:6333

# TEI embeddings (on axon network — container DNS)
TEI_URL=http://axon-tei:80

# LLM / Gemini headless completion settings
# Gemini CLI is required for ask/summarize/evaluate/suggest/extract fallback/debug/research synthesis.
AXON_HEADLESS_GEMINI_CMD=gemini
AXON_HEADLESS_GEMINI_HOME=
AXON_HEADLESS_GEMINI_MODEL=
AXON_LLM_COMPLETION_CONCURRENCY=4
AXON_LLM_COMPLETION_TIMEOUT_SECS=300

# CDP endpoint for headless_browser (axon-chrome management API)
AXON_CHROME_REMOTE_URL=http://axon-chrome:6000

# Qdrant collection (default: axon)
AXON_COLLECTION=axon

# Search and research (required for search/research commands)
TAVILY_API_KEY=your-tavily-api-key

# Ingest credentials (Reddit required; Git providers optional for private repos and higher rate limits)
GITHUB_TOKEN=                       # optional — raises GitHub rate limits
GITLAB_TOKEN=                       # optional — private GitLab projects / higher rate limits
GITEA_TOKEN=                        # optional — private Gitea/Forgejo repos / higher rate limits
REDDIT_CLIENT_ID=                   # required for Reddit ingest targets
REDDIT_CLIENT_SECRET=               # required for Reddit ingest targets

# Worker tuning (optional, defaults shown)
AXON_INGEST_LANES=2                 # parallel ingest worker lanes
AXON_EMBED_DOC_TIMEOUT_SECS=300     # per-document embed timeout
AXON_JOB_STALE_TIMEOUT_SECS=300    # seconds before a running job is considered stale
AXON_JOB_STALE_CONFIRM_SECS=60     # additional grace period before stale reclaim
```

### MCP Security Env

MCP HTTP auth is selected at startup:
- `AXON_MCP_AUTH_MODE=oauth` enables the lab-auth Google OAuth/JWT flow and mounts `/.well-known/*`, `/authorize`, `/token`, `/register`, and related routes.
- `AXON_MCP_HTTP_TOKEN` enables static bearer auth and also remains accepted in OAuth dual-mode.
- OAuth email allowlisting is the access boundary. Allowed OAuth users receive full Axon server access; newly issued OAuth tokens default to both `axon:read` and `axon:write`, and either Axon scope is accepted for all Axon read/write routes for compatibility with existing tokens.
- Tokenless HTTP is allowed only for loopback development binds; non-loopback binds require either OAuth mode or a static token.

```bash
# Static bearer token accepted as Authorization: Bearer ... or x-api-key
AXON_MCP_HTTP_TOKEN=

# OAuth mode (optional; HTTP transport only)
AXON_MCP_AUTH_MODE=oauth
AXON_MCP_PUBLIC_URL=https://axon.example.com
AXON_MCP_GOOGLE_CLIENT_ID=
AXON_MCP_GOOGLE_CLIENT_SECRET=
AXON_MCP_AUTH_ADMIN_EMAIL=
AXON_MCP_AUTH_ALLOWED_REDIRECT_URIS=

# MCP allowed origins (comma-separated)
AXON_MCP_ALLOWED_ORIGINS=
```

## Runtime Mode

Jobs are stored in SQLite and workers run in-process inside the same tokio runtime. Only Qdrant and TEI are required as external services. The legacy Postgres/Redis/RabbitMQ/AMQP path has been removed.

```bash
axon scrape https://example.com           # SQLite/in-process runtime (only mode)
```

**Supported commands:** scrape, summarize, diff, brand, crawl (sync + async), map, embed, query, ask, evaluate, suggest, retrieve, extract, ingest, sessions, search, research, sources, domains, stats, status, doctor, debug, dedupe, screenshot, migrate, MCP server, serve.

**Watch scheduler:** `watch list`, `watch create`, `watch run-now`, and `watch history` are wired through `src/services/watch.rs` → `src/jobs/watch.rs` and work today. `watch get`, `watch update`, `watch pause`, `watch resume`, `watch delete`, and `watch artifacts` parse but are not yet implemented.

```bash
# Env vars for runtime tuning
AXON_SQLITE_PATH=/path/to/jobs.db        # optional; default: $AXON_DATA_DIR/jobs.db (i.e. ~/.axon/jobs.db)
```

The `ServiceContext` (in `src/services/context.rs`) is constructed at startup and carries `cfg: Arc<Config>` plus `jobs: Arc<dyn ServiceJobRuntime>`. CLI fire-and-forget callers use `ServiceContext::new(cfg)` (no in-process workers); long-running services (`serve`, `mcp`, sync `--wait true` paths) use `ServiceContext::new_with_workers(cfg)`.

See `src/jobs/CLAUDE.md` for the `JobBackend` trait and `SqliteJobBackend` details, and `src/services/CLAUDE.md` for the `ServiceJobRuntime` abstraction.

## Gotchas

### `scrape` auto-routes to vertical extractors
With `cfg.enable_verticals = true` (the default), `services::scrape::scrape` calls `src/extract::dispatch_by_url()` before the generic HTTP path. Any URL matching a registered vertical (github_repo, pypi, npm, etc. — see `src/extract/CLAUDE.md`) returns a richer `ScrapedDoc` with `extractor_name`/`extractor_version` payload fields, not the raw HTML→markdown output. Disable in `~/.axon/config.toml` with `enable_verticals = false` for A/B comparison or to force the generic path. The MCP `vertical_scrape` action is **discovery-only** (`list`/`capabilities`); `subaction=run` was removed in favor of routing through `scrape`.

### `--wait false` (default) = fire-and-forget
By default, `crawl`, `extract`, `embed`, and `ingest` enqueue jobs and return immediately. Use `--wait true` to block until completion. Without workers running, enqueued jobs will pend forever.

### `render-mode auto-switch`
The default mode. Runs an HTTP crawl first; if >60% of pages are thin (<200 chars) or total coverage is too low, automatically retries with Chrome. Chrome requires a running Chrome instance — if none is available, the HTTP result is kept.

### `crawl_raw()` vs `crawl()`
When Chrome feature is compiled in, `crawl()` expects a Chrome instance. `crawl_raw()` is pure HTTP and always works. `engine.rs` calls `crawl_raw()` for `RenderMode::Http` and `crawl()` for Chrome/AutoSwitch.

### Gemini headless completion path
All LLM operations — `ask`, `summarize`, `evaluate`, `suggest`, `extract` LLM fallback, `debug`, and `research` synthesis — run through the Gemini CLI headless path (`AXON_HEADLESS_GEMINI_CMD`). Deterministic and vertical extractors in `src/extract/` and `src/core/content/deterministic.rs` run pure Rust without LLM calls; Gemini is invoked only when deterministic extraction yields nothing (the LLM fallback path). `AXON_HEADLESS_GEMINI_MODEL` is the model override knob. The legacy `OPENAI_BASE_URL` / `OPENAI_API_KEY` / `OPENAI_MODEL` env vars and the `--openai-*` CLI flags were removed in 3.0.0.

### TEI batch size / 413 handling
`tei_embed()` in `vector/ops/tei.rs` auto-splits batches on HTTP 413 (Payload Too Large). Set `TEI_MAX_CLIENT_BATCH_SIZE` env var to control default chunk size (default: 64, max: 128).

### TEI retries
On HTTP 429, any 5xx status, transport errors, or response decode failures, `tei_embed()` makes up to 5 attempts (1 initial + 4 retries) with exponential backoff starting at 1s (1s, 2s, 4s, 8s) plus jitter (up to 500ms each). Override with `TEI_MAX_RETRIES` env var. Worst-case retry budget: 4 backoff sleeps (15s) + 5 request timeouts (5x30s=150s) + jitter (2s) = ~167s, well inside the 300s doc timeout.

### Locale path prefix matching
`--exclude-path-prefix` (and the default locale list) treats both `/` and `-` as word boundaries. This means `/ja` blocks both `/ja/docs` and `/ja-jp/docs`. Pass `none` to disable all locale filtering.

### Text chunking
`chunk_text()` splits at 2000 chars with 200-char overlap. Each chunk = one Qdrant point. Very long pages produce many points.

### Thin page filtering
Pages with fewer than `--min-markdown-chars` (default: 200) are flagged as thin. If `--drop-thin-markdown true` (default), thin pages are skipped — not saved to disk or embedded.

### `readability: false` — do NOT change
`build_transform_config()` in `src/core/content.rs` sets `readability: false`. Changing this to `true` causes Mozilla Readability to score VitePress/sidebar doc layouts as low-quality and strip them to just the page title — produces ~97% thin pages on most documentation sites. `main_content: true` handles structural extraction without the scoring penalty. This setting is the result of a confirmed production regression; do not "improve" it.

### Collection must exist before upsert
`ensure_collection()` does a GET first; only issues PUT on 404 (collection not found). This means it's safe on existing collections — no 409 Conflict. Safe to call on every embed.

### `migrate` — one-time collection upgrade
`axon migrate --from cortex --to cortex_v2` scrolls all points from the source, computes BM42 sparse vectors locally from `chunk_text` payload fields (no TEI calls), and upserts named-mode points to the destination. After migration, set `AXON_COLLECTION=cortex_v2` in `.env`.

- Source must be an **unnamed** collection (`"vectors": {"size": N}` schema); named collections are rejected with a clear error.
- Destination is created automatically if it doesn't exist; if it already exists as a named collection, migration is idempotent (re-runs upsert existing points with fresh sparse vectors).
- Progress is logged every 100 pages (~25,600 points). At 256 points/page over 2.57M points, expect 1–2 hours.
- The scroll loop uses the raw Qdrant `/points/scroll` API directly (not the shared `qdrant_scroll_pages_while` helper) to enable async upserts after each page.

**After migration, restart all worker processes.** The process-wide VectorMode cache is not invalidated on migration — workers that embedded to the source collection before migration will retain stale `Unnamed` mode in memory and fall back to dense-only search even for the new named-mode destination collection.

### Sitemap backfill
After a crawl, `append_sitemap_backfill()` discovers URLs via sitemap.xml that the crawler missed and fetches them individually. Respects `--max-sitemaps` (default: 512) and `--include-subdomains`. Use `--sitemap-since-days N` to restrict backfill to URLs whose `<lastmod>` falls within the last N days; URLs without `<lastmod>` are always included.


The compose file sets `context: .` — run `docker compose build` from this directory, not from a parent workspace.

### `spider_agent` path dep (CI / fresh environments)

`Cargo.toml` uses `spider_agent = { path = "../spider/spider_agent", ... }` for local dev with a sibling `spider/` checkout. In CI or any environment without that sibling repo, switch to the registry version:

```toml
spider = { version = "2", default-features = false, features = [
    "basic", "chrome", "regex", "sitemap", "adblock",
    "chrome_stealth", "chrome_screenshot", "chrome_store_page",
    "chrome_headless_new", "chrome_simd",
    "simd", "inline-more", "cache_mem",
    "ua_generator", "headers", "time", "control",
] }
spider_agent = { version = "2.45", default-features = false, features = ["search_tavily", "openai"] }
```

### Spider feature flags with observable behavior
- **`firewall`**: NOT enabled — `spider_firewall`'s build.rs fetches blocklists from `api.github.com` unauthenticated and panics when GitHub rate-limits the CI runner. It doesn't read `GITHUB_TOKEN`, so external auth isn't possible. `validate_url()` in `src/core/http/ssrf.rs` remains the primary SSRF guard; this was defense-in-depth on top. Re-enable when upstream supports an auth knob.
- **`chrome_headless_new`**: Uses `--headless=new` instead of legacy headless. Better DOM fidelity but slightly different rendering behavior on some sites.
- **`balance`**: NOT enabled — silently throttles concurrency with zero logging. We manage concurrency explicitly via performance profiles.
- **`glob`**: NOT enabled — glob URL patterns (`{a,b}`, `[0-9]`) change `crawl_establish` to use `is_allowed()` (budget-aware) instead of `is_allowed_default()`. With `with_limit(1)`, the budget check immediately returns `BudgetExceeded` for the FIRST URL, producing 0 pages from Chrome crawls. axon doesn't use URL glob patterns in its CLI, so this feature is excluded. Do NOT add it back.
- Full flag inventory: [`docs/SPIDER-FEATURE-FLAGS.md`](docs/SPIDER-FEATURE-FLAGS.md)

### Subprocess stdout vs stderr
CLI commands output JSON data to stdout and progress/logs to stderr (Spinner via indicatif, tracing via `log_info`/`log_done`). Keep this split intact so server-mode and MCP callers can safely parse command output.

### Crawl queue cap (`AXON_MAX_PENDING_CRAWL_JOBS`)
New crawl job submissions check the count of pending jobs before inserting. If the count is ≥ `AXON_MAX_PENDING_CRAWL_JOBS` (default 100, 0 = unlimited), the submission is rejected with a human-readable error. Set to 0 to disable. Implemented in `src/jobs/ops/enqueue.rs` via `check_pending_cap_for()`.

### Auto path-prefix scoping
When crawling a URL with ≥2 path segments and no explicit `--url-whitelist`, the crawl is automatically scoped to the directory subtree of the start URL via a derived whitelist regex. For example, crawling `https://ai.google.dev/api/python/google/generativeai/GenerativeModel` auto-scopes to `^https?://ai\.google\.dev/api/python/google/generativeai(/|$)`. Root paths (`/`) and single-segment paths (`/docs`) are not scoped — they're already broad enough. Pass `--url-whitelist <pattern>` to override auto-scoping.

### Adding fields to `Config` struct
When adding a new non-`Option` field to `Config` in `src/core/config/types/config.rs`, you **must** also update the inline `Config { .. }` struct literals used in test helpers:
- `src/cli/commands/research.rs`
- `src/cli/commands/search.rs`
- Any `make_test_config()` helpers in `src/jobs/common/`

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
just rebuild     # check + test
just services-up # start infra (qdrant, tei, chrome)
just services-down # stop infra
just stop        # stop running mcp and worker processes
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

Checks: Qdrant, TEI, LLM endpoint reachability.

## Database Schema

Tables are auto-created via `ensure_schema()` in each `*_jobs.rs`. Schema lives in SQLite.

| Table | Key columns |
|-------|-------------|
| `axon_crawl_jobs` | `id`, `url`, `status`, `config_json`, `result_json` — index on `status` |
| `axon_extract_jobs` | `id`, `status`, `urls_json`, `config_json`, `result_json` |
| `axon_embed_jobs` | `id`, `status`, `input_text`, `config_json`, `result_json` |
| `axon_ingest_jobs` | `id`, `source_type`, `target`, `status`, `config_json`, `result_json` — partial index on pending |

All tables share: `created_at`, `updated_at`, `started_at`, `finished_at`, `error_text`.

`axon_ingest_jobs` differs from the others: it uses `source_type` (`github`/`gitlab`/`gitea`/`git`/`reddit`/`youtube`) + `target` instead of `url` or `urls_json` to identify the ingest target.

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
- This applies everywhere: `src/`, `src/*/`, nested modules — no exceptions

### Test files — sidecar `_tests.rs` convention (ENFORCED)

**Tests live in sibling files**, not inline `#[cfg(test)] mod tests { ... }` blocks. For each source file with tests, create a sibling `_tests.rs` file and declare it inside the source with the `#[path]` attribute:

```plaintext
foo.rs          ← source code
foo_tests.rs    ← sidecar test file (one per original `#[cfg(test)] mod X` block)
```

In `foo.rs`:

```rust
#[cfg(test)]
#[path = "foo_tests.rs"]
mod tests;
```

In `foo_tests.rs`:

```rust
use super::*;  // tests still see foo.rs's private items

#[test]
fn it_works() { ... }
```

**Rules:**

- **One sidecar per original `#[cfg(test)] mod X { ... }` block.** Never wrap multiple blocks under a single `mod tests` — this breaks `cargo test foo::<orig_mod_name>::test_x` selectors and risks visibility escalation. If `foo.rs` had `mod tests`, `mod legacy`, and `mod proptest_tests`, emit three sidecar files: `foo_tests.rs`, `foo_legacy_tests.rs`, `foo_proptest_tests.rs`, with three matching `#[path]` declarations in `foo.rs`.
- **Source-side `mod` name must match the original block's mod name** (`mod legacy`, `mod proptest_tests`, not always `mod tests`). Test selectors stay identical to pre-migration.
- **Why `#[path]`?** It decouples disk location from module hierarchy. The file is a sibling of `foo.rs` on disk, but the module is a **child** of `foo`, so `use super::*;` keeps private-item access. A sibling-declared `mod foo_tests;` (without `#[path]`) would make `foo_tests` a sibling of `foo` in the module tree and lose private access.
- **Compound cfg gates carry over.** A source with `#[cfg(all(test, unix))]` becomes:

  ```rust
  #[cfg(all(test, unix))]
  #[path = "foo_tests.rs"]
  mod tests;
  ```

  The sidecar inherits the parent's gate; do not re-gate items inside it.

- **`mod test_support;` and other non-`#[cfg(test)]` helper modules are NOT sidecars** — they stay declared as regular submodules with their files in `foo/`.
- **Footgun.** If a sidecar `foo_tests.rs` itself declares `mod bar;` (without `#[path]`), rustc resolves `bar` relative to the sidecar's on-disk location and looks for `foo_tests/bar.rs`, *not* `foo/bar.rs`. Inline the submodule or pass an explicit `#[path]` from the sidecar.
- **Monolith policy.** `**/*_tests.*` is exempt from the 500-line cap — sidecars can hold large test suites without splitting.
- **No `xtask` CI guardrail** for inline-test regressions; the convention is enforced by docs + reviewer attention. The pre-commit `test` hook runs `cargo test --no-run --workspace --lib --locked` which compiles every sidecar — broken `#[path]` strings fail there. Do not rely on `cargo check` alone; it skips `cfg(test)` modules and will pass a misnamed `#[path]`.
- **`#[cfg(test)] impl` blocks stay in the source file.** Inherent impls of a parent type can't move to a sibling module (orphan rules apply to traits, but inherent impls must live with the type). If you have `#[cfg(test)] impl Foo { fn test_only_helper() {} }` in the source, leave it inline.
- **Block-scoped `use` semantics shift.** Inside an inline `mod tests { ... }`, `use super::X;` and similar imports are scoped to the block. After moving to a sidecar, those imports become file-scoped (still inside the same module, but visible to every test in the file). Always use `use super::*;` in sidecars — it keeps private-item access and matches the sidecar convention.
- **Directory-split footgun.** If `foo.rs` later splits into `foo/sub.rs`-style submodules (and the source moves into `foo/`), the `#[path = "foo_tests.rs"]` string is now relative to the new source's directory, not the old one. Move the `_tests.rs` files to match, or update the `#[path]` to the correct relative location. Mitigated by the test-compile gate above, but watch for it during structural refactors.

Worked examples in the repo: `src/cli/commands/mcp.rs` + `src/cli/commands/mcp_tests.rs` (single block), `src/ingest/sessions.rs` + `src/ingest/sessions_tests.rs` + `src/ingest/sessions_decode_tests.rs` (multi-block).

## Worktrees

- Use `.worktrees/` under the repository root for all future git worktrees for this repo.
- Do not create sibling worktrees under `/home/jmagar/workspace/` for new Axon work.
- Before switching branches for PR or stack work, check `git worktree list` and reuse an existing `.worktrees/<branch>` checkout when present.

<!-- BEGIN BEADS INTEGRATION v:1 profile:minimal hash:ca08a54f -->
## Beads Issue Tracker

This project uses **bd (beads)** for issue tracking. Run `bd prime` to see full workflow context and commands.

### Quick Reference

```bash
bd ready              # Find available work
bd show <id>          # View issue details
bd update <id> --claim  # Claim work
bd close <id>         # Complete work
```

### Rules

- Use `bd` for ALL task tracking — do NOT use TodoWrite, TaskCreate, or markdown TODO lists
- Run `bd prime` for detailed command reference and session close protocol
- Use `bd remember` for persistent knowledge — do NOT use MEMORY.md files

## Session Completion

**When ending a work session**, you MUST complete ALL steps below. Work is NOT complete until `git push` succeeds.

**MANDATORY WORKFLOW:**

1. **File issues for remaining work** - Create issues for anything that needs follow-up
2. **Run quality gates** (if code changed) - Tests, linters, builds
3. **Update issue status** - Close finished work, update in-progress items
4. **PUSH TO REMOTE** - This is MANDATORY:
   ```bash
   git pull --rebase
   bd dolt push
   git push
   git status  # MUST show "up to date with origin"
   ```
5. **Clean up** - Clear stashes, prune remote branches
6. **Verify** - All changes committed AND pushed
7. **Hand off** - Provide context for next session

**CRITICAL RULES:**
- Work is NOT complete until `git push` succeeds
- NEVER stop before pushing - that leaves work stranded locally
- NEVER say "ready to push when you are" - YOU must push
- If push fails, resolve and retry until it succeeds
<!-- END BEADS INTEGRATION -->


## Version Bumping

**Every feature branch push MUST bump the version in ALL version-bearing files.**

Bump type is determined by the commit message prefix:
- `feat!:` or `BREAKING CHANGE` → **major** (X+1.0.0)
- `feat` or `feat(...)` → **minor** (X.Y+1.0)
- Everything else (`fix`, `chore`, `refactor`, `test`, `docs`, etc.) → **patch** (X.Y.Z+1)

**Files to update (if they exist in this repo):**
- `Cargo.toml` — `version = "X.Y.Z"` in `[package]`
- `package.json` — `"version": "X.Y.Z"`
- `pyproject.toml` — `version = "X.Y.Z"` in `[project]`
- `.claude-plugin/plugin.json` — `"version": "X.Y.Z"`
- `.codex-plugin/plugin.json` — `"version": "X.Y.Z"`
- `gemini-extension.json` — `"version": "X.Y.Z"`
- `README.md` — version badge or header
- `CHANGELOG.md` — new entry under the bumped version

All files MUST have the same version. Never bump only one file.
CHANGELOG.md must have an entry for every version bump.
