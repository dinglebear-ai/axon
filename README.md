# Axon

[![crates.io](https://img.shields.io/crates/v/axon)](https://crates.io/crates/axon)

Rust-based crawl, scrape, ingest, embed, query, and RAG engine with a unified CLI, MCP server, async workers, and web panel. Self-hosted research and knowledge-base backbone.

Version: 1.10.0 | License: MIT

---

## Table of Contents

1. [Overview](#overview)
2. [Installation](#installation)
3. [CLI Reference](#cli-reference)
   - [Global Flags](#global-flags)
   - [scrape](#scrape)
   - [crawl](#crawl)
   - [map](#map)
   - [extract](#extract)
   - [search](#search)
   - [research](#research)
   - [embed](#embed)
   - [query](#query)
   - [retrieve](#retrieve)
   - [ask](#ask)
   - [evaluate](#evaluate)
   - [suggest](#suggest)
   - [ingest](#ingest)
   - [sessions](#sessions)
   - [watch](#watch)
   - [sources](#sources)
   - [domains](#domains)
   - [stats](#stats)
   - [status](#status)
   - [doctor](#doctor)
   - [debug](#debug)
   - [migrate](#migrate)
   - [serve](#serve)
   - [mcp](#mcp)
   - [Job Lifecycle Subcommands](#job-lifecycle-subcommands)
4. [MCP Server](#mcp-server)
   - [Transport Modes](#transport-modes)
   - [Tool Contract](#tool-contract)
   - [Direct Actions](#direct-actions)
   - [Lifecycle Families](#lifecycle-families)
   - [Response Modes](#response-modes)
   - [Artifact Inspection](#artifact-inspection)
   - [Claude Desktop Integration](#claude-desktop-integration)
5. [Configuration](#configuration)
   - [Environment Variables](#environment-variables)
6. [Infrastructure](#infrastructure)
   - [Docker Compose Stack](#docker-compose-stack)
   - [Runtime Mode](#runtime-mode)
7. [Ingest Sources](#ingest-sources)
   - [GitHub](#github-ingest)
   - [Reddit](#reddit-ingest)
   - [YouTube](#youtube-ingest)
   - [AI Sessions](#ai-sessions-ingest)
8. [Vector Storage and RAG](#vector-storage-and-rag)
   - [Collection Modes](#collection-modes)
   - [Hybrid Search](#hybrid-search)
   - [RAG Pipeline](#rag-pipeline)
9. [Browser Rendering](#browser-rendering)
10. [Performance Profiles](#performance-profiles)
11. [Web UI](#web-ui)
12. [Security](#security)
13. [Development](#development)
14. [End-to-End Workflow Example](#end-to-end-workflow-example)
15. [Related Files](#related-files)

---

## Overview

Axon is three things simultaneously:

- A CLI binary: `axon`
- An MCP server: `axon mcp`
- A unified HTTP server: `axon serve`
- An SSH deployment helper: `axon setup`

The stack has the following components:

| Crate / Directory | Role |
|---|---|
| `src/lib.rs`, `src/main.rs` | Binary entry and command dispatch |
| `src/cli/` | Command handlers |
| `src/core/` | Config, HTTP safety, content transforms |
| `src/crawl/` | Crawl engine, render modes, sitemap backfill |
| `src/jobs/` | SQLite-backed job runtime, in-process worker lanes, state machine |
| `src/vector/` | Qdrant operations, TEI embedding, RAG |
| `src/ingest/` | GitHub, Reddit, YouTube, AI session ingestion |
| `src/mcp/` | MCP server, tool schema, action router |
| `src/services/` | Typed service entry points for CLI, MCP, and web |
| `apps/web/` | Static web panel source/assets |
| `docker-compose.yaml` | Axon server and infrastructure deployment |

---

## Installation

### Prerequisites

- Rust toolchain (see `rust-toolchain.toml`, currently stable 1.94.0+)
- Docker Engine + Compose v2 plugin
- GPU with NVIDIA CUDA (required for TEI; CPU fallback possible but slower)

### Bootstrap

Run the setup script once:

```bash
./scripts/dev-setup.sh
```

This script:
- Installs rustup and the pinned toolchain
- Installs `just`, `lefthook`, `cargo-nextest`, `cargo-watch`, `sccache`
- Checks Node.js ≥ v24 and pnpm ≥ v10
- Runs `pnpm install` for `apps/web`
- Creates `~/.axon/.env` from `.env.example` and auto-generates secrets
- Starts production infrastructure containers

Optional flags:

```bash
./scripts/dev-setup.sh --build       # also compile the release binary
./scripts/dev-setup.sh --no-docker   # skip Docker startup
```

After the script, edit `~/.axon/.env` to set `TEI_URL`, `OPENAI_*`, and `TAVILY_API_KEY`.

### Build

```bash
cargo build --release --bin axon
# or, with just:
just build
```

### Install to PATH

```bash
just install
# symlinks target/release/axon to ~/.local/bin/axon
```

---

## CLI Reference

All commands share global flags documented below. Commands listed as **async by default** enqueue a job and return immediately; pass `--wait true` to block until completion.

### Global Flags

#### Core Behavior

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--wait <bool>` | bool | `false` | Block until async job completes. Without this, async commands return a job ID immediately. |
| `--yes` | flag | — | Skip destructive confirmation prompts. |
| `--json` | flag | — | Machine-readable JSON output on stdout. |
| `--lite` | flag | — | Accepted for backwards compatibility; SQLite/in-process jobs are always used. |

#### Crawl and Scrape

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--max-pages <n>` | u32 | `0` | Page cap for crawl (0 = uncapped). |
| `--max-depth <n>` | usize | `10` | Maximum crawl depth from start URL. |
| `--render-mode <mode>` | enum | `auto-switch` | `http`, `chrome`, or `auto-switch`. Auto-switch tries HTTP first and falls back to Chrome when >60% of pages are thin. |
| `--format <fmt>` | enum | `markdown` | Output format: `markdown`, `html`, `rawHtml`, `json`. |
| `--include-subdomains <bool>` | bool | `false` | Crawl all subdomains of the start URL's parent domain. |
| `--respect-robots <bool>` | bool | `false` | Respect `robots.txt` directives. |
| `--discover-sitemaps <bool>` | bool | `true` | Discover and backfill URLs from sitemap.xml after crawl. |
| `--max-sitemaps <n>` | usize | `512` | Maximum sitemap URLs to backfill per crawl. |
| `--sitemap-since-days <n>` | u32 | `0` | Only backfill sitemap URLs with `<lastmod>` within the last N days (0 = no filter). |
| `--min-markdown-chars <n>` | usize | `200` | Minimum markdown character count; pages below this are flagged thin. |
| `--drop-thin-markdown <bool>` | bool | `true` | Skip thin pages — do not save or embed them. |
| `--delay-ms <ms>` | u64 | `0` | Delay between requests in milliseconds. |
| `--header <HEADER>` | string | — | Custom HTTP header in `Key: Value` format. Repeatable. |

#### Output

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--output-dir <dir>` | path | `.cache/axon-rust/output` | Directory for saved output files. |
| `--output <path>` | path | — | Explicit output file path (single-file commands). |

#### Vector and Embedding

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--collection <name>` | string | `cortex` | Qdrant collection name. Also set via `AXON_COLLECTION`. |
| `--embed <bool>` | bool | `true` | Auto-embed scraped content into Qdrant. |
| `--limit <n>` | usize | `10` | Result limit for search and query commands. |
| `--query <text>` | string | — | Query text (alternative to positional argument). |
| `--urls <csv>` | string | — | Comma-separated URL list (alternative to positional arguments). |

#### Performance Tuning

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--performance-profile <p>` | enum | `high-stable` | `high-stable`, `extreme`, `balanced`, `max`. Sets concurrency, timeout, and retry defaults. |
| `--batch-concurrency <n>` | usize | `16` | Concurrent connections for batch operations (1–512). |
| `--concurrency-limit <n>` | usize | — | Override crawl, sitemap, and backfill concurrency at once. |
| `--crawl-concurrency-limit <n>` | usize | profile | Override crawl concurrency. |
| `--backfill-concurrency-limit <n>` | usize | profile | Override sitemap backfill concurrency. |
| `--request-timeout-ms <ms>` | u64 | profile | Per-request timeout in milliseconds. |
| `--fetch-retries <n>` | usize | profile | Number of retries on failed fetches. |
| `--retry-backoff-ms <ms>` | u64 | profile | Backoff between retries in milliseconds. |

#### Service URL Overrides

| Flag | Env Var | Default |
|------|---------|---------|
| `--server-url <url>` | `AXON_SERVER_URL` | — |
| `--local` | `AXON_LOCAL_MODE` | `false` |
| `--qdrant-url <url>` | `QDRANT_URL` | `http://127.0.0.1:53333` |
| `--tei-url <url>` | `TEI_URL` | — |
| `--openai-base-url <url>` | `OPENAI_BASE_URL` | — |
| `--openai-api-key <key>` | `OPENAI_API_KEY` | — |
| `--openai-model <name>` | `OPENAI_MODEL` | — |
| `--sqlite-path <path>` | `AXON_SQLITE_PATH` | `$AXON_DATA_DIR/jobs.db` (default `~/.axon/jobs.db`) |

`AXON_SERVER_URL` turns the host CLI into a client for a running `axon serve`
process. Supported stateful commands execute on the server and use
server-owned job/output/artifact state. Use `--local` to force in-process CLI
execution for one command.

---

### scrape

Scrape one or more URLs. Runs inline (no queue). Validates URLs, fetches content, converts to the requested format, and optionally embeds into Qdrant.

```bash
axon scrape <url>... [FLAGS]
axon scrape --urls "<url1>,<url2>" [FLAGS]
axon scrape --url-glob "https://docs.example.com/{1..10}" [FLAGS]
```

| Flag | Default | Description |
|------|---------|-------------|
| `--format <fmt>` | `markdown` | Output format: `markdown`, `html`, `rawHtml`, `json`. |
| `--render-mode <mode>` | `auto-switch` | `http`, `chrome`, `auto-switch` (behaves like HTTP for scrape). |
| `--embed <bool>` | `true` | Batch-embed scraped markdown into Qdrant after all URLs finish. |
| `--output <path>` | — | Write output to a file (single URL only). |
| `--header "Key: Value"` | — | Repeatable custom HTTP headers. |

Examples:

```bash
axon scrape https://example.com
axon scrape --urls "https://a.dev,https://b.dev"
axon scrape https://example.com --format html --output page.html
axon scrape https://example.com --embed false --json
```

---

### crawl

Full site crawl. **Async by default** (enqueue and return). Use `--wait true` for synchronous mode.

```bash
axon crawl <url>... [FLAGS]
axon crawl --urls "<url1>,<url2>" [FLAGS]
axon crawl <SUBCOMMAND> [ARGS]
```

Key flags:

| Flag | Default | Description |
|------|---------|-------------|
| `--wait <bool>` | `false` | `false`: enqueue and return job IDs. `true`: run inline and block. |
| `--max-pages <n>` | `0` | Page cap (0 = uncapped). |
| `--max-depth <n>` | `10` | Maximum crawl depth. |
| `--render-mode <mode>` | `auto-switch` | `http`, `chrome`, `auto-switch`. |
| `--embed <bool>` | `true` | Queue embed job from crawl output. |
| `--sitemap-only` | `false` | Sync-only: run sitemap backfill without full crawl. |

Job subcommands:

```bash
axon crawl status <job_id>
axon crawl cancel <job_id>
axon crawl errors <job_id>
axon crawl list
axon crawl cleanup
axon crawl clear
axon crawl recover
axon crawl worker
axon crawl audit <url>
axon crawl diff
```

Examples:

```bash
axon crawl https://example.com                         # async enqueue
axon crawl https://example.com --wait true             # sync
axon crawl https://example.com --render-mode chrome --max-pages 200
axon crawl status 550e8400-e29b-41d4-a716-446655440000
```

---

### map

Discover all URLs from a site without scraping content. Combines crawler traversal with sitemap.xml discovery.

```bash
axon map <url> [FLAGS]
```

Returns a deduplicated, sorted URL list. No output files written.

---

### extract

LLM-powered structured data extraction from URLs. **Async by default.**

```bash
axon extract <url>... [FLAGS]
axon extract <SUBCOMMAND> [ARGS]
```

Job subcommands follow the same pattern as `crawl`: `status`, `cancel`, `errors`, `list`, `cleanup`, `clear`, `recover`, `worker`.

---

### search

Web search via Tavily. Automatically queues crawl jobs for top results.

```bash
axon search "<query>" [FLAGS]
```

Requires `TAVILY_API_KEY`. Results are de-duplicated by domain and optionally crawled.

---

### research

Web research via Tavily AI search with LLM synthesis. Returns a structured research report.

```bash
axon research "<query>" [FLAGS]
```

| Flag | Default | Description |
|------|---------|-------------|
| `--limit <n>` | `10` | Number of Tavily results. |
| `--search-time-range` | — | Filter results by recency: `day`, `week`, `month`, `year`. |

Requires `TAVILY_API_KEY` and the Gemini CLI headless configuration.

---

### embed

Embed a file, directory, or URL into Qdrant. **Async by default.**

```bash
axon embed [input] [FLAGS]
axon embed <SUBCOMMAND> [ARGS]
```

Input can be a file path, directory path, or URL. When omitted, reads from standard input.

Job subcommands: `status`, `cancel`, `errors`, `list`, `cleanup`, `clear`, `recover`, `worker`.

---

### query

Semantic vector search against the Qdrant collection.

```bash
axon query "<text>" [FLAGS]
```

| Flag | Default | Description |
|------|---------|-------------|
| `--collection <name>` | `cortex` | Qdrant collection to search. |
| `--limit <n>` | `10` | Number of results. |
| `--since <date>` | — | Filter by index date (ISO 8601). |
| `--before <date>` | — | Filter by index date upper bound. |

---

### retrieve

Fetch stored document chunks from Qdrant by URL.

```bash
axon retrieve <url> [FLAGS]
```

| Flag | Default | Description |
|------|---------|-------------|
| `--max-points <n>` | — | Maximum chunks to return. |
| `--collection <name>` | `cortex` | Qdrant collection. |

---

### ask

RAG-powered Q&A. Runs synchronously. Retrieves relevant chunks, reranks them, builds a context window, and calls the configured LLM.

```bash
axon ask "<question>" [FLAGS]
axon ask --query "<question>" [FLAGS]
```

| Flag | Default | Description |
|------|---------|-------------|
| `--collection <name>` | `cortex` | Qdrant collection to search. |
| `--diagnostics` | `false` | Print retrieval diagnostics. |

RAG pipeline:

1. Embed the question via TEI
2. Query Qdrant for top `AXON_ASK_CANDIDATE_LIMIT` (default: 150) candidates
3. Filter by `AXON_ASK_MIN_RELEVANCE_SCORE` (default: 0.45)
4. Rerank by score; take top `AXON_ASK_CHUNK_LIMIT` (default: 10)
5. Backfill additional chunks from top `AXON_ASK_FULL_DOCS` (default: 4) documents
6. Assemble context up to `AXON_ASK_MAX_CONTEXT_CHARS` (default: 120,000) characters
7. Call the LLM via Gemini headless
8. Apply citation-quality gates and return the answer

```bash
axon ask "how does spider.rs handle JavaScript-heavy sites?"
axon ask "list all indexed rust crates" --collection rust-libs
axon ask "qdrant HNSW parameters" --diagnostics
```

---

### evaluate

Baseline vs RAG evaluation with an independent LLM judge. Compares an answer without context against a RAG answer on accuracy, relevance, completeness, specificity, and verdict.

```bash
axon evaluate "<question>" [FLAGS]
```

Requires the Gemini CLI headless configuration.

---

### suggest

Suggest new documentation URLs to crawl based on existing indexed content.

```bash
axon suggest [focus] [FLAGS]
```

`[focus]` is an optional topic hint. Outputs ranked URL candidates.

---

### ingest

Ingest external sources into Qdrant. Source type is auto-detected from the target. **Async by default.**

```bash
axon ingest <TARGET> [FLAGS]
axon ingest <SUBCOMMAND> [ARGS]
```

Auto-detection rules (first match wins):

| Input pattern | Detected as |
|---|---|
| `r/subreddit` or `reddit.com/*` | Reddit |
| `@handle`, `youtube.com/*`, `youtu.be/*`, bare 11-char video ID | YouTube |
| `github.com/owner/repo` or `owner/repo` slug | GitHub |

Key flags:

| Flag | Default | Description |
|------|---------|-------------|
| `--wait <bool>` | `false` | Block until ingestion completes. |
| `--collection <name>` | `cortex` | Target Qdrant collection. |

GitHub-specific flags:

| Flag | Default | Description |
|------|---------|-------------|
| `--no-source` | `false` | Skip source-code file indexing. |
| `--max-issues <n>` | `100` | Maximum issues to fetch (0 = unlimited). |
| `--max-prs <n>` | `100` | Maximum pull requests to fetch (0 = unlimited). |

Reddit-specific flags:

| Flag | Default | Description |
|------|---------|-------------|
| `--sort <sort>` | `hot` | Post sort: `hot`, `top`, `new`, `rising`. |
| `--time <range>` | `day` | Time range for `top` sort: `hour`, `day`, `week`, `month`, `year`, `all`. |
| `--max-posts <n>` | `25` | Maximum posts to fetch. |
| `--min-score <n>` | `0` | Minimum score threshold for posts and comments. |
| `--depth <n>` | `2` | Comment traversal depth. |
| `--scrape-links` | off | Scrape content of linked URLs in link posts. |

Job subcommands: `status`, `cancel`, `errors`, `list`, `cleanup`, `clear`, `recover`, `worker`.

```bash
axon ingest rust-lang/rust                                        # GitHub slug
axon ingest https://github.com/anthropics/claude-code --wait true
axon ingest tokio-rs/tokio --no-source --wait true               # skip source files
axon ingest "https://www.youtube.com/watch?v=..." --wait true
axon ingest @SpaceinvaderOne                                     # YouTube @handle
axon ingest r/unraid --sort top --time week
axon ingest "https://www.reddit.com/r/rust/" --wait true
```

---

### sessions

Ingest local AI session history (Claude, Codex, Gemini) into Qdrant.

```bash
axon sessions [FLAGS]
axon sessions <SUBCOMMAND> [ARGS]
```

Provider paths:

| Provider | Path |
|---|---|
| Claude | `~/.claude/projects/` |
| Codex | `~/.codex/sessions/` |
| Gemini | `~/.gemini/history/`, `~/.gemini/tmp/` |

| Flag | Default | Description |
|------|---------|-------------|
| `--claude` | off | Include Claude sessions. |
| `--codex` | off | Include Codex sessions. |
| `--gemini` | off | Include Gemini sessions. |
| `--project <name>` | — | Case-insensitive substring filter on project name. |
| `--wait <bool>` | `false` | Block until ingestion completes. |

If none of `--claude`, `--codex`, `--gemini` is set, all providers are ingested.

Job subcommands: `status`, `cancel`, `errors`, `list`, `cleanup`, `clear`, `recover`, `worker`.

```bash
axon sessions                                         # all providers, async
axon sessions --codex --wait true                     # Codex only, sync
axon sessions --claude --gemini --project axon        # filtered by project name
```

---

### watch

Scheduled task management. Recurring definitions backed by `axon_watch_defs` and `axon_watch_runs` tables.

```bash
axon watch <SUBCOMMAND> [ARGS]
```

Implemented subcommands:

```bash
axon watch create <name> --task-type <type> --every-seconds <n> [--task-payload <json>]
axon watch list
axon watch run-now <id>
axon watch history <id> [--limit <n>]
```

Schema-defined but not yet implemented:

```bash
axon watch get <id>
axon watch update <id> [--every-seconds <n>]
axon watch pause <id>
axon watch resume <id>
axon watch delete <id>
axon watch artifacts <run_id>
```

`axon watch` with no subcommand defaults to `list`.

Task payload for `task_type=refresh`:

```json
{"urls":["https://example.com/docs","https://example.com/api"]}
```

```bash
axon watch create docs-refresh \
  --task-type refresh \
  --every-seconds 300 \
  --task-payload '{"urls":["https://docs.rs/spider"]}'
axon watch list --json
axon watch run-now <uuid>
axon watch history <uuid> --limit 20
```

Note: `watch list`, `watch create`, `watch run-now`, and `watch history` work with the current SQLite scheduler. The other listed subcommands parse but are not yet wired to a scheduler.

---

### sources

List all indexed URLs with chunk counts.

```bash
axon sources [FLAGS]
```

---

### domains

List indexed domains with document and chunk statistics.

```bash
axon domains [FLAGS]
```

---

### stats

Show Qdrant collection statistics (point count, vector size, indexed segments).

```bash
axon stats [FLAGS]
```

---

### status

Show async job queue status across all job families.

```bash
axon status [FLAGS]
```

---

### doctor

Diagnose service connectivity. Checks Qdrant, TEI, Chrome, and LLM endpoint reachability.

```bash
axon doctor [FLAGS]
```

---

### debug

Run `doctor` plus LLM-assisted troubleshooting with recommendations. Requires the Gemini CLI headless configuration.

```bash
axon debug [FLAGS]
```

---

### migrate

Migrate an unnamed-vector Qdrant collection to named-mode, enabling hybrid RRF search. One-time operation; no re-embedding required.

```bash
axon migrate --from <source> --to <destination>
```

| Flag | Required | Description |
|------|----------|-------------|
| `--from <name>` | Yes | Source collection (must use unnamed-vector schema). |
| `--to <name>` | Yes | Destination collection (auto-created if absent). |

```bash
axon migrate --from cortex --to cortex_v2
# then update ~/.axon/.env: AXON_COLLECTION=cortex_v2
# then restart all workers
```

Progress is logged every 100 pages (~25,600 points). At 2.57M points, expect 1–2 hours.

---

### serve

Start Axon's unified HTTP server. It mounts MCP HTTP, the web panel, `/v1/ask`,
and first-party CLI client/server routes on `AXON_MCP_HTTP_PORT` (default 8001).

```bash
axon serve
```

```bash
cargo run --bin axon -- serve
```

---

### mcp

Start the stdio MCP server. See [MCP Server](#mcp-server) for full details.

```bash
axon mcp [--transport stdio|http|both]
axon serve mcp
```

---

### Job Lifecycle Subcommands

All async job families (`crawl`, `extract`, `embed`, `ingest`, `sessions`) share these subcommands:

| Subcommand | Description |
|---|---|
| `status <job_id>` | Show current state and metadata for a job. |
| `cancel <job_id>` | Cancel a pending or running job. |
| `errors <job_id>` | Show error text for a failed job. |
| `list` | List recent jobs (last 50). |
| `cleanup` | Remove failed, canceled, and old completed jobs. |
| `clear` | Delete all jobs and purge the queue. Requires `--yes` or prompts for confirmation. |
| `recover` | Reclaim stale or interrupted running jobs. |
| `worker` | Run a worker inline (blocking). |

Job state machine: `pending` → `running` → `completed` | `failed` | `canceled`

---

## MCP Server

`axon mcp` exposes a single MCP tool named `axon` with `action`/`subaction` routing.

### Transport Modes

```bash
axon mcp                           # stdio only (default, for local MCP clients)
axon serve mcp                     # HTTP only (default for serving /mcp)
axon mcp --transport http          # HTTP only
axon mcp --transport both          # stdio + HTTP concurrently
```

HTTP endpoint: `http://<AXON_MCP_HTTP_HOST>:<AXON_MCP_HTTP_PORT>/mcp`

HTTP bind environment:

```bash
AXON_MCP_HTTP_HOST=127.0.0.1      # default
AXON_MCP_HTTP_PORT=8001            # default
AXON_MCP_HTTP_TOKEN=               # required for non-loopback binds
```

HTTP transport uses static bearer auth by default and can use Google OAuth/DCR
when `AXON_MCP_AUTH_MODE=oauth` is configured. Tokenless HTTP is allowed only
for loopback binds; non-loopback binds such as `0.0.0.0` are rejected at
startup unless bearer or OAuth auth is configured.

Authenticated clients send either `Authorization: Bearer $AXON_MCP_HTTP_TOKEN`
or `x-api-key: $AXON_MCP_HTTP_TOKEN` on every `/mcp` request.

### Tool Contract

Tool name: `axon`
Primary route field: `action`
Lifecycle route: `action` + `subaction`
Response field: `response_mode` (default: `path`)

Parser rules (strict):
- `action` is required and must match canonical names exactly
- `subaction` is required for lifecycle families
- No fallback fields (`command`, `op`, `operation`)
- No token normalization or case folding

Canonical success envelope:

```json
{
  "ok": true,
  "action": "<resolved action>",
  "subaction": "<resolved subaction>",
  "data": { "...": "..." }
}
```

### Direct Actions

These actions do not require `subaction`:

| Action | Optional Fields |
|--------|-----------------|
| `ask` | `query`, `diagnostics`, `collection`, `since`, `before`, `response_mode` |
| `elicit_demo` | `message`, `response_mode` |
| `help` | `response_mode` |
| `map` | `url`, `limit`, `offset`, `response_mode` |
| `query` | `query`, `limit`, `offset`, `collection`, `since`, `before`, `response_mode` |
| `research` | `query`, `limit`, `offset`, `search_time_range`, `response_mode` |
| `retrieve` | `url`, `max_points`, `response_mode` |
| `scrape` | `url`, `render_mode`, `format`, `embed`, `response_mode`, `root_selector`, `exclude_selector` |
| `screenshot` | `url`, `full_page`, `viewport`, `output`, `response_mode` |
| `search` | `query`, `limit`, `offset`, `search_time_range`, `response_mode` |

### Lifecycle Families

These actions require `subaction`:

| Family | Subactions |
|--------|------------|
| `artifacts` | `head\|grep\|wc\|read\|list\|delete\|clean\|search` |
| `crawl` | `start\|status\|cancel\|list\|cleanup\|clear\|recover` |
| `embed` | `start\|status\|cancel\|list\|cleanup\|clear\|recover` |
| `extract` | `start\|status\|cancel\|list\|cleanup\|clear\|recover` |
| `ingest` | `start\|status\|cancel\|list\|cleanup\|clear\|recover` |

Also available as direct actions (no subaction): `doctor`, `domains`, `sources`, `stats`, `status`

Example requests:

```json
{ "action": "scrape", "url": "https://example.com" }
{ "action": "crawl", "subaction": "start", "urls": ["https://example.com"] }
{ "action": "ingest", "subaction": "start", "source_type": "github", "target": "rust-lang/rust" }
{ "action": "ask", "query": "how does hybrid search work?" }
{ "action": "artifacts", "subaction": "list" }
```

### Response Modes

All actions accept a `response_mode` field:

| Mode | Behavior |
|---|---|
| `path` (default) | Write payload to artifact file. Response returns compact metadata: `path`, `bytes`, `line_count`, `sha256`, `preview`. |
| `inline` | Return payload directly in the response (truncated to safe size). |
| `both` | Write artifact AND include inline payload. |
| `auto-inline` | System-assigned only. Any payload ≤ `AXON_INLINE_BYTES_THRESHOLD` (default: 8,192 bytes) is returned inline automatically with `"response_mode": "auto-inline"`. |

Per-action overrides:
- `ask` and `research`: always write an artifact AND include `key_fields.answer` / `key_fields.summary` in path-mode responses. The answer is always immediately readable.
- `scrape` and `retrieve`: always return path mode. These payloads can be megabytes. Use `artifacts head` or `artifacts grep` to access content.

### Artifact Inspection

Artifact responses are stored under `$AXON_MCP_ARTIFACT_DIR` (default: `~/.axon/artifacts/<context>`). Inspect them with the `artifacts` action.

Preferred order (least to most expensive):

1. Read the `shape` field in path-mode responses — summarizes key/value types and status distributions.
2. `artifacts head path="<rel_path>"` — first 25 lines. Quick orientation.
3. `artifacts grep pattern="..." context_lines=N` — regex search with context.
4. `artifacts search pattern="..."` — cross-artifact regex search.
5. `artifacts read pattern="..."` — filtered line dump.
6. `artifacts read full=true` — full paginated dump (explicit opt-in required).

All artifact access uses the `relative_path` field — this works for both local stdio and remote HTTP clients.

Cleanup:

```json
{ "action": "artifacts", "subaction": "clean", "max_age_hours": 24, "dry_run": true }
{ "action": "artifacts", "subaction": "clean", "max_age_hours": 24, "dry_run": false }
```

`max_age_hours` is required. `dry_run` defaults to `true`. Files in `screenshots/` are never deleted.

### Claude Desktop Integration

`.mcp.json` example for stdio transport:

```json
{
  "mcpServers": {
    "axon": {
      "command": "axon",
      "args": ["mcp", "--transport", "stdio"],
      "env": {
        "QDRANT_URL": "http://127.0.0.1:53333",
        "TEI_URL": "http://YOUR_TEI_HOST:52000",
        "AXON_DATA_DIR": "/home/yourname/appdata"
      }
    }
  }
}
```

HTTP transport example:

```json
{
  "mcpServers": {
    "axon-http": {
      "url": "https://axon.example.com/mcp"
    }
  }
}
```

The Web UI MCP settings page writes MCP server definitions to `${AXON_DATA_DIR}/mcp.json` (default: `~/.axon/mcp.json`; falls back to `~/.config/axon/mcp.json`).

---

## Configuration

Copy `.env.example` to `~/.axon/.env` and fill in values. `~/.axon` is the canonical Axon appdata root for config, secrets, Docker/systemd server state, jobs, output, logs, artifacts, Qdrant data, and TEI cache data.

- `~/.axon/.env` — canonical app runtime variables, secrets, and Docker Compose interpolation
- `~/.axon/config.toml` — non-secret tuning knobs
- repo-root `.env` — development fallback only, used when `~/.axon/.env` is absent

For client/server operation, put server process settings and secrets in
`~/.axon/.env`, start `axon serve`, then point host CLIs at it:

```bash
AXON_SERVER_URL=http://127.0.0.1:8001 axon status --json
AXON_SERVER_URL=http://127.0.0.1:8001 axon scrape https://example.com --json
```

Server-mode scrape/crawl output is not written as host-local markdown by the
CLI. The server owns files under its `AXON_DATA_DIR` and returns portable
artifact handles. If bearer auth is enabled, non-loopback plaintext HTTP is
refused by default; use HTTPS or loopback unless `AXON_SERVER_INSECURE=1` is
an intentional operator override.

### Environment Variables

The minimum set needed to start:

| Variable | Required | Description |
|---|---|---|
| `QDRANT_URL` | Yes | Qdrant REST API base URL (e.g. `http://127.0.0.1:53333`) |
| `TEI_URL` | Yes | Text Embeddings Inference base URL (e.g. `http://127.0.0.1:52000`) |
| `AXON_HEADLESS_GEMINI_CMD` | For ask/research/evaluate/suggest/debug/extract fallback | Gemini CLI command (default: `gemini`) |
| `AXON_HEADLESS_GEMINI_HOME` | For Gemini auth isolation | Source HOME to copy Gemini auth files from |
| `AXON_HEADLESS_GEMINI_MODEL` | For ask/research | Gemini model override; defaults to `gemini-3.1-flash-lite-preview` |
| `AXON_LLM_COMPLETION_CONCURRENCY` | No | Max concurrent Gemini headless completions (default: `4`) |
| `AXON_LLM_COMPLETION_TIMEOUT_SECS` | No | Gemini completion timeout in seconds (default: `300`) |
| `OPENAI_BASE_URL` | Compatibility | OpenAI-compatible API base URL for legacy callers; Gemini headless does not require it |
| `OPENAI_MODEL` | Compatibility | Only `gemini-*` values are reused as Gemini overrides; older OpenAI model names are ignored |
| `TAVILY_API_KEY` | For search/research | Tavily search API key |
| `GITHUB_TOKEN` | For GitHub ingest | GitHub personal access token (optional, raises rate limits) |
| `REDDIT_CLIENT_ID` | For Reddit ingest | Reddit OAuth2 app client ID |
| `REDDIT_CLIENT_SECRET` | For Reddit ingest | Reddit OAuth2 app client secret |
| `AXON_CHROME_REMOTE_URL` | For Chrome rendering | Chrome management API URL (e.g. `http://axon-chrome:6000`) |
| `AXON_COLLECTION` | No | Default Qdrant collection name (default: `cortex`) |
| `AXON_DATA_DIR` | No | Persistent data root (default: `~/.axon`, flat layout — no nested `axon/` subdir) |
| `AXON_HOME` | Docker only | Host appdata root for Compose bind mounts (default: `${HOME}/.axon`; keep aligned with `AXON_DATA_DIR`) |
| `AXON_MCP_HTTP_PUBLISH` | Docker only | Compose host publish address for Axon MCP HTTP (default: `127.0.0.1:8001`; use `0.0.0.0:8001` only intentionally) |
| `AXON_MCP_AUTH_MODE` | For MCP OAuth | `bearer` by default; set `oauth` for Google OAuth + DCR through lab-auth |
| `AXON_MCP_PUBLIC_URL` | For MCP OAuth | Public origin used in OAuth metadata |
| `AXON_MCP_GOOGLE_CLIENT_ID` / `AXON_MCP_GOOGLE_CLIENT_SECRET` | For MCP OAuth | Google OAuth app credentials |
| `AXON_MCP_AUTH_ADMIN_EMAIL` | For MCP OAuth | Admin email accepted by the auth layer |
| `AXON_LITE` | No | Accepted for compatibility only; setting this has no effect beyond signalling intent. |
| `AXON_SQLITE_PATH` | No | SQLite jobs database path (default: `$AXON_DATA_DIR/jobs.db` → `~/.axon/jobs.db`) |

> **Full reference:** See [`docs/CONFIG.md`](docs/CONFIG.md) for every environment variable, its default, and description. `docs/CONFIG.md` is the single authoritative source — when in doubt, it wins over this file.

---

## Infrastructure

### Docker Compose Stack

The local stack uses one tracked compose file for the Axon server plus infrastructure services on the `axon` bridge network. Host-side state defaults to `~/.axon`; the container sees that same appdata tree as `/home/axon/.axon`.

| File | Contents |
|---|---|
| `docker-compose.yaml` | Axon server, Qdrant, TEI, Chrome |

Infrastructure services:

| Service | Image | Port | Purpose |
|---|---|---|---|
| `axon-qdrant` | qdrant/qdrant:v1.13.1 | `53333` (REST), `53334` (gRPC) | Vector store |
| `axon-tei` | ghcr.io/huggingface/text-embeddings-inference | `52000` | Embedding generation (NVIDIA GPU) |
| `axon-chrome` | built from `config/chrome/Dockerfile` | `9222` (CDP), `6000` (management) | Headless browser for JavaScript rendering |

Infrastructure service ports are bound to `127.0.0.1` (loopback only) by default. The optional `axon` HTTP service also publishes `127.0.0.1:8001` by default; set `AXON_MCP_HTTP_PUBLISH=0.0.0.0:8001` only when intentionally exposing it beyond the host and `AXON_MCP_HTTP_TOKEN` is configured.

Start infrastructure:

```bash
docker compose --env-file ~/.axon/.env up -d axon-qdrant axon-tei axon-chrome
# or: just services-up
```

Start local development (infra + Axon server process):

```bash
just dev
# equivalent to: just services-up && axon serve
```

Stop infrastructure:

```bash
just services-down
```

### Job Runtime

Jobs are stored in SQLite and workers run in-process inside the same tokio runtime. Postgres, Redis, and RabbitMQ are no longer used.

```bash
axon scrape https://example.com               # default
AXON_LITE=1 axon scrape https://example.com   # accepted, no behavior change
axon --lite scrape https://example.com        # accepted, no behavior change
```

All commands use the SQLite/in-process runtime. The `watch` scheduler exposes `list`, `create`, `run-now`, and `history` today; the remaining `watch` subcommands (`get`, `update`, `pause`, `resume`, `delete`, `artifacts`) parse but are not yet implemented.

---

## Ingest Sources

### GitHub Ingest

Ingests source code (tree-sitter AST-aware chunking), issues, pull requests, and wiki.

Supported tree-sitter languages: Rust, Python, JavaScript, TypeScript, Go, Bash.

Source code is indexed by default. Use `--no-source` to skip code and ingest only documentation and issues.

```bash
axon ingest rust-lang/rust
axon ingest https://github.com/tokio-rs/tokio --wait true
axon ingest tokio-rs/tokio --no-source       # docs/issues/PRs/wiki only
axon ingest tokio-rs/tokio --max-issues 0 --max-prs 0   # source only
```

Requires `GITHUB_TOKEN` for rates above 60 req/hr.

### Reddit Ingest

Ingests subreddit posts, comments, and linked URLs. Requires Reddit OAuth2 credentials.

```bash
axon ingest r/unraid --sort top --time week
axon ingest "https://www.reddit.com/r/rust/"
axon ingest r/programming --max-posts 100 --min-score 10 --depth 3
```

### YouTube Ingest

Ingests video transcripts via `yt-dlp`. Supports video URLs, `@channel` handles, playlists, and bare 11-character video IDs. `yt-dlp` must be on `PATH`.

```bash
axon ingest "https://www.youtube.com/watch?v=dQw4w9WgXcQ" --wait true
axon ingest @SpaceinvaderOne
axon ingest dQw4w9WgXcQ                    # bare video ID
```

### AI Sessions Ingest

Ingests local conversation history from Claude, Codex, and Gemini into Qdrant.

```bash
axon sessions                              # all providers
axon sessions --claude --project axon      # filtered by project name
```

Uses an incremental state tracker (`axon_session_ingest_state`) so re-runs skip already-indexed sessions.

---

## Vector Storage and RAG

### Collection Modes

Axon supports two Qdrant collection modes:

| Mode | Schema | Search |
|---|---|---|
| Unnamed (legacy) | `"vectors": {"size": N}` | Dense-only cosine |
| Named (current) | `"vectors": {"dense": ..., "sparse": {"bm42": ...}}` | RRF hybrid (dense + BM42 sparse) |

New collections are created in named mode automatically. Legacy collections use `axon migrate` to upgrade.

### Hybrid Search

Named-mode collections use Reciprocal Rank Fusion (RRF) via the Qdrant `/query` endpoint when hybrid search is active. Falls back to dense-only when the sparse query is empty or hybrid is disabled. No re-embedding is needed for migration — BM42 sparse vectors are computed locally from `chunk_text` payload fields.

### RAG Pipeline

Text chunking: 2,000 characters with 200-character overlap. Each chunk is one Qdrant point.

Embedding: TEI batches with auto-retry on HTTP 413 (payload too large), 429, and 5xx. Up to 5 attempts with exponential backoff (1s, 2s, 4s, 8s plus jitter).

The `ask` retrieval pipeline is described under [ask](#ask). Key tuning variables: `AXON_ASK_CANDIDATE_LIMIT`, `AXON_ASK_MIN_RELEVANCE_SCORE`, `AXON_ASK_CHUNK_LIMIT`, `AXON_ASK_MAX_CONTEXT_CHARS`.

---

## Browser Rendering

Axon supports two fetch modes:

| Mode | Description |
|---|---|
| `http` | Pure HTTP with `reqwest`. Fast; no JavaScript execution. |
| `chrome` | Chrome DevTools Protocol (CDP). Full JavaScript execution. |
| `auto-switch` (default) | HTTP first. If >60% of crawled pages are thin (<200 chars) or total coverage is too low, retries with Chrome. |

Chrome requires a running Chrome instance. The `axon-chrome` container provides a `headless_browser` manager:
- Port `6000`: management API (`GET /` returns "healthy", `/fork`, `/shutdown`)
- Port `9222`: CDP proxy

Set `AXON_CHROME_REMOTE_URL` to point at the management API (e.g. `http://axon-chrome:6000`).

Use `chrome` mode explicitly for Single-Page Applications and sites that require JavaScript execution. Use `http` mode for static sites to minimize resource usage.

---

## Performance Profiles

Set with `--performance-profile <name>` or the default `high-stable`.

| Profile | Crawl concurrency | Sitemap concurrency | Backfill concurrency | Timeout | Retries | Backoff |
|---|---|---|---|---|---|---|
| `high-stable` (default) | CPUs×8 (64–192) | CPUs×12 (64–256) | CPUs×6 (32–128) | 20s | 2 | 250ms |
| `balanced` | CPUs×4 (32–96) | CPUs×6 (32–128) | CPUs×3 (16–64) | 30s | 2 | 300ms |
| `extreme` | CPUs×16 (128–384) | CPUs×20 (128–512) | CPUs×10 (64–256) | 15s | 1 | 100ms |
| `max` | CPUs×24 (256–1024) | CPUs×32 (256–1536) | CPUs×20 (128–1024) | 12s | 1 | 50ms |

Override individual limits with `--crawl-concurrency-limit`, `--sitemap-concurrency-limit`, `--backfill-concurrency-limit`, `--request-timeout-ms`, `--fetch-retries`, and `--retry-backoff-ms`.

---

## Web Panel

`axon serve` serves the embedded setup/config panel on the same HTTP listener as
MCP, default `http://127.0.0.1:8001`.

Mounted routes include:

- `/` - static web panel assets.
- `/api/panel/state`, `/api/panel/login`, `/api/panel/config`, `/api/panel/ops` - setup and config APIs.
- `/api/panel/setup/targets`, `/api/panel/setup/deploy` - remote setup helpers.
- `/v1/ask` - ask endpoint used by `axon ask` when `AXON_SERVER_URL` is set.
- `/v1/capabilities` and `/v1/actions` - first-party CLI client/server mode.
- `/mcp` - MCP streamable HTTP transport.

The older Next.js dashboard, websocket bridge, shell websocket, and `/download/*`
routes are not part of the current `axon serve` runtime.

---

## Security

### URL Validation

All user-provided URLs are validated before any network request:
- Scheme allowlist: `http` and `https` only
- Blocked hosts: `localhost`, `.localhost`, `.internal`, `.local`
- Blocked IP ranges: loopback, link-local, RFC-1918 private ranges, IPv4-mapped IPv6

DNS rebinding is mitigated via `SsrfBlockingResolver`, which re-checks resolved IPs at TCP connect time.

### HTTP Authentication

MCP and first-party server actions share the same HTTP auth boundary. Static
bearer auth uses `AXON_MCP_HTTP_TOKEN`; OAuth mode uses
`AXON_MCP_AUTH_MODE=oauth` with Google OAuth and lab-auth.

### LLM Process Isolation

Gemini headless completions run with an isolated temporary HOME and an allowlisted environment. The Gemini command path is validated before launch, and LLM completion concurrency is capped by `AXON_LLM_COMPLETION_CONCURRENCY`.

### Destructive Operations

The following operations are unauthenticated at the application level (network-level controls apply):
- `axon crawl clear`, `axon extract clear`, `axon embed clear`, `axon ingest clear`
- `axon crawl cancel <id>`, `axon ingest cancel <id>`

Axon is designed for self-hosted single-user operation. All service ports are loopback-bound by default.

Never commit `.env`. For network-accessible deployments, configure `AXON_MCP_HTTP_TOKEN` or OAuth and avoid exposing service ports directly.

---

## Development

### Build and Test

```bash
just setup          # bootstrap environment (once)
just check          # cargo check
just test           # run unit tests (excludes e2e)
just test-fast      # lib tests only
just fmt            # cargo fmt
just clippy         # cargo clippy
just build          # release build
just verify         # fmt-check + clippy + check + test (pre-PR gate)
just fix            # cargo fmt + clippy --fix
```

### Local Development Stack

```bash
just services-up    # start infra (Qdrant, TEI, Chrome)
just dev            # infra + axon serve (builds debug binary)
just stop           # kill running serve and worker processes
```

### MCP Smoke Tests

```bash
just mcp-smoke
# Runs the MCP smoke suite against the SQLite/in-process runtime
```

Manual mcporter commands:

```bash
mcporter --config config/mcporter.json call axon.axon action:help response_mode:inline --output json
mcporter --config config/mcporter.json call axon.axon action:doctor --output json
mcporter --config config/mcporter.json call axon.axon action:scrape url:https://www.rust-lang.org --output json
mcporter --config config/mcporter.json call axon.axon action:query query:'rust mcp sdk' --output json
mcporter --config config/mcporter.json call axon.axon action:crawl subaction:list limit:5 offset:0 --output json
mcporter --config config/mcporter.json call axon.axon action:artifacts subaction:list --output json
```

### Web Panel Assets

```bash
cd apps/web && npm run build    # production build
cd apps/web && npm run lint     # lint
```

### Wrapper Script

Use `./scripts/axon` instead of the cargo binary for local dev — it auto-sources `~/.axon/.env` and falls back to repo `.env` only when the canonical env file is absent:

```bash
./scripts/axon doctor
./scripts/axon scrape https://example.com --wait true
./scripts/axon mcp
```

---

## End-to-End Workflow Example

Ingest a repository, embed it, and ask questions over the indexed content.

```bash
# 1. Start infrastructure
just services-up

# 2. Run the local server (starts workers and web panel)
just dev

# 3. Verify connectivity
axon doctor

# 4. Ingest the tokio repository (source code + issues + PRs, async)
axon ingest tokio-rs/tokio

# 5. Check ingest job status
axon ingest list
axon ingest status <job_id>

# 6. Wait for ingest to complete (or use --wait true in step 4)
# Embed happens automatically after ingest

# 7. Verify content is indexed
axon sources
axon stats

# 8. Basic semantic search
axon query "async runtime architecture" --limit 5

# 9. RAG question answering
axon ask "how does tokio handle task scheduling?"

# 10. Set up a watch definition that re-ingests on a schedule
axon watch create tokio-refresh \
  --task-type ingest \
  --every-seconds 21600 \
  --task-payload '{"target":"tokio-rs/tokio"}'

# 11. View indexed domains and sources
axon domains
axon sources
```

---

## Related Files

| File | Contents |
|---|---|
| `CLAUDE.md` | Canonical developer reference: commands, architecture, gotchas |
| `docs/ARCHITECTURE.md` | Subsystem map and data-flow diagrams |
| `docs/CONFIG.md` | Authoritative environment variable reference |
| `docs/MCP.md` | MCP runtime and design guide |
| `docs/MCP-TOOL-SCHEMA.md` | MCP tool schema source of truth |
| `docs/SECURITY.md` | Security model, SSRF controls, MCP auth, and LLM process isolation |
| `docs/JOB-LIFECYCLE.md` | Async job state machine, worker architecture |
| `docs/DEPLOYMENT.md` | Deploy and rollback procedures |
| `docs/TESTING.md` | Test strategy and mcporter smoke harness |
| `docs/commands/` | Per-command reference documentation |
| `docs/ingest/` | Per-source ingest deep dives |
| `docs/archive/pre-lite-mode/` | Historical docs from the removed Postgres/Redis/RabbitMQ runtime |
| `CHANGELOG.md` | Release history |

---

## Related plugins

| Plugin | Category | Description |
|--------|----------|-------------|
| [homelab-core](https://github.com/jmagar/claude-homelab) | core | Core agents, commands, skills, and setup/health workflows for homelab management. |
| [overseerr-mcp](https://github.com/jmagar/overseerr-mcp) | media | Search movies and TV shows, submit requests, and monitor failed requests via Overseerr. |
| [unraid-mcp](https://github.com/jmagar/unraid-mcp) | infrastructure | Query, monitor, and manage Unraid servers: Docker, VMs, array, parity, and live telemetry. |
| [unifi-mcp](https://github.com/jmagar/unifi-mcp) | infrastructure | Monitor and manage UniFi devices, clients, firewall rules, and network health. |
| [gotify-mcp](https://github.com/jmagar/gotify-mcp) | utilities | Send and manage push notifications via a self-hosted Gotify server. |
| [swag-mcp](https://github.com/jmagar/swag-mcp) | infrastructure | Create, edit, and manage SWAG nginx reverse proxy configurations. |
| [synapse-mcp](https://github.com/jmagar/synapse-mcp) | infrastructure | Docker management (Flux) and SSH remote operations (Scout) across homelab hosts. |
| [arcane-mcp](https://github.com/jmagar/arcane-mcp) | infrastructure | Manage Docker environments, containers, images, volumes, networks, and GitOps via Arcane. |
| [syslog-mcp](https://github.com/jmagar/syslog-mcp) | infrastructure | Receive, index, and search syslog streams from all homelab hosts via SQLite FTS5. |
| [plugin-lab](https://github.com/jmagar/plugin-lab) | dev-tools | Scaffold, review, align, and deploy homelab MCP plugins with agents and canonical templates. |

## License

MIT
