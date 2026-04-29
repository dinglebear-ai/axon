# axon Simplification Plan
*Date: 2026-04-27*

## Problem Statement

The axon repo has accumulated ~170k LOC of Rust plus ~546k LOC of TypeScript serving two audiences that are no longer both needed:

1. **The CLI/MCP core** — what users actually need: `axon scrape`, `axon crawl`, `axon ask`, `axon mcp`, etc. Backed by SQLite + in-process workers (lite mode). External dependencies: Qdrant, TEI, optionally Chrome.

2. **The full-stack web product** — a Next.js dashboard, WebSocket execution bridge, Postgres job persistence, Redis caching, RabbitMQ AMQP queue, Neo4j graph store, and a process supervisor that orchestrates all of the above. This is ~60% of the codebase by file count and >90% by raw LOC.

The dual-runtime architecture (lite mode vs full mode) was designed to let the two coexist. But "full mode" is now baggage: it forces every contributor to understand two job backends, two service runtimes, and a five-service Docker stack just to run the binary. The lite mode already covers all CLI and MCP use cases. Full mode adds complexity without benefit.

**Goal:** Strip the repo to the lean core. Keep the Rust binary, CLI, MCP, SQLite/in-process execution. Remove everything else.

---

## Target End-State Architecture

```
axon binary
├── CLI commands (scrape, crawl, embed, query, ask, ingest, ...)
├── MCP server (stdio + optional HTTP)
└── In-process job execution (SQLite + tokio workers)

External services:
├── Qdrant        (vector store — required)
├── TEI           (embeddings — required)
└── Chrome        (headless browser — optional, for JS-heavy sites)

Removed:
├── apps/web/     (Next.js dashboard)
├── crates/web/   (axum HTTP server + WS bridge)
├── Postgres      (job persistence → SQLite only)
├── Redis         (cache → removed)
├── RabbitMQ      (AMQP queue → in-process channels)
├── Neo4j         (graph store → feature removed)
├── crates/jobs/common/      (Postgres/AMQP pools)
├── crates/jobs/worker_lane/ (AMQP polling abstraction)
├── crates/jobs/crawl/runtime/ (AMQP consumer loops)
├── crates/jobs/full.rs      (FullBackend adapter)
├── crates/jobs/graph/       (Neo4j integration)
├── crates/jobs/refresh/     (periodic scheduler)
├── crates/services/graph.rs
├── crates/services/export.rs
├── crates/services/refresh_schedule.rs
└── crates/services/runtime/full.rs
```

**Config after simplification:**
- `pg_url`, `redis_url`, `amqp_url` — removed
- `serve_port`, `serve_host` — removed
- `crawl_queue`, `extract_queue`, `embed_queue`, `ingest_queue` — removed
- `shared_queue` — removed
- `web_allowed_origins` — moved to MCP-specific config (needed for CORS)

**Docker after simplification:**
```
docker-compose.services.yaml:
  axon-qdrant    (vector store)
  axon-tei       (embeddings)
  axon-chrome    (headless browser)

docker-compose.yaml:
  (removed — no separate app containers needed)
  (MCP HTTP server runs directly via axon mcp)
```

---

## Phased Todo List

### Phase 0 — PREREQUISITE: Extract shared CORS module
**Do first.** This is the only hidden coupling that blocks web deletion.

`crates/mcp/server.rs` imports `crate::crates::web::cors::cors_middleware`. Before deleting `crates/web/`, this function must be moved to a shared location.

- [ ] Create `crates/core/cors.rs` with `cors_middleware()` and `check_auth()` logic extracted from `crates/web/cors.rs`
- [ ] Update `crates/mcp/server.rs` import: `use crate::crates::core::cors::cors_middleware`
- [ ] Update `crates/web.rs` import to also use `crates/core/cors.rs` (so old code still compiles)
- [ ] Run `cargo check` — must compile clean

**Files:** `crates/core/cors.rs` (new), `crates/mcp/server.rs`, `crates/web.rs`  
**Risk:** None — purely additive. Old code still compiles.

---

### Phase 1 — Delete full-mode job backend
**Do second.** After Phase 0, this is the largest Rust deletion.

- [ ] Delete `crates/jobs/full.rs` (415 LOC — FullBackend + FullJobBackend impls)
- [ ] Delete `crates/jobs/worker_lane.rs` (418 LOC — AMQP polling abstraction)
- [ ] Delete `crates/jobs/common/` directory (~3,000 LOC — Postgres/AMQP pool, channel mgmt, claim/mark helpers)
- [ ] Delete `crates/jobs/crawl/runtime/` directory (~2,925 LOC — AMQP consumer loops, worker state machine)
- [ ] Delete `crates/jobs/refresh/` directory (~3,000 LOC — periodic schedule worker)
- [ ] Update `crates/services/runtime.rs`:
  - Remove `FullServiceRuntime` struct and impl
  - Simplify `resolve_runtime()` to always return `LiteServiceRuntime::new(cfg)` (no branch)
  - Remove `use crate::crates::jobs::full::FullBackend` import
- [ ] Update `crates/jobs/backend.rs`: remove docs/comments referencing `FullBackend`
- [ ] Remove `mod full;`, `mod worker_lane;`, `mod common;` from `crates/jobs.rs`
- [ ] Remove `mod refresh;` from `crates/jobs.rs`
- [ ] Run `cargo check` — compiler will surface all remaining `FullBackend` references

**Files:** ~30 files deleted, ~5 files modified  
**Risk:** Medium — compiler will catch all coupling. Fix each compilation error.

---

### Phase 2 — Delete graph, export, refresh_schedule features
**Can be parallel with Phase 1.**

- [ ] Delete `crates/jobs/graph.rs` (229 LOC)
- [ ] Delete `crates/jobs/graph/` directory (~1,800 LOC — Neo4j schema, persist, worker, extractor)
- [ ] Delete `crates/services/graph.rs` (212 LOC)
- [ ] Delete `crates/services/export.rs` (502 LOC)
- [ ] Delete `crates/services/export/` directory (if exists)
- [ ] Delete `crates/services/refresh_schedule.rs` (427 LOC)
- [ ] Update `crates/cli/commands/graph.rs` → remove or replace with "unsupported" message
- [ ] Update `crates/cli/commands/watch.rs` → remove scheduler functionality, or replace with "unsupported"
- [ ] Update `crates/cli/commands/status/presentation.rs`: remove `GraphJob` import
- [ ] Update `crates/cli/commands/status/failure_summary.rs`: remove `GraphJob` import
- [ ] Remove `mod graph;`, `mod export;` from `crates/services.rs`
- [ ] Remove `mod graph;`, `mod refresh;` from `crates/jobs.rs`
- [ ] Run `cargo check`

**Files:** ~20 files deleted, ~8 files modified  
**Risk:** Medium — status command, watch command need careful updating.

---

### Phase 3 — Delete web server, WS bridge, serve supervisor
**Requires Phase 0 (CORS extracted).** Can otherwise be parallel with Phases 1 & 2.

- [ ] Delete `crates/web.rs` (top-level module file)
- [ ] Delete `crates/web/` directory (~10,000 LOC — axum server, CORS, docker_stats, execute, download, logs, pack, shell, ws_handler, tailscale_auth)
- [ ] Delete `crates/cli/commands/serve.rs` (30 LOC)
- [ ] Delete `crates/cli/commands/serve_supervisor.rs` (45 LOC)
- [ ] Delete `crates/cli/commands/serve_supervisor/` directory (~100 LOC — model, preflight, runtime)
- [ ] Update `crates/cli/commands.rs`: remove `pub use serve::*` and `pub use serve_supervisor::*`
- [ ] Update `lib.rs`: remove `CommandKind::Serve` match arm and `pub mod web`
- [ ] Delete `tests/web_ws_async_fire_and_forget.rs`
- [ ] Delete `tests/web_ws_override_mapping.rs`
- [ ] Run `cargo check`

**Files:** ~30 files deleted, ~3 files modified  
**Risk:** Medium-high — large deletion but compiler will catch all loose ends.

---

### Phase 4 — Delete Next.js webapp
**Independent — can be done any time, even first.**

- [ ] Delete `apps/web/` directory entirely (~546,000 LOC TypeScript/JSON)
- [ ] Remove `apps/` directory reference from root `Cargo.toml` (if workspace member)
- [ ] Update `just` commands in `justfile` that reference `apps/web` or the Next.js dev server
- [ ] Update `docker/web/Dockerfile` — DELETE
- [ ] Update any scripts in `scripts/` that reference the web app

**Files:** ~1,000 files deleted, ~3 files modified  
**Risk:** Low — pure deletion, zero Rust coupling.

---

### Phase 5 — Simplify ServiceCapabilities
**Requires Phases 1 & 2 complete.**

`ServiceCapabilities` has flags `export`, `graph`, `refresh_schedule`, `watch_scheduler`. Per institutional learning (bead rs8.7), these flags are **never checked at the MCP handler layer** — they're dead code. With graph/export/refresh removed, the remaining capabilities may be empty.

- [ ] Delete `ServiceCapabilities` struct from `crates/services/context.rs` (or wherever defined)
- [ ] Remove all `ctx.capabilities.*` references from MCP handlers and CLI commands
- [ ] Simplify `ServiceContext` — remove `capabilities` field
- [ ] If `AXON_LITE` env var check remains, simplify to a single bool (not a capability struct)
- [ ] Run `cargo check`

**Files:** ~5 files modified  
**Risk:** Low — the flags were already not enforced. Deletion is safe.

---

### Phase 6 — Simplify Config struct
**Requires Phase 3 (web deleted) and Phase 1 (full-mode jobs deleted).**

- [ ] Remove from `crates/core/config/types.rs` (or wherever Config lives):
  - `pg_url` field
  - `redis_url` field
  - `amqp_url` field
  - `serve_port` / `serve_host` fields
  - `crawl_queue` / `extract_queue` / `embed_queue` / `ingest_queue` / `ingest_graph_queue` fields
  - `shared_queue` field
  - Any AMQP reconnect config fields
  - Any Postgres pool size config fields
  - `shell_allowed_origins` field (web-only)
  - Check: does `web_allowed_origins` need to stay for MCP CORS? If yes, rename to `mcp_allowed_origins`.
- [ ] Remove corresponding CLI flags from `crates/core/config/cli.rs`
- [ ] Update inline `Config { ... }` struct literals in:
  - `crates/cli/commands/research.rs`
  - `crates/cli/commands/search.rs`
  - All `make_test_config()` helpers (search with `grep -r "make_test_config"`)
- [ ] Update `.env.example` — remove PG/Redis/AMQP variables
- [ ] Run `cargo test` (not just `cargo check`) — struct literal failures only surface at test compile time

**Files:** ~10 files modified  
**Risk:** Medium — Config struct is used everywhere. Compiler enforces completeness but only at test compilation.

---

### Phase 7 — Update Cargo.toml dependencies
**Requires Phases 1, 2, 3 complete.**

- [ ] Remove `lapin` (RabbitMQ/AMQP crate)
- [ ] Remove `redis` (Redis client crate)
- [ ] Remove `deadpool-postgres` or `deadpool` if used only for Postgres pooling
- [ ] Modify `sqlx` feature flags: remove `"postgres"`, keep `"sqlite"`, `"uuid"`, `"chrono"`, `"time"`
- [ ] Check if `tokio-postgres` is a direct dependency (likely transitive via sqlx) — should auto-drop
- [ ] Check if `neo4j` / `neo4rs` / similar appears as direct dependency — remove
- [ ] Run `cargo tree` to verify no stale full-mode transitive deps
- [ ] Run `cargo build --release` — full clean build must succeed

**Files:** `Cargo.toml` only  
**Risk:** Low — Cargo enforces correctness. If a dep is still needed, compilation fails clearly.

---

### Phase 8 — Update Docker Compose
**Independent — can be done in parallel with any phase.**

- [ ] Update `docker-compose.services.yaml`:
  - Remove `axon-postgres` service
  - Remove `axon-redis` service
  - Remove `axon-rabbitmq` service
  - Remove associated volumes and network references for removed services
- [ ] Update `docker-compose.yaml`:
  - Remove `axon-web` service entirely
  - Simplify `axon-workers` healthcheck (remove s6 worker references)
  - Remove port 49010 (Next.js) mapping
- [ ] Delete `docker/web/` directory (Next.js Dockerfile)
- [ ] Delete `docker/rabbitmq/` directory
- [ ] Simplify `docker/Dockerfile`:
  - Remove s6 supervisor entries for crawl-worker, embed-worker, extract-worker, ingest-worker, graph-worker, web-server
  - Remove RabbitMQ/Postgres client tools (psql, rabbitmqadmin)
  - Keep only: Rust binary build, Qdrant/TEI health check tools
- [ ] Update `docker/s6/s6-rc.d/`:
  - Remove crawl-worker, embed-worker, extract-worker, ingest-worker, graph-worker, web-server directories
  - Keep only the minimal supervisor init if any persistent process remains

**Files:** ~15 files deleted/modified  
**Risk:** Low-medium — Docker changes don't affect Rust compilation. Validate by running `docker compose up -d`.

---

### Phase 9 — Docs and CLAUDE.md cleanup
**Do last — after all code changes complete.**

- [ ] Update `docs/ARCHITECTURE.md`: remove "dual-runtime", "full mode", "FullBackend" sections; update architecture diagram
- [ ] Update `docs/MCP.md`: remove graph/export/refresh_schedule actions
- [ ] Update `docs/MCP-TOOL-SCHEMA.md`: remove same actions; regenerate if auto-generated
- [ ] Delete `docs/SCHEMA.md` (Postgres table schema — SQLite schema lives in code)
- [ ] Update root `CLAUDE.md`:
  - Remove full-mode architecture docs
  - Remove Postgres/Redis/RabbitMQ env var docs
  - Remove `serve` command from Commands table
  - Simplify Docker Compose section to just `docker-compose.services.yaml`
  - Keep ACP docs (used for ask/research synthesis)
  - Keep lite-mode notes
- [ ] Update `.env.example`: remove PG/Redis/AMQP vars, add note that SQLite is the only persistence layer
- [ ] Update `README.md` if present: remove web UI references, simplify quickstart

**Files:** ~8 files modified, ~2 files deleted  
**Risk:** Low — documentation only.

---

## Dependency / Order Constraints

```
Phase 0 (CORS extract)
  └── REQUIRED BEFORE: Phase 3 (web server delete)

Phase 1 (full-mode jobs)
  └── REQUIRED BEFORE: Phase 5 (ServiceCapabilities delete)
  └── REQUIRED BEFORE: Phase 6 (Config simplify)
  └── REQUIRED BEFORE: Phase 7 (Cargo.toml)

Phase 2 (graph/export/refresh)
  └── REQUIRED BEFORE: Phase 5 (ServiceCapabilities delete)
  └── REQUIRED BEFORE: Phase 7 (Cargo.toml)

Phase 3 (web server delete) — requires Phase 0
  └── REQUIRED BEFORE: Phase 6 (Config simplify)
  └── REQUIRED BEFORE: Phase 7 (Cargo.toml)

Phase 4 (Next.js webapp) — INDEPENDENT
Phase 8 (Docker) — INDEPENDENT

Phase 5 — requires Phases 1, 2
Phase 6 — requires Phases 1, 3
Phase 7 — requires Phases 1, 2, 3
Phase 9 — requires all phases done
```

**Safe parallel execution waves:**

```
Wave 1 (start together):
  Phase 0 (CORS extract)
  Phase 4 (Next.js delete)
  Phase 8 (Docker update)

Wave 2 (after Phase 0):
  Phase 1 (full-mode jobs)
  Phase 2 (graph/export)
  Phase 3 (web server) ← unblocked by Phase 0

Wave 3 (after Phases 1, 2, 3):
  Phase 5 (ServiceCapabilities)
  Phase 6 (Config)
  Phase 7 (Cargo.toml)

Wave 4 (after all):
  Phase 9 (Docs)
```

---

## Risk Notes

### HIGH RISK

**R1 — CORS coupling (single highest-risk blocker)**
`crates/mcp/server.rs` imports `cors_middleware` from `crates/web/cors.rs`. If Phase 3 runs before Phase 0, the MCP server breaks. Phase 0 is mandatory first.

**R2 — Config struct literals cascade**
Removing fields from `Config` breaks inline struct literals in `research.rs`, `search.rs`, and test helpers. These only fail at *test* compile time, not `cargo check`. Always run `cargo test` to surface these — `cargo check` is not sufficient.

### MEDIUM RISK

**R3 — SQLite pool double-open**
(From bead rs8.1) If any refactoring creates a code path where two `SqlitePool`s open to the same file, WAL consistency breaks silently. After removing full-mode branching, ensure exactly one pool creation path survives.

**R4 — Monolith size limits**
Files ≤ 500 lines enforced at CI. Some files being modified (e.g., `crates/services/context.rs`, `crates/jobs/lite/workers.rs`) may already be near the limit. Pre-check with `wc -l` before each phase. Split preemptively if within 50 lines of limit.

**R5 — SQLite migrations**
When removing Postgres schema entirely, ensure SQLite migration files cover all tables. Migrations touching data MUST be wrapped in transactions (bead rs8.3). Unknown status values must be preserved, not dropped.

### LOW RISK

**R6 — Status command graph job references**
`crates/cli/commands/status/presentation.rs` and `failure_summary.rs` import `GraphJob`. Easy to fix by removing the import; compiler will surface it.

**R7 — ServiceCapabilities flags never enforced**
(Bead rs8.7) These flags exist but are not checked at MCP handler boundaries. Their removal has zero runtime impact — they were dead code.

**R8 — sqlx postgres feature removal**
Removing `"postgres"` from sqlx features removes the `PgPool` type and all Postgres-specific query macros. Since all remaining code uses `SqlitePool`, this is safe. But verify no stray `PgPool` references survive.

**R9 — s6 supervisor in Docker**
The current `docker/Dockerfile` uses an s6-based supervisor for multiple services. After simplification, if there's only one long-running process, s6 can be replaced with a simple `ENTRYPOINT`. Verify there's not a deep s6 integration that requires careful unwinding.

---

## Do First / Do Later / Maybe Drop Entirely

### DO FIRST
1. **Phase 0** — CORS extraction. Single blocking dependency. 3-file change. Do this before anything else.
2. **Phase 4** — Delete `apps/web/`. Pure deletion, zero Rust coupling, immediate 546K LOC reduction. Easy psychological win.
3. **Phase 8** — Docker Compose simplification. Infrastructure-only change, can proceed in parallel.

### DO SECOND
4. **Phase 1 + Phase 2** (in parallel) — Full-mode jobs + graph/export deletion. These are the core Rust simplifications. Do them together since they both affect `crates/jobs/` and `crates/services/`.
5. **Phase 3** — Web server + WS bridge. Now safe (CORS already extracted).

### DO LATER
6. **Phase 5** — ServiceCapabilities deletion. Low-value but clean. Do after Phases 1+2.
7. **Phase 6** — Config simplification. Satisfying but has Config literal cascade risk. Do after Phases 1+3.
8. **Phase 7** — Cargo.toml cleanup. Do last among code phases (verifies complete removal).
9. **Phase 9** — Docs cleanup. Final pass.

### MAYBE DROP ENTIRELY (features to consider before removing)

| Feature | Removal Confidence | Notes |
|---------|-------------------|-------|
| `axon graph` | HIGH — remove | Neo4j + Qwen; complex, tightly coupled to full-mode; can be re-added as separate plugin |
| `axon refresh` | HIGH — remove | Periodic scheduling; no analogous lite-mode replacement; can be a cron job externally |
| `axon export` | HIGH — remove | Export manifest; depends on full-mode Postgres schema; not used via MCP |
| `axon watch` | HIGH — remove | Watch scheduler; same dependency as refresh |
| `axon serve` | HIGH — remove | Supervisor for web UI; web UI being removed |
| `axon sessions` | LOW — keep | Session ingestion from Claude/Codex; does not require full mode |
| `axon suggest` | LOW — keep | URL discovery; pure CLI, no full-mode coupling |

---

## LOC Summary

| Component | Action | Est. LOC |
|-----------|--------|---------|
| `apps/web/**` | Delete | 546,000 |
| `crates/web/**` | Delete | 10,063 |
| `crates/jobs/full.rs` + `worker_lane.rs` | Delete | 833 |
| `crates/jobs/common/**` | Delete | ~3,000 |
| `crates/jobs/crawl/runtime/**` | Delete | ~2,925 |
| `crates/jobs/refresh/**` | Delete | ~3,000 |
| `crates/jobs/graph.rs` + `graph/**` | Delete | ~2,029 |
| `crates/services/graph.rs` | Delete | 212 |
| `crates/services/export.rs` + `export/**` | Delete | ~502 |
| `crates/services/refresh_schedule.rs` | Delete | 427 |
| `crates/cli/commands/serve.rs` + supervisor | Delete | ~175 |
| `tests/web_ws_*.rs` | Delete | ~200 |
| Docker/infra config | Delete/Modify | ~650 |
| **TOTAL REMOVED** | | **~569,416** |
| **TOTAL REMAINING** | | **~84,000** |

---

## Validation Protocol

After each phase:
```bash
cargo check                  # fast type check
cargo test                   # REQUIRED — struct literal failures only surface here
cargo build --release        # verify full release build
```

After Phase 7:
```bash
cargo tree | grep -E "lapin|redis|deadpool-postgres"   # must show nothing
```

After Phase 8:
```bash
docker compose -f docker-compose.services.yaml up -d
docker compose -f docker-compose.services.yaml ps     # should show 3-4 containers
./scripts/axon doctor                                  # must pass
./target/release/axon scrape https://example.com --wait true  # CLI works
./target/release/axon mcp                             # MCP server starts
```

---

*See also: bead epic (created via lavra-plan) for individual implementation tasks.*
