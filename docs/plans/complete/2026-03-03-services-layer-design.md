# Services Layer Design

**Date:** 2026-03-03
**Status:** Approved
**Branch:** feat/sidebar → new branch per wave

---

## Problem

Axon has three entry points (CLI, MCP, Web) that converge on the same lower crates but orchestrate differently:

- **CLI** — business logic inline in command handlers, mixed with output formatting
- **MCP** — calls `*_payload()` functions directly, bypasses CLI formatting
- **Web** — shells out to the CLI binary as a subprocess (200-500ms overhead)

This causes drift between entry points, duplicated orchestration, and the Web layer paying a subprocess tax on every operation.

## Solution

Extract business logic from CLI handlers into a shared `crates/services/` module. All three entry points become thin shells that call service functions and format results for their transport.

## Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Location | `crates/services/` module | Parallel to `cli/`, `mcp/`, `web/`. Single import path. |
| Module style | Modern Rust (named files, no `mod.rs`) | `services.rs` + `services/` directory |
| Config passing | `&Config` as-is | Zero refactoring of the 122-field monolith. Services ignore fields they don't need. |
| Return types | Domain-specific structs with `#[derive(Serialize)]` | CLI formats as text/JSON, MCP wraps as artifacts, Web sends over WebSocket. |
| Progress | Optional `mpsc::Sender<ServiceEvent>` | Zero overhead when not subscribed. CLI → spinner, Web → WS, MCP → None. |
| Infra ownership | Injected as params (`&PgPool`, etc.) | Entry point creates and owns connections. Services are pure logic + I/O through injected handles. |
| Migration | Incremental, one command per PR | Each extraction is self-contained. CLI + MCP rewired together. Web rewired as final phase. |
| Consumers | CLI + MCP + Web (all three) | Full unification. Web subprocess proxy eliminated. |

---

## Module Structure

```
crates/
├── services.rs                  // pub mod declarations + re-exports
├── services/
│   ├── types.rs                 // ScrapeResult, CrawlResult, QueryResult, ...
│   ├── events.rs                // ServiceEvent enum + emit() helper
│   ├── crawl.rs                 // start, status, cancel, list, errors, cleanup, clear, recover
│   ├── scrape.rs                // scrape
│   ├── query.rs                 // query, retrieve, ask, evaluate, suggest
│   ├── embed.rs                 // start, status, cancel, list, cleanup, clear, recover
│   ├── extract.rs               // start, status, cancel, list, cleanup, clear, recover
│   ├── search.rs                // search, research
│   ├── ingest.rs                // github, reddit, youtube, sessions
│   ├── map.rs                   // discover
│   ├── system.rs                // doctor, stats, sources, domains, full_status, dedupe
│   └── screenshot.rs            // capture
```

---

## Result Types

```rust
// crates/services/types.rs

#[derive(Debug, Clone, Serialize)]
pub struct ScrapeResult {
    pub url: String,
    pub title: Option<String>,
    pub markdown: String,
    pub html: Option<String>,
    pub word_count: usize,
    pub embedded: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct CrawlResult {
    pub job_id: Option<Uuid>,
    pub status: JobStatus,
    pub pages_crawled: usize,
    pub pages_embedded: usize,
    pub thin_pages: usize,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct QueryMatch {
    pub url: String,
    pub title: Option<String>,
    pub snippet: String,
    pub score: f32,
    pub chunk_index: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct QueryResult {
    pub matches: Vec<QueryMatch>,
    pub total: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct AskResult {
    pub answer: String,
    pub citations: Vec<Citation>,
    pub context_chunks_used: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchResult {
    pub results: Vec<WebSearchHit>,
    pub jobs_enqueued: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ResearchResult {
    pub answer: String,
    pub sources: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MapResult {
    pub urls: Vec<String>,
    pub source: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct IngestResult {
    pub source_type: String,
    pub target: String,
    pub items_ingested: usize,
    pub items_embedded: usize,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExtractResult {
    pub job_id: Option<Uuid>,
    pub status: JobStatus,
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EmbedResult {
    pub job_id: Option<Uuid>,
    pub status: JobStatus,
    pub chunks_embedded: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct EvaluateResult {
    pub rag_answer: String,
    pub baseline_answer: String,
    pub judge: JudgeVerdict,
}

#[derive(Debug, Clone, Serialize)]
pub struct JobInfo {
    pub id: Uuid,
    pub status: JobStatus,
    pub url: Option<String>,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub error_text: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DoctorResult {
    pub checks: Vec<HealthCheck>,
    pub all_healthy: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct StatsResult {
    pub collection: String,
    pub points_count: u64,
    pub segments_count: usize,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScreenshotResult {
    pub url: String,
    pub path: PathBuf,
    pub width: u32,
    pub height: u32,
    pub bytes: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct DedupeResult {
    pub duplicates_removed: usize,
    pub points_scanned: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct SourceEntry {
    pub url: String,
    pub chunk_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct DomainEntry {
    pub domain: String,
    pub url_count: usize,
    pub chunk_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct StatusResult {
    pub queue: QueueStatus,
    pub jobs: JobCounts,
}
```

---

## Event Protocol

```rust
// crates/services/events.rs

#[derive(Debug, Clone, Serialize)]
pub enum ServiceEvent {
    Progress {
        phase: String,
        percent: Option<f32>,
        message: String,
    },
    PageCrawled {
        url: String,
        chars: usize,
        thin: bool,
    },
    Embedded {
        url: String,
        chunks: usize,
    },
    Warning {
        message: String,
    },
    Log {
        level: String,
        message: String,
    },
}

pub fn emit(tx: &Option<mpsc::Sender<ServiceEvent>>, event: ServiceEvent) {
    if let Some(tx) = tx {
        let _ = tx.try_send(event);
    }
}
```

---

## Service Function Signatures

### scrape.rs

```rust
pub async fn scrape(
    cfg: &Config,
    urls: &[String],
    events: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<Vec<ScrapeResult>, Box<dyn Error>>;
```

### map.rs

```rust
pub async fn discover(
    cfg: &Config,
    url: &str,
    events: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<MapResult, Box<dyn Error>>;
```

### crawl.rs

```rust
pub async fn start(
    cfg: &Config, pool: &PgPool, urls: &[String],
    events: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<CrawlResult, Box<dyn Error>>;

pub async fn status(pool: &PgPool, job_id: Uuid) -> Result<JobInfo, Box<dyn Error>>;
pub async fn cancel(pool: &PgPool, job_id: Uuid) -> Result<JobInfo, Box<dyn Error>>;
pub async fn list(pool: &PgPool) -> Result<Vec<JobInfo>, Box<dyn Error>>;
pub async fn errors(pool: &PgPool, job_id: Uuid) -> Result<Option<String>, Box<dyn Error>>;
pub async fn cleanup(pool: &PgPool) -> Result<usize, Box<dyn Error>>;
pub async fn clear(pool: &PgPool, amqp_url: &str, queue: &str) -> Result<usize, Box<dyn Error>>;
pub async fn recover(pool: &PgPool) -> Result<usize, Box<dyn Error>>;
```

### embed.rs

```rust
pub async fn start(
    cfg: &Config, pool: &PgPool, input: &str,
    events: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<EmbedResult, Box<dyn Error>>;

pub async fn status(pool: &PgPool, job_id: Uuid) -> Result<JobInfo, Box<dyn Error>>;
pub async fn cancel(pool: &PgPool, job_id: Uuid) -> Result<JobInfo, Box<dyn Error>>;
pub async fn list(pool: &PgPool) -> Result<Vec<JobInfo>, Box<dyn Error>>;
pub async fn cleanup(pool: &PgPool) -> Result<usize, Box<dyn Error>>;
pub async fn clear(pool: &PgPool, amqp_url: &str, queue: &str) -> Result<usize, Box<dyn Error>>;
pub async fn recover(pool: &PgPool) -> Result<usize, Box<dyn Error>>;
```

### extract.rs

```rust
pub async fn start(
    cfg: &Config, pool: &PgPool, urls: &[String],
    events: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<ExtractResult, Box<dyn Error>>;

// status, cancel, list, cleanup, clear, recover — same pattern as embed
```

### query.rs

```rust
pub async fn query(cfg: &Config, text: &str, limit: usize) -> Result<QueryResult, Box<dyn Error>>;
pub async fn retrieve(cfg: &Config, url: &str) -> Result<Vec<RetrievedChunk>, Box<dyn Error>>;
pub async fn ask(
    cfg: &Config, question: &str,
    events: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<AskResult, Box<dyn Error>>;
pub async fn evaluate(cfg: &Config, question: &str) -> Result<EvaluateResult, Box<dyn Error>>;
pub async fn suggest(cfg: &Config, focus: Option<&str>) -> Result<Vec<String>, Box<dyn Error>>;
```

### search.rs

```rust
pub async fn search(
    cfg: &Config, pool: &PgPool, query: &str,
    events: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<SearchResult, Box<dyn Error>>;

pub async fn research(cfg: &Config, query: &str) -> Result<ResearchResult, Box<dyn Error>>;
```

### ingest.rs

```rust
pub async fn github(
    cfg: &Config, pool: &PgPool, owner: &str, repo: &str,
    events: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<IngestResult, Box<dyn Error>>;

pub async fn reddit(
    cfg: &Config, pool: &PgPool, target: &str,
    events: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<IngestResult, Box<dyn Error>>;

pub async fn youtube(
    cfg: &Config, pool: &PgPool, url: &str,
    events: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<IngestResult, Box<dyn Error>>;

pub async fn sessions(
    cfg: &Config,
    events: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<IngestResult, Box<dyn Error>>;
```

### system.rs

```rust
pub async fn doctor(cfg: &Config) -> Result<DoctorResult, Box<dyn Error>>;
pub async fn stats(cfg: &Config) -> Result<StatsResult, Box<dyn Error>>;
pub async fn sources(cfg: &Config) -> Result<Vec<SourceEntry>, Box<dyn Error>>;
pub async fn domains(cfg: &Config, detailed: bool) -> Result<Vec<DomainEntry>, Box<dyn Error>>;
pub async fn full_status(cfg: &Config, pool: &PgPool) -> Result<StatusResult, Box<dyn Error>>;
pub async fn dedupe(
    cfg: &Config,
    events: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<DedupeResult, Box<dyn Error>>;
```

### screenshot.rs

```rust
pub async fn capture(cfg: &Config, url: &str) -> Result<ScreenshotResult, Box<dyn Error>>;
```

---

## Consumer Wiring

### CLI (thin formatter)

```rust
// Pattern for all CLI handlers after refactor
pub async fn run_scrape(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let urls = parse_urls(cfg)?;

    let (tx, mut rx) = mpsc::channel(64);
    let spinner_handle = tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            render_event_to_spinner(&event);
        }
    });

    let results = services::scrape::scrape(cfg, &urls, Some(tx)).await?;
    spinner_handle.await?;

    if cfg.json_output {
        println!("{}", serde_json::to_string_pretty(&results)?);
    } else {
        for r in &results {
            log_done!("{} — {} chars", r.url, r.markdown.len());
        }
    }
    Ok(())
}
```

### MCP (artifact wrapper)

```rust
async fn handle_query(cfg: &Config, params: &QueryParams) -> CallToolResult {
    let result = services::query::query(cfg, &params.query, params.limit).await
        .map_err(|e| mcp_error(e))?;
    respond_with_mode(serde_json::to_value(&result)?, &params.response_mode)
}
```

### Web (direct call + WS forwarding)

```rust
async fn handle_execute(cfg: &Config, pool: &PgPool, mode: &str, input: &str, ws_tx: WsTx) {
    let (tx, mut rx) = mpsc::channel(64);

    let ws_tx2 = ws_tx.clone();
    tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            let msg = serde_json::json!({ "type": "log", "data": event });
            let _ = ws_tx2.send(Message::Text(msg.to_string())).await;
        }
    });

    let result: serde_json::Value = match mode {
        "scrape" => serde_json::to_value(
            services::scrape::scrape(cfg, &[input.to_string()], Some(tx)).await?
        )?,
        "query" => serde_json::to_value(
            services::query::query(cfg, input, cfg.search_limit).await?
        )?,
        "crawl" => serde_json::to_value(
            services::crawl::start(cfg, pool, &[input.to_string()], Some(tx)).await?
        )?,
        _ => return send_error(&ws_tx, "unknown mode").await,
    };

    let done = serde_json::json!({ "type": "done", "data": result });
    let _ = ws_tx.send(Message::Text(done.to_string())).await;
}
```

---

## Migration Path

### Wave 1: Foundation + Simple Commands (PRs 1-6)

| PR | Scope |
|----|-------|
| 1 | Create `crates/services/` module tree, `types.rs`, `events.rs`. Wire into `lib.rs`. Zero behavioral change. |
| 2 | `sources` — extract `sources_payload()` → `services::system::sources()`. Rewire CLI + MCP. |
| 3 | `domains` — same pattern. |
| 4 | `stats` — same pattern. |
| 5 | `query` — `query_results()` → `services::query::query()`. |
| 6 | `retrieve` — `retrieve_results()` → `services::query::retrieve()`. |

### Wave 2: Core Operations (PRs 7-12)

| PR | Scope | Complexity |
|----|-------|-----------|
| 7 | `scrape` — batch embedding, format selection | Medium |
| 8 | `map` — HTTP→Chrome fallback | Medium |
| 9 | `search` — Tavily wrapper + job enqueue | Low |
| 10 | `research` — Tavily + LLM synthesis | Low |
| 11 | `doctor` — probe functions | Low |
| 12 | `status` — aggregation query | Low |

### Wave 3: Complex Operations + Job Lifecycle (PRs 13-19)

| PR | Scope | Complexity |
|----|-------|-----------|
| 13 | `ask` — retrieval + rerank + LLM + citation gates | High |
| 14 | `evaluate` — RAG vs baseline + judge | Medium |
| 15 | `suggest` — facet + LLM | Low |
| 16 | `crawl` — sync/async dispatch, 8 subcommands, events | High |
| 17 | `embed` — sync/async, job lifecycle | Medium |
| 18 | `extract` — same pattern as embed | Medium |
| 19 | `dedupe` — full scan + progress | Medium |

### Wave 4: Ingest + Screenshot (PRs 20-24)

| PR | Scope | Complexity |
|----|-------|-----------|
| 20 | `github` — already returns structured data | Low |
| 21 | `reddit` — same | Low |
| 22 | `youtube` — same | Low |
| 23 | `sessions` — format parsing inline | Medium |
| 24 | `screenshot` — CDP protocol, file writes | Medium |

### Wave 5: Web Layer Rewire (PRs 25-27)

| PR | Scope |
|----|-------|
| 25 | `Config::from_env()` — factory for non-CLI construction |
| 26 | Web direct calls — replace subprocess proxy. Delete `exe.rs`, `args.rs`. |
| 27 | Event→WebSocket bridge — wire `ServiceEvent` channel to WS forwarding |

### Files Deleted

| File | When | Why |
|------|------|-----|
| `crates/web/execute/exe.rs` | Wave 5 | No more binary resolution |
| `crates/web/execute/args.rs` | Wave 5 | No more CLI arg building |
| `crates/web/execute/polling.rs` | Wave 5 | Direct job status calls replace subprocess polling |
| Scattered `*_payload()` functions | Per-wave | Replaced by service functions |

---

## Impact Summary

| Metric | Value |
|--------|-------|
| Files created | ~15 |
| Files modified | ~40-45 |
| Files removed | ~2-3 |
| Lines changed | ~4,400 |
| % of codebase | ~25% |
| PRs | ~27 |
| Waves | 5 |
