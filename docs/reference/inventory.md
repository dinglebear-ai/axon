# Component Inventory -- Axon

Complete listing of all Axon components.

> Current runtime inventory. The canonical source-pipeline contract is recorded
> in [`../pipeline-unification/`](../pipeline-unification/README.md). Removed
> commands, actions, and routes are clean-break absence requirements, not
> compatibility surfaces.

## MCP tools

| Tool | Description |
|------|-------------|
| `axon` | Single entry point with `action`/`subaction` routing for all operations |

Axon exposes one MCP tool with the full operation space routed via the `action` parameter.

The full current action set is defined by `MCP_ACTION_SPECS` in
`crates/axon-mcp/src/server/authz.rs`; see
`docs/reference/mcp/tool-schema.md` for the current generated runtime snapshot.

### Direct actions (no subaction required)

| Action | Description |
|--------|-------------|
| `ask` | RAG: semantic search + LLM answer synthesis |
| `brand` | Analyze a URL's brand identity |
| `capabilities` | Machine-readable server capability document |
| `diff` | Diff two URLs |
| `doctor` | Diagnose service connectivity |
| `endpoints` | Discover API endpoints from page HTML and JS bundles |
| `evaluate` | RAG vs baseline with LLM judge |
| `help` | Return action reference |
| `map` | Discover all URLs at a domain without scraping |
| `query` | Semantic vector search |
| `research` | Web research via SearXNG/Tavily with LLM synthesis and Source auto-indexing |
| `resolve` | Resolve source identity and adapter route without acquiring content |
| `retrieve` | Fetch stored document chunks from Qdrant |
| `screenshot` | Capture page screenshot via Chrome |
| `search` | Web search via SearXNG/Tavily, auto-queues Source jobs |
| `source` | Unified source acquisition/indexing |
| `status` | Job queue, worker, and service status |
| `summarize` | Scrape and summarize one or more URLs |
| `suggest` | Suggest new documentation URLs to index |

### Lifecycle action families (subaction required)

| Action | Subactions | Description |
|--------|-----------|-------------|
| `extract` | `start` | Start LLM-powered structured extraction; use `jobs` for lifecycle |
| `graph` | `kinds`, `resolve`, `query`, `node`, `edge`, `source` | Read-only SourceGraph queries |
| `jobs` | `list`, `get`, `status`, `events`, `stream`, `cancel`, `retry`, `recover`, `cleanup`, `clear` | Unified durable job lifecycle |
| `memory` | `remember`, `search`, `show`, `link`, `supersede`, `context`, ... | Persistent memory lifecycle |
| `providers` | `list`, `get` | Provider capability and health discovery |
| `prune` | `plan`, `exec` | Cleanup planning/execution behind `axon-prune` |
| `watch` | `create`, `list`, `get`, `status`, `exec`, `history`, `update`, `pause`, `resume`, `delete` | Source-request-backed watches |

### REST-only or CLI-only operations

Removed source-family MCP actions (`scrape`, `crawl`, `embed`, `ingest`,
`code_search`, `vertical_scrape`) are not exposed. Use `action=source`.

`debug`, `domains`, `migrate`, `setup`, `sources`, `stats`, and artifact file serving are
documented in `docs/reference/api-parity.md`. They are not dedicated MCP action
routes in the generated `docs/reference/mcp/tool-schema.md` contract.

## MCP resources

| URI | Description |
|-----|-------------|
| `axon://schema/mcp-tool` | Tool schema definition |

## CLI commands

The full command surface is defined by the `CommandKind` enum in
`crates/axon-core/src/config/types/enums.rs`. Many commands are also exposed as
MCP actions (see the MCP tables above).

### Web and extraction

| Command | Async | Description |
|---------|-------|-------------|
| `source <target>` / bare `<target>` | Yes | Acquire, normalize, embed, and optionally register a watch for any supported source |
| `scrape <url>...` | Yes | One-page SourceRequest projection with clean content output and embedding by default |
| `map <url>` | No | URL discovery without scraping |
| `endpoints <url>...` | No | Discover API endpoints from page HTML and JavaScript bundles |
| `search <query>` | No | Web search via SearXNG/Tavily, auto-queues Source jobs |
| `research <query>` | No | Web research with LLM synthesis |
| `extract <urls...>` | Yes | LLM-powered structured extraction |
| `screenshot <url>...` | No | Capture a full-page screenshot via headless Chrome |
| `diff <url-a> <url-b>` | No | Diff two URLs — show what changed |
| `brand <url>` | No | Analyze a URL's brand identity (colors, fonts, logos, favicon) |

### Vector and RAG

| Command | Async | Description |
|---------|-------|-------------|
| `query <text>` | No | Semantic vector search over indexed knowledge; excludes local `code-search` vectors |
| `retrieve <url>` | No | Fetch stored chunks from Qdrant by URL |
| `ask <question>` | No | RAG search + LLM answer |
| `evaluate <question>` | No | RAG vs baseline with independent LLM judge |
| `train` | No | Collect human preference votes for retrieved RAG candidates |
| `summarize <url>...` | No | Scrape one or more URLs and summarize them |
| `suggest [focus]` | No | Suggest new documentation URLs to index |
| `sources` | No | List indexed source URLs with chunk counts |
| `domains` | No | List indexed domains with stats |
| `stats` | No | Qdrant collection + SQLite job statistics |
| `prune` | No | Plan or execute cleanup over sources, generations, collections, duplicate policies, and targeted removal selectors |
| `migrate` | No | Collection upgrade (unnamed to named vectors) |

### Jobs and imports

| Command | Async | Description |
|---------|-------|-------------|
| `status` | No | Async job queue status |
| `sessions [format]` | No | Index AI session exports (Claude/Codex/Gemini) |
| `watch <sub>` | Depends | Manage recurring watch definitions and runs |
| `monitor` | No | Monitor job lifecycle events as a line-oriented stream |
| `sync` | No | Reconcile pending canonical source artifacts and publication state |

### Runtime and setup

| Command | Async | Description |
|---------|-------|-------------|
| `debug` | No | Doctor diagnostics + LLM-assisted troubleshooting |
| `doctor` | No | Check connectivity to all required services |
| `mcp` | No | Start MCP stdio or unified HTTP runtime |
| `serve` | No | Start unified HTTP server (`/mcp`, web panel, `/v1/*`) |
| `setup` | No | Initialize and inspect Axon infrastructure |
| `preflight` | No | Check host prerequisites and service readiness |
| `smoke` | No | Run source/ask smoke checks against the running stack |
| `compose` | No | Manage the local Docker Compose service stack |
| `completions <shell>` | No | Generate shell completions (bash, zsh, fish) |
| `config <sub>` | No | Read/write entries in `~/.axon/.env` and `~/.axon/config.toml` |

## Infrastructure services

| Service | Image | Exposed Port | Purpose |
|---------|-------|-------------|---------|
| `axon-qdrant` | qdrant/qdrant:v1.18.2 | 53333, 53334 (gRPC) | Vector store |
| `axon-tei` | ghcr.io/huggingface/text-embeddings-inference | 52000 | Embedding generation |
| `axon-chrome` | config/chrome/Dockerfile | 6000, 9222/9223 (CDP) | Headless browser |

## App services

| Service | Image | Exposed Port | Purpose |
|---------|-------|-------------|---------|
| `axon` | config/Dockerfile | 8001 | Unified Axon HTTP server (`serve`) |

## Worker types

| Worker | Durable kind | Description |
|--------|--------------|-------------|
| Source | `source` | Web page/site/docs, local files, hosted git, feeds, registry, Reddit, YouTube, sessions, and other source-family indexing |
| Extract | `extract` | LLM-powered structured extraction |
| Watch scheduler | `watch` | Recurring watches that enqueue Source jobs when changes are detected |
| Prune | `prune` | Cleanup planning/execution over vectors, artifacts, and source generations |

## Source modules

| Module | Path | Purpose |
|-------|------|---------|
| `cli` | `crates/axon-cli/` | Command handlers for all CLI subcommands |
| `core` | `crates/axon-core/` | Config, HTTP safety, paths, content helpers, redaction, artifacts |
| `adapters` | `crates/axon-adapters/` | Source-owned acquisition adapters |
| `document` | `crates/axon-document/` | Document preparation and chunk build helpers |
| `parse` | `crates/axon-parse/` | Parser-facing source facts and graph candidates |
| `prune` | `crates/axon-prune/` | Cleanup planning/execution, dedupe, purge, receipts |
| `jobs` | `crates/axon-jobs/` | SQLite-backed durable job framework |
| `mcp` | `crates/axon-mcp/` | MCP server schema and handlers |
| `services` | `crates/axon-services/` | Typed service layer (consumed by CLI, MCP, web) |
| `vectors` | `crates/axon-vectors/` | Qdrant ops, payload validation, hybrid search |
| `web` | `crates/axon-web/` | Static setup panel, MCP HTTP, and `/v1/*` REST routes (`/v1/ask`, `/v1/query`, `/v1/sources`, job lifecycle, etc.) |

## Database tables

| Table family | Purpose |
|-------|---------|
| `jobs`, `job_attempts`, `job_stages`, `job_events` | Durable job lifecycle, stage, and event state |
| `sources`, `source_generations`, `source_manifests`, `source_items`, `document_status` | Source ledger and manifest state |
| `provider_reservations` | Provider capacity reservation state |
| `job_artifacts` | Artifacts attached to durable jobs |

## Scripts

| Script | Purpose |
|--------|---------|
| `scripts/axon` | Wrapper script (auto-sources `~/.axon/.env`, with repo `.env` fallback) |
| `scripts/dev-setup.sh` | Bootstrap development environment |
| `scripts/enforce_monoliths.py` | Enforce file/function size limits |
| `scripts/generate_mcp_schema_doc.py` | Regenerate MCP-TOOL-SCHEMA.md |
| `scripts/live-test-all-commands.sh` | Integration test all CLI commands |
| `scripts/test-client-server-mode.sh` | Legacy CLI client/server smoke test if present |
| `scripts/test-mcp-tools-mcporter.sh` | MCP smoke test suite |
