# src/services — Typed Service Layer
Last Modified: 2026-06-13

The contract boundary between all entry points (CLI commands, MCP handlers, web routes) and the underlying business logic crates (`vector`, `jobs`, `crawl`, `ingest`). Every external caller goes through a service function — no entry point calls `crates/axon-vector/src/ops/*` directly.

## Module Layout

```
services/
├── context.rs              # ServiceContext — canonical handler entry point (cfg + jobs only)
├── runtime.rs              # Narrow job capability traits + ServiceJobRuntime umbrella + SqliteServiceRuntime
├── crawl.rs                # crawl start/status/cancel/list/cleanup/recover
├── crawl_sync.rs           # Synchronous crawl orchestration (24h cache, sitemap-only, HTTP→Chrome fallback)
├── debug.rs                # doctor + LLM-assisted debug
├── embed.rs                # embed start/status/cancel/list + shared server-side input guard
├── error.rs                # service error types
├── events.rs               # ServiceEvent enum + emit() — async channel helper
├── extract.rs              # extract start/status/cancel/list
├── ingest.rs               # ingest start/status/cancel/list; re-exports classify_target, ingest_generic_git_with_progress, ingest_gitea_with_progress, source_from_mcp_request, validate_ingest_source
├── ingest/classify.rs      # ingest classification helpers (services-layer wrapper)
├── ingest/git_services.rs  # ingest_generic_git_with_progress(), ingest_gitea_with_progress() — thin orchestration wrappers
├── ingest/request.rs       # source_from_mcp_request(), validate_ingest_source() — MCP request → IngestSource mapping (factored out of handlers_embed_ingest.rs)
├── jobs.rs                 # shared job status helpers
├── map.rs                  # map
├── migrate.rs              # collection migration (unnamed → named mode)
├── query.rs                # query, retrieve, ask, evaluate, suggest
├── scrape.rs               # scrape
├── screenshot.rs           # screenshot
├── artifacts.rs            # artifact handles + root-confined atomic writes for service-owned outputs
├── search.rs               # search, research
├── setup.rs                # Setup-flow service entry
├── setup/
│   ├── assets.rs           # Setup assets
│   ├── config_store.rs     # Persistent config-store helpers
│   ├── deploy.rs           # Setup deploy steps
│   └── ssh_targets.rs      # Remote SSH target management
├── system.rs               # doctor, stats, sources, domains, status, dedupe
├── types.rs                # types/ module root
├── types/
│   ├── contracts.rs        # External-facing service contract types
│   ├── service.rs          # Re-export glue for domain-specific service result modules
│   └── service/            # Result contracts by domain (query, content, system, lifecycle, ...)
└── watch.rs                # CRUD shim — actual scheduler runtime lives in crates/axon-jobs/src/watch.rs
```

## `ServiceContext` — The Entry Point

`ServiceContext` (in `context.rs`) is the canonical handler object passed to every CLI handler, MCP handler, and web route. Constructed once at startup:

```rust
let ctx = ServiceContext::new(Arc::new(cfg)).await?;
// then pass &ctx to every handler
```

Fields:
| Field | Type | Description |
|-------|------|-------------|
| `cfg` | `Arc<Config>` | Runtime config |
| `jobs` | `Arc<dyn ServiceJobRuntime>` | Backend-agnostic job operations |

**Never construct `ServiceContext` in tests** — use `ServiceContext::from_runtime(cfg, jobs)` with a mock `ServiceJobRuntime` instead.

## Job Runtime Traits (`runtime.rs`)

**This is the canonical job abstraction.** All callers (CLI, MCP, REST) interact with jobs through `ServiceContext.jobs` — never through `JobBackend` directly.

`ServiceJobRuntime` remains the object-safe umbrella trait stored in `ServiceContext`, but `runtime.rs` also exposes narrower capability traits for implementations and tests:

- `ServiceRuntimeIdentity`
- `ServiceSqliteRuntimeAccess`
- `ServiceJobSubmission`
- `ServiceJobQuery`
- `ServiceJobMaintenance`
- `ServiceWorkerControl`

`ServiceJobRuntime` is a strict superset of [`JobBackend`](../jobs/backend.rs): it adds active-job checks, recover/maintenance operations, worker control, pagination (`limit`/`offset` on `list_jobs`), and returns the richer `ServiceJob` type everywhere instead of `JobStatusRow`/`JobSummary`. `SqliteServiceRuntime` delegates enqueue/wait/error primitives through `SqliteJobBackend`; query and maintenance paths call `job_query::*` or backend helpers directly to avoid lossy type mapping. See the module-level doc comment in `runtime.rs` for the full rationale.

The job operations interface consumed by `ServiceContext.jobs`:

- `enqueue(payload)` → `Uuid`
- `job_status(kind, id)` → `Option<ServiceJob>`
- `cancel_job(kind, id)` → `bool`
- `list_jobs(kind, limit, offset)` → `Vec<ServiceJob>`
- `cleanup_jobs(kind)`, `clear_jobs(kind)`, `recover_jobs(kind, stale_ms)` → `u64`
- `start_worker(kind)` / `drain_jobs(kind)` → `WorkerMode` (`Started` / `InProcess` / `Unsupported`)
- `wait_for_job(id, kind)` → `String` (final status)

Two public entry points construct the runtime: `resolve_runtime(cfg)` (no workers) and `resolve_runtime_with_workers(cfg, spawn)` (driven by `ServiceContext::new_with_workers`). Both return `Arc<dyn ServiceJobRuntime>` backed by `SqliteServiceRuntime`, which wraps `SqliteJobBackend`.

`drain_jobs()` is bounded by `cfg.job_wait_timeout_secs` and reports progress through `tracing`, not `stderr`. Keep service/runtime code free of direct `println!`/`eprintln!` output so CLI, MCP, and REST transports can format independently.

### SqliteJobBackend construction modes

`SqliteJobBackend` has **two** construction modes — workers do **not** spawn unconditionally:

| Constructor | Workers? | Used by |
|-------------|----------|---------|
| `SqliteJobBackend::new(cfg)` | **No** — enqueue-only | CLI commands that just enqueue/inspect jobs (status, list, cancel, fire-and-forget submit), all `ServiceContext::new(cfg)` callers |
| `SqliteJobBackend::new_with_workers(cfg)` | **Yes** — spawns in-process tokio workers (crawl + N×embed + extract + N×ingest) | `ServiceContext::new_with_workers(cfg)`: serve, MCP server, web routes, sync `--wait true` CLI paths that need a worker to drain the queue |

CLI fire-and-forget contexts must use `new()`. Spawning workers in a short-lived CLI process orphans claimed jobs when the process exits before they finish.

## Architecture Contract

**Rule:** CLI handlers, MCP handlers, and web API routes call **service functions only** — never raw `crates/axon-vector/src/ops/*` or `crates/axon-jobs/src/*` functions directly.

```
CLI handler (run_ask)
    └─→ services::query::ask(cfg, question, tx)
            └─→ vector::ops::commands::ask::ask_payload(cfg, question)
                    └─→ vector::ops::tei, qdrant, ranking ...
```

This enforces a single call path that can be tested, typed, and evolved independently of the entry points.

## Typed Result Pattern

Every service function returns a typed `Result<SomeResult, Box<dyn Error>>` — no printing to stdout, no JSON serialization inside service functions. Callers format the result for their target (CLI human text, CLI JSON, MCP response, HTTP JSON).

```rust
// ✓ Correct: service function returns typed result
pub async fn query(cfg: &Config, text: &str, opts: Pagination) -> Result<QueryResult, Box<dyn Error>>

// ✗ Wrong: never do this inside a service function
println!("{}", serde_json::to_string(&results)?);
```

Key result types live in domain-specific modules under `types/service/` and are
re-exported through `types/service.rs` for compatibility:

| Result Type | Service function(s) |
|-------------|---------------------|
| `QueryResult` | `query::query` |
| `RetrieveResult` | `query::retrieve` |
| `AskResult` | `query::ask` |
| `EvaluateResult` | `query::evaluate` |
| `SuggestResult` | `query::suggest` |
| `SourcesResult` | `system::sources` |
| `DomainsResult` | `system::domains` |
| `StatsResult` | `system::stats` |
| `DoctorResult` | `system::doctor` |
| `StatusResult` | `system::status` |
| `CrawlStartResult` | `crawl::start_crawl` |
| `EmbedStartResult` | `embed::start_embed` |
| `IngestStartResult` | `ingest::start_ingest` |
| `ScrapeResult` | `scrape::scrape` |
| `SearchResult` | `search::search` |
| `ResearchResult` | `search::research` |

When adding a new typed result, put it in the matching domain module under
`crates/axon-services/src/types/service/` (for example `query.rs`, `content.rs`,
`system.rs`, or `lifecycle.rs`) and re-export it from `types/service.rs`.
Create a new small domain module when no existing module owns the contract.

## Indexing Service Semantics

The service layer is responsible for deciding whether an embedding summary can
be partial or must be all-or-error. The low-level vector pipeline reports
`EmbedSummary { docs_embedded, docs_failed, chunks_embedded }`; services that
expose user-facing indexing should call `require_success(...)` unless partial
success is explicitly part of that service's contract.

Current contracts:
- `scrape::scrape_batch_with_optional_embed()` preserves in-memory scrape and
  vertical-extractor metadata by converting each `ScrapeResult` into a
  `SourceDocument::try_new_web_markdown(...)`, then fails the whole batch if any
  scrape embed document fails.
- REST sync post handlers use the same scrape service behavior, so `/v1/scrape`
  does not report success while silently dropping embedded docs.
- `memory::remember()` uses `SourceDocument::new_memory(...)` with the memory
  UUID as the stable Qdrant point ID. If the SQLite write fails after Qdrant
  upsert, the service attempts to delete the Qdrant memory URL to avoid
  split-brain. `memory::supersede()` similarly rolls the Qdrant status back to
  `active` if SQLite edge creation fails.

Do not bypass these service functions from CLI/MCP/REST adapters. If a new
indexing surface needs a different partial-failure policy, document it in that
service and cover it with a sidecar test.

## ServiceEvent — Async Progress Channel

Service functions that do multi-step work (e.g. `ask`, `scrape`) accept an optional `tx: Option<mpsc::Sender<ServiceEvent>>`. Callers subscribe to get progress logs without polling.

```rust
let (tx, mut rx) = mpsc::channel::<ServiceEvent>(32);
let result = services::query::ask(cfg, "my question", Some(tx)).await?;

while let Some(event) = rx.recv().await {
    match event {
        ServiceEvent::Log { level, message } => eprintln!("[{level}] {message}"),
        ServiceEvent::EditorWrite { content, operation } => { /* apply to editor */ }
    }
}
```

Pass `None` for `tx` in CLI commands that don't need streaming progress. `emit()` is a no-op when `tx` is `None`.

**Backpressure:** `emit()` uses `.send().await` — it blocks if the channel is full. Use a channel size that matches expected burst rate (default 32 for ask/research streaming).

## LLM Backend (`crates/axon-core/src/llm/`)

The typed completion facade no longer lives under `services/` — it is now
`crate::core::llm` (`crates/axon-core/src/llm.rs` + `crates/axon-core/src/llm/{types,concurrency,headless,openai_compat}.rs`).
It is consumed by the service layer (ask synthesis, evaluate, suggest, research
summaries, extract fallback, debug) but is not itself a service module.

`core::llm` launches Gemini headless with an isolated temporary HOME, an
allowlisted environment, command validation, timeout enforcement, and
backend/limit-keyed concurrency semaphores. The OpenAI-compatible backend reuses
reqwest clients by timeout bucket and returns bounded, redacted upstream error
bodies.

Use `CompletionRequest` and `CompletionResponse` for service-facing synthesis
calls. Entry points should not spawn Gemini directly.

## Testing

```bash
cargo test services          # all service unit tests
cargo test map_query         # QueryResult mapping tests (no services needed)
cargo test map_retrieve      # RetrieveResult mapping tests
cargo test map_suggest       # SuggestResult mapping tests
cargo test log_level         # LogLevel from/display tests
cargo test emit              # ServiceEvent channel tests
cargo test -- --nocapture    # show log output
```

Pure mapping tests (`map_*` functions) and channel tests run without live services. Tests for `query`, `ask`, `sources`, etc. that call into `src/vector` require Qdrant + TEI.

## Two-Tier Signature Convention

Service functions split into two tiers depending on what they need from the runtime:

| Tier | Signature | Used by |
|------|-----------|---------|
| Job-lifecycle | `fn start_*(ctx: &ServiceContext, ...)` | Enqueue/cancel/list jobs — needs `ctx.jobs` |
| Read/RAG | `fn <name>(cfg: &Config, ...)` | Qdrant/TEI queries only — `&Config` is sufficient |

**Rule:** If a function never touches `ctx.jobs`, accept `&Config` directly — not `&ctx.cfg`. This keeps read/RAG functions independently testable without constructing a full `ServiceContext`.

```rust
// ✓ Job-lifecycle: needs the job runtime
pub async fn start_crawl(ctx: &ServiceContext, url: Url, ...) -> Result<CrawlStartResult, ...>

// ✓ Read/RAG: only needs config
pub async fn query(cfg: &Config, text: &str, ...) -> Result<QueryResult, ...>

// ✗ Wrong: read function unnecessarily requires full ServiceContext
pub async fn query(ctx: &ServiceContext, text: &str, ...) -> Result<QueryResult, ...>
```

## Adding a New Service Function

1. Add the function to the appropriate `crates/axon-services/src/<name>.rs` — signature takes `&ServiceContext`
2. Add a typed result struct to the appropriate `crates/axon-services/src/types/service/<domain>.rs` module and re-export it from `crates/axon-services/src/types/service.rs`
3. Call from the CLI handler in `crates/axon-cli/src/commands/<name>.rs` — receives `&ServiceContext`
4. Call from the MCP handler in `crates/axon-mcp/src/server/handlers_*.rs` — receives `&ServiceContext`
5. If the feature is unavailable in the current runtime, return an appropriate error
6. Add mapping helpers and unit tests for pure logic (no live services needed)
7. Never print, log, or serialize inside the service function — return the typed result

## `watch.rs` and `events.rs` — Live Streaming

`crates/axon-services/src/watch.rs` is a thin CRUD layer (~2 KB) that exposes watch definition + run lookups to CLI, MCP, and HTTP callers. The actual scheduler runtime lives in `crates/axon-jobs/src/watch.rs` (SQLite-backed, in-process). Streaming is plumbed through `ServiceEvent` so callers can forward progress without putting logging or serialization inside the service function.
