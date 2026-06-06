# src/services — Typed Service Layer
Last Modified: 2026-05-09

The contract boundary between all entry points (CLI commands, MCP handlers, web routes) and the underlying business logic crates (`vector`, `jobs`, `crawl`, `ingest`). Every external caller goes through a service function — no entry point calls `src/vector/ops/*` directly.

## Module Layout

```
services/
├── context.rs              # ServiceContext — canonical handler entry point (cfg + jobs only)
├── runtime.rs              # ServiceJobRuntime trait + resolve_runtime{,_with_workers}() + SqliteServiceRuntime
├── llm_backend.rs          # Gemini headless LLM completion gateway module root
├── llm_backend/           # Gemini dispatch, env allowlist, concurrency, and typed completion API
├── crawl.rs                # crawl start/status/cancel/list/cleanup/recover
├── crawl_sync.rs           # Synchronous crawl orchestration (24h cache, sitemap-only, HTTP→Chrome fallback)
├── debug.rs                # doctor + LLM-assisted debug
├── embed.rs                # embed start/status/cancel/list
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
│   └── service.rs          # All typed result structs (QueryResult, AskResult, ...)
└── watch.rs                # CRUD shim — actual scheduler runtime lives in src/jobs/watch.rs
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

## `ServiceJobRuntime` Trait (`runtime.rs`)

**This is the canonical job abstraction.** All callers (CLI, MCP) interact with jobs exclusively through `ServiceJobRuntime` via `ServiceContext.jobs` — never through `JobBackend` directly.

`ServiceJobRuntime` is a strict superset of [`JobBackend`](../jobs/backend.rs): it adds `has_active_jobs`, `recover_jobs`, `run_worker`, pagination (`limit`/`offset` on `list_jobs`), and returns the richer `ServiceJob` type everywhere instead of `JobStatusRow`/`JobSummary`. `SqliteServiceRuntime` delegates only `enqueue`, `wait_for_job`, and `job_errors` through `JobBackend`; all other operations call `job_query::*` directly to avoid lossy type mapping. See the module-level doc comment in `runtime.rs` for the full rationale.

The job operations interface consumed by `ServiceContext.jobs`:

- `enqueue(payload)` → `Uuid`
- `job_status(kind, id)` → `Option<ServiceJob>`
- `cancel_job(kind, id)` → `bool`
- `list_jobs(kind, limit, offset)` → `Vec<ServiceJob>`
- `cleanup_jobs(kind)`, `clear_jobs(kind)`, `recover_jobs(kind, stale_ms)` → `u64`
- `run_worker(kind)` → `WorkerMode` (`Started` / `InProcess` / `Unsupported`)
- `wait_for_job(id, kind)` → `String` (final status)

Two public entry points construct the runtime: `resolve_runtime(cfg)` (no workers) and `resolve_runtime_with_workers(cfg, spawn)` (driven by `ServiceContext::new_with_workers`). Both return `Arc<dyn ServiceJobRuntime>` backed by `SqliteServiceRuntime`, which wraps `SqliteJobBackend`.

### SqliteJobBackend construction modes

`SqliteJobBackend` has **two** construction modes — workers do **not** spawn unconditionally:

| Constructor | Workers? | Used by |
|-------------|----------|---------|
| `SqliteJobBackend::new(cfg)` | **No** — enqueue-only | CLI commands that just enqueue/inspect jobs (status, list, cancel, fire-and-forget submit), all `ServiceContext::new(cfg)` callers |
| `SqliteJobBackend::new_with_workers(cfg)` | **Yes** — spawns in-process tokio workers (crawl + N×embed + extract + N×ingest) | `ServiceContext::new_with_workers(cfg)`: serve, MCP server, web routes, sync `--wait true` CLI paths that need a worker to drain the queue |

CLI fire-and-forget contexts must use `new()`. Spawning workers in a short-lived CLI process orphans claimed jobs when the process exits before they finish.

## Architecture Contract

**Rule:** CLI handlers, MCP handlers, and web API routes call **service functions only** — never raw `src/vector/ops/*` or `src/jobs/*` functions directly.

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

Key result types in `types/service.rs`:

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

## ServiceEvent — Async Progress Channel

Service functions that do multi-step work (e.g. `ask`) accept an optional `tx: Option<mpsc::Sender<ServiceEvent>>`. Callers subscribe to get progress logs without polling.

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

## LLM Backend (`llm_backend/`)

`llm_backend` is the typed completion facade used by ask synthesis, evaluate,
suggest, research summaries, summarize, extract fallback, debug, and the watch
change-report summarizer. `complete_text` / `complete_streaming` dispatch on
`LlmBackendKind` (set from `AXON_LLM_BACKEND`) to one of three backends:
- `gemini-headless` (default) — `headless/gemini.rs`, spawns the Gemini CLI with
  an isolated temp HOME, allowlisted env, command validation, timeout.
- `openai-compat` — `openai_compat.rs`, any OpenAI-compatible chat endpoint.
- `codex-app-server` — `codex_app_server.rs` (+ `codex_app_server/{protocol,home}.rs`),
  spawns `codex app-server` over stdio in an isolated `CODEX_HOME` and runs the
  JSON-RPC synthesis handshake. `protocol.rs` is a pure, unit-tested state machine.

All backends share an allowlisted environment, timeout enforcement, and a
process-wide concurrency semaphore. Backend selection is global — there is no
per-action override.

Use `CompletionRequest` and `CompletionResponse` for service-facing synthesis
calls. Entry points should not spawn the backend process directly.

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

## Adding a New Service Function

1. Add the function to the appropriate `src/services/<name>.rs` — signature takes `&ServiceContext`
2. Add a typed result struct to `src/services/types/service.rs`
3. Call from the CLI handler in `src/cli/commands/<name>.rs` — receives `&ServiceContext`
4. Call from the MCP handler in `src/mcp/server/handlers_*.rs` — receives `&ServiceContext`
5. If the feature is unavailable in the current runtime, return an appropriate error
6. Add mapping helpers and unit tests for pure logic (no live services needed)
7. Never print, log, or serialize inside the service function — return the typed result

## `watch.rs` and `events.rs` — Live Streaming

`src/services/watch.rs` is a thin CRUD layer (~2 KB) that exposes watch definition + run lookups to CLI, MCP, and HTTP callers. The actual scheduler runtime lives in `src/jobs/watch.rs` (SQLite-backed, in-process). Streaming is plumbed through `ServiceEvent` so callers can forward progress without putting logging or serialization inside the service function.
