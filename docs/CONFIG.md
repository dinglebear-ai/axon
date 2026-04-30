# Configuration Reference -- Axon

Axon is configured through three layers: environment variables, the `axon.json` file, and CLI flags.

## Precedence (highest to lowest)

1. CLI flags (`--pg-url`, `--collection`, etc.)
2. Environment variables (`AXON_PG_URL`, `AXON_COLLECTION`, etc.)
3. `axon.json` configuration file
4. Built-in defaults

## Environment files

Two env files are used:

| File | Purpose | Loaded by |
|------|---------|-----------|
| `.env` | App runtime + shared Docker Compose interpolation | Docker Compose (automatic), `dotenvy` in binary |
| `services.env` | Infrastructure container credentials | Docker Compose `env_file:` directive |

```bash
cp .env.example .env
chmod 600 .env
cp .env.example services.env
chmod 600 services.env
```

## axon.json

The `axon.json` file provides structured configuration with schema validation (`axon.schema.json`). Key sections:

| Section | Keys | Purpose |
|---------|------|---------|
| `services` | `qdrant_url`, `tei_url`, `chrome_remote_url`, `neo4j_url`, `backend_url` | Service endpoint URLs |
| `llm` | `base_url`, `model` | LLM provider settings |
| `tei` | `max_retries`, `request_timeout_ms`, `max_client_batch_size`, `embedding_model`, `pooling` | TEI embedding configuration |
| `search` | `hybrid_enabled`, `hybrid_candidates`, `hnsw_ef` | Vector search tuning |
| `ask` | `max_context_chars`, `candidate_limit`, `chunk_limit`, `min_relevance_score` | RAG answer pipeline |
| `embed` | `collection`, `doc_concurrency`, `doc_timeout_secs`, `strict_predelete` | Embedding pipeline |
| `queues` | `crawl`, `extract`, `embed`, `ingest`, `refresh`, `graph` | AMQP queue names |
| `workers` | `ingest_lanes`, `max_pending_crawl_jobs`, `job_stale_timeout_secs` | Worker tuning |
| `graph` | `concurrency`, `llm_model`, `similarity_threshold` | Neo4j graph RAG |
| `acp` | `adapter_cmd`, `prewarm`, `auto_approve`, `max_concurrent_sessions` | ACP orchestration |
| `web` | `allowed_origins`, `allow_insecure_dev`, `docker_socket_path` | Web UI settings |
| `mcp` | `transport`, `http_host`, `http_port`, `artifact_dir` | MCP server config |
| `serve` | `host`, `port` | Backend bridge config |
| `chrome` | `diagnostics`, `proxy`, `user_agent` | Chrome browser settings |
| `logging` | `file`, `max_bytes`, `max_files`, `no_color` | Log output config |
| `output` | `dir`, `extract_est_cost_per_1k_tokens` | Output directory config |
| `ingest` | `github_max_issues`, `github_max_prs`, `download_max_bytes` | Ingest limits |
| `oauth` | `auth_url`, `token_url`, `redirect_uri`, `scopes` | MCP OAuth broker |

## Environment variables by category

### Core runtime (required)

| Variable | Default | Description |
|----------|---------|-------------|
| `QDRANT_URL` | -- | Qdrant vector database URL |
| `TEI_URL` | -- | Text Embeddings Inference URL |

### Host paths

| Variable | Default | Description |
|----------|---------|-------------|
| `AXON_DATA_DIR` | `./data` | Root directory for all persistent data |
| `HOST_HOME` | -- | Host user home (for session ingestion bind mount) |
| `AXON_WORKSPACE` | -- | Host workspace dir mounted into axon-web |
| `HOST_WORKSPACE` | -- | Host path to axon_rust repo |
| `AXON_BIN` | -- | Path to pre-built axon binary inside container |

### Server ports

| Variable | Default | Description |
|----------|---------|-------------|
| `AXON_SERVE_HOST` | `127.0.0.1` | Backend bridge bind address |
| `AXON_SERVE_PORT` | `49000` | Backend bridge port |
| `AXON_WEB_DEV_PORT` | `49010` | Next.js dev server port |
| `SHELL_SERVER_PORT` | `49011` | Shell WebSocket server port |
| `AXON_MCP_HTTP_PORT` | `8001` | MCP HTTP server port |
| `AXON_MCP_HTTP_HOST` | `0.0.0.0` | MCP HTTP server bind address |
| `AXON_MCP_TRANSPORT` | `http` | MCP transport: `http`, `stdio`, or `both` |

### Lite mode

| Variable | Default | Description |
|----------|---------|-------------|
| `AXON_LITE` | -- | Set to `1` to enable lite mode (default; uses SQLite) |
| `AXON_SQLITE_PATH` | `~/.local/share/axon/jobs.db` | SQLite path for lite mode |

**Worker spawn is conditional**, not unconditional, in lite mode. The `LiteBackend` has two construction modes:

- `LiteBackend::new(cfg)` — **enqueue-only**. No workers spawn. Used by `ServiceContext::new()` for short-lived CLI commands (status/list/cancel/fire-and-forget submit).
- `LiteBackend::new_with_workers(cfg)` — spawns in-process tokio workers (crawl + N×embed + extract + N×ingest). Used by `ServiceContext::new_with_workers()` for long-running processes: `axon serve`, MCP server, web routes, and CLI commands that block on `--wait true`.

Spawning workers in a fire-and-forget CLI process orphans claimed jobs at process exit, so the CLI defaults to enqueue-only and lets a separate `serve`/`mcp` process drain the queue.

### TEI embedding

| Variable | Default | Description |
|----------|---------|-------------|
| `TEI_MAX_RETRIES` | `5` | Max retry attempts per request |
| `TEI_REQUEST_TIMEOUT_MS` | `30000` | Per-attempt timeout (clamped 100-600000) |
| `TEI_MAX_CLIENT_BATCH_SIZE` | `128` | Default batch size (auto-splits on 413) |
| `TEI_HTTP_PORT` | `52000` | Host port for TEI container |
| `TEI_EMBEDDING_MODEL` | `Qwen/Qwen3-Embedding-0.6B` | HuggingFace embedding model |
| `TEI_MAX_CONCURRENT_REQUESTS` | `80` | Max concurrent TEI requests |
| `TEI_MAX_BATCH_TOKENS` | `163840` | Max batch tokens |
| `TEI_MAX_BATCH_REQUESTS` | `80` | Max batch requests |
| `TEI_POOLING` | `last-token` | Pooling strategy |
| `TEI_TOKENIZATION_WORKERS` | `8` | Tokenization workers |
| `HF_TOKEN` | -- | HuggingFace token for gated models |

### LLM / ACP

| Variable | Default | Description |
|----------|---------|-------------|
| `OPENAI_BASE_URL` | -- | OpenAI-compatible base URL (legacy) |
| `OPENAI_API_KEY` | -- | API key for LLM provider |
| `OPENAI_MODEL` | -- | Model override for ACP-backed completions |
| `AXON_ASK_AGENT` | `claude` | Which ACP agent handles ask/research |
| `AXON_ACP_ADAPTER_CMD` | -- | Global ACP adapter override |
| `AXON_ACP_ADAPTER_ARGS` | -- | Global ACP adapter args (pipe-delimited) |
| `AXON_ACP_AUTO_APPROVE` | `true` | Auto-approve agent tool permissions |
| `AXON_ACP_MAX_CONCURRENT_SESSIONS` | `8` | Max concurrent ACP sessions |
| `AXON_ACP_TURN_TIMEOUT_MS` | `300000` | Per-turn timeout for Pulse Chat |
| `AXON_ACP_PREWARM` | `true` | Prewarm adapter on startup |
| `AXON_ACP_WS_URL` | -- | Remote ACP WebSocket URL |
| `AXON_ACP_WS_TOKEN` | -- | Remote ACP WebSocket token |

### Queues and collections

| Variable | Default | Description |
|----------|---------|-------------|
| `AXON_COLLECTION` | `cortex` | Qdrant collection name |
| `AXON_CRAWL_QUEUE` | `axon.crawl.jobs` | Crawl job queue |
| `AXON_EXTRACT_QUEUE` | `axon.extract.jobs` | Extract job queue |
| `AXON_EMBED_QUEUE` | `axon.embed.jobs` | Embed job queue |
| `AXON_INGEST_QUEUE` | `axon.ingest.jobs` | Ingest job queue |
| `AXON_GRAPH_QUEUE` | `axon.graph.jobs` | Graph job queue |
| `AXON_INGEST_LANES` | `2` | Parallel ingest worker lanes |

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
| `CHROME_URL` | `http://127.0.0.1:6000` | Spider-rs native CDP var |
| `AXON_CHROME_DIAGNOSTICS` | `false` | Enable browser diagnostics |
| `AXON_CHROME_PROXY` | -- | Proxy URL for Chrome requests |
| `AXON_CHROME_USER_AGENT` | -- | Custom User-Agent |

### Hybrid search

| Variable | Default | Description |
|----------|---------|-------------|
| `AXON_HYBRID_SEARCH` | `true` | Enable BM42 sparse + dense RRF fusion |
| `AXON_HYBRID_CANDIDATES` | `100` | Candidates per prefetch arm (10-500) |
| `AXON_ASK_HYBRID_CANDIDATES` | `150` | Ask pipeline hybrid window |
| `AXON_HNSW_EF_SEARCH` | `128` | HNSW ef for named-mode search (32-512) |
| `AXON_HNSW_EF_SEARCH_LEGACY` | `64` | HNSW ef for legacy unnamed-mode |

### Worker tuning

| Variable | Default | Description |
|----------|---------|-------------|
| `AXON_EMBED_DOC_CONCURRENCY` | CPU count | Max concurrent embed docs |
| `AXON_EMBED_DOC_TIMEOUT_SECS` | `300` | Per-document embed timeout |
| `AXON_EMBED_STRICT_PREDELETE` | `true` | Require pre-delete before upsert |
| `AXON_MAX_PENDING_CRAWL_JOBS` | `100` | Crawl queue cap (0 = unlimited) |
| `AXON_CRAWL_SIZE_WARN_THRESHOLD` | `10000` | Warn above N pages |
| `AXON_JOB_STALE_TIMEOUT_SECS` | `300` | Stale job detection |
| `AXON_JOB_STALE_CONFIRM_SECS` | `60` | Stale confirmation grace period |

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
| `AXON_NEO4J_URL` | -- | Neo4j HTTP URL (empty = disabled) |
| `AXON_NEO4J_USER` | `neo4j` | Neo4j username |
| `AXON_NEO4J_PASSWORD` | -- | Neo4j password |
| `AXON_GRAPH_CONCURRENCY` | `4` | Parallel extraction jobs |
| `AXON_GRAPH_LLM_URL` | `http://localhost:11434` | Ollama/OpenAI URL for extraction |
| `AXON_GRAPH_LLM_MODEL` | `qwen3.5:4b` | Graph extraction model |
| `AXON_GRAPH_SIMILARITY_THRESHOLD` | `0.75` | Cross-document edge threshold |
| `AXON_GRAPH_SIMILARITY_LIMIT` | `20` | Max similar URLs for edges |
| `AXON_GRAPH_CONTEXT_MAX_CHARS` | `2000` | Graph context chars for `ask --graph` |

### Logging

| Variable | Default | Description |
|----------|---------|-------------|
| `RUST_LOG` | `info` | Rust tracing filter |
| `AXON_LOG_FILE` | -- | Structured log file path |
| `AXON_LOG_MAX_BYTES` | `10485760` | Log rotation size (10 MB) |
| `AXON_LOG_MAX_FILES` | `3` | Rotated log files retained |
| `AXON_NO_COLOR` | -- | Disable ANSI color output |

### Miscellaneous

| Variable | Default | Description |
|----------|---------|-------------|
| `AXON_MCP_ARTIFACT_DIR` | `$AXON_DATA_DIR/axon/artifacts` | MCP response artifact directory |
| `AXON_INLINE_BYTES_THRESHOLD` | `8192` | Auto-inline payload threshold |
| `AXON_OUTPUT_DIR` | `$AXON_DATA_DIR/axon/output` | Output directory for file-writing commands |
| `AXON_GIT_SHA` | `dev` | Git SHA baked into Docker labels |
| `AXON_NO_WIPE` | -- | Prevent destructive cache wipes |

## Dev vs container URL resolution

The CLI auto-detects its runtime environment:

- **Inside Docker** (`/.dockerenv` exists): uses container DNS for Qdrant/TEI
- **Outside Docker** (local dev): rewrites to localhost with mapped ports

This means `.env` can use container DNS names -- `normalize_local_service_url()` in `config.rs` handles translation transparently.
