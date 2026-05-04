# axon_cli — Axon CLI (Rust + Spider.rs)
Last Modified: 2026-04-27

Web crawl, scrape, extract, embed, and query — all in one binary backed by a self-hosted RAG stack.

## Quick Start

> **Lite mode (default)**: axon requires only Qdrant and TEI. Jobs are stored in SQLite and workers run in-process inside the same tokio runtime.

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
| `watch <sub>` | Scheduled task management: `create`, `list`, `get`, `update`, `run-now`, `pause`, `resume`, `delete`, `history`, `artifacts`. | Depends |
| `migrate --from <src> --to <dst>` | Copy all points from an unnamed-vector collection to a new named-mode collection (dense + bm42 sparse), enabling RRF hybrid search. No re-embedding needed. | No |

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
| `--qdrant-url <url>` | string | `QDRANT_URL` | `http://127.0.0.1:53333` |
| `--tei-url <url>` | string | `TEI_URL` | *(empty)* |
| `--openai-base-url <url>` | string | `OPENAI_BASE_URL` | *(empty)* |
| `--openai-api-key <key>` | string | `OPENAI_API_KEY` | *(empty)* |
| `--openai-model <name>` | string | `OPENAI_MODEL` | *(empty)* |

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
  - Hybrid search: new collections use named `dense` + `bm42` sparse vectors with Reciprocal Rank Fusion (RRF) via Qdrant `/query` when hybrid search is active; falls back to dense-only when the sparse query is empty or hybrid is disabled. Legacy collections use dense-only. See `crates/vector/CLAUDE.md`.
- Services layer (services-first contract) — see `crates/services/CLAUDE.md`:
  - `crates/services/` — typed entry points consumed by both CLI handlers and MCP/web routes
  - CLI commands call `crates/services::{query,retrieve,ask,sources,domains,stats,system}` — **not** raw `run_*_native()` functions (those public call-site entry points are removed from the API surface; callers must go through the services layer)
  - Each service function returns a typed result struct (defined in `crates/services/types/service.rs`) — no raw JSON printing or stdout side-effects
  - MCP handlers and web routes call the same service functions, mapping typed results to wire format
  - ACP orchestration lives in `crates/services/acp/` (session lifecycle, permission bridge, adapter subprocess)
  - ACP-backed LLM completions (fire-and-forget, pre-warmed) live in `crates/services/acp_llm/` — used by ask synthesis, research, extract fallback; see `docs/ACP.md` for full protocol reference
- MCP server:
  - `crates/mcp/` (schema, server routing, handler modules, config)
  - Single `axon` tool with `action`/`subaction` routing

## Infrastructure

### Docker Compose

The stack uses a single compose file for infrastructure services shared on the `axon` bridge network:

| File | Contents | Env file |
|------|----------|----------|
| `docker-compose.services.yaml` | Infrastructure (qdrant, chrome, TEI) | `services.env` |
| `docker-compose.gpu.yaml` | GPU override — NVIDIA reservations for `axon-tei` and `axon-ollama` | *(none)* |

**GPU acceleration:** On NVIDIA hosts, layer the GPU override on top of the services file:
```bash
docker compose -f docker-compose.services.yaml -f docker-compose.gpu.yaml up -d
```
CPU-only hosts use `docker-compose.services.yaml` alone — no GPU block, no startup failure.

### Infrastructure Services (`docker-compose.services.yaml`)

| Service | Image | Exposed Port | Purpose |
|---------|-------|-------------|---------|
| `axon-qdrant` | qdrant/qdrant:v1.13.1 | `53333`, `53334` (gRPC) | Vector store |
| `axon-tei` | ghcr.io/huggingface/text-embeddings-inference:latest | `52000` | Embedding generation (GPU, NVIDIA) |
| `axon-chrome` | built from docker/chrome/Dockerfile | `6000` (management), `9222` (CDP proxy) | headless_browser + chrome-headless-shell |

```bash
# Start infrastructure (qdrant, tei, chrome)
just services-up
# or: docker compose -f docker-compose.services.yaml up -d

# Check infra health
docker compose -f docker-compose.services.yaml ps

# Stop everything
just down-all
```

## Configuration (Two-Layer System)

Axon uses two configuration layers:

| Layer | File | Purpose | Secrets? |
|-------|------|---------|---------|
| Tuning knobs | `~/.axon/config.toml` | Search params, worker limits, TEI settings | No — safe to commit |
| URLs + secrets | `.env` | Service URLs, API keys, passwords | Yes — never commit |

**Priority:** CLI flags > env vars > `~/.axon/config.toml` > built-in defaults.

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

`.env` is for **URLs and secrets only** (v0.36+). Tuning params live in `~/.axon/config.toml`.

Copy `.env.example` → `.env`, then fill in values:

```bash
# Data root on host
AXON_DATA_DIR=/home/yourname/appdata

# Qdrant
QDRANT_URL=http://axon-qdrant:6333

# TEI embeddings (on axon network — container DNS)
TEI_URL=http://axon-tei:80

# LLM / ACP completion settings
# ACP adapter is required for ask/evaluate/suggest/extract fallback/debug/research synthesis.
AXON_ACP_ADAPTER_CMD=codex
AXON_ACP_ADAPTER_ARGS=
# OPENAI_MODEL is used as ACP model override (compatibility key name retained).
OPENAI_BASE_URL=http://YOUR_LLM_HOST/v1
OPENAI_API_KEY=your-key-or-empty
OPENAI_MODEL=your-model-name

# CDP endpoint for headless_browser (axon-chrome management API)
AXON_CHROME_REMOTE_URL=http://axon-chrome:6000

# Qdrant collection (default: cortex)
AXON_COLLECTION=cortex

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

### MCP Security Env

MCP OAuth (`atk_` tokens) is the auth system for MCP clients:

```bash
# MCP allowed origins (comma-separated)
AXON_MCP_ALLOWED_ORIGINS=
```

## Lite Mode (`AXON_LITE=1`)

Lite mode is the default operating mode. Jobs are stored in SQLite and workers run in-process inside the same tokio runtime. Only Qdrant and TEI are required as external services.

```bash
AXON_LITE=1 axon scrape https://example.com   # no external services needed
# or
axon --lite scrape https://example.com
```

**What works in lite mode:** scrape, crawl (sync), map, embed, query, ask, extract, ingest, search, research, sources, stats, doctor, MCP server.

**Unsupported in lite mode:** watch scheduler.

```bash
# Env vars for lite mode
AXON_LITE=1                              # enable lite mode
AXON_SQLITE_PATH=/path/to/jobs.db        # optional; default: $AXON_DATA_DIR/axon/jobs.db
```

The `ServiceContext` (in `crates/services/context.rs`) is constructed at startup and carries a `ServiceCapabilities` struct that gates unsupported operations. MCP handlers check `ctx.capabilities.<cap>.supported` before executing.

See `crates/jobs/CLAUDE.md` for the `JobBackend` trait and backend selection details.

## Gotchas

### `--wait false` (default) = fire-and-forget
By default, `crawl`, `extract`, `embed`, and `ingest` enqueue jobs and return immediately. Use `--wait true` to block until completion. Without workers running, enqueued jobs will pend forever.

### `render-mode auto-switch`
The default mode. Runs an HTTP crawl first; if >60% of pages are thin (<200 chars) or total coverage is too low, automatically retries with Chrome. Chrome requires a running Chrome instance — if none is available, the HTTP result is kept.

### `crawl_raw()` vs `crawl()`
When Chrome feature is compiled in, `crawl()` expects a Chrome instance. `crawl_raw()` is pure HTTP and always works. `engine.rs` calls `crawl_raw()` for `RenderMode::Http` and `crawl()` for Chrome/AutoSwitch.

### ACP-backed completion path
`ask`, `evaluate`, `suggest`, extract fallback, `debug`, and research synthesis run through ACP (`AXON_ACP_ADAPTER_CMD`).
`OPENAI_MODEL` remains the model override knob for ACP-backed calls.

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
`build_transform_config()` in `crates/core/content.rs` sets `readability: false`. Changing this to `true` causes Mozilla Readability to score VitePress/sidebar doc layouts as low-quality and strip them to just the page title — produces ~97% thin pages on most documentation sites. `main_content: true` handles structural extraction without the scoring penalty. This setting is the result of a confirmed production regression; do not "improve" it.

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

### Docker build context
The `Dockerfile` builds from `docker/Dockerfile`. The build command inside the container is:

```bash
cargo build --release --bin axon
```

Both compose files set `context: .` — run `docker compose build` from this directory, not from a parent workspace.

### `spider_agent` path dep (CI / fresh environments)

`Cargo.toml` uses `spider_agent = { path = "../spider/spider_agent", ... }` for local dev with a sibling `spider/` checkout. In CI or any environment without that sibling repo, switch to the registry version:

```toml
spider = { version = "2", default-features = false, features = [
    "basic", "chrome", "regex", "sitemap", "adblock",
    "chrome_stealth", "chrome_screenshot", "chrome_store_page",
    "chrome_headless_new", "chrome_simd",
    "simd", "inline-more", "cache_mem",
    "ua_generator", "headers", "time", "control",
    "firewall",
] }
spider_agent = { version = "2.45", default-features = false, features = ["search_tavily", "openai"] }
```

### Spider feature flags with observable behavior
- **`firewall`**: Blocks known-bad domains (malware, phishing, spam) before fetch via `spider_firewall` crate. Some URLs may be rejected that weren't before — this is defense-in-depth on top of `validate_url()`.
- **`chrome_headless_new`**: Uses `--headless=new` instead of legacy headless. Better DOM fidelity but slightly different rendering behavior on some sites.
- **`balance`**: NOT enabled — silently throttles concurrency with zero logging. We manage concurrency explicitly via performance profiles.
- **`glob`**: NOT enabled — glob URL patterns (`{a,b}`, `[0-9]`) change `crawl_establish` to use `is_allowed()` (budget-aware) instead of `is_allowed_default()`. With `with_limit(1)`, the budget check immediately returns `BudgetExceeded` for the FIRST URL, producing 0 pages from Chrome crawls. axon doesn't use URL glob patterns in its CLI, so this feature is excluded. Do NOT add it back.
- Full flag inventory: [`docs/SPIDER-FEATURE-FLAGS.md`](docs/SPIDER-FEATURE-FLAGS.md)

### Subprocess stdout vs stderr
CLI commands output JSON data to stdout and progress/logs to stderr (Spinner via indicatif, tracing via `log_info`/`log_done`). The web UI streams both: stdout as `"type": "output"`, stderr as `"type": "log"`. ANSI codes stripped via `console::strip_ansi_codes()`.

### Crawl queue cap (`AXON_MAX_PENDING_CRAWL_JOBS`)
New crawl job submissions check the count of pending jobs before inserting. If the count is ≥ `AXON_MAX_PENDING_CRAWL_JOBS` (default 100, 0 = unlimited), the submission is rejected with a human-readable error. Set to 0 to disable. Implemented in `crates/jobs/crawl/runtime/db.rs` via `check_pending_cap()`.

### Crawl size warning (`AXON_CRAWL_SIZE_WARN_THRESHOLD`)
After an uncapped crawl completes (`--max-pages 0`, the default), if the total pages crawled exceeds `AXON_CRAWL_SIZE_WARN_THRESHOLD` (default 10,000), a warning is logged suggesting the user add `--max-pages`. Set to 0 to disable the warning.

### Auto path-prefix scoping
When crawling a URL with ≥2 path segments and no explicit `--url-whitelist`, the crawl is automatically scoped to the directory subtree of the start URL via a derived whitelist regex. For example, crawling `https://ai.google.dev/api/python/google/generativeai/GenerativeModel` auto-scopes to `^https?://ai\.google\.dev/api/python/google/generativeai(/|$)`. Root paths (`/`) and single-segment paths (`/docs`) are not scoped — they're already broad enough. Pass `--url-whitelist <pattern>` to override auto-scoping.

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
just services-up # start infra (qdrant, tei, chrome)
just services-down # stop infra
just down-all    # stop everything
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

Tables are auto-created via `ensure_schema()` in each `*_jobs.rs`. Schema lives in SQLite (lite mode).

| Table | Key columns |
|-------|-------------|
| `axon_crawl_jobs` | `id`, `url`, `status`, `config_json`, `result_json` — index on `status` |
| `axon_extract_jobs` | `id`, `status`, `urls_json`, `config_json`, `result_json` |
| `axon_embed_jobs` | `id`, `status`, `input_text`, `config_json`, `result_json` |
| `axon_ingest_jobs` | `id`, `source_type`, `target`, `status`, `config_json`, `result_json` — partial index on pending |

All tables share: `created_at`, `updated_at`, `started_at`, `finished_at`, `error_text`.

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