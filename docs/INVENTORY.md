# Component Inventory -- Axon

Complete listing of all Axon components.

## MCP tools

| Tool | Description |
|------|-------------|
| `axon` | Single entry point with `action`/`subaction` routing for all operations |

Axon exposes one MCP tool with the full operation space routed via the `action` parameter.

### Direct actions (no subaction required)

| Action | Description |
|--------|-------------|
| `ask` | RAG: semantic search + LLM answer synthesis |
| `export` | Export full index manifest to JSON |
| `map` | Discover all URLs at a domain without scraping |
| `query` | Semantic vector search |
| `research` | Web research via Tavily with LLM synthesis |
| `retrieve` | Fetch stored document chunks from Qdrant |
| `scrape` | Scrape URLs to markdown |
| `screenshot` | Capture page screenshot via Chrome |
| `search` | Web search via Tavily, auto-queues crawl jobs |

### Lifecycle action families (subaction required)

| Action | Subactions | Description |
|--------|-----------|-------------|
| `crawl` | `start`, `status`, `cancel`, `list`, `cleanup`, `clear`, `recover` | Full site crawling |
| `extract` | `start`, `status`, `cancel`, `list`, `cleanup`, `clear`, `recover` | LLM-powered structured extraction |
| `embed` | `start`, `status`, `cancel`, `list`, `cleanup`, `clear`, `recover` | Vector embedding into Qdrant |
| `ingest` | `start`, `status`, `cancel`, `list`, `cleanup`, `clear`, `recover` | External source ingestion (GitHub, Reddit, YouTube, sessions) |
| `refresh` | `start`, `status`, `cancel`, `list`, `cleanup`, `clear`, `recover`, `schedule` | Periodic URL re-indexing |
| `graph` | `build`, `status`, `explore`, `stats` | Neo4j knowledge graph operations |
| `artifacts` | `head`, `grep`, `wc`, `read`, `list`, `delete`, `clean`, `search` | MCP artifact file management |

### Info actions

| Action | Description |
|--------|-------------|
| `doctor` | Diagnose service connectivity |
| `domains` | List indexed domains + stats |
| `help` | Return action reference |
| `sources` | List all indexed URLs + chunk counts |
| `stats` | Qdrant collection statistics |
| `status` | Async job queue status |

## MCP resources

| URI | Description |
|-----|-------------|
| `axon://schema/mcp-tool` | Tool schema definition |

## CLI commands

All MCP actions are also available as CLI commands:

| Command | Async | Description |
|---------|-------|-------------|
| `scrape <url>...` | No | Scrape URLs to markdown |
| `crawl <url>...` | Yes | Full site crawl |
| `map <url>` | No | URL discovery without scraping |
| `extract <urls...>` | Yes | LLM-powered structured extraction |
| `search <query>` | No | Web search via Tavily |
| `research <query>` | No | Web research with LLM synthesis |
| `embed [input]` | Yes | Embed into Qdrant |
| `export` | No | Export index manifest |
| `query <text>` | No | Semantic vector search |
| `retrieve <url>` | No | Fetch stored chunks |
| `ask <question>` | No | RAG search + answer |
| `evaluate <question>` | No | RAG vs baseline comparison |
| `suggest [focus]` | No | Suggest new URLs to crawl |
| `ingest <target>` | Yes | Ingest GitHub, Reddit, YouTube |
| `sessions [format]` | No | Ingest AI session exports |
| `sources` | No | List indexed URLs |
| `domains` | No | List indexed domains |
| `stats` | No | Qdrant collection stats |
| `status` | No | Job queue status |
| `doctor` | No | Service connectivity check |
| `debug` | No | Doctor + LLM troubleshooting |
| `mcp` | No | Start MCP stdio server |
| `refresh <url>` | Yes | Periodic re-indexing |
| `graph <sub>` | Depends | Knowledge graph operations |
| `serve` | No | Start web UI supervisor |
| `watch <sub>` | Depends | Scheduled task management |
| `migrate` | No | Collection upgrade (unnamed to named vectors) |

## Infrastructure services

| Service | Image | Exposed Port | Purpose |
|---------|-------|-------------|---------|
| `axon-postgres` | postgres:17-alpine | 53432 | Job persistence |
| `axon-redis` | redis:8.2-alpine | 53379 | Queue state and caching |
| `axon-rabbitmq` | rabbitmq:4.0-management | 45535 | AMQP job queue |
| `axon-qdrant` | qdrant/qdrant:v1.13.1 | 53333, 53334 (gRPC) | Vector store |
| `axon-tei` | ghcr.io/huggingface/text-embeddings-inference | 52000 | Embedding generation |
| `axon-chrome` | docker/chrome/Dockerfile | 6000, 9222 (CDP) | Headless browser |

## App services

| Service | Image | Exposed Port | Purpose |
|---------|-------|-------------|---------|
| `axon-workers` | docker/Dockerfile | 49000, 8001 | Workers + serve + MCP HTTP |
| `axon-web` | docker/web/Dockerfile | 49010 | Next.js dashboard |

## Worker types

| Worker | Queue | Description |
|--------|-------|-------------|
| Crawl | `axon.crawl.jobs` | Full site crawling with sitemap backfill |
| Extract | `axon.extract.jobs` | LLM-powered structured data extraction |
| Embed | `axon.embed.jobs` | TEI embedding + Qdrant upsert |
| Ingest | `axon.ingest.jobs` | GitHub/Reddit/YouTube source ingestion |
| Refresh | `axon.refresh.jobs` | Periodic URL re-indexing |
| Graph | `axon.graph.jobs` | Neo4j entity extraction and graph building |

## Workspace crates

| Crate | Path | Purpose |
|-------|------|---------|
| `cli` | `crates/cli/` | Command handlers for all CLI subcommands |
| `core` | `crates/core/` | Config, HTTP client, content processing |
| `crawl` | `crates/crawl/` | Spider-based crawl engine |
| `ingest` | `crates/ingest/` | GitHub, Reddit, YouTube ingest adapters |
| `jobs` | `crates/jobs/` | Async job framework (AMQP + SQLite backends) |
| `mcp` | `crates/mcp/` | MCP server schema and handlers |
| `services` | `crates/services/` | Typed service layer (consumed by CLI, MCP, web) |
| `vector` | `crates/vector/` | Qdrant ops, TEI embedding, hybrid search |
| `web` | `crates/web/` | WebSocket execution bridge |

## Database tables

| Table | Purpose |
|-------|---------|
| `axon_crawl_jobs` | Crawl job metadata and results |
| `axon_extract_jobs` | Extract job metadata and results |
| `axon_embed_jobs` | Embed job metadata and results |
| `axon_ingest_jobs` | Ingest job metadata and results |

## Scripts

| Script | Purpose |
|--------|---------|
| `scripts/axon` | Wrapper script (auto-sources .env) |
| `scripts/dev-setup.sh` | Bootstrap development environment |
| `scripts/rebuild-fresh.sh` | Build + start Docker containers |
| `scripts/check-container-revisions.sh` | Verify container git SHA matches |
| `scripts/check_dockerignore_guards.sh` | Verify .dockerignore patterns |
| `scripts/enforce_monoliths.py` | Enforce file/function size limits |
| `scripts/generate_mcp_schema_doc.py` | Regenerate MCP-TOOL-SCHEMA.md |
| `scripts/live-test-all-commands.sh` | Integration test all CLI commands |
| `scripts/test-mcp-tools-mcporter.sh` | MCP smoke test suite |
