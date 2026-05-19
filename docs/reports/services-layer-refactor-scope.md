# Services Layer Refactor — Scope Report

**Date:** 2026-03-03
**Branch:** feat/sidebar
**Prepared by:** 4-agent parallel investigation (CLI, MCP, Web, Infrastructure)

---

## Executive Summary

Axon's architecture is **CLI-first** — business logic flows through CLI command handlers, MCP re-implements dispatch with direct function calls, and the Web layer shells out to the CLI binary as a subprocess. All three entry points converge on the same lower crates (`core`, `crawl`, `jobs`, `vector`, `ingest`), but the **orchestration layer is duplicated three ways**.

A services layer refactor would:
1. Extract business logic from CLI handlers into **entry-point-agnostic service functions**
2. Have CLI, MCP, and Web all call the same service functions
3. Eliminate the Web layer's subprocess proxy pattern (200-500ms overhead per command)
4. Prevent drift between CLI and MCP behavior

**Overall service-readiness: 7/10** — The lower crates are solid. The main work is extracting orchestration from `crates/cli/commands/` into a shared services layer.

---

## Current Architecture

```
┌──────────────┐    ┌──────────────┐    ┌──────────────────────┐
│   CLI (clap) │    │  MCP (rmcp)  │    │  Web (axum + WS)     │
│  lib.rs      │    │  server.rs   │    │  execute.rs          │
│  run_once()  │    │  dispatch()  │    │  spawn(axon binary)  │
└──────┬───────┘    └──────┬───────┘    └──────────┬───────────┘
       │                   │                       │
       │  direct fn call   │  direct fn call       │  subprocess I/O
       │                   │                       │
       ▼                   ▼                       ▼
┌──────────────────────────────────────────────────────────────┐
│                    CLI Command Handlers                       │
│              crates/cli/commands/ (4,193 lines)               │
│  ┌─────────┐ ┌──────────┐ ┌────────┐ ┌──────────┐          │
│  │ scrape  │ │  crawl   │ │  ask   │ │  embed   │  ...     │
│  │         │ │ +10 subs │ │        │ │ +4 subs  │          │
│  └────┬────┘ └────┬─────┘ └───┬────┘ └────┬─────┘          │
│       │           │           │            │                 │
│  Output formatting + I/O + orchestration mixed together      │
└──────┬────────────┬───────────┬────────────┬─────────────────┘
       │            │           │            │
       ▼            ▼           ▼            ▼
┌──────────┐ ┌───────────┐ ┌──────────┐ ┌──────────┐
│  core    │ │  crawl    │ │  vector  │ │  jobs    │
│  config  │ │  engine   │ │  ops     │ │  AMQP    │
│  http    │ │  spider   │ │  TEI     │ │  PG      │
│  content │ │  manifest │ │  Qdrant  │ │  workers │
└──────────┘ └───────────┘ └──────────┘ └──────────┘
```

### Target Architecture

```
┌──────────────┐    ┌──────────────┐    ┌──────────────────────┐
│   CLI (clap) │    │  MCP (rmcp)  │    │  Web (axum + WS)     │
│  Thin shell  │    │  Thin shell  │    │  Direct fn calls     │
│  format only │    │  format only │    │  no subprocess       │
└──────┬───────┘    └──────┬───────┘    └──────────┬───────────┘
       │                   │                       │
       └───────────────────┼───────────────────────┘
                           │
                           ▼
              ┌────────────────────────┐
              │    Services Layer      │
              │  crates/services/      │
              │                        │
              │  CrawlService          │
              │  EmbedService          │
              │  QueryService          │
              │  IngestService         │
              │  ...                   │
              │                        │
              │  Returns typed results │
              │  No I/O formatting     │
              │  No CLI assumptions    │
              └────────────┬───────────┘
                           │
              ┌────────────┼────────────┐
              ▼            ▼            ▼
         ┌────────┐  ┌──────────┐  ┌────────┐
         │ crawl  │  │  vector  │  │  jobs  │
         │ core   │  │  ingest  │  │        │
         └────────┘  └──────────┘  └────────┘
```

---

## Inventory: What Each Layer Currently Does

### Layer 1: CLI Command Handlers (`crates/cli/commands/`)

**28 commands, 4,193 lines across all modules.**

Each handler currently mixes three concerns:

| Concern | Example | Should Live In |
|---------|---------|----------------|
| **Business orchestration** | Sync vs async dispatch, cache checks, fallback logic | Services layer |
| **Output formatting** | `if cfg.json_output { println!(...) }` | CLI layer (keep) |
| **I/O side effects** | Write markdown to disk, read manifest files | Services layer (pluggable) |

#### Commands by Category

**Content Acquisition (scrape, map, crawl, screenshot)**
| Command | Lines | Business Logic in CLI | Delegation | Service Readiness |
|---------|-------|----------------------|------------|-------------------|
| `scrape` | 299 | Config→Spider mapping, batch embed orchestration, format selection | content.rs, http.rs, vector ops | 7/10 — `scrape_payload()` exists for MCP |
| `map` | 172 | HTTP→Chrome fallback, sitemap dedup | crawl engine, audit/sitemap | 6/10 — `map_payload()` exists for MCP |
| `crawl` | 210 + submodules | Sync/async dispatch, 10 subcommands, cache, audit/diff | crawl engine, jobs/crawl | 5/10 — heavy orchestration |
| `screenshot` | ~200 | CDP protocol, file writes | Raw HTTP/WS to Chrome | 4/10 — fully CLI-bound |

**Job Lifecycle (crawl, extract, embed, ingest, refresh subcommands)**
| Subcommand | Pattern | Service Readiness |
|------------|---------|-------------------|
| `status <id>` | Fetch job from PG, format | 8/10 — pure DB read |
| `cancel <id>` | Mark canceled in PG | 8/10 — pure DB write |
| `errors <id>` | Fetch error_text from PG | 8/10 — pure DB read |
| `list` | List jobs with pagination | 8/10 — pure DB read |
| `cleanup` | Delete completed jobs | 8/10 — pure DB write |
| `clear` | Delete all + purge AMQP | 7/10 — needs AMQP access |
| `recover` | Reclaim stale jobs | 8/10 — pure DB write |
| `worker` | Run AMQP consumer loop | N/A — stays in jobs crate |

**Vector/RAG (query, retrieve, ask, evaluate, suggest, sources, domains, stats, dedupe)**
| Command | Lines | Business Logic in CLI | Service Readiness |
|---------|-------|----------------------|-------------------|
| `query` | ~100 | Query text parsing, limit | 8/10 — `query_results()` exists |
| `retrieve` | ~80 | URL parsing | 8/10 — `retrieve_results()` exists |
| `ask` | ~200 | Retrieval + rerank + LLM call + citation gates | 6/10 — monolithic, needs splitting |
| `evaluate` | ~150 | RAG vs baseline + judge scoring | 6/10 — similar to ask |
| `suggest` | ~100 | Facet query + LLM | 7/10 |
| `sources` | ~60 | Facet query | 9/10 — `sources_payload()` exists |
| `domains` | ~80 | Facet query + optional detail | 9/10 — clean |
| `stats` | ~100 | Qdrant + PG metrics | 9/10 — clean |
| `dedupe` | ~150 | Full collection scan + dedup | 7/10 — long-running |

**Search & Research**
| Command | Lines | Service Readiness |
|---------|-------|-------------------|
| `search` | 120 | 7/10 — `search_results()` exists, Tavily wrapper |
| `research` | 150 | 7/10 — `research_payload()` exists, Tavily+LLM |

**Diagnostics**
| Command | Lines | Service Readiness |
|---------|-------|-------------------|
| `doctor` | ~200 | 7/10 — probe functions are reusable |
| `debug` | ~150 | 6/10 — wraps doctor + LLM |
| `status` | ~150 | 8/10 — `status_full()` exists |

**Ingest**
| Command | Lines | Service Readiness |
|---------|-------|-------------------|
| `github` | ~150 | 7/10 — `ingest_github()` is clean |
| `reddit` | ~150 | 7/10 — `ingest_reddit()` is clean |
| `youtube` | ~100 | 7/10 — `ingest_youtube()` is clean |
| `sessions` | ~150 | 6/10 — format parsing inline |

---

### Layer 2: MCP Server (`crates/mcp/`)

**1,763 lines across 6 handler modules + schema + common.**

The MCP server demonstrates the **correct pattern for a services layer consumer**:

- **Zero duplication** for lifecycle jobs — calls `start_crawl_job()` directly from `crates/jobs/crawl`
- **~5% duplication** for direct operations — calls `query_results()`, `search_results()`, etc. directly, bypassing CLI output formatting
- **Strict routing** — single `axon` tool with action/subaction enum dispatch, `deny_unknown_fields`

**What MCP does right (and what the services layer should formalize):**
- Calls `*_payload()` functions (e.g., `scrape_payload()`, `map_payload()`, `query_results()`) that return structured data
- Applies its own response formatting (artifact files, inline clipping, response_mode)
- Has its own config override pattern (`apply_crawl_overrides()`)

**What MCP does wrong (that a services layer would fix):**
- Some `*_payload()` functions don't exist — MCP must call the full `run_*_native()` which prints to stdout
- Config construction requires copying CLI patterns
- No shared progress/event protocol between MCP and CLI

**MCP Action Coverage (20 actions):**

| Category | Actions | Implementation |
|----------|---------|----------------|
| Lifecycle (crawl) | start, status, cancel, list, cleanup, clear, recover | Direct job API calls |
| Lifecycle (extract) | start, status, cancel, list, cleanup, clear, recover | Direct job API calls |
| Lifecycle (embed) | start, status, cancel, list, cleanup, clear, recover | Direct job API calls |
| Lifecycle (ingest) | start, status, cancel, list, cleanup, clear, recover | Direct job API calls |
| Lifecycle (refresh) | start, status, cancel, list + schedule CRUD | Direct job API calls |
| Direct ops | query, retrieve, search, map, scrape, research, ask | `*_payload()` functions |
| System | screenshot, artifacts, help, doctor, domains, sources, stats, status | Mixed direct calls |

---

### Layer 3: Web/API (`crates/web/`)

**~1,000 lines. Pure subprocess execution bridge. Zero business logic.**

The web layer:
1. Accepts WebSocket messages: `{ type: "execute", mode: "crawl", input: "https://..." }`
2. Validates mode against `ALLOWED_MODES` whitelist (24 modes)
3. Validates flags against `ALLOWED_FLAGS` whitelist (30 flags)
4. Spawns: `Command::new(axon_binary).args(["crawl", url, "--json", "--wait", "true"]).spawn()`
5. Streams stdout/stderr back over WebSocket
6. For async commands: captures job_id, polls `axon crawl status <id> --json` every 1s

**What changes with a services layer:**
- **Eliminate subprocess overhead** (200-500ms per command)
- **Direct function calls** instead of JSON-over-stdio parsing
- **Typed errors** instead of exit code detection
- **Streaming progress** instead of polling (native `tokio::sync::broadcast`)
- **No ANSI stripping** — structured data from the start

**What stays the same:**
- WebSocket protocol (client ↔ server message format)
- Security whitelists (mode + flag validation)
- Docker stats broadcaster
- Shell PTY bridge
- Artifact download endpoints

---

### Layer 4: Shared Infrastructure Crates

| Crate | Lines | Service Ready? | Changes Needed |
|-------|-------|----------------|----------------|
| `core/config` | ~600 | 9/10 | Add `ConfigBuilder` for non-CLI construction |
| `core/content` | ~400 | 9/10 | None — pure transforms |
| `core/http` | ~300 | 9/10 | None — pure validation |
| `core/ui` | ~200 | N/A | CLI-only, stays in CLI |
| `crawl/engine` | ~500 | 6/10 | Pluggable output handler trait |
| `jobs/common` | ~300 | 8/10 | None — clean async infra |
| `jobs/crawl` | ~800 | 8/10 | None — job lifecycle is clean |
| `jobs/extract` | ~300 | 8/10 | None |
| `jobs/embed` | ~300 | 8/10 | None |
| `jobs/ingest` | ~200 | 8/10 | None |
| `vector/ops/tei` | ~300 | 8/10 | None — batch handling is clean |
| `vector/ops/qdrant` | ~500 | 8/10 | None — client is well-factored |
| `vector/ops/commands` | ~800 | 6/10 | Split into service fns + CLI formatters |
| `vector/ops/ranking` | ~500 | 9/10 | None — pure algorithms |
| `ingest/github` | ~500 | 7/10 | Minor — already returns structured data |
| `ingest/reddit` | ~400 | 7/10 | Minor — already returns structured data |
| `ingest/youtube` | ~200 | 7/10 | Minor — already returns structured data |

---

## Exact Scope of Work

### Phase 1: Service Result Types (Foundation)

**Effort: ~500 lines new code**

Create `crates/services/types.rs` with entry-point-agnostic result types:

```rust
// Every service function returns one of these
pub struct ServiceResult<T> {
    pub data: T,
    pub warnings: Vec<String>,
    pub elapsed_ms: u64,
}

// Domain-specific result types
pub struct CrawlResult {
    pub job_id: Option<Uuid>,
    pub status: JobStatus,
    pub pages_crawled: usize,
    pub pages_embedded: usize,
    pub thin_pages: usize,
    pub errors: Vec<String>,
    pub output_dir: Option<PathBuf>,
}

pub struct ScrapeResult {
    pub url: String,
    pub title: Option<String>,
    pub markdown: String,
    pub html: Option<String>,
    pub meta_description: Option<String>,
    pub embedded: bool,
}

pub struct QueryResult {
    pub matches: Vec<SearchMatch>,
    pub total: usize,
}

pub struct AskResult {
    pub answer: String,
    pub citations: Vec<Citation>,
    pub confidence: f32,
    pub context_chunks: Vec<ContextChunk>,
}

// ... similar for Extract, Embed, Search, Research, Ingest, etc.
```

**Files to create:**
- `crates/services/mod.rs`
- `crates/services/types.rs`

### Phase 2: Extract Service Functions (~2,500 lines refactored)

For each command, extract the business logic into a service function that returns typed results.

**Pattern:**

```rust
// BEFORE (in crates/cli/commands/scrape.rs)
pub async fn run_scrape(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let urls = parse_urls(cfg)?;
    // ... 200 lines of orchestration + formatting
    if cfg.json_output { println!(...) }
    Ok(())
}

// AFTER: service function (in crates/services/scrape.rs)
pub async fn scrape(cfg: &Config, urls: &[String]) -> Result<Vec<ScrapeResult>, Box<dyn Error>> {
    // ... orchestration only, returns data
}

// AFTER: CLI handler (in crates/cli/commands/scrape.rs)
pub async fn run_scrape(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let urls = parse_urls(cfg)?;
    let results = services::scrape(cfg, &urls).await?;
    format_scrape_output(cfg, &results);
    Ok(())
}
```

**Service functions to extract (by priority):**

| Priority | Service Function | Source | Estimated Lines |
|----------|-----------------|--------|-----------------|
| P0 | `services::crawl::start(cfg, urls)` | crawl.rs + sync_crawl.rs | ~150 |
| P0 | `services::crawl::status(cfg, job_id)` | crawl/subcommands.rs | ~30 |
| P0 | `services::crawl::cancel(cfg, job_id)` | crawl/subcommands.rs | ~20 |
| P0 | `services::crawl::list(cfg)` | crawl/subcommands.rs | ~30 |
| P0 | `services::scrape::scrape(cfg, urls)` | scrape.rs | ~100 |
| P0 | `services::query::query(cfg, text)` | vector/ops/commands | ~50 |
| P0 | `services::query::ask(cfg, question)` | vector/ops/commands/ask | ~150 |
| P0 | `services::query::retrieve(cfg, url)` | vector/ops/commands | ~40 |
| P1 | `services::embed::start(cfg, input)` | embed.rs | ~80 |
| P1 | `services::extract::start(cfg, urls, prompt)` | extract.rs | ~100 |
| P1 | `services::search::search(cfg, query)` | search.rs | ~60 |
| P1 | `services::search::research(cfg, query)` | research.rs | ~80 |
| P1 | `services::map::discover(cfg, url)` | map.rs | ~80 |
| P1 | `services::ingest::github(cfg, owner, repo)` | github.rs | ~60 |
| P1 | `services::ingest::reddit(cfg, target)` | reddit.rs | ~60 |
| P1 | `services::ingest::youtube(cfg, url)` | youtube.rs | ~50 |
| P2 | `services::screenshot::capture(cfg, url)` | screenshot/ | ~80 |
| P2 | `services::doctor::diagnose(cfg)` | doctor.rs | ~100 |
| P2 | `services::status::full(cfg)` | status.rs | ~80 |
| P2 | `services::stats::collection(cfg)` | vector/ops/commands | ~50 |
| P2 | `services::sources::list(cfg)` | vector/ops/commands | ~30 |
| P2 | `services::domains::list(cfg)` | vector/ops/commands | ~40 |

**Total: ~22 service functions, ~1,500 lines of new service code**

### Phase 3: Config Builder (~200 lines)

Replace inline `Config { ... }` struct literals with a builder:

```rust
pub struct ConfigBuilder {
    command: Option<CommandKind>,
    pg_url: Option<String>,
    // ... all 100+ fields with Option
}

impl ConfigBuilder {
    pub fn new() -> Self { ... }
    pub fn from_env() -> Self { ... }  // Reads env vars only, no clap
    pub fn command(mut self, cmd: CommandKind) -> Self { ... }
    pub fn start_url(mut self, url: String) -> Self { ... }
    pub fn build(self) -> Config { ... }
}
```

**Files affected:**
- `crates/core/config/types/config.rs` — add builder
- `crates/core/config/types/config_impls.rs` — add `from_env()` factory
- `crates/cli/commands/research.rs` — replace inline Config literal
- `crates/cli/commands/search.rs` — replace inline Config literal
- `crates/jobs/common/tests/` — replace test Config literals

### Phase 4: Rewire MCP to Use Services (~400 lines changed)

MCP already calls payload functions directly. Rewire to use service layer:

```rust
// BEFORE (crates/mcp/handlers_query.rs)
let results = query_results(cfg, &query, limit).await?;
let json = serde_json::to_value(&results)?;
respond_with_mode(json, ...)

// AFTER
let result = services::query::query(cfg, &query).await?;
let json = serde_json::to_value(&result)?;
respond_with_mode(json, ...)
```

**Files affected:**
- `crates/mcp/handlers_query.rs` — ~50 lines changed
- `crates/mcp/handlers_crawl_extract.rs` — ~30 lines changed
- `crates/mcp/handlers_embed_ingest.rs` — ~30 lines changed
- `crates/mcp/handlers_system.rs` — ~40 lines changed
- `crates/mcp/handlers_refresh_status.rs` — ~30 lines changed

### Phase 5: Rewire Web to Use Services (~600 lines changed)

Replace subprocess proxy with direct service calls:

```rust
// BEFORE (crates/web/execute/mod.rs)
let exe = resolve_exe()?;
let mut child = Command::new(exe).args(args).spawn()?;
// ... stream stdout/stderr

// AFTER
let result = match mode {
    "crawl" => services::crawl::start(cfg, &[input]).await?,
    "query" => services::query::query(cfg, input).await?,
    "ask"   => services::query::ask(cfg, input).await?,
    // ...
};
send_command_output_json(tx, ctx, serde_json::to_value(&result)?).await;
```

**Files affected:**
- `crates/web/execute/mod.rs` — major rewrite (~100 lines)
- `crates/web/execute/sync_mode.rs` — simplify significantly or remove
- `crates/web/execute/async_mode.rs` — simplify (direct job ID, no subprocess)
- `crates/web/execute/polling.rs` — simplify (direct DB query, no subprocess)
- `crates/web/execute/cancel.rs` — simplify (direct service call)
- `crates/web/execute/args.rs` — remove (no longer building CLI args)
- `crates/web/execute/exe.rs` — remove (no longer resolving binary path)
- `crates/web/execute/constants.rs` — keep ALLOWED_MODES, remove ALLOWED_FLAGS mapping

### Phase 6: Progress/Event Protocol (~300 lines)

Replace indicatif spinners + polling with structured events:

```rust
pub enum ServiceEvent {
    Progress { phase: String, percent: Option<f32>, message: String },
    Warning { message: String },
    Log { level: Level, message: String },
}

// Service functions accept optional event channel
pub async fn crawl(
    cfg: &Config,
    urls: &[String],
    events: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<CrawlResult, Box<dyn Error>> { ... }
```

CLI subscribes with a spinner renderer. Web subscribes with WebSocket forwarding. MCP ignores events (artifact-first).

---

## File Impact Summary

| Category | Files Created | Files Modified | Files Removed | Est. Lines Changed |
|----------|--------------|----------------|---------------|-------------------|
| Service types | 2 | 0 | 0 | ~500 |
| Service functions | 8-10 | 0 | 0 | ~1,500 |
| Config builder | 1 | 5 | 0 | ~300 |
| CLI handler rewire | 0 | 15-20 | 0 | ~800 (simplified) |
| MCP rewire | 0 | 5 | 0 | ~400 |
| Web rewire | 0 | 5 | 2 | ~600 |
| Event protocol | 2 | 10 | 0 | ~300 |
| **Total** | **~15** | **~40-45** | **~2** | **~4,400** |

---

## Risk Assessment

| Risk | Severity | Mitigation |
|------|----------|------------|
| Breaking MCP contract | High | MCP schema tests (36 cases) catch regressions |
| Breaking Web WS protocol | Medium | WS protocol tests exist; keep message shapes stable |
| Config field explosion | Low | ConfigBuilder solves test friction |
| Spider.rs coupling | Low | Engine wrapper already isolates spider |
| Test coverage gaps | Medium | Many CLI handlers have few tests — add service-level tests |
| Monolith policy violations | Medium | New service files must stay ≤500 lines each |

---

## Recommended Execution Order

```
Phase 1: Service Result Types          (foundation — no breaking changes)
    ↓
Phase 3: Config Builder                (reduce friction — no breaking changes)
    ↓
Phase 2: Extract Service Functions     (the big one — incremental, per-command)
    ↓
Phase 4: Rewire MCP                    (uses new service functions)
    ↓
Phase 5: Rewire Web                    (uses new service functions)
    ↓
Phase 6: Progress/Event Protocol       (polish — streaming progress)
```

Phases 1-3 can be done without breaking anything. Phase 2 can be done **one command at a time** — extract `scrape` service, verify CLI+MCP still work, then move to `crawl`, etc. Phases 4-5 depend on Phase 2 being mostly complete. Phase 6 is optional polish.

---

## What's Already Done (Leverage Points)

The MCP server already demonstrates the pattern we want to formalize:

1. **`*_payload()` functions exist** for: scrape, map, query, search, research, ask, sources, domains, stats, status, doctor
2. **Job lifecycle API is clean** — `start_crawl_job()`, `get_crawl_job()`, `cancel_crawl_job()`, `list_jobs()` are already service-level
3. **`crates/core`** is fully entry-point agnostic — no changes needed
4. **`crates/jobs`** is fully entry-point agnostic — no changes needed
5. **Ingest handlers** (`ingest_github()`, `ingest_reddit()`, `ingest_youtube()`) already return structured data

The refactor is largely **formalizing what MCP already does** into a shared services layer that CLI and Web also use.

---

## Lines of Code by Module (Current)

| Module | Lines | Role |
|--------|-------|------|
| `crates/cli/commands/` | 4,193 | CLI handlers (refactor target) |
| `crates/mcp/` | 1,763 | MCP dispatch (rewire target) |
| `crates/web/` | ~1,000 | Web bridge (rewire target) |
| `crates/core/` | ~2,500 | Shared infra (keep as-is) |
| `crates/crawl/` | ~1,500 | Crawl engine (minor changes) |
| `crates/jobs/` | ~3,000 | Job system (keep as-is) |
| `crates/vector/` | ~3,500 | Vector/RAG ops (extract commands) |
| `crates/ingest/` | ~1,100 | Ingest handlers (minor changes) |
| `lib.rs` | ~200 | Dispatch (simplify) |
| **Total Rust** | **~18,761** | |

**Estimated refactor touches ~25% of total codebase** (4,400 lines changed out of ~18,800).
