# MCP Code Patterns -- Axon

Reusable patterns in the Axon MCP server implementation.

## Single-tool dispatch

Axon uses a single MCP tool (`axon`) with `action`/`subaction` routing. All operations go through one entry point.

### Schema parsing

Input is parsed strictly with serde in `crates/mcp/schema.rs`:

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
- `subaction` is required for lifecycle families (crawl, extract, embed, ingest, refresh, graph, artifacts)
- No fallback fields, no token normalization, no case folding
- Invalid input returns MCP `invalid_params` error

### Handler dispatch

In `crates/mcp/server.rs`, the main handler matches on action:

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

All MCP handlers call through the services layer (`crates/services/`), never directly to infrastructure:

```
MCP handler -> services::query() -> vector::ops::search() -> Qdrant
MCP handler -> services::ask()   -> vector::ops::ask()    -> Qdrant + ACP
CLI handler -> services::query() -> (same path)
Web route   -> services::query() -> (same path)
```

Each service function returns a typed result struct defined in `crates/services/types/service.rs`. No raw JSON printing or stdout side-effects in the service layer.

## Artifact response pattern

Heavy operations write results to artifact files instead of returning inline:

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

The `ServiceContext` is constructed at startup and shared across all handlers:

```rust
pub struct ServiceContext {
    pub config: Config,
    pub capabilities: ServiceCapabilities,
    pub pg_pool: Option<PgPool>,
    pub redis: Option<RedisConnection>,
    pub amqp: Option<AmqpConnection>,
    // ...
}
```

`ServiceCapabilities` gates operations based on runtime mode:

```rust
pub struct ServiceCapabilities {
    pub jobs: CapabilityGate,      // SQLite-backed (lite mode, always available)
    pub graph: CapabilityGate,     // Requires Neo4j
    pub search: CapabilityGate,    // Requires Tavily API key
    // ...
}
```

MCP handlers check capabilities before executing:

```rust
if !ctx.capabilities.jobs.supported {
    return Err(McpError::new("Operation not available in this configuration"));
}
```

## Lifecycle job pattern

Lifecycle actions (crawl, extract, embed, ingest, refresh) share a common pattern:

1. `start` -- enqueue job to AMQP queue, return job ID
2. `status` -- query Postgres for job state
3. `cancel` -- set cancel flag in Redis
4. `list` -- list recent jobs from Postgres
5. `cleanup` -- remove completed/failed jobs
6. `clear` -- remove all jobs
7. `recover` -- reclaim stale/interrupted jobs

Each job type has:
- A `Processor` trait implementation in `crates/jobs/<type>/`
- A queue name in `axon.json` (e.g., `axon.crawl.jobs`)
- A database table (e.g., `axon_crawl_jobs`)
- A worker binary path (e.g., `axon crawl worker`)

## Hybrid search pattern

Vector search uses Reciprocal Rank Fusion (RRF) with two prefetch arms:

```
Query
  ├── Dense vector (TEI embedding) → HNSW search
  ├── BM42 sparse vector (keyword) → Sparse index search
  └── RRF fusion → merged, re-ranked results
```

Named-mode collections (new) support hybrid search. Legacy unnamed-mode collections fall back to dense-only. The `VectorMode` is cached per-process -- restart workers after collection migration.

## ACP completion pattern

Operations requiring LLM synthesis (`ask`, `evaluate`, `suggest`, `research`, `extract` fallback, `debug`) use the Agent Client Protocol (ACP):

```
1. Service function prepares prompt + context
2. ACP adapter spawned as subprocess (configured via AXON_ACP_ADAPTER_CMD)
3. Adapter communicates with LLM provider
4. Response streamed back through ACP protocol
5. Service function parses and returns typed result
```

Pre-warming (`AXON_ACP_PREWARM=true`) eliminates cold-start latency by spawning the adapter at server startup.

## See also

- [TOOLS.md](TOOLS.md) -- action/subaction reference
- [DEV.md](DEV.md) -- development workflow
- [../stack/ARCH.md](../stack/ARCH.md) -- architecture overview
