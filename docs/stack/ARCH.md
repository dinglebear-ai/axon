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

All modes share the same services layer (`crates/services/`), ensuring consistent behavior across CLI and MCP interfaces.

## Services layer

The services layer is the API boundary between all consumers (CLI, MCP, web) and the underlying infrastructure:

```
CLI handlers  ─┐
MCP handlers  ─┼── services::{query, ask, sources, ...} ── vector/ops, jobs, ...
Web routes    ─┘
```

Each service function:
- Takes typed input parameters
- Returns a typed result struct (defined in `crates/services/types/service.rs`)
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

**Docker (production):** Workers run as s6-supervised services inside `axon-workers` container. Each drops to UID 1001 via `s6-setuidgid`.

**Local dev:** `axon serve` supervises all workers in-process. Or run individually: `axon crawl worker`.

**Lite mode:** Workers run in the same tokio runtime as the CLI. No separate processes.

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
   ├── ACP adapter generates answer with citations
   └── Optional: Neo4j graph context injection
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

## Web UI architecture

The web UI consists of:

| Component | Technology | Purpose |
|-----------|-----------|---------|
| Next.js app | React + App Router | Dashboard, omnibox, Pulse workspace |
| Backend bridge | Axum + WebSocket | Command execution, Docker stats |
| Shell server | Node.js WebSocket | Terminal emulation |
| Proxy | Next.js API routes | Auth + rewrite to backend |

The web UI communicates with the backend via:
- HTTP API routes (proxied through Next.js `/api/*`)
- WebSocket connections for real-time command output
- Shell WebSocket for terminal sessions

### Auth model

Two-tier token system:
1. `AXON_WEB_API_TOKEN` -- server-only, gates `/api/*` and `/ws`
2. `AXON_WEB_BROWSER_API_TOKEN` -- browser-visible, gates `/api/*` only

## Serve supervisor

`axon serve` acts as a process supervisor, managing:

| Child process | Default port | Purpose |
|--------------|-------------|---------|
| Backend bridge | 49000 | HTTP/WS API server |
| MCP HTTP | 8001 | MCP streamable-http server |
| Workers (6) | -- | Job processors |
| Shell server | 49011 | Terminal WebSocket |
| Next.js | 49010 | Web UI dev server |

All children are supervised: if one exits, the supervisor restarts it.

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

The `Config` struct in `crates/core/config.rs` merges all sources at startup.

## See also

- [TECH.md](TECH.md) -- technology choices
- [PRE-REQS.md](PRE-REQS.md) -- prerequisites
- [../mcp/PATTERNS.md](../mcp/PATTERNS.md) -- MCP code patterns
- [../ARCHITECTURE.md](../ARCHITECTURE.md) -- detailed architecture doc
