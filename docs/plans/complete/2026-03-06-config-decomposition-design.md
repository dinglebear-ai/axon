# Config Decomposition & Typed Service Params

**Date:** 2026-03-06
**Branch:** `feat/services-layer-refactor`
**Status:** Design approved, pending implementation plan

## Problem

`Config` is a 120-field god struct. Every function in the codebase takes `&Config` regardless of which fields it actually needs. Three surfaces (CLI, MCP, web) each convert their inputs into Config differently, with no shared service-level request types. A planned REST API would be a fourth surface hitting the same problem.

Additional issues:
- 3 service modules still import from `crates::cli` (inverted dependency)
- Job serialization stores full Config (120 fields) when jobs need ~20
- `apply_crawl_overrides` clones all 120 fields to override 8
- No compile-time enforcement of which fields a function actually needs

## Design Decisions

| Decision | Choice | Rationale |
|---|---|---|
| Migration strategy | Bottom-up phases (4 phases, each shippable) | Smallest blast radius per PR, validates assumptions incrementally |
| Infra passing | `ServiceContext` struct (endpoints + pool + http_client) | Single arg for cross-cutting infra, built once at startup, shared via `Arc` |
| Type co-location | Params live with their service module, shared types in `types.rs` | Keeps each service self-contained, avoids a new god file |
| Param field types | Concrete values, not `Option` | Surfaces resolve defaults before calling services; service logic stays simple |
| CLI inversions | Move implementation to domain crates | Fixes layering violation, completes existing refactor pattern |
| Job serialization | Serialize domain params, not full Config | Smaller payloads, workers use own env for infra URLs |
| Job migration | Drain-and-deploy (no dual-read) | Pre-production, easy to arrange empty queue window |
| Serde derives | `#[derive(Debug, Clone, Serialize, Deserialize)]` on all params/results | REST API readiness — axum handlers can deserialize directly |

## Architecture

### Before

```
CLI flags ──→ Config (120 fields) ──→ services (take &Config) ──→ domain crates
MCP JSON  ──→ Config (clone+mutate) ──→ services (take &Config) ──→ domain crates
WS msg    ──→ Config (apply_overrides) ──→ services (take &Config) ──→ domain crates
```

### After

```
CLI flags ──→ CrawlParams ──┐
MCP JSON  ──→ CrawlParams ──┼──→ services::crawl_start(ctx, params) ──→ crates::crawl
WS msg    ──→ CrawlParams ──┤
REST body ──→ CrawlParams ──┘
                             │
                    ServiceContext (Arc, built once)
                    ├── endpoints: ServiceEndpoints
                    ├── pool: PgPool
                    └── http_client: reqwest::Client
```

## Phase 1: Extract `ServiceContext`

**Goal:** Introduce `ServiceContext` alongside `&Config`. No breaking changes.
**Files:** ~15
**Risk:** Low

### New file: `crates/services/context.rs`

```rust
use sqlx::PgPool;

#[derive(Clone)]
pub struct ServiceContext {
    pub endpoints: ServiceEndpoints,
    pub pool: PgPool,
    pub http_client: reqwest::Client,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceEndpoints {
    pub pg_url: String,
    pub redis_url: String,
    pub amqp_url: String,
    pub qdrant_url: String,
    pub tei_url: String,
    pub collection: String,
    pub openai_base_url: String,
    pub openai_api_key: String,   // Secret — exclude from Debug
    pub openai_model: String,
    pub tavily_api_key: String,   // Secret — exclude from Debug
}

impl ServiceContext {
    pub async fn from_config(cfg: &Config) -> Result<Self, Box<dyn Error>> {
        let pool = make_pool(&cfg.pg_url).await?;
        let http_client = http_client()?.clone();
        Ok(Self {
            endpoints: ServiceEndpoints::from_config(cfg),
            pool,
            http_client,
        })
    }
}
```

### Changes

- `AxonMcpServer` stores `Arc<ServiceContext>` instead of `Arc<Config>`
  - Temporarily also keeps `Arc<Config>` for fields not yet on params
- Web `serve` builds `ServiceContext` at startup
- CLI `main()` builds `ServiceContext`, passes alongside Config
- Services gain `ctx: &ServiceContext` parameter (additive, Config still passed too)
- `jobs/common.rs::make_pool()` reused by `ServiceContext::from_config()`

### Queue config

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueConfig {
    pub crawl_queue: String,
    pub extract_queue: String,
    pub embed_queue: String,
    pub ingest_queue: String,
    pub refresh_queue: String,
    pub shared_queue: bool,
}
```

Lives on `ServiceContext` (or as a field of it) since queue names are process-level, not per-request.

## Phase 2: Fix CLI Inversions

**Goal:** Move 5 implementation functions from CLI to domain crates. Services stop importing `crates::cli`.
**Files:** ~10
**Risk:** Low

| Function | From | To |
|---|---|---|
| `scrape_payload()` | `cli/commands/scrape.rs` | `crates/crawl/scrape.rs` |
| `search_results()` | `cli/commands/search.rs` | `crates/crawl/search.rs` |
| `research_payload()` | `cli/commands/research.rs` | `crates/crawl/search.rs` |
| `build_doctor_report()` | `cli/commands/doctor.rs` | `crates/core/health.rs` |
| `status_full()` | `cli/commands/status.rs` | `crates/jobs/status.rs` |

CLI commands become thin shells: parse args → call service → format output.

Dependency direction after: `cli` → `services` → `{crawl, jobs, vector, core}`. No upward arrows.

## Phase 3: Typed Request Params

**Goal:** Services take typed params instead of `&Config`. All surfaces build params from their own input.
**Files:** ~30
**Risk:** Medium (widest change surface)

### Params structs (co-located with service modules)

**`crates/services/scrape.rs`**
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrapeParams {
    pub url: String,
    pub format: ScrapeFormat,
    pub embed: bool,
    pub root_selector: Option<String>,
    pub exclude_selector: Option<String>,
    pub custom_headers: Vec<String>,
}
```

**`crates/services/crawl.rs`**
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrawlParams {
    pub urls: Vec<String>,
    pub max_pages: u32,
    pub max_depth: usize,
    pub include_subdomains: bool,
    pub render_mode: RenderMode,
    pub delay_ms: u64,
    pub respect_robots: bool,
    pub discover_sitemaps: bool,
    pub sitemap_since_days: u32,
    pub min_markdown_chars: usize,
    pub drop_thin_markdown: bool,
    pub exclude_path_prefix: Vec<String>,
    pub root_selector: Option<String>,
    pub exclude_selector: Option<String>,
    pub custom_headers: Vec<String>,
    pub chrome: CrawlChromeParams,
    pub performance: CrawlPerformanceParams,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrawlChromeParams {
    pub remote_url: Option<String>,
    pub headless: bool,
    pub stealth: bool,
    pub anti_bot: bool,
    pub intercept: bool,
    pub bootstrap: bool,
    pub bootstrap_timeout_ms: u64,
    pub bootstrap_retries: usize,
    pub bypass_csp: bool,
    pub accept_invalid_certs: bool,
    pub network_idle_timeout_secs: u64,
    pub wait_for_selector: Option<String>,
    pub screenshot: bool,
    pub proxy: Option<String>,
    pub user_agent: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrawlPerformanceParams {
    pub crawl_concurrency_limit: Option<usize>,
    pub backfill_concurrency_limit: Option<usize>,
    pub request_timeout_ms: Option<u64>,
    pub fetch_retries: usize,
    pub retry_backoff_ms: u64,
    pub batch_concurrency: usize,
}
```

**`crates/services/query.rs`**
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryParams {
    pub text: String,
    pub limit: usize,
    pub offset: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AskParams {
    pub question: String,
    pub diagnostics: bool,
    pub max_context_chars: usize,
    pub candidate_limit: usize,
    pub chunk_limit: usize,
    pub full_docs: usize,
    pub backfill_chunks: usize,
    pub doc_fetch_concurrency: usize,
    pub doc_chunk_limit: usize,
    pub min_relevance_score: f64,
    pub authoritative_domains: Vec<String>,
    pub authoritative_boost: f64,
    pub authoritative_allowlist: Vec<String>,
    pub min_citations_nontrivial: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluateParams {
    pub question: String,
    pub diagnostics: bool,
    // inherits ask tuning from AskParams defaults
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrieveParams {
    pub url: String,
    pub max_points: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestParams {
    pub focus: Option<String>,
    pub limit: usize,
}
```

**`crates/services/search.rs`**
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchParams {
    pub query: String,
    pub limit: usize,
    pub time_range: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchParams {
    pub query: String,
    pub depth: Option<usize>,
}
```

**`crates/services/embed.rs`**
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbedParams {
    pub input: String,
    pub collection: String,
}
```

**`crates/services/extract.rs`**
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractParams {
    pub urls: Vec<String>,
    pub prompt: String,
    pub limit: u32,
}
```

**`crates/services/ingest.rs`**
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestGitHubParams {
    pub repo: String,
    pub token: Option<String>,
    pub include_source: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestRedditParams {
    pub target: String,
    pub client_id: String,
    pub client_secret: String,
    pub sort: RedditSort,
    pub time: RedditTime,
    pub max_posts: usize,
    pub min_score: i32,
    pub depth: usize,
    pub scrape_links: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestYouTubeParams {
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestSessionsParams {
    pub claude: bool,
    pub codex: bool,
    pub gemini: bool,
    pub project: Option<String>,
}
```

**`crates/services/screenshot.rs`**
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenshotParams {
    pub url: String,
    pub viewport_width: u32,
    pub viewport_height: u32,
    pub full_page: bool,
    pub chrome_remote_url: Option<String>,
}
```

### Service signatures after

```rust
// crates/services/scrape.rs
pub async fn scrape(ctx: &ServiceContext, params: ScrapeParams) -> Result<ScrapeResult, ...>

// crates/services/crawl.rs
pub async fn crawl_start(ctx: &ServiceContext, params: CrawlParams) -> Result<CrawlStartResult, ...>
pub async fn crawl_status(ctx: &ServiceContext, job_id: Uuid) -> Result<CrawlJobResult, ...>

// crates/services/query.rs
pub async fn query(ctx: &ServiceContext, params: QueryParams) -> Result<QueryResult, ...>
pub async fn ask(ctx: &ServiceContext, params: AskParams) -> Result<AskResult, ...>
pub async fn evaluate(ctx: &ServiceContext, params: EvaluateParams) -> Result<EvaluateResult, ...>
pub async fn retrieve(ctx: &ServiceContext, params: RetrieveParams) -> Result<RetrieveResult, ...>
pub async fn suggest(ctx: &ServiceContext, params: SuggestParams) -> Result<SuggestResult, ...>

// crates/services/search.rs
pub async fn search(ctx: &ServiceContext, params: SearchParams) -> Result<SearchResult, ...>
pub async fn research(ctx: &ServiceContext, params: ResearchParams) -> Result<ResearchResult, ...>
```

### Surface adapter pattern

Each surface converts its own input to the canonical params type:

```rust
// CLI (crates/cli/commands/crawl.rs)
let params = CrawlParams::from_config(&cfg);
let result = services::crawl_start(&ctx, params).await?;

// MCP (crates/mcp/server/handlers_crawl_extract.rs)
let params = CrawlParams {
    urls,
    max_pages: req.max_pages.unwrap_or(ctx.defaults.max_pages),
    // ... resolve all defaults from context
};
let result = services::crawl_start(&ctx, params).await?;

// Web (crates/web/execute/sync_mode.rs)
let params = ScrapeParams { url, ..ScrapeParams::defaults_from(&ctx) };
let result = services::scrape(&ctx, params).await?;

// Future REST (axum handler)
async fn post_crawl(
    State(ctx): State<Arc<ServiceContext>>,
    Json(params): Json<CrawlParams>,
) -> Result<Json<CrawlStartResult>, ...> {
    Ok(Json(services::crawl_start(&ctx, params).await?))
}
```

### Convenience constructors

For CLI, where Config has all the values already resolved:

```rust
impl CrawlParams {
    pub fn from_config(cfg: &Config) -> Self { /* map fields */ }
}
impl ScrapeParams {
    pub fn from_config(cfg: &Config) -> Self { /* map fields */ }
}
```

These live on the params structs themselves (not on Config) to keep the dependency direction correct.

## Phase 4: Slim Config & Job Serialization

**Goal:** Remove migrated fields from Config. Update job serialization to use params structs.
**Files:** ~20
**Risk:** Medium (touches Config struct, all test helpers)

### Config after slimming (~35 fields)

```rust
pub struct Config {
    // CLI dispatch
    pub command: CommandKind,
    pub positional: Vec<String>,
    pub start_url: String,
    pub urls_csv: Option<String>,
    pub url_glob: Vec<String>,
    pub query: Option<String>,
    pub wait: bool,
    pub yes: bool,
    pub json_output: bool,
    pub reclaimed_status_only: bool,

    // Output control
    pub output_dir: PathBuf,
    pub output_path: Option<PathBuf>,
    pub format: ScrapeFormat,

    // Performance profile (resolved at startup, feeds into params)
    pub performance_profile: PerformanceProfile,

    // Cron scheduling
    pub cron_every_seconds: Option<u64>,
    pub cron_max_runs: Option<usize>,

    // Session ingest flags
    pub sessions_claude: bool,
    pub sessions_codex: bool,
    pub sessions_gemini: bool,
    pub sessions_project: Option<String>,

    // Evaluate display
    pub evaluate_responses_mode: EvaluateResponsesMode,
    pub ask_diagnostics: bool,

    // Serve
    pub serve_port: u16,
}
```

### Job serialization

```rust
// Enqueue (services/crawl.rs)
let config_json = serde_json::to_value(&params)?;  // CrawlParams
insert_crawl_job(&ctx.pool, url, config_json).await?;

// Worker (jobs/crawl/worker)
let params: CrawlParams = serde_json::from_value(row.config_json)?;
let ctx = ServiceContext::from_env().await?;  // worker builds own context
run_crawl(&ctx, params).await;
```

Migration: drain queue, deploy. No dual-read fallback needed (pre-production).

### Test helper changes

```rust
// Before: 120-field struct literal
fn test_config(pg_url: &str) -> Config {
    Config { pg_url: pg_url.into(), redis_url: ..., /* 118 more */ }
}

// After: slim Config + ServiceContext
fn test_context() -> ServiceContext {
    ServiceContext::test_default()  // in-memory pool, localhost endpoints
}
fn test_config() -> Config {
    Config::default()  // only ~35 fields
}
```

## Result Types

Existing `payload: serde_json::Value` result types should migrate to properly typed fields for REST readiness. This can happen incrementally — not blocked on the Config decomposition.

## Non-Goals

- **Config sub-struct composition** (Config embedding `CrawlConfig`, `ChromeConfig` etc.) — Deferred. The params structs serve this role at the service boundary. Config internals can be restructured later if desired.
- **REST API implementation** — Separate effort. This design ensures REST is zero-friction when it lands.
- **readability/clean_html revisit** — Separate from Config decomposition. Research confirmed both stay `false`.

## Verification

Per phase:
1. `cargo check` — clean compile
2. `cargo test` — all existing tests pass
3. `cargo clippy` — no warnings
4. No `use crate::crates::cli` in `crates/services/` (after phase 2)
5. No `&Config` in service function signatures (after phase 3)
6. Config field count < 40 (after phase 4)
