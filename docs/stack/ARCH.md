# Architecture Overview -- Axon

## Dual-mode design

Axon is a single Rust binary that operates in two modes:

```
                    +-----------+
                    |  axon.rs  |  (single binary)
                    +-----+-----+
                          |
          +---------------+
          |               |
    +-----+-----+  +-----+-----+
    |  CLI mode  |  | MCP mode  |
    | axon <cmd> |  | axon mcp  |
    +-----+-----+  +-----+-----+
          |               |
          +-------+-------+
                  |
            +-----+-----+
            |  Services  |
            |   Layer    |
            +-----+-----+
                  |
          +-------+-------+
          |               |
    +-----+-----+  +-----+-----+
    |   Jobs    |  |   Vector  |
    | Framework |  |    Ops    |
    | (SQLite)  |  |           |
    +-----------+  +-----+-----+
                         |
                   +-----+-----+
                   |  Qdrant   |
                   |  (vector  |
                   |   store)  |
                   +-----------+
```

All modes share the same services layer (`src/services/`), ensuring consistent behavior across CLI and MCP interfaces.

## Services layer

The services layer is the API boundary between all consumers (CLI, MCP, web) and the underlying infrastructure:

```
CLI handlers  ─┐
MCP handlers  ─┼── services::{query, ask, sources, ...} ── vector/ops, jobs, ...
Web routes    ─┘
```

Each service function:
- Takes typed input parameters
- Returns a typed result struct (defined in `src/services/types/service.rs`)
- Has no stdout side-effects
- Can be called from any entry point

## Worker topology

Worker types run in-process, processing SQLite-backed jobs:

| Worker | Processing |
|--------|------------|
| Crawl | Spider-based site crawling with render mode switching |
| Extract | LLM-powered structured data extraction |
| Embed | TEI embedding + Qdrant upsert |
| Ingest | Source ingestion (GitHub, Reddit, YouTube) |

### Worker deployment

**Docker:** The `axon` container runs the unified server. Jobs are stored in
SQLite and drained by in-process workers in the same runtime.

**Local dev:** `axon serve` runs workers in-process. Or run individual worker
commands such as `axon crawl worker` for focused debugging.

## Async job lifecycle

```
Submitted ─> Pending ─> Running ─> Completed
                 │          │
                 │          ├─> Failed
                 │          └─> Cancelled
                 └─> Stale (watchdog reclaim)
```

Jobs are persisted in SQLite. The `JobBackend` trait abstracts the storage backend.

Key behaviors:
- `--wait false` (default): fire-and-forget, returns job ID immediately
- `--wait true`: blocks until completion
- Stale detection: `AXON_JOB_STALE_TIMEOUT_SECS` (300s) + confirmation grace period
- Cancel: sets cancellation flag in SQLite, worker checks on next iteration

## Data flow: crawl to RAG

```
1. axon crawl https://docs.example.com
   ├── Spider crawls pages (HTTP or Chrome)
   ├── Auto-switch: HTTP first, Chrome if >60% thin pages
   ├── Sitemap backfill discovers missed URLs
   └── Pages saved as markdown

2. axon embed (automatic after crawl)
   ├── chunk_text(): 2000 chars, 200 overlap
   ├── TEI generates dense embeddings
   ├── BM42 sparse vectors computed locally
   └── Qdrant upsert (named-mode: dense + sparse)

3. axon ask "How does X work?"
   ├── Hybrid search: dense + BM42 with RRF fusion
   ├── Candidate selection and re-ranking
   ├── Context assembly (120K char limit)
   └── Gemini headless generates answer with citations
```

## MCP request flow

```
MCP Client (Claude Code / Codex / Gemini)
    │
    ▼
Transport (stdio / streamable-http)
    │
    ▼
rmcp framework (JSON-RPC handling)
    │
    ▼
AxonMcpServer::call_tool()
    │
    ▼
Schema parser (serde strict parsing)
    │
    ▼
Action dispatcher (match on action enum)
    │
    ▼
Service function (typed result)
    │
    ▼
Response formatter (artifact or inline)
    │
    ▼
MCP response (canonical envelope)
```

## Web panel architecture

The current web surface is served by `axon serve` on the same listener as MCP
and the first-party HTTP API.

| Component | Technology | Purpose |
|-----------|-----------|---------|
| Embedded assets | TypeScript build output | Setup/config panel |
| Axum routes | Rust | Panel state, login, config, setup, and ops APIs |
| MCP route | rmcp + Axum | Streamable HTTP MCP endpoint |
| Client/server routes | Axum | Direct `/v1` REST routes for server-mode CLI commands |

The removed Next.js dashboard, command WebSocket bridge, shell WebSocket, and
download routes are historical surfaces only.

### Auth model

The web panel uses local setup/session cookies for panel state. MCP and
first-party REST routes share the HTTP auth boundary controlled by
`AXON_MCP_HTTP_TOKEN` or `AXON_MCP_AUTH_MODE=oauth`.

## Serve runtime

`axon serve` runs one Axum server:

| Route group | Default port | Purpose |
|-------------|--------------|---------|
| `/` and `/api/panel/*` | 8001 | Embedded setup/config panel |
| `/mcp` | 8001 | MCP streamable HTTP |
| `/v1/ask` | 8001 | Ask endpoint |
| `/v1/capabilities`, direct `/v1` routes | 8001 | CLI client/server mode |

Jobs are stored in SQLite and drained by in-process workers when the service
context is worker-enabled.

## Configuration resolution

```
CLI flags (highest precedence)
    │
    ▼
Environment variables ($AXON_*)
    │
    ▼
~/.axon/config.toml (tuning knobs, safe to commit)
    │
    ▼
Built-in defaults (lowest precedence)
```

The `Config` struct in `src/core/config.rs` merges all sources at startup.

## See also

- [TECH.md](TECH.md) -- technology choices
- [PRE-REQS.md](PRE-REQS.md) -- prerequisites
- [../mcp/PATTERNS.md](../mcp/PATTERNS.md) -- MCP code patterns
- [../ARCHITECTURE.md](../ARCHITECTURE.md) -- detailed architecture doc
