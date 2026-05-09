# Configuration Reference -- Axon

> **Source of truth.** This file is the canonical reference for all Axon environment variables. When in doubt, this file wins over README.md, MCP.md, or any other doc. Keep `.env.example` in sync — every variable listed here that has a sensible example value should appear there, and every variable in `.env.example` should be documented here. User-level values live in `~/.axon/.env`.

Axon is configured through three layers: `~/.axon/config.toml`, environment variables, and CLI flags.

## Precedence (highest to lowest)

1. CLI flags (`--pg-url`, `--collection`, etc.)
2. Environment variables (`AXON_PG_URL`, `AXON_COLLECTION`, etc.)
3. `~/.axon/config.toml` — tuning knobs (safe to commit, no secrets)
4. Built-in defaults

## Canonical `~/.axon/` layout

`~/.axon/` is the canonical home for all Axon user-level config, secrets, runtime state, infrastructure data, and generated output. All app data lives directly under this directory — no nested `axon/` subdirectory.

```
~/.axon/
├── config.toml              # tuning knobs (CLI > env > this > default)
├── .env                     # URLs + secrets (loaded after AXON_ENV_FILE,
│                            #   before repo-root .env ancestor walk)
│
├── jobs.db                  # SQLite job queue
├── jobs.db-wal
├── jobs.db-shm
│
├── output/                  # scraped markdown / HTML / JSON
├── logs/
│   └── axon.log             # size-rotated, 10 MiB default
├── artifacts/               # MCP JSON artifacts (response_mode=path)
├── screenshots/             # spider chrome_store_page captures
├── chrome-diagnostics/      # opt-in browser diagnostics
│
├── qdrant/                  # Docker Compose Qdrant bind mount
├── tei/                     # Docker Compose TEI model/cache data
└── lab-auth/                # OAuth/lab-auth state for server deployments
```

`AXON_DATA_DIR` defaults to `~/.axon` for the binary. Docker Compose uses `AXON_HOME` for host-side bind mounts and defaults it to `${HOME}/.axon`; keep `AXON_HOME` and `AXON_DATA_DIR` aligned unless you are deliberately relocating the entire Axon appdata tree.

### Migration from `~/.local/share/axon`

If you previously stored axon data under `~/.local/share/axon/`, axon does NOT auto-migrate. Either move the directory yourself (`mv ~/.local/share/axon ~/.axon`), or set `AXON_DATA_DIR=~/.local/share` explicitly to keep the old location. Tuning knobs that were previously env-only are now also accepted in `~/.axon/config.toml`.

## Environment files

Three env files are auto-loaded in this order; the first one that exists and parses wins (later files do **not** override earlier ones):

| Order | Path | Notes |
|-------|------|-------|
| 1 | `$AXON_ENV_FILE` | Explicit override; only consulted when set |
| 2 | `~/.axon/.env` | Canonical user-level secrets, loaded automatically |
| 3 | First `.env` found by walking ancestors of CWD (or the binary's parent) | Repo-root `.env` fallback for development only |

Docker Compose also reads `~/.axon/.env` by default as the service env file and uses `AXON_HOME` for host bind mounts:

| File | Purpose | Loaded by |
|------|---------|-----------|
| `~/.axon/.env` | Canonical app runtime variables, secrets, and Docker Compose interpolation | `dotenvy` in binary; `docker compose --env-file ~/.axon/.env`; compose service `env_file` |
| Repo `.env` | Development fallback only | `dotenvy` ancestor walk; `scripts/axon` only when `~/.axon/.env` is absent |

```bash
mkdir -m 700 -p ~/.axon
cp .env.example ~/.axon/.env
chmod 600 ~/.axon/.env
```

## CLI server mode

`AXON_SERVER_URL` is the generic client/server switch for the CLI. When it is
set, supported stateful commands call a running `axon serve` HTTP endpoint
instead of executing locally:

```bash
AXON_SERVER_URL=http://127.0.0.1:8001 axon status --json
AXON_SERVER_URL=http://127.0.0.1:8001 axon scrape https://example.com --json
```

Server mode currently covers `status`, `scrape`, `crawl`, `extract`, `embed`,
`ingest`, `sessions`, and `screenshot`. The server owns SQLite job state,
output files, screenshots, and artifacts under its `AXON_DATA_DIR` (default
`~/.axon`). CLI responses use server-owned artifact handles and root-relative
identifiers; absolute paths are display/debug information only.

Use `--local` or `AXON_LOCAL_MODE=1` to force local execution for one command
or shell:

```bash
axon scrape https://example.com --local
AXON_LOCAL_MODE=1 axon crawl https://example.com
```

`AXON_ASK_SERVER_URL` is retained only as a compatibility alias for `axon ask`
when `AXON_SERVER_URL` is unset. New setups should use `AXON_SERVER_URL`.

If `AXON_MCP_HTTP_TOKEN` is set, the CLI refuses to send it over plaintext
HTTP to non-loopback hosts. Use loopback, HTTPS, or set
`AXON_SERVER_INSECURE=1` only for an explicitly trusted network.

## ~/.axon/config.toml

`~/.axon/config.toml` holds tuning knobs — parameters that are safe to commit to source control because they contain no secrets or security toggles. Copy `config.example.toml` from the repo root and place it at `~/.axon/config.toml` (create `~/.axon/` with `chmod 700` and the file with `chmod 600`).

```bash
mkdir -m 700 ~/.axon
cp config.example.toml ~/.axon/config.toml
chmod 600 ~/.axon/config.toml
```

To point at a custom path: `AXON_CONFIG_PATH=/path/to/config.toml`.

All TOML keys below are wired through `Config` — setting them in `~/.axon/config.toml` takes effect. The env var shown for each key still overrides the TOML value at the precedence chain above.

| Section | Keys | Env override |
|---------|------|---------------|
| `[services]` | `qdrant-url`, `tei-url`, `chrome-remote-url` | `QDRANT_URL`, `TEI_URL`, `AXON_CHROME_REMOTE_URL` |
| `[search]` | `hybrid-enabled`, `hybrid-candidates`, `ask-hybrid-candidates`, `hnsw-ef`, `hnsw-ef-legacy`, `collection` | `AXON_HYBRID_SEARCH`, `AXON_HYBRID_CANDIDATES`, `AXON_ASK_HYBRID_CANDIDATES`, `AXON_HNSW_EF_SEARCH`, `AXON_HNSW_EF_SEARCH_LEGACY`, `AXON_COLLECTION` |
| `[ask]` | `chunk-limit`, `candidate-limit`, `min-relevance-score` | `AXON_ASK_CHUNK_LIMIT`, `AXON_ASK_CANDIDATE_LIMIT`, `AXON_ASK_MIN_RELEVANCE_SCORE` |
| `[tei]` | `max-retries`, `request-timeout-ms`, `max-client-batch-size` | `TEI_MAX_RETRIES`, `TEI_REQUEST_TIMEOUT_MS`, `TEI_MAX_CLIENT_BATCH_SIZE` |
| `[workers]` | `ingest-lanes`, `embed-doc-timeout-secs`, `max-pending-crawl-jobs`, `max-pending-embed-jobs`, `max-pending-extract-jobs`, `max-pending-ingest-jobs` | `AXON_INGEST_LANES`, `AXON_EMBED_DOC_TIMEOUT_SECS`, `AXON_MAX_PENDING_CRAWL_JOBS`, `AXON_MAX_PENDING_EMBED_JOBS`, `AXON_MAX_PENDING_EXTRACT_JOBS`, `AXON_MAX_PENDING_INGEST_JOBS` |

URLs, API keys, secrets, and Gemini headless runtime controls belong in `~/.axon/.env` — not in `config.toml`. Gemini headless is the only LLM synthesis path; `config.toml` only carries RAG tuning knobs. See `config.example.toml` for the full annotated example with defaults.

> **Replaced by:** `axon.json` was removed in v0.36. Migrate tuning params to `~/.axon/config.toml`.

## Environment variables by category

### Core runtime (required)

| Variable | Default | Description |
|----------|---------|-------------|
| `QDRANT_URL` | -- | Qdrant vector database URL |
| `TEI_URL` | -- | Text Embeddings Inference URL |

### Host paths

| Variable | Default | Description |
|----------|---------|-------------|
| `AXON_DATA_DIR` | `~/.axon` | Root directory for all persistent data (flat — no `axon/` subdir nesting) |
| `HOST_HOME` | -- | Host user home (for session ingestion bind mount) |
| `AXON_WORKSPACE` | -- | Host workspace dir mounted into axon-web |
| `HOST_WORKSPACE` | -- | Host path to axon_rust repo |
| `AXON_BIN` | -- | Path to pre-built axon binary inside container |

### Server ports

| Variable | Default | Description |
|----------|---------|-------------|
| `AXON_SERVER_URL` | -- | Generic CLI server-mode endpoint. When set, supported stateful CLI commands call `axon serve` through `/v1/actions`. |
| `AXON_LOCAL_MODE` | `false` | Force local CLI execution even when `AXON_SERVER_URL` is configured. Equivalent to `--local`. |
| `AXON_SERVER_INSECURE` | -- | Set to `1` to allow bearer-token auth over plaintext HTTP to non-loopback hosts. Not recommended; prefer HTTPS. |
| `AXON_MCP_HTTP_PUBLISH` | `127.0.0.1:8001` | Docker Compose host publish address for the `axon` MCP HTTP service. Set to `0.0.0.0:8001` only when intentionally exposing beyond the host and `AXON_MCP_HTTP_TOKEN` is configured. |
| `AXON_SERVE_HOST` | `127.0.0.1` | Backend bridge bind address |
| `AXON_SERVE_PORT` | `49000` | Backend bridge port |
| `AXON_WEB_DEV_PORT` | `49010` | Next.js dev server port |
| `SHELL_SERVER_PORT` | `49011` | Shell WebSocket server port |

### Lite mode

| Variable | Default | Description |
|----------|---------|-------------|
| `AXON_LITE` | -- | Set to `1` to enable lite mode (default; uses SQLite) |
| `AXON_SQLITE_PATH` | `$AXON_DATA_DIR/jobs.db` (default `~/.axon/jobs.db`) | SQLite path for lite mode |

**Worker spawn is conditional**, not unconditional, in lite mode. The `LiteBackend` has two construction modes:

- `LiteBackend::new(cfg)` — **enqueue-only**. No workers spawn. Used by `ServiceContext::new()` for short-lived CLI commands (status/list/cancel/fire-and-forget submit).
- `LiteBackend::new_with_workers(cfg)` — spawns in-process tokio workers (crawl + N×embed + extract + N×ingest). Used by `ServiceContext::new_with_workers()` for long-running processes: `axon serve`, MCP server, web routes, and CLI commands that block on `--wait true`.

Spawning workers in a fire-and-forget CLI process orphans claimed jobs at process exit, so the CLI defaults to enqueue-only and lets a separate `serve`/`mcp` process drain the queue.

`--wait false` is intentionally fire-and-forget for lite crawl/embed/ingest submits: the command enqueues the job, prints the job ID, and exits without draining the table. `--wait true` starts in-process workers where the service path needs queued workers, then waits only for the job IDs submitted by the current command and any explicit dependent job IDs.

### TEI embedding

| Variable | Default | Description |
|----------|---------|-------------|
| `TEI_MAX_RETRIES` | `5` | Max retry attempts after the initial request |
| `TEI_REQUEST_TIMEOUT_MS` | `30000` | Per-attempt timeout (clamped 1000-300000) |
| `TEI_MAX_CLIENT_BATCH_SIZE` | `64` | Default batch size sent to TEI (auto-splits on 413; max: 128) |
| `TEI_HTTP_PORT` | `52000` | Host port for TEI container |
| `TEI_EMBEDDING_MODEL` | `Qwen/Qwen3-Embedding-0.6B` | HuggingFace embedding model |
| `TEI_MAX_CONCURRENT_REQUESTS` | `80` | Max concurrent TEI requests |
| `TEI_MAX_BATCH_TOKENS` | `163840` | Max batch tokens |
| `TEI_MAX_BATCH_REQUESTS` | `80` | Max batch requests |
| `TEI_POOLING` | `last-token` | Pooling strategy |
| `TEI_TOKENIZATION_WORKERS` | `8` | Tokenization workers |
| `HF_TOKEN` | -- | HuggingFace token for gated models |

### LLM / Gemini headless

| Variable | Default | Description |
|----------|---------|-------------|
| `OPENAI_BASE_URL` | -- | Compatibility setting retained for callers that also use OpenAI-compatible providers; Gemini headless does not require it. |
| `OPENAI_API_KEY` | -- | Compatibility setting retained for callers that also use OpenAI-compatible providers; Gemini headless does not require it. |
| `AXON_HEADLESS_GEMINI_MODEL` | -- | Gemini model override for synthesis. Headless Gemini defaults to `gemini-3.1-flash-lite-preview` when unset. |
| `OPENAI_MODEL` | -- | Compatibility model setting. Only `gemini-*` values are reused as Gemini overrides; older OpenAI model names are ignored. |
| `AXON_HEADLESS_GEMINI_CMD` | `gemini` | Gemini CLI command for headless synthesis. Path-like values are validated before launch. |
| `AXON_HEADLESS_GEMINI_HOME` | `HOME` | Source HOME to copy Gemini CLI auth files from before running with isolated temporary HOME. |
| `AXON_LLM_COMPLETION_CONCURRENCY` | `4` | Runtime-only max concurrent Gemini headless completion requests. |
| `AXON_LLM_COMPLETION_TIMEOUT_SECS` | `300` | Runtime-only timeout for each Gemini headless completion request. |

### Queues and collections

| Variable | Default | Description |
|----------|---------|-------------|
| `AXON_COLLECTION` | `cortex` | Qdrant collection name |
| `AXON_CRAWL_QUEUE` | `axon.crawl.jobs` | Crawl job queue |
| `AXON_EXTRACT_QUEUE` | `axon.extract.jobs` | Extract job queue |
| `AXON_EMBED_QUEUE` | `axon.embed.jobs` | Embed job queue |
| `AXON_INGEST_QUEUE` | `axon.ingest.jobs` | Ingest job queue |
| `AXON_GRAPH_QUEUE` | `axon.graph.jobs` | Graph job queue |
| `AXON_INGEST_LANES` | `2` | Parallel ingest worker lanes (clamped 1-16) |

### Search and research

| Variable | Default | Description |
|----------|---------|-------------|
| `TAVILY_API_KEY` | -- | Tavily AI Search API key |

### Ingest credentials

| Variable | Default | Description |
|----------|---------|-------------|
| `GITHUB_TOKEN` | -- | GitHub PAT for private repos and rate limits |
| `REDDIT_CLIENT_ID` | -- | Reddit OAuth2 client ID |
| `REDDIT_CLIENT_SECRET` | -- | Reddit OAuth2 client secret |

### Chrome browser

| Variable | Default | Description |
|----------|---------|-------------|
| `AXON_CHROME_REMOTE_URL` | `http://axon-chrome:6000` | CDP management endpoint |
| `CHROME_URL` | `http://127.0.0.1:6000` | Spider-rs native CDP var (always use localhost URL here) |
| `AXON_CHROME_PROXY` | -- | Proxy URL for Chrome requests |
| `AXON_CHROME_USER_AGENT` | -- | User-Agent override for Chrome requests |
| `AXON_CHROME_DIAGNOSTICS` | `false` | Enable browser diagnostics artifact collection |
| `AXON_CHROME_DIAGNOSTICS_DIR` | `$AXON_DATA_DIR/chrome-diagnostics` (default `~/.axon/chrome-diagnostics`) | Output directory for diagnostics artifacts |
| `AXON_CHROME_DIAGNOSTICS_EVENTS` | `false` | Include event-log capture in diagnostics |
| `AXON_CHROME_DIAGNOSTICS_SCREENSHOT` | `false` | Include screenshot capture in diagnostics |

### Hybrid search

| Variable | Default | Description |
|----------|---------|-------------|
| `AXON_HYBRID_SEARCH` | `true` | Enable BM42 sparse + dense RRF fusion |
| `AXON_HYBRID_CANDIDATES` | `100` | Candidates per prefetch arm (10-500) |
| `AXON_ASK_HYBRID_CANDIDATES` | `100` | Ask pipeline hybrid window |
| `AXON_HNSW_EF_SEARCH` | `128` | HNSW ef for named-mode search (32-512) |
| `AXON_HNSW_EF_SEARCH_LEGACY` | `64` | HNSW ef for legacy unnamed-mode |

### Ask / RAG tuning

| Variable | Default | Description |
|----------|---------|-------------|
| `AXON_ASK_MAX_CONTEXT_CHARS` | `120000` | Max context characters passed to the LLM (clamped 20000–400000) |
| `AXON_ASK_CANDIDATE_LIMIT` | `150` | Max retrieval candidates per prefetch (clamped 8–300) |
| `AXON_ASK_DOC_FETCH_CONCURRENCY` | `4` | Concurrent document fetches during context build (clamped 1–16) |
| `AXON_ASK_DOC_CHUNK_LIMIT` | `192` | Max chunks per document in context (clamped 8–2000) |
| `AXON_ASK_CHUNK_LIMIT` | `10` | Max total chunks selected for LLM context |
| `AXON_ASK_FULL_DOCS` | `4` | Max full documents included in context |
| `AXON_ASK_BACKFILL_CHUNKS` | `3` | Backfill chunks from top documents to pad context (clamped 0–20) |
| `AXON_ASK_MIN_RELEVANCE_SCORE` | `0.45` | Minimum relevance score for candidate inclusion |
| `AXON_ASK_AUTHORITATIVE_BOOST` | `0.0` | Boost weight for authoritative domains in reranking (clamped 0.0–0.5) |
| `AXON_ASK_AUTHORITATIVE_DOMAINS` | -- | Comma-separated authoritative domains to boost in reranking |
| `AXON_ASK_MIN_CITATIONS_NONTRIVIAL` | `2` | Min unique citations for non-trivial answers (clamped 1–5) |

### Worker tuning

| Variable | Default | Description |
|----------|---------|-------------|
| `AXON_EMBED_DOC_CONCURRENCY` | CPU count | Max concurrent embed docs |
| `AXON_EMBED_DOC_TIMEOUT_SECS` | `300` | Per-document embed timeout |
| `AXON_MAX_PENDING_CRAWL_JOBS` | `100` | Crawl queue cap — new submissions rejected when exceeded (0 = unlimited) |
| `AXON_MAX_PENDING_EMBED_JOBS` | `50` | Embed queue cap (0 = unlimited) |
| `AXON_MAX_PENDING_EXTRACT_JOBS` | `50` | Extract queue cap (0 = unlimited) |
| `AXON_MAX_PENDING_INGEST_JOBS` | `50` | Ingest queue cap (0 = unlimited) |
| `AXON_JOB_STALE_TIMEOUT_SECS` | `300` | Seconds before a running job is considered stale |
| `AXON_JOB_STALE_CONFIRM_SECS` | `60` | Grace period before stale job reclaim |

### Web app

| Variable | Default | Description |
|----------|---------|-------------|
| `AXON_BACKEND_URL` | `http://axon-workers:49000` | Backend URL for Next.js rewrites |
| `AXON_WEB_API_TOKEN` | -- | Primary API/WS auth token (server-only) |
| `AXON_WEB_BROWSER_API_TOKEN` | -- | Second-tier /api/* token (browser) |
| `NEXT_PUBLIC_AXON_API_TOKEN` | -- | Browser-exposed API token |
| `AXON_WEB_ALLOWED_ORIGINS` | -- | Comma-separated allowed origins |
| `AXON_WEB_ALLOW_INSECURE_DEV` | `false` | Allow localhost without auth |
| `AXON_SHELL_WS_TOKEN` | -- | Shell WebSocket auth token |
| `AXON_ALLOWED_CLAUDE_BETAS` | `interleaved-thinking` | Allowed Claude betas for Pulse |

### Neo4j / GraphRAG

| Variable | Default | Description |
|----------|---------|-------------|
| `AXON_NEO4J_URL` | -- | Neo4j HTTP URL (empty = graph features disabled) |
| `AXON_NEO4J_USER` | `neo4j` | Neo4j username |
| `AXON_NEO4J_PASSWORD` | -- | Neo4j password |

### Logging

| Variable | Default | Description |
|----------|---------|-------------|
| `RUST_LOG` | `info` | Rust tracing filter |
| `AXON_LOG_DIR` | `$AXON_DATA_DIR/logs` (default `~/.axon/logs`) | Directory holding the active log + rotated archives |
| `AXON_LOG_FILE` | `axon.log` | Filename of the active log (joined under `AXON_LOG_DIR`); rotated archives are `<file>.1`, `<file>.2`, … |
| `AXON_LOG_MAX_BYTES` | `10485760` | Size threshold (bytes) that triggers rotation. `0` disables rotation (single file grows unboundedly). Default is 10 MB. |
| `AXON_LOG_MAX_FILES` | `3` | Number of rotated archives to retain. `0` truncates without keeping any archive. |

### MCP server

| Variable | Default | Description |
|----------|---------|-------------|
| `AXON_MCP_HTTP_HOST` | `127.0.0.1` | HTTP bind address; non-loopback requires `AXON_MCP_HTTP_TOKEN` |
| `AXON_MCP_HTTP_PORT` | `8001` | HTTP listen port |
| `AXON_MCP_HTTP_TOKEN` | -- | Bearer or `x-api-key` token; required for non-loopback binds |
| `AXON_MCP_ALLOWED_ORIGINS` | -- | Comma-separated allowed origins for MCP HTTP CORS |
| `AXON_MCP_ARTIFACT_DIR` | `$AXON_DATA_DIR/artifacts` (default `~/.axon/artifacts`) | Directory for response artifacts |
| `AXON_INLINE_BYTES_THRESHOLD` | `8192` | Payload size below which auto-inline triggers (0 = disable) |
| `AXON_MCP_EMBED_ALLOWED_ROOTS` | -- | Comma-separated local filesystem roots for MCP embed (unset = local file embedding disabled) |
| `AXON_MCP_EMBED_MAX_LOCAL_BYTES` | -- | Max bytes per local file embedding request via MCP |

### Ask cache

The `[ask.cache]` section in `~/.axon/config.toml` controls the optional
process-local full-document cache used by ask retrieval. It is disabled by
default and only useful for long-lived `axon serve` / `axon mcp` processes.
`max-capacity-bytes` limits the summed `chunk_text` bytes retained in memory;
`ttl-secs` is capped at 300 seconds as a security backstop. When enabled in
`serve` or `mcp`, startup enforces `RLIMIT_CORE=0` to avoid core files
containing cached source text.

### Output and CLI

| Variable | Default | Description |
|----------|---------|-------------|
| `AXON_OUTPUT_DIR` | `$AXON_DATA_DIR/output` (default `~/.axon/output`) | Output directory for file-writing commands |
| `AXON_NO_COLOR` | -- | Disable ANSI color output (any non-empty value) |
| `AXON_NO_WIPE` | -- | Prevent destructive cache wipes |
| `AXON_DOMAINS_DETAILED` | -- | Enable detailed per-domain breakdown in `axon domains` |
| `AXON_EXTRACT_EST_COST_PER_1K_TOKENS` | -- | Estimated cost per 1k tokens shown in `axon extract` output |
| `AXON_SOURCES_FACET_LIMIT` | `100000` | Facet limit for `axon sources` |
| `AXON_DOMAINS_FACET_LIMIT` | `100000` | Facet limit for `axon domains` |
| `AXON_SESSION_INGEST_MAX_BYTES` | -- | Max bytes per session ingest payload |

### Miscellaneous

| Variable | Default | Description |
|----------|---------|-------------|
| `AXON_GIT_SHA` | `dev` | Git SHA baked into Docker image labels |
| `AXON_TEST_QDRANT_URL` | `http://127.0.0.1:53333` | Host-accessible Qdrant URL for integration tests |

## Dev vs container URL resolution

The CLI auto-detects its runtime environment:

- **Inside Docker** (`/.dockerenv` exists): uses container DNS for Qdrant/TEI
- **Outside Docker** (local dev): rewrites to localhost with mapped ports

This means `.env` can use container DNS names -- `normalize_local_service_url()` in `config.rs` handles translation transparently.

## Keeping this file in sync

`docs/CONFIG.md` is the single source of truth for env var documentation. When adding a new env variable:

1. Add it here in the appropriate section.
2. Add it to `.env.example` with a sensible default or blank value and a `[OPTIONAL]`/`[REQUIRED]` comment.
3. If it is MCP-server-specific, also add it to `docs/mcp/ENV.md`.
4. Do not add full env tables to `README.md` — keep that to a short essentials list with a link here.

To spot drift between `.env.example` and this file, extract keys from both and diff:

```bash
# Keys in .env.example (non-comment, non-blank)
grep -v '^\s*#' .env.example | grep '=' | cut -d= -f1 | sort > /tmp/example_keys.txt

# Keys in CONFIG.md table rows (backtick-wrapped identifiers)
grep -oP '`[A-Z][A-Z0-9_]+`' docs/CONFIG.md | tr -d '`' | sort -u > /tmp/config_keys.txt

# Vars in .env.example but missing from CONFIG.md
comm -23 /tmp/example_keys.txt /tmp/config_keys.txt

# Vars in CONFIG.md but missing from .env.example
comm -13 /tmp/example_keys.txt /tmp/config_keys.txt
```
