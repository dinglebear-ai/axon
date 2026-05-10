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
| `serve` | No | Start unified HTTP server (`/mcp`, web panel, `/v1/*`) |
| `watch <sub>` | Depends | Scheduled task management |
| `migrate` | No | Collection upgrade (unnamed to named vectors) |

## Infrastructure services

| Service | Image | Exposed Port | Purpose |
|---------|-------|-------------|---------|
| `axon-qdrant` | qdrant/qdrant:v1.13.1 | 53333, 53334 (gRPC) | Vector store |
| `axon-tei` | ghcr.io/huggingface/text-embeddings-inference | 52000 | Embedding generation |
| `axon-chrome` | config/chrome/Dockerfile | 6000, 9222/9223 (CDP) | Headless browser |

## App services

| Service | Image | Exposed Port | Purpose |
|---------|-------|-------------|---------|
| `axon` | config/Dockerfile | 8001 | Unified Axon HTTP server (`serve`) |

## Worker types

| Worker | SQLite table | Description |
|--------|--------------|-------------|
| Crawl | `axon_crawl_jobs` | Full site crawling with sitemap backfill |
| Extract | `axon_extract_jobs` | LLM-powered structured data extraction |
| Embed | `axon_embed_jobs` | TEI embedding + Qdrant upsert |
| Ingest | `axon_ingest_jobs` | GitHub/Reddit/YouTube/source-session ingestion |

## Source modules

| Module | Path | Purpose |
|-------|------|---------|
| `cli` | `src/cli/` | Command handlers for all CLI subcommands |
| `core` | `src/core/` | Config, HTTP client, content processing |
| `crawl` | `src/crawl/` | Spider-based crawl engine |
| `ingest` | `src/ingest/` | GitHub, Reddit, YouTube ingest adapters |
| `jobs` | `src/jobs/` | SQLite-backed job framework |
| `mcp` | `src/mcp/` | MCP server schema and handlers |
| `services` | `src/services/` | Typed service layer (consumed by CLI, MCP, web) |
| `vector` | `src/vector/` | Qdrant ops, TEI embedding, hybrid search |
| `web` | `src/web/` | Static setup panel, `/v1/ask`, and client/server action routes |

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
| `scripts/axon` | Wrapper script (auto-sources `~/.axon/.env`, with repo `.env` fallback) |
| `scripts/dev-setup.sh` | Bootstrap development environment |
| `scripts/enforce_monoliths.py` | Enforce file/function size limits |
| `scripts/generate_mcp_schema_doc.py` | Regenerate MCP-TOOL-SCHEMA.md |
| `scripts/live-test-all-commands.sh` | Integration test all CLI commands |
| `scripts/test-client-server-mode.sh` | CLI client/server smoke test |
| `scripts/test-mcp-tools-mcporter.sh` | MCP smoke test suite |
