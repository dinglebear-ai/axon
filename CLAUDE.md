# Axon CLI (Rust + Spider.rs)
Last Modified: 2026-06-26

Web crawl, scrape, extract, embed, and query — all in one binary backed by a self-hosted RAG stack.

## Long-Lived Branches

- `marketplace-no-mcp` is an intentional long-lived marketplace variant branch,
  not stale cleanup. It keeps Axon's plugin/skill surface available while
  removing bundled MCP server registration for environments where Axon is
  already connected through the Labby gateway.
- Do not merge `marketplace-no-mcp` into `main` by default, and do not delete it
  as stale unless Jacob explicitly retires the no-MCP marketplace variant.

## Verification Scope Guard

- Before running tests, builds, CI reruns, or other expensive validation, classify
  the changed-file surface and pick the smallest check that proves the change.
- `.agents/skills/**`, `docs/sessions/**`, and prose-only docs changes do not
  justify full Rust, web, Android, Docker, or release CI. Use structural checks
  instead, such as file counts, required `SKILL.md` presence, symlink checks, or
  targeted formatter/parser validation for the files touched.
- Run broader tests only when code, workflow, schema, generated artifacts,
  package manifests, or runtime configuration changed, or when Jacob explicitly
  asks for a build/test/CI pass.
- If a workflow skill asks for baseline tests but the change is non-code, state
  the narrower verification choice and why. Do not spend minutes compiling Axon
  just to prove copied agent skills exist.

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
- `docs/reference/mcp/overview.md` (runtime/design guide)
- `docs/reference/mcp/tool-schema.md` (wire contract schema source of truth)

## Commands

| Command | Purpose | Async? |
|---------|---------|--------|
| `scrape <url>...` | Scrape one or more URLs to markdown | No |
| `crawl <url>...` | Full site crawl for one or more start URLs | Yes (default) |
| `map <url>` | Discover all URLs without scraping | No |
| `extract <urls...>` | LLM-powered structured data extraction | Yes (default) |
| `search <query>` | Web search (SearXNG when `AXON_SEARXNG_URL` set, else Tavily), auto-queues crawl jobs for results | No |
| `research <query>` | Web research with LLM synthesis. Backend: SearXNG when `AXON_SEARXNG_URL` set, else Tavily. Synthesizes over **full-page content** of the top sources (`AXON_RESEARCH_FULL_CONTENT=false` for snippet-only/fast), then auto-queues bounded crawl/index jobs for result URLs. | No |
| `embed [input]` | Embed file/dir/URL into Qdrant; existing local paths attach foreground local code-index refresh progress by default (`--no-watch` keeps one-shot local embedding) | Yes (default) |
| `fresh <sub>` | CLI-only freshness schedules created by `--fresh <Nd>` on scrape/crawl/embed/ingest. Subcommands: `list`, `run-now`, `history`. | No |
| `memory <sub>` | Persistent agent memory: `remember`, `search`, `show`, `link`, `supersede`, `context` backed by Qdrant content + SQLite metadata/edges | No |
| `query <text>` | Semantic vector search | No |
| `code-search <text>` | Local Git checkout semantic code search with foreground freshness | No |
| `retrieve <url>` | Fetch stored document chunks from Qdrant | No |
| `ask <question>` | RAG: search + LLM answer. | No |
| `summarize <url>...` | Scrape URL content and summarize it with the configured LLM | No |
| `diff <url-a> <url-b>` | Compare two URLs, show content/metadata/link changes | No |
| `brand <url>` | Extract brand identity: colors, fonts, logos, favicon | No |
| `evaluate <question>` | RAG vs baseline + independent LLM judge (accuracy, relevance, completeness, specificity, verdict) | No |
| `suggest [focus]` | Suggest new docs URLs to crawl | No |
| `ingest <target>` | Ingest external source (GitHub repo, GitLab project URL, Gitea/Forgejo repo, generic HTTPS Git repo, Reddit subreddit/thread, YouTube video/playlist/channel, RSS/Atom/JSON feed) — auto-detects source type from target where possible (feeds match `.rss`/`.atom`/`.rdf` extensions, `feed`/`rss`/`atom` path segments, `?feed=` queries, or an explicit `rss:`/`feed:`/`atom:` prefix). One document is embedded per feed entry (HTML → markdown). Git providers: source code indexed by default; use `--no-source` to skip. | Yes (default) |
| `sessions [format]` | Ingest AI session exports (Claude/Codex/Gemini) into Qdrant | No |
| `sources` | List all indexed URLs + chunk counts | No |
| `domains` | List indexed domains + stats | No |
| `stats` | Qdrant collection stats | No |
| `purge <url>` | Delete indexed Qdrant points by URL/seed URL. Use `--prefix` for a whole docs subtree/origin and `--dry-run` to preview matches. Alias: `delete-url`. | No |
| `status` | Show async job queue status | No |
| `doctor` | Diagnose service connectivity | No |
| `debug` | Run doctor + LLM-assisted troubleshooting | No |
| `mcp` | Start MCP stdio/HTTP server | No |
| `serve` | Start the unified HTTP server (web panel, MCP HTTP, `/v1/ask`, `/v1/actions`, in-process workers) | No |
| `setup` | First-run local setup wrapper plus SSH target helper | No |
| `screenshot <url>` | Capture a full-page screenshot via headless Chrome | No |
| `dedupe` | Deduplicate near-identical chunks within a Qdrant collection | No |
| `refresh [filter]` | Re-enqueue crawl/ingest jobs for previously indexed origins (full docs refresh). Facets the collection on the `seed_url` payload field, classifies each distinct origin (web URL → crawl, ingest target → ingest, sessions/non-URL → skipped), prints a plan, and confirms before enqueuing (respects `--yes`/non-TTY). Optional `filter` narrows by `source_type` (e.g. `github`) or a `seed_url` substring (e.g. a domain). Each origin re-enqueues with the **original job's stored config snapshot** (depth, page caps, scoping, headers) when one exists in the jobs DB — collection and service endpoints always follow the current process config. Exits nonzero when any origin fails to enqueue (e.g. pending-cap hit). Bounded by `AXON_REFRESH_FACET_LIMIT` (default 10,000). Only content indexed with `seed_url` participates. | No |
| `completions <shell>` | Emit shell completion scripts | No |
| `watch <sub>` | URL change-detection scheduler. A `watch` (task_type `watch`, the only supported type) diffs each URL against a stored snapshot every tick (conditional probe + `compute_diff` + `ignore_patterns` + threshold), summarizes meaningful changes via the LLM, records `url-change` artifacts, and enqueues clustered depth-bounded crawls (skipping in-flight clusters). SQLite-backed implementations: `create`, `list`, `run-now`, `history`. Schema-defined but not yet implemented: `get`, `update`, `pause`, `resume`, `delete`, `artifacts`. | Depends |
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
| `--fresh <Nd>` | duration | — | CLI-only schedule creation for `scrape`, `crawl`, `embed`, and `ingest`. Whole days only, `1d` through `366d`. |

#### Crawl & Scrape

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--max-pages <n>` | u32 | `2000` for `crawl`, `1` for omitted `extract` | Page cap. Set `0` explicitly for an uncapped crawl. Uncapped crawls require an explicit `--budget`/`--url-whitelist` scope or `AXON_ALLOW_UNBOUNDED_BROAD_CRAWL=true`. |
| `--max-depth <n>` | usize | `10` | Maximum crawl depth from start URL. |
| `--budget <PATH=N>` | string | — | Per-path page cap (repeatable), e.g. `--budget /blog=100 --budget '*=1000'`. `*` applies to all paths. Unset = no budget. Wired to spider's `with_budget`. |
| `--etag-conditional` | flag | `false` | Conditional re-crawl: seed spider's ETag cache from a persisted `etag.json` sidecar so unchanged pages return 304 and are reused from the previous run instead of re-fetched. Independent of `--cache`. 304 skips are reconciled back into the manifest as `changed=false` entries, gated on spider's visited set so deleted/undiscovered pages are not resurrected. |
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
| `--warc <PATH>` | path | — | Write every fetched page of a crawl to a WARC 1.1 archive at this path. HTTP and Chrome render paths both archive (spider `warc` feature). Crawl path only; round-trips through the crawl job config snapshot. |
| `--automation-script <PATH>` | path | — | JSON file mapping URL path prefixes → ordered Chrome web-automation steps (`click`/`click_all`/`scroll_x`/`scroll_y`/`infinite_scroll`/`wait`/`wait_for`/`wait_for_and_click`/`wait_for_navigation`/`fill`/`evaluate`/`screenshot`). Steps run against each matching page before capture. **Requires a Chrome render path** (`--render-mode chrome`/`auto-switch`); ignored with a warning on HTTP-only. See `src/crawl/automation.rs`. |

Memory safety:
- Crawl page bodies are capped at 4 MiB by default (set `scrape.max_page_bytes` in `~/.axon/config.toml` to override; `0` disables — there is no CLI flag).
- Long-running crawls self-abort when Axon's RSS reaches `AXON_CRAWL_MEMORY_ABORT_PERCENT` of the host/cgroup memory limit (the lower of the two, so inside a container the denominator is the cgroup cap, not host RAM); default is `85`, and `0` disables the guard. Linux-only — the guard never trips on other platforms.

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

Canonical architecture and data-flow diagrams live in `docs/architecture/overview.md`.

### Workspace layout (Rust crates)

The product is a Cargo workspace of 13 library/adapter crates under `crates/`,
consumed by the thin `axon` binary at the repo root (`src/main.rs` +
`src/lib.rs`, which re-exports `axon_cli::run`). Each crate keeps its module
guide in `crates/<crate>/src/CLAUDE.md`. All crates inherit the product version
via `[workspace.package] version` (so `CARGO_PKG_VERSION` tracks releases).

Dependency layering (lower → higher; no cycles):

```
axon-api      (transport-neutral DTOs: results, diff, mcp_schema, job_dto, service_job, job_status)
axon-authz    (scope checking)
   ↓
axon-core     (config, http/SSRF, content/chunking, llm, paths, artifacts, events, redact)
   ↓
axon-crawl · axon-vector · axon-ingest · axon-extract · axon-code-index
   ↓                        (DOMAIN crates: own their logic + a typed public service entry)
axon-jobs     (SQLite job runtime + in-process workers; constructs axon_api::ServiceJob)
   ↓
axon-services (composition + job-runtime entry points + a re-export FACADE — NOT a mandatory reimplementation hop)
   ↓
axon-mcp · axon-web   (siblings; the unified `serve` bootstrap lives in axon-cli)
   ↓
axon-cli      (argv dispatch + all command handlers)
   ↓
axon (binary) (main.rs + build.rs web-asset embed)
```

**Crate ownership rule (read before adding an operation):** own the contract where
the data lives — single-domain logic in its domain crate, the `*Result` DTO in
`axon-api`, `axon-services` as a thin facade; only cross-domain or job-runtime
work lives *in* `axon-services`. Transports never import a domain crate's
internal `::ops::*` modules. Canonical doc:
[`docs/architecture/crate-ownership.md`](docs/architecture/crate-ownership.md);
enforced by `cargo xtask check-layering`.

High-level subsystem map (paths are `crates/<crate>/src/...`):

- Entrypoint and dispatch:
  - `src/main.rs` loads environment and calls `axon::run()` (re-exports `axon_cli::run`)
  - `crates/axon-cli/src/lib.rs` owns `run`/`run_once` and command dispatch
- Command + config:
  - `crates/axon-cli/src/commands/*` command handlers
  - `crates/axon-core/src/config/{cli,parse,types}.rs` flag/env parsing and runtime config resolution
- Crawl + content:
  - `crates/axon-crawl/src/engine.rs` (collector pipeline runs antibot detect, structured-data pass, DOM ladder before commit)
  - `crates/axon-core/src/http.rs` and `crates/axon-core/src/content.rs` (including `extract_ladder.rs` retry strategy)
- Vertical extractors:
  - `crates/axon-extract/src/` — per-site extractor framework (registry + verticals) + `scrape`/`sync` — see `crates/axon-extract/src/CLAUDE.md`
  - Auto-routed from `services::scrape::scrape` via `dispatch_by_url()` when `cfg.enable_verticals = true` (default on)
- Async jobs:
  - `crates/axon-jobs/src/runtime.rs` + the crate (SQLite-backed enqueue/query/store/cancel)
  - `crates/axon-jobs/src/workers.rs` + `workers/runners/{crawl,embed,extract,ingest}.rs` (in-process worker lanes)
  - `crates/axon-jobs/src/{crawl,embed,extract,ingest}.rs` (per-family job payload + dispatch helpers)
  - `crates/axon-jobs/src/watch.rs` (recurring task scheduler) + `workers/watch_scheduler.rs`
  - `crates/axon-jobs/src/backend.rs` (`JobBackend` trait + `SqliteJobBackend`); `JobStatus`/`ServiceJob` now live in `axon-api`
  - migrations in `crates/axon-jobs/src/migrations`
- Vector + RAG:
  - `crates/axon-vector/src/ops/*` (TEI embedding, Qdrant upsert/search, ask/evaluate/query)
  - Hybrid search: new collections use named `dense` + `bm42` sparse vectors with RRF. See `crates/axon-vector/src/CLAUDE.md`.
- Services layer (services-first contract) — see `crates/axon-services/src/CLAUDE.md`:
  - `crates/axon-services/src/` — typed entry points consumed by CLI handlers and MCP/web routes
  - Each service function returns a typed result struct — no raw JSON printing or stdout side-effects
  - Gemini headless LLM completions live in `crates/axon-core/src/llm/`
- MCP server:
  - `crates/axon-mcp/src/` (server routing, handler modules, auth); wire-contract DTOs in `axon-api::mcp_schema`
  - Single `axon` tool with `action`/`subaction` routing
- HTTP server (`axon serve`):
  - `crates/axon-web/src/` — Axum router, auth, health, first-run + stack panel UI, security headers
  - The unified web+MCP `serve` bootstrap (`run_unified_server`) lives in `crates/axon-cli/src/commands/unified_server.rs` (the only layer depending on both web and mcp)

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

`~/.axon/` is the canonical home for axon's persistent data — `jobs.db`, `output/`, `logs/`, `artifacts/`, `screenshots/`, and `chrome-diagnostics/` all live flat under it. `AXON_DATA_DIR` defaults to `~/.axon` (no nested `axon/` subdirectory). See `docs/guides/configuration.md` for the full directory tree.

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

See `config.example.toml` at the repo root for all supported keys with defaults and docs. See `docs/guides/configuration.md` for the full environment variable reference.

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

# LLM completion backend selection
# AXON_LLM_BACKEND selects the synthesis path for ask/summarize/evaluate/suggest/
# extract fallback/debug/research. Supported backends:
#   gemini-headless (default; also accepts "gemini"/"headless"/empty) — Gemini CLI
#   openai-compat — any OpenAI-compatible /v1/chat/completions endpoint
#   codex-app-server — Codex CLI app-server over stdio with isolated CODEX_HOME
AXON_LLM_BACKEND=gemini-headless
AXON_LLM_COMPLETION_CONCURRENCY=4
AXON_LLM_COMPLETION_TIMEOUT_SECS=300

# Gemini headless backend (used when AXON_LLM_BACKEND=gemini-headless)
AXON_HEADLESS_GEMINI_CMD=gemini
AXON_HEADLESS_GEMINI_HOME=
AXON_SYNTHESIS_HEADLESS_GEMINI_MODEL=
# Legacy alias still accepted:
# AXON_HEADLESS_GEMINI_MODEL=

# OpenAI-compatible backend (used when AXON_LLM_BACKEND=openai-compat)
# BASE_URL is the API root (axon appends /chat/completions itself — do NOT include
# it; include /v1 if the endpoint serves /v1/chat/completions).
AXON_OPENAI_BASE_URL=
AXON_SYNTHESIS_OPENAI_MODEL=
# Legacy alias still accepted:
# AXON_OPENAI_MODEL=
AXON_OPENAI_API_KEY=

# Codex app-server backend (used when AXON_LLM_BACKEND=codex-app-server)
# Spawns `codex app-server` per completion; not an OpenAI-compatible endpoint
# and not the desktop Unix socket transport.
AXON_CODEX_CMD=codex
AXON_CODEX_HOME=
AXON_SYNTHESIS_CODEX_MODEL=
# Legacy alias still accepted:
# AXON_CODEX_MODEL=
AXON_CODEX_COMPLETION_CONCURRENCY=1
# Opt-in: load the user's real Codex config (MCP servers, skills, hooks) by
# running against the real CODEX_HOME + inherited env instead of the isolated
# stripped home. Default false; surrenders synthesis isolation.
AXON_CODEX_LOAD_USER_CONFIG=false

# CDP endpoint for headless_browser (axon-chrome management API)
AXON_CHROME_REMOTE_URL=http://axon-chrome:6000

# Qdrant collection (default: axon)
AXON_COLLECTION=axon

# Ask-retrieval recall knobs (hybrid RRF prefetch window + dual-embedding)
# AXON_HYBRID_CANDIDATES controls the per-arm (dense + sparse) prefetch window
# before RRF fusion for `query`; AXON_ASK_HYBRID_CANDIDATES overrides it for the
# `ask` path only, defaulting wider to preserve recall before LLM synthesis.
# Both clamp to 10–500. Authoritative defaults in src/vector/CLAUDE.md.
AXON_HYBRID_CANDIDATES=100
AXON_ASK_HYBRID_CANDIDATES=150
# Dual-embedding (ask only): when the keyword form of the question (3+ non-stopword
# tokens, differing from the trimmed NL question) is distinct, axon embeds BOTH the
# NL form and the keyword form in a single TEI batch and dispatches them to Qdrant in
# parallel, merging by (url, chunk-prefix). The NL form gets QUERY_INSTRUCTION; the
# keyword form does NOT (keyword tokens are document-shaped). Short/single-keyword/
# already-keyword-shaped questions skip the second dispatch. No env knob — gated by
# query shape. See "Dual-Embedding for Ask" in src/vector/CLAUDE.md.
#
# Synthesis context-window capability is resolved from config (the active LLM
# backend + its configured model): when the synthesis model is treated as
# high-context, the ask path raises its full-doc context floor (e.g. a minimum of
# 4 full docs on adaptive/complex queries). Resolved in
# src/vector/ops/commands/ask/context.rs::high_context_synthesis_model from
# cfg.llm_backend + the configured model name.
# TODO: a parallel change is converting this to an explicit config field; once
# landed, document the exact config/env knob name here instead of the model-family
# resolution described above.

# Search and research. Provide a SearXNG instance OR a Tavily key.
# When AXON_SEARXNG_URL is set, search/research use SearXNG (JSON format must be
# enabled in its settings.yml); otherwise they fall back to Tavily.
AXON_SEARXNG_URL=                       # e.g. https://searx.example.com
TAVILY_API_KEY=your-tavily-api-key
# research synthesizes over full page content by default; set false for snippet-only.
AXON_RESEARCH_FULL_CONTENT=true

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
AXON_WATCH_TICK_SECS=15            # watch scheduler sweep interval (min 1)
AXON_WATCH_LEASE_SECS=300          # watch lease TTL; must exceed one run's wall time (min 1)
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

**Watch scheduler:** `watch list`, `watch create`, `watch run-now`, and `watch history` are wired through `src/services/watch.rs` → `src/jobs/watch.rs` and work today. A `watch` task (the only supported `task_type`) is a **URL change detector**: each run probes/scrapes every URL, filters noise (`ignore_patterns`), reuses `services::diff::compute_diff` against the stored `axon_watch_url_state` snapshot, applies a meaningfulness threshold, summarizes real changes via the Gemini `core/llm` backend, writes `url-change` artifacts, and enqueues one crawl per common-prefix cluster (in-flight-guarded). The change-detection logic lives in `src/jobs/watch/{change_detect,cluster,dispatch,filter,report,url_state,orchestrate}.rs` with the conditional probe in `src/core/http/conditional.rs`. Enabled watches also **fire automatically**: the in-process scheduler loop in `src/jobs/workers/watch_scheduler.rs` (spawned by `spawn_workers`, so active under `axon serve`/`axon mcp`) leases due watches (`next_run_at <= now`) each `AXON_WATCH_TICK_SECS` and runs them, advancing `next_run_at` by `every_seconds`. `watch get`, `watch update`, `watch pause`, `watch resume`, `watch delete`, and `watch artifacts` parse but are not yet implemented.

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

### LLM completion backend (`AXON_LLM_BACKEND`)
All LLM operations — `ask`, `summarize`, `evaluate`, `suggest`, `extract` LLM fallback, `debug`, and `research` synthesis — run through the backend selected by `AXON_LLM_BACKEND`, dispatched in `src/core/llm.rs` on `LlmBackendKind`:
- **`gemini-headless`** (default; also accepts `gemini`/`headless`/empty) — Gemini CLI headless path (`AXON_HEADLESS_GEMINI_CMD`, synthesis model override `AXON_SYNTHESIS_HEADLESS_GEMINI_MODEL`; legacy alias `AXON_HEADLESS_GEMINI_MODEL`). Implemented in `src/core/llm/headless/` Gemini dispatch.
- **`openai-compat`** — any OpenAI-compatible endpoint (`src/core/llm/openai_compat.rs`). Requires `AXON_OPENAI_BASE_URL` (the API root — the code appends `/chat/completions` and errors if you include it; include `/v1` when the endpoint serves `/v1/chat/completions`) and `AXON_SYNTHESIS_OPENAI_MODEL` (legacy alias `AXON_OPENAI_MODEL`); `AXON_OPENAI_API_KEY` is optional (sent as a bearer token when set). llama.cpp and proxy endpoints both work.
- **`codex-app-server`** — spawns `codex app-server` through the Codex CLI over stdio with an isolated throwaway `CODEX_HOME` and rehomed `HOME`/XDG directories. Configure with `AXON_CODEX_CMD`, optional `AXON_CODEX_HOME`, optional `AXON_SYNTHESIS_CODEX_MODEL`, and `AXON_CODEX_COMPLETION_CONCURRENCY` (default `1`). This is not an OpenAI-compatible HTTP endpoint and does not connect to the desktop Unix socket in this slice. **Escape hatch:** `AXON_CODEX_LOAD_USER_CONFIG=true` (default `false`) runs the child against the **real** `CODEX_HOME` with the full inherited environment so MCP servers, skills, and hooks load — surrendering the isolation above. Implemented as the passthrough branch in `src/core/llm/codex_app_server.rs` (`spawn_codex_child_passthrough`). **In-container:** the production image installs `@openai/codex` (`config/Dockerfile`), so the backend works in the container against the container's **fresh** codex home (no host servers/skills) — fast by default, MCP-capable only when the container's own codex home is configured. There is no longer a host-only restriction. Persistence: `docker-compose.prod.yaml` sets `CODEX_HOME=/home/axon/.axon/codex` so the container's codex home (config.toml/MCP, auth.json, refreshed tokens, sessions) persists inside the already-mounted `~/.axon`, fresh by default and separate from the host `~/.codex`. Seed once with `CODEX_HOME=~/.axon/codex codex login` on the host (or copy `~/.codex/auth.json` into `~/.axon/codex/`); `OPENAI_API_KEY` works without seeding.

Deterministic and vertical extractors in `src/extract/` and `src/core/content/deterministic.rs` run pure Rust without LLM calls; the LLM is invoked only when deterministic extraction yields nothing (the fallback path). The legacy un-prefixed `OPENAI_BASE_URL` / `OPENAI_MODEL` env vars and the `--openai-*` CLI flags were removed in 3.0.0 and replaced by the `AXON_LLM_BACKEND` + `AXON_OPENAI_*` / `AXON_SYNTHESIS_*` scheme above. Bare `OPENAI_API_KEY` is still ignored by Axon config, but the Codex app-server backend may forward it only to the isolated child process as an optional auth fallback.

### TEI batch size / 413 handling
`tei_embed()` in `vector/ops/tei.rs` auto-splits batches on HTTP 413 (Payload Too Large). Set `TEI_MAX_CLIENT_BATCH_SIZE` env var to control default chunk size (default: 64, max: 128).

### TEI retries
On **429 and any 5xx**, `tei_embed()` makes up to **6 attempts** (1 initial + 5 retries) with exponential backoff starting at 1s (1s, 2s, 4s, 8s, 16s) plus jitter (up to 500ms each). Override with `TEI_MAX_RETRIES` env var. Worst-case retry budget: 5 backoff sleeps (31s) + 6 request timeouts (6x30s=180s) + jitter (2.5s) = ~213s, well inside the 300s doc timeout.

### Locale path prefix matching
`--exclude-path-prefix` (and the default locale list) treats both `/` and `-` as word boundaries. This means `/ja` blocks both `/ja/docs` and `/ja-jp/docs`. Pass `none` to disable all locale filtering.

### Text chunking
`chunk_text()` splits at 2000 chars with 200-char overlap. Each chunk = one Qdrant point. Very long pages produce many points.

### Local directory embed (`axon embed <dir>`)
Reads files via `read_inputs()`/`collect_embed_files()` in `src/vector/ops/tei/prepare.rs`: **recursive** walk that prunes VCS/dependency/build dirs (`.git`, `node_modules`, `target`, `dist`, `.venv`, … — see `src/vector/ops/input/select.rs`), skips known-binary extensions, and **skips (does not abort on) files that fail UTF-8 decode**. Chunking is per-file by extension: local source files with a tree-sitter grammar (`rs`, `py`, `js`, `jsx`, `ts`, `tsx`, `go`, `sh`, `bash`) get AST-aware `chunk_code` (`content_type = "text"`); everything else uses `chunk_markdown`/`chunk_text` (`content_type = "markdown"`). Crawl-output directories (those with a `manifest.jsonl`) carry http URLs and stay on prose chunking — the `changed == false` skip and structured-payload reconstruction are preserved. The MCP/REST server validator (`src/services/embed.rs`) enumerates the same selected set (shared `select::*` predicates) and layers its server-only sandbox (allowed roots, symlink/secret/size rejection) on top; that sandbox is intentionally NOT applied on the CLI path. The validator prunes **before** its symlink check — symlinks inside pruned dirs (`node_modules/.bin/*` is symlinks by design) do not fail validation because the reader never visits them. Symlink policy is POSIX-style: a symlink named explicitly as the embed target is followed (CLI only; the server rejects a symlinked root), while symlinks encountered during traversal are always skipped. Two more reader rules: files over **10 MB** are skipped with a `skip_oversized_file` warning on directory walks (hard error when explicitly named — `MAX_LOCAL_EMBED_FILE_BYTES`, matching the server validator default), and a **path-shaped input that doesn't resolve is a hard error**, never free-text. The CLI runs local-path embeds **in-process even without `--wait`** — the shared jobs DB is also polled by the axon container's workers, which cannot see host paths like `/home/<user>/...`; before this rule a fire-and-forget host-path embed was claimed by the container and "successfully" embedded the literal path string as content.

### GitHub stale cleanup only sees schema-v7+ points
`qdrant_delete_stale_repo_file_urls` (run after a full-source GitHub ingest) filters on `git_provider`/`git_owner`/`git_repo`/`git_content_kind` — payload fields that exist only on chunks indexed at payload schema v7 (5.5.0) or later. File chunks indexed **before** v7 never match the filter: files deleted upstream keep their pre-v7 chunks in the index until the repo's URLs are re-indexed at v7+ (one full re-ingest upserts over live URLs by deterministic point ID and makes subsequent cleanups authoritative). Same-target ingest jobs are serialized at claim time (`src/jobs/ops/lifecycle.rs`) so one job's cleanup cannot race a concurrent ingest of the same repo.

### `seed_url` origin tracking (distinct from `url`)
Every chunk payload carries `seed_url` — the **origin** that started its acquisition — alongside the chunk's own page `url`. It is stamped once in `src/vector/ops/tei/pipeline.rs` from `cfg.seed_url`, which the job runners set: the crawl runner (`jobs/workers/runners/crawl.rs`) sets it to the crawl start URL (propagated to the downstream embed job via the config snapshot); the ingest runner (`jobs/workers/runners/ingest.rs`) and sync ingest set it to the re-ingestable target (`owner/repo`, `r/rust`, …). When `cfg.seed_url` is unset (direct `embed`/`scrape`), the pipeline falls back to the doc's own `url`. `seed_url` is an indexed keyword (faceted), introduced at payload schema **v5** (the current `PAYLOAD_SCHEMA_VERSION` is **8** — see `src/vector/ops/qdrant/utils.rs` doc-comment for the full version history). To add an origin-bearing path, set `cfg.seed_url` before embedding — do **not** thread it through the ~28 `PreparedDoc` builders. `axon refresh` facets on this field to re-enqueue origins; chunks indexed before 5.2.0 lack it and are invisible to refresh until re-indexed.

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

**After migration, restart all worker processes** (if the destination collection name differs from the source). The process-wide VectorMode cache is not invalidated on migration — workers that embedded to the source collection before migration will retain stale `Unnamed` mode in memory and fall back to dense-only search even for the new named-mode destination collection. If you migrate in-place (same collection name), the cache self-heals on the next embed because the collection-name key is unchanged.

### Sitemap backfill
After a crawl, `append_sitemap_backfill()` discovers URLs via sitemap.xml that the crawler missed and fetches them individually. Respects `--max-sitemaps` (default: 512) and `--include-subdomains`. Use `--sitemap-since-days N` to restrict backfill to URLs whose `<lastmod>` falls within the last N days; URLs without `<lastmod>` are always included.

### llms.txt probe

After a crawl (and during `map`), if `cfg.discover_llms_txt` (default true; first-run panel default false), axon fetches `/llms.txt` at the site root, parses its markdown links, scopes them like sitemap URLs, and caps them to `max_llms_txt_urls` (512). The scoped links are unioned (deduped, no blanket truncation) with sitemap discovery into one `append_candidate_backfill` pass, both sources discovered concurrently in the crawl runner. The cap bounds only the llms.txt fan-out — sitemap-URL backfill stays uncapped. Raw `.md`/`.markdown`/`.txt` targets skip the HTML→markdown transform (else they'd be dropped as thin). `fetch_text_with_retry` caps the `/llms.txt` document at 512 KB and `sitemap.xml` at 50 MB; HTML page backfill is uncapped and charset-aware. `llms-full.txt` is intentionally NOT parsed — it is a content dump, not a link index.


The compose file sets `context: .` — run `docker compose build` from this directory, not from a parent workspace.

### Spider feature flags with observable behavior
- **`firewall`**: NOT enabled — `spider_firewall`'s build.rs fetches blocklists from `api.github.com` unauthenticated and panics when GitHub rate-limits the CI runner. It doesn't read `GITHUB_TOKEN`, so external auth isn't possible. `validate_url()` in `src/core/http/ssrf.rs` remains the primary SSRF guard; this was defense-in-depth on top. Re-enable when upstream supports an auth knob.
- **`chrome_headless_new`**: Uses `--headless=new` instead of legacy headless. Better DOM fidelity but slightly different rendering behavior on some sites.
- **`balance`**: NOT enabled — silently throttles concurrency with zero logging. We manage concurrency explicitly via performance profiles.
- **`glob`**: NOT enabled — glob URL patterns (`{a,b}`, `[0-9]`) change `crawl_establish` to use `is_allowed()` (budget-aware) instead of `is_allowed_default()`. With `with_limit(1)`, the budget check immediately returns `BudgetExceeded` for the FIRST URL, producing 0 pages from Chrome crawls. axon doesn't use URL glob patterns in its CLI, so this feature is excluded. Do NOT add it back.
- **`adaptive_concurrency`**: Available through Spider's `basic` meta-feature but opt-in in Axon via `[workers.adaptive-concurrency]`. Do not add arbitrary Spider adaptive knobs until Spider actually honors them. Keep Axon controller logic in `src/crawl/engine/adaptive.rs`, not `runtime.rs`.
- Full flag inventory: [`docs/reference/spider-feature-flags.md`](docs/reference/spider-feature-flags.md)

### Subprocess stdout vs stderr
CLI commands output JSON data to stdout and progress/logs to stderr (Spinner via indicatif, tracing via `log_info`/`log_done`). Keep this split intact so server-mode and MCP callers can safely parse command output.

### Crawl queue cap (`AXON_MAX_PENDING_CRAWL_JOBS`)
New crawl job submissions check the count of pending jobs before inserting. If the count is ≥ `AXON_MAX_PENDING_CRAWL_JOBS` (default 100, 0 = unlimited), the submission is rejected with a human-readable error. Set to 0 to disable. Implemented in `src/jobs/ops/enqueue.rs` via `check_pending_cap_for()`.

### Auto path-prefix scoping
When crawling a URL with ≥2 path segments and no explicit `--url-whitelist`, the crawl is automatically scoped to the directory subtree of the start URL via a derived whitelist regex. For example, crawling `https://ai.google.dev/api/python/google/generativeai/GenerativeModel` auto-scopes to `^https?://ai\.google\.dev/api/python/google/generativeai(/|$)`. Root paths (`/`) and single-segment paths (`/docs`) are not scoped — they're already broad enough. Pass `--url-whitelist <pattern>` to override auto-scoping.

### Adding fields to `Config` struct
When adding a new non-`Option` field to `Config` in `src/core/config/types/config.rs`, add it (with a default) to `Config::default()` in `src/core/config/types/config_impls.rs` — that is the **single source of truth**. Test helpers build on it via `Config::test_default()` (which spreads `..Default::default()`), so they do **not** need updating when a field is added. If the field is env/TOML-configurable, also wire it in `src/core/config/parse/build_config/config_literal.rs` (and the TOML struct in `parse/toml_config.rs`). The whole-`Config` struct literals the old guidance pointed at no longer exist — adding a field compiles cleanly because every test path goes through `Config::default()`/`test_default()`.

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
| `axon_watch_url_state` | `watch_id`, `url`, `etag`, `last_modified`, `content_hash`, `last_markdown`, `last_links_json`, `last_checked_at`, `last_changed_at`, `last_crawl_job_id` — per-URL change-detection snapshot (migration `0007`), PK `(watch_id, url)`, FK → `axon_watch_defs(id)` ON DELETE CASCADE |

The **job** tables (`axon_*_jobs`) share: `created_at`, `updated_at`, `started_at`, `finished_at`, `error_text`. `axon_watch_url_state` is a snapshot table and does not carry those columns.

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


## Release Pipeline

### How releases work

Releases are **per-component and selective**. Every push to `main` triggers
`.github/workflows/auto-tag.yml`, which consumes the shared release plan from
`cargo xtask check-release-versions --mode main --json` and only cuts a release
for the components whose shipped code actually changed since their last release.
`release/components.toml` is the source of truth for component shipping paths,
tag prefixes, release workflows, version sources, and version-bearing files:

| Component | Shipping paths | Version source | Tag prefix | Release workflow |
|-----------|----------------|----------------|-----------|------------------|
| **cli** (Linux + Windows; web panel bundled in) | `src`, `Cargo.toml`/`Cargo.lock`, `build.rs`, `migrations`, `apps/web`, `rust-toolchain.toml`, `vendor` | `Cargo.toml` `[package]` version | `v` | `release.yml` |
| **palette** (Linux + Windows) | `apps/palette-tauri` | `apps/palette-tauri/src-tauri/tauri.conf.json` | `palette-v` | `palette-release.yml` |
| **android** (APK) | `apps/android` | `apps/android/app/build.gradle.kts` `versionName` | `android-v` | `android-release.yml` |
| **chrome** (extension zip) | `apps/chrome-extension`, `assets` | `apps/chrome-extension/manifest.json` `version` | `chrome-ext-v` | `chrome-extension-release.yml` |

For each component, the shared release checker diffs that component's shipping
paths against its most recent tag. If the code changed **and** the version was
bumped (the computed tag does not yet exist), `auto-tag.yml` waits for `CI` to
pass on the commit, creates the tag, and dispatches the component's release
workflow (which builds, packages with SHA256 checksums, and publishes a GitHub
Release). The `axon` `v*` release remains the repo's "latest";
palette/android/chrome publish with `make_latest: false`.

**Implications:**

- A change touching only one component releases only that component — e.g. an
  `apps/android/**`-only change cuts an Android release and nothing else; it
  does **not** rebuild the CLI.
- Dev-only trees (`xtask`, `benches`, `.github`, `docs`, and non-shipping
  repo policy/config files) are
  **not** in any component's shipping paths, so a tooling/docs-only merge cuts
  no release and needs no version bump. `Cargo.toml`, `Cargo.lock`, and
  `rust-toolchain.toml` are CLI shipping paths and are not part of this carve-out.
- If a component's code changed but its version was **not** bumped (the tag
  already exists), `cargo xtask check-release-versions --base origin/main --head
  HEAD --mode pr` fails before merge with a message naming the component.

To cut a release manually (e.g. re-release or hotfix without a code change),
push the component's tag directly:

```bash
git tag vX.Y.Z         && git push origin vX.Y.Z          # cli
git tag palette-vX.Y.Z && git push origin palette-vX.Y.Z  # palette
git tag android-vX.Y.Z && git push origin android-vX.Y.Z  # android
git tag chrome-ext-vX.Y.Z && git push origin chrome-ext-vX.Y.Z  # chrome
```

### Version bumping rules

**Bump ONLY the component(s) whose shipping code you changed** — versions are
independent per component. Bump type is determined by the commit message prefix
(auto-derived by `cargo xtask bump-version`, via git-cliff + `cliff.toml`):

- `feat!:` or `BREAKING CHANGE` → **major** (X+1.0.0)
- `feat` or `feat(...)` → **minor** (X.Y+1.0)
- Everything else (`fix`, `chore`, `refactor`, `test`, `docs`, etc.) → **patch** (X.Y.Z+1)

`cargo xtask bump-version <component>` derives the level from the conventional
commits touching that component's shipping paths since its last tag; pass an
explicit `patch|minor|major` to override. It bumps from `max(version_source,
latest tag)` so a worktree that lags `main` cannot collide with an existing tag.

**CLI component — all of these MUST move together (Cargo.toml is the source of truth):**
- `Cargo.toml` — `version = "X.Y.Z"` in `[package]` (Cargo.lock follows on next build)
- `README.md` — version header
- `CHANGELOG.md` — new entry under the bumped version
- `apps/web/package.json` + `apps/web/openapi/axon.json` — `"version": "X.Y.Z"`

**Palette component — all three MUST move together:**
- `apps/palette-tauri/src-tauri/tauri.conf.json`, `apps/palette-tauri/package.json`,
  `apps/palette-tauri/src-tauri/Cargo.toml`

**Android component:** `apps/android/app/build.gradle.kts` `versionName` (bump
`versionCode` too). **Chrome component:** `apps/chrome-extension/manifest.json`.

`plugins/axon/.claude-plugin/plugin.json` must **NOT** carry a `version` key —
`just validate-plugin` (part of `just verify`) hard-fails on it; the plugin is
versioned by the marketplace, not the manifest.

**Changelogs are generated, not hand-stamped.** Each component has its own
`CHANGELOG.md` (`CHANGELOG.md`, `apps/palette-tauri/CHANGELOG.md`,
`apps/android/CHANGELOG.md`, `apps/chrome-extension/CHANGELOG.md`). `bump-version`
prepends a real section via git-cliff, scoped to the component's shipping paths +
tag prefix (config in `cliff.toml`). **`git-cliff` must be installed where you run
`bump-version`** (`mise use -g git-cliff`); the CI gate
(`check-release-versions`) does **not** require it. Use `--skip-changelog` to fall
back to an empty heading in an emergency, or
`cargo xtask regen-changelog <component> --output <path>` to rebuild a changelog
from full history. **Editing a `CHANGELOG.md` never triggers a release** — change
detection ignores it, so documenting a release can't recursively cut another.

Use `cargo xtask bump-version <component> [patch|minor|major]` to bump every
version-bearing file for one component (level optional — see above). The PR gate is:

```bash
cargo xtask check-release-versions --base origin/main --head HEAD --mode pr
```

Short release checklist:

1. Identify changed components with `cargo xtask release-plan --base origin/main --head HEAD`.
2. Bump only those components with `cargo xtask bump-version <component> patch|minor|major`.
3. Run `cargo xtask check-release-versions --base origin/main --head HEAD --mode pr`.
4. Run `cargo xtask check`.

The compatibility command `cargo xtask check-version-sync` still enforces
**CLI** version parity across `Cargo.toml`, `README.md`, `CHANGELOG.md`,
`apps/web/package.json`, and `apps/web/openapi/axon.json`, and checks that
`plugins/axon/.claude-plugin/plugin.json` has no `version` key. The full
multi-component gate is `cargo xtask check-release-versions`.
