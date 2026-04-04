# Axon

Rust-based crawl, scrape, ingest, embed, query, and RAG engine with a unified CLI, MCP server, async workers, and web UI. This repo is the research/runtime backbone for the Axon stack.

## What this repository ships

- `main.rs`, `lib.rs`: CLI entrypoint and dispatch
- `crates/cli/`: CLI handlers
- `crates/core/`: shared config, HTTP, content, and core types
- `crates/crawl/`: crawl engine
- `crates/jobs/`: async job runtime, queues, and workers
- `crates/vector/`: Qdrant/embedding/search/RAG operations
- `crates/mcp/`: MCP server and action router
- `apps/web/`: Next.js web UI
- `docs/`: architecture, MCP, auth, graph, export, restore, testing, and service references
- `docker/` and compose files: infra and runtime deployment

## Runtime surfaces

Axon is both:

- a CLI binary: `axon`
- an MCP server subcommand: `axon mcp`
- a local stack supervisor: `axon serve`

### Major CLI commands

| Command | Purpose |
| --- | --- |
| `scrape` | Scrape URLs to markdown or other output formats |
| `crawl` | Crawl sites asynchronously or synchronously |
| `map` | Discover URLs without scraping |
| `extract` | Structured extraction |
| `search` | Web search and optional crawl seeding |
| `research` | Search with synthesized research output |
| `embed` | Embed files, dirs, or URLs into Qdrant |
| `query` | Semantic search |
| `retrieve` | Fetch stored document chunks |
| `ask` | RAG answer generation |
| `evaluate` | Baseline vs RAG evaluation with judging |
| `suggest` | Crawl-target suggestion |
| `ingest` | GitHub, Reddit, or YouTube ingestion |
| `sessions` | Ingest AI session exports |
| `sources`, `domains`, `stats`, `status`, `doctor`, `debug` | Inspection and diagnostics |
| `refresh` | Re-index scheduling and maintenance |
| `graph` | Knowledge-graph operations |
| `watch` | Scheduled task management |
| `serve` | Local app stack supervisor |
| `mcp` | MCP stdio server |

The MCP server exposes a single `axon` tool with `action`/`subaction` routing. The canonical MCP contract lives in `docs/MCP.md` and `docs/MCP-TOOL-SCHEMA.md`.

## Quick start

### Infrastructure

```bash
docker compose -f docker-compose.services.yaml up -d
```

### Local checks

```bash
./scripts/axon doctor
./scripts/axon scrape https://example.com --wait true
```

### Local stack supervisor

```bash
cargo run --bin axon -- serve
```

### MCP server

```bash
cargo run --bin axon -- mcp
```

## Configuration

This repo uses a large `.env.example`; the important core settings are:

```bash
AXON_SERVE_PORT=49000
AXON_WEB_DEV_PORT=49010
SHELL_SERVER_PORT=49011
AXON_MCP_HTTP_PORT=8001
AXON_MCP_TRANSPORT=http
AXON_PG_URL=postgresql://...
AXON_REDIS_URL=redis://...
AXON_AMQP_URL=amqp://...
QDRANT_URL=http://...
TEI_URL=http://...
AXON_COLLECTION=cortex
```

Optional subsystems include:

- Neo4j graph retrieval
- OpenAI-compatible LLM endpoints
- Chrome rendering and diagnostics
- source-ingestion credentials for GitHub, Reddit, and Tavily

## Development commands

```bash
just setup
just check
just test
just fmt
just clippy
just build
just services-up
just serve
```

Key workflows:

- `just services-up`: start infrastructure only
- `just serve`: start the local app stack supervisor
- `just mcp-smoke`: MCP smoke test harness
- `just verify`: Docker guards, fmt, clippy, check, and tests

## Verification

Recommended:

```bash
just check
just test
just fmt-check
just clippy
```

For end-to-end stack work:

```bash
just services-up
just serve
```

## Related docs

- `CLAUDE.md`: canonical high-level command and architecture notes
- `docs/ARCHITECTURE.md`: subsystem map
- `docs/MCP.md`: MCP runtime and design guide
- `docs/MCP-TOOL-SCHEMA.md`: MCP schema source of truth
- `docs/TESTING.md`: test strategy and mcporter examples
- `CHANGELOG.md`: release history

## License

MIT
