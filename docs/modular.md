# Axon Modularization Vision

## Deployable Units

### 1. Crawler (MCP Server — Standalone)
Minimal deployment: just the crawler, no infrastructure dependencies.
- No AMQP, Postgres, Redis, RabbitMQ, or Qdrant
- Retains all crawler features: scrape, crawl, map, extract, search, research
- Output written to local filesystem (`--output-dir`)
- MCP server interface (`axon mcp`)
- Spider.rs + Chrome CDP (optional) still fully supported

**Built-in job queue via SQLite + JSON** — part of core crawler, no extra services:
- Single `.db` file, zero infrastructure
- Job tables mirror the current schema (`config_json`, `result_json` columns — JSON1 built-in)
- Workers claim jobs atomically via `BEGIN IMMEDIATE` + WAL mode; safe for multiple local workers
- Cancel tracked via `canceled_at` column (replaces Redis cancel keys)
- Polling interval replaces RabbitMQ dispatch (typically 500ms–2s)
- Trade-off: single-machine only (all workers share the same file); no distributed fanout

### 2. RAG Pipeline
Requires crawler output or direct input. Minimum viable RAG:
- **Crawler** (output dir as source)
- **Embeddings** — HF TEI (self-hosted) or OpenAI-compatible external API
- **Vector DB** — bring your own (Qdrant default, but pluggable)

Full RAG stack adds:
- **Jobs** — Redis + Postgres + RabbitMQ (async job queue)
- **Knowledge Graph** — Neo4j
- **Local Synthesis** — Ollama + local model

RAG pipeline CAN operate without the jobs layer — generate embeddings directly from crawler output dir.

### 3. Web UI (Modular)
Base web app = WebUI + ACP (no terminal, no file explorer).

Optional modules:
- **Files** — file explorer / browser
- **Editor** — Plate.js rich text editor
- **Terminal** — shell access
- **Jobs** — job queue management UI
- **RAG** — modular RAG UI (search, ask, sources, stats)

### 4. Full Package
Everything: Crawler + RAG (jobs + embeddings + Qdrant + Neo4j + Ollama) + Web UI (all modules)

---

## Deployment Matrix

| Deployment | Crawler | Jobs (SQLite) | Embeddings | Vector DB | Jobs (full) | Graph | Web UI |
|---|---|---|---|---|---|---|---|
| Crawler only | ✅ | ✅ built-in | ❌ | ❌ | ❌ | ❌ | ❌ |
| Crawler + RAG (minimal) | ✅ | ✅ built-in | ✅ | ✅ | ❌ | ❌ | ❌ |
| Crawler + RAG (full) | ✅ | ✅ built-in | ✅ | ✅ | ✅ | ✅ | ❌ |
| Web UI (base) | ✅ | ✅ built-in | ❌ | ❌ | ❌ | ❌ | ✅ base |
| Full package | ✅ | ✅ built-in | ✅ | ✅ | ✅ | ✅ | ✅ all |

> **Jobs (SQLite)** = lightweight queue built into core crawler (single file, single machine).
> **Jobs (full)** = Redis + Postgres + RabbitMQ for distributed/high-throughput workloads.

---

## RAG Sub-Components

| Module | Services | Purpose |
|---|---|---|
| `jobs` | Redis + Postgres + RabbitMQ | Async job queue for crawl/embed/extract |
| `embeddings` | HF TEI or OpenAI-compatible | Text → vector conversion |
| `vector-db` | Qdrant (default, pluggable) | Vector storage + semantic search |
| `graph` | Neo4j | Knowledge graph / GraphRAG |
| `synthesis` | Ollama + local model | Local LLM for ask/evaluate/research |

---

## Deployment Model

**Single binary, env-var gated features.**
No separate builds or cargo feature flags per tier — one binary ships everything. Active features are determined at runtime by which env vars are present:

| Env var present | Feature unlocked |
|---|---|
| `TEI_URL` | Embeddings enabled |
| `QDRANT_URL` | Vector DB enabled |
| `AXON_AMQP_URL` + `AXON_PG_URL` + `AXON_REDIS_URL` | Full jobs stack (replaces SQLite queue) |
| `AXON_NEO4J_URL` | Knowledge graph enabled |
| `OPENAI_BASE_URL` | LLM synthesis enabled (ask, extract, research) |

Without any of the above, the binary runs as a standalone crawler with SQLite job queue. Each service is detected at startup and the binary gracefully degrades — no crashes on missing services.

**Deployment targets:**
- **Local** — run the binary directly, SQLite queue, no Docker required
- **Docker Compose** (preferred) — compose files per tier, infrastructure services only; binary runs as a container or local process

Compose files:
- `docker-compose.crawler.yml` — Chrome CDP only (optional)
- `docker-compose.rag.yml` — crawler + TEI + Qdrant
- `docker-compose.full.yml` — everything

---

## Next Steps

1. **Phase 1 — Standalone Crawler MCP**
   - Feature-flag or cargo feature to compile out AMQP/Postgres/Redis/Qdrant deps
   - MCP server works with filesystem-only output
   - Single binary, zero infrastructure required
   - Target: `cargo build --features crawler-only`

2. **Phase 2 — RAG Modularization**
   - Split `crates/jobs/` behind `jobs` feature flag
   - Split `crates/vector/` into `embeddings` + `vector-db` + `graph` crates
   - Direct-embed path (no jobs) from crawler output dir

3. **Phase 3 — Web UI Modules**
   - Base app = WebUI + ACP
   - Feature-flagged modules: files, editor, terminal, jobs, rag

4. **Phase 4 — Crate Separation**
   - Independent crates publishable/deployable separately
   - Shared types crate for cross-crate interfaces
