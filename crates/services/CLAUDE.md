# crates/services — Typed Service Layer
Last Modified: 2026-03-28

The contract boundary between all entry points (CLI commands, MCP handlers, web routes) and the underlying business logic crates (`vector`, `jobs`, `crawl`, `ingest`). Every external caller goes through a service function — no entry point calls `crates/vector/ops/*` directly.

## Module Layout

```
services/
├── context.rs              # ServiceContext — canonical handler entry point
├── runtime.rs              # ServiceJobRuntime trait + resolve_runtime() + LiteServiceRuntime
├── acp/                    # ACP adapter orchestration (Claude/Codex/Gemini subprocess)
│   ├── adapters.rs         # Adapter subprocess wrappers (spawn, stdin/stdout)
│   ├── bridge.rs           # Shared turn finalization: logging, EditorWrite, TurnResult dispatch
│   ├── config.rs           # ACP session/model/tool config builder
│   ├── mapping/            # SDK event mapping: SessionInfoUpdate, UsageUpdate, etc.
│   ├── permission.rs       # Permission bridge: maps ACP tool calls to gated operations
│   ├── persistent_conn/    # Persistent-connection mode: single process per WS lifetime
│   ├── preflight.rs        # Pre-flight checks before spawning an adapter
│   ├── runtime.rs          # One-shot mode: spawn → init → turn → teardown per prompt
│   ├── session.rs          # Session setup: context injection, system prompt assembly
│   └── session_cache/      # WS reconnect replay buffer (TTL, byte cap, reaper)
│       ├── cache.rs        # SessionCache impl — insert, replay, reap
│       └── entry.rs        # SessionEntry type + message buffer
├── acp_llm.rs              # ACP-backed LLM completion gateway (module root + re-exports)
├── acp_llm/                # Submodules for the completion gateway
│   ├── runner.rs           # AcpRuntimeCompletionRunner — one-shot adapter execution
│   ├── types.rs            # AcpCompletionRequest/Response, AcpUsageSnapshot, helpers
│   └── warm.rs             # WarmAcpSession — pre-warmed adapter (overlaps cold-start)
├── events.rs               # ServiceEvent enum + emit() — async channel helper
├── types/
│   ├── acp.rs              # AcpBridgeEvent enum (all ACP → client wire events)
│   └── service.rs          # All typed result structs (QueryResult, AskResult, ...)
├── query.rs                # query, retrieve, ask, evaluate, suggest
├── system.rs               # doctor, stats, sources, domains, status, dedupe
├── scrape.rs               # scrape
├── search.rs               # search, research
├── map.rs                  # map
├── screenshot.rs           # screenshot
├── crawl.rs                # crawl start/status/cancel/list/cleanup/recover
├── embed.rs                # embed start/status/cancel/list
├── extract.rs              # extract start/status/cancel/list
├── ingest.rs               # ingest start/status/cancel/list
├── jobs.rs                 # shared job status helpers
├── migrate.rs              # collection migration (unnamed → named mode)
├── watch.rs                # watch definition + run management
├── debug.rs                # doctor + LLM-assisted debug
└── error.rs                # service error types
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

`ServiceJobRuntime` is a strict superset of [`JobBackend`](../jobs/backend.rs): it adds `has_active_jobs`, `recover_jobs`, `run_worker`, pagination (`limit`/`offset` on `list_jobs`), and returns the richer `ServiceJob` type everywhere instead of `JobStatusRow`/`JobSummary`. `LiteServiceRuntime` delegates only `enqueue`, `wait_for_job`, and `job_errors` through `JobBackend`; all other operations call `lite_query::*` directly to avoid lossy type mapping. See the module-level doc comment in `runtime.rs` for the full rationale.

The job operations interface consumed by `ServiceContext.jobs`:

- `enqueue(payload)` → `Uuid`
- `job_status(kind, id)` → `Option<ServiceJob>`
- `cancel_job(kind, id)` → `bool`
- `list_jobs(kind, limit, offset)` → `Vec<ServiceJob>`
- `cleanup_jobs(kind)`, `clear_jobs(kind)`, `recover_jobs(kind, stale_ms)` → `u64`
- `run_worker(kind)` → `WorkerMode` (`Started` / `InProcess` / `Unsupported`)
- `wait_for_job(id, kind)` → `String` (final status)

`resolve_runtime(cfg)` in `runtime.rs` constructs `LiteServiceRuntime` (wraps `LiteBackend`).

## Architecture Contract

**Rule:** CLI handlers, MCP handlers, and web API routes call **service functions only** — never raw `crates/vector/ops/*` or `crates/jobs/*` functions directly.

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
        ServiceEvent::AcpBridge { event } => { /* forward to WS client */ }
        ServiceEvent::EditorWrite { content, operation } => { /* apply to editor */ }
    }
}
```

Pass `None` for `tx` in CLI commands that don't need streaming progress. `emit()` is a no-op when `tx` is `None`.

**Backpressure:** `emit()` uses `.send().await` — it blocks if the channel is full. Use a channel size that matches expected burst rate (default 32 for ask, larger for ACP streaming turns).

## ACP Module (`acp/`)

The ACP module orchestrates adapter subprocess execution (Claude Code, Codex, Gemini CLI). Two complete code paths exist:

### One-Shot (`acp/runtime.rs`)
- Spawns a fresh adapter subprocess per prompt turn
- Lifecycle: spawn → init → set session → apply config → execute → teardown
- After turn: awaits adapter exit with **10-second timeout** (allows session flush before SIGKILL)
- Higher latency, clean state per turn

### Persistent-Connection (`acp/persistent_conn/`)
- Keeps a single adapter process alive for the entire WebSocket connection lifetime
- Turns dispatched via `mpsc` channel to the long-running process
- Adapter set up lazily on first turn
- Timeout: **3600 seconds** (configurable via `adapter_timeout_secs`)
- Lower latency on subsequent turns; process managed across full WS lifetime

Both paths call `bridge::finalize_successful_turn()` for consistent completion behavior: logging, `EditorWrite` emission, `TurnResult` dispatch.

### Session Cache (`acp/session_cache/`)

WS reconnect replay buffer. Hardcoded constants:

| Parameter | Value | Description |
|-----------|-------|-------------|
| `SESSION_TTL` | 30 minutes | Idle session eviction |
| `MAX_REPLAY_BUFFER` | 4096 messages | Per-session message count cap |
| `MAX_REPLAY_BUFFER_BYTES` | 4 MiB | Per-session byte cap |
| Reaper interval | 60 seconds | Background cleanup frequency |
| `AXON_ACP_MAX_CONCURRENT_SESSIONS` | 8 (default) | Semaphore limit on concurrent ACP sessions |

The reaper starts lazily via `Once` on first session insertion.

## ACP LLM Completion Gateway (`acp_llm/`)

`acp_llm.rs` is a thin completion facade on top of the ACP adapter. Unlike the interactive session paths in `acp/`, this module is designed for request/response LLM calls (no streaming turns, no WS connection).

Two code paths:

| Path | Function | Use case |
|------|----------|----------|
| **One-shot** | `complete_text(cfg, req)` / `complete_streaming(cfg, req, callback)` | Spawns a fresh adapter per call — highest isolation |
| **Pre-warmed** | `warm_session(cfg) -> WarmAcpSession` | Starts the adapter eagerly to overlap cold-start; call `.complete(req)` on the returned handle |

`AcpCompletionRequest` fields: `system_prompt: Option<String>`, `prompt: String`, `model: Option<String>`, `stream: bool`.

**When to use vs `acp/runtime.rs`:** Use `acp_llm` for fire-and-forget completions (ask synthesis, research summaries, extract fallback). Use `acp/runtime.rs` / `persistent_conn` for interactive Pulse Chat sessions where turn state and WS streaming matter.

> Full ACP protocol reference: `docs/ACP.md`

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

Pure mapping tests (`map_*` functions) and channel tests run without live services. Tests for `query`, `ask`, `sources`, etc. that call into `crates/vector` require Qdrant + TEI.

## Adding a New Service Function

1. Add the function to the appropriate `crates/services/<name>.rs` — signature takes `&ServiceContext`
2. Add a typed result struct to `crates/services/types/service.rs`
3. Call from the CLI handler in `crates/cli/commands/<name>.rs` — receives `&ServiceContext`
4. Call from the MCP handler in `crates/mcp/server/handlers_*.rs` — receives `&ServiceContext`
5. If the feature is unavailable in the current runtime, return an appropriate error
6. Add mapping helpers and unit tests for pure logic (no live services needed)
7. Never print, log, or serialize inside the service function — return the typed result

## `watch.rs` and `events.rs` — Live Streaming

`watch.rs` manages watch definition and run lifecycle for `axon watch` commands. It uses `ServiceEvent` as the streaming primitive — the watch runner emits events via `tx` that the web/WS layer forwards to clients in real-time.
