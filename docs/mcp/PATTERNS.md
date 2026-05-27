# MCP Code Patterns -- Axon

Reusable patterns in the Axon MCP server implementation.

## Single-tool dispatch

Axon uses a single MCP tool (`axon`) with `action`/`subaction` routing. All operations go through one entry point.

### Schema parsing

Input is parsed strictly with serde in `src/mcp/schema.rs`:

```rust
#[derive(Deserialize)]
pub struct McpInput {
    pub action: Action,
    pub subaction: Option<Subaction>,
    pub url: Option<String>,
    pub urls: Option<Vec<String>>,
    pub query: Option<String>,
    pub response_mode: Option<ResponseMode>,
    // ... per-action fields
}
```

Rules:
- `action` is required and must match canonical enum names
- `subaction` is required for lifecycle families (crawl, extract, embed, ingest, artifacts)
- No fallback fields, no token normalization, no case folding
- Invalid input returns MCP `invalid_params` error

### Handler dispatch

In `src/mcp/server.rs`, the main handler matches on action:

```rust
match input.action {
    Action::Scrape => handle_scrape(ctx, input).await,
    Action::Crawl => match input.subaction {
        Some(CrawlSubaction::Start) => handle_crawl_start(ctx, input).await,
        Some(CrawlSubaction::Status) => handle_crawl_status(ctx, input).await,
        // ...
    },
    Action::Ask => handle_ask(ctx, input).await,
    // ...
}
```

Each handler calls the services layer and maps the typed result to MCP wire format.

## Services layer

All MCP handlers call through the services layer (`src/services/`), never directly to infrastructure:

```
MCP handler -> services::query() -> vector::ops::search() -> Qdrant
MCP handler -> services::ask()   -> vector::ops::ask()    -> Qdrant + configured LLM backend
CLI handler -> services::query() -> (same path)
Web route   -> services::query() -> (same path)
```

Each service function returns a typed result struct defined in `src/services/types/service.rs`. No raw JSON printing or stdout side-effects in the service layer.

## Artifact response pattern

Most heavy operations write results to artifact files instead of returning inline:

```
1. Handler executes operation
2. Result serialized to JSON
3. Written to $AXON_MCP_ARTIFACT_DIR/<hash>.json
4. MCP response returns compact metadata:
   - path: artifact file path
   - bytes: file size
   - line_count: lines in artifact
   - sha256: content hash
   - preview: first N lines
   - preview_truncated: boolean
```

### Response mode selection

```rust
enum ResponseMode {
    Path,        // Default — artifact only, return metadata
    Inline,      // Return full result inline (capped/truncated)
    Both,        // Write artifact AND return inline
    AutoInline,  // Inline if below threshold, else artifact
}
```

`auto_inline` checks `AXON_INLINE_BYTES_THRESHOLD` (default 8192 bytes). Payloads at or below the threshold are returned inline without requiring a separate artifact read.

`scrape` and `retrieve` are the document-reader exceptions: they default to inline paged responses and use artifacts as a secondary debug/inspection path rather than the primary reading UX.

## Error handling

### Structured errors

```rust
// Service layer returns typed errors
pub enum AxonError {
    ServiceUnavailable(String),  // Infrastructure not reachable
    InvalidInput(String),        // Bad parameters
    NotFound(String),            // Resource not found
    Internal(String),            // Unexpected failure
}
```

### MCP error mapping

| Source | MCP error code |
|--------|---------------|
| Invalid action/subaction | `invalid_params` |
| Missing required field | `invalid_params` |
| Service unreachable | `internal_error` |
| Job not found | `internal_error` |

### Canonical error envelope

```json
{
  "ok": false,
  "action": "crawl",
  "subaction": "status",
  "error": "Job abc-123 not found"
}
```

## ServiceContext

The `ServiceContext` is constructed at startup and shared across all handlers
(definition in `src/services/context.rs`):

```rust
pub struct ServiceContext {
    pub cfg: Arc<Config>,
    pub jobs: Arc<dyn ServiceJobRuntime>,
}
```

There is no separate `capabilities` struct on `ServiceContext`. Jobs are backed by SQLite via
`SqliteServiceRuntime`. Two construction modes exist:

- `ServiceContext::new(cfg)` — enqueue-only, no in-process workers (CLI default).
- `ServiceContext::new_with_workers(cfg)` — spawns in-process workers for jobs.
  Used by `axon serve` and the MCP server.

The MCP server constructs its `ServiceContext` once via `new_with_workers` (see
`AxonMcpServer::base_service_context` in `src/mcp/server.rs`) so jobs
enqueued via MCP are drained by the same process.

## Lifecycle job pattern

Lifecycle actions (crawl, extract, embed, ingest) share a common pattern:

1. `start` -- enqueue job, return job ID
2. `status` -- query SQLite for job state
3. `cancel` -- mark the job cancelled
4. `list` -- list recent jobs from SQLite (paginated by `limit`/`offset`)
5. `cleanup` -- remove completed/failed jobs
6. `clear` -- remove all jobs
7. `recover` -- reclaim stale/interrupted jobs

Each job type has:
- A payload/schema helper in `src/jobs/<type>.rs`
- A SQLite table (e.g., `axon_crawl_jobs`) created by the migrations under `src/jobs/migrations/`
- A runner under `src/jobs/workers/runners/`
- An in-process worker lane started by `SqliteJobBackend::new_with_workers`

## Hybrid search pattern

Vector search uses Reciprocal Rank Fusion (RRF) with two prefetch arms:

```
Query
  ├── Dense vector (TEI embedding) → HNSW search
  ├── BM42 sparse vector (keyword) → Sparse index search
  └── RRF fusion → merged, re-ranked results
```

Named-mode collections (new) support hybrid search. Legacy unnamed-mode collections fall back to dense-only. The `VectorMode` is cached per-process -- restart workers after collection migration.

## LLM completion backend pattern

Operations requiring LLM synthesis (`ask`, `evaluate`, `suggest`, `research`, `extract` fallback, `debug`) call the typed `services::llm_backend` facade. Gemini headless is the default backend and launches with an isolated temporary HOME, an allowlisted environment, timeout enforcement, and a concurrency semaphore. `AXON_LLM_BACKEND=openai-compat` selects an OpenAI-compatible chat-completions endpoint such as llama.cpp via `AXON_OPENAI_BASE_URL` and `AXON_OPENAI_MODEL`.

## See also

- [TOOLS.md](TOOLS.md) -- action/subaction reference
- [DEV.md](DEV.md) -- development workflow
- [../stack/ARCH.md](../stack/ARCH.md) -- architecture overview
