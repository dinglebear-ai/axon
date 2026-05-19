# RMCP Research Guide for Building the Axon MCP Server
**Author:** Codex  
**Date:** 2026-02-25  
**Scope:** Deep technical research of `../rmcp` (official Rust MCP SDK) and direct mapping to `axon_rust` for a production-ready MCP server interface to crawler + RAG pipeline.

## Table of Contents
1. Executive Summary
2. Goals and Non-Goals
3. What RMCP Provides
4. RMCP Server Architecture
5. Transport Options and Tradeoffs
6. Capability Surface (Tools, Prompts, Resources, Completion, Tasks, Sampling)
7. RMCP Version and Compatibility Findings
8. Axon Codebase Mapping
9. Proposed Axon MCP API Surface
10. Recommended Server Architecture in `axon_rust`
11. Error Handling, Security, and Operational Requirements
12. Testing and Verification Plan
13. Incremental Delivery Plan
14. Open Questions and Decisions Needed
15. Proposed Project Structure
16. RMCP Code Snippets (Official Patterns)
17. Axon Integration Matrix
18. Concrete Implementation Blueprint
19. Deployment and Client Configuration Examples
20. Source Index

## 1) Executive Summary
This report establishes a complete path for implementing an MCP server inside `axon_rust` using `rmcp` as the SDK layer.

Key result:
- `rmcp` already supports the full capabilities needed for Axon’s interface: tools, prompts, resources, completion, task lifecycle, and streamable HTTP/stdio transport.
- `axon_rust` already exposes most of the business operations as reusable Rust functions. We should integrate those functions directly into an MCP server binary instead of shelling out to CLI commands.
- The most pragmatic v1 is a `stdio` MCP server with task-aware tools for long-running operations (`crawl`, `extract`, `embed`, `ingest`) and synchronous tools for short operations (`query`, `retrieve`, `sources`, `stats`, `doctor`).

This is the way.

## 2) Goals and Non-Goals
### Goals
- Build a native MCP interface to Axon’s crawler/RAG system.
- Reuse existing Axon command and jobs modules.
- Support both synchronous and asynchronous workflows safely.
- Provide a stable schema-driven tool contract for external MCP clients.

### Non-Goals (initial phase)
- Re-implementing Axon business logic under MCP.
- Replacing existing CLI.
- Building full OAuth-protected streamable HTTP transport in v1.

## 3) What RMCP Provides
`rmcp` is the official Rust MCP SDK and includes:
- A service runtime handling protocol handshake and message loop.
- Traits and macros for server/client handlers.
- Tool/prompt routers with schema generation support.
- Multiple transports including stdio and streamable HTTP.
- Task support aligned with MCP tasks spec.
- Sampling support for server-to-client model requests.

Primary docs and examples:
- [README](/home/jmagar/workspace/rmcp/README.md)
- [Core crate README](/home/jmagar/workspace/rmcp/crates/rmcp/README.md)
- [Server examples index](/home/jmagar/workspace/rmcp/examples/servers/README.md)

## 4) RMCP Server Architecture
### Core flow
1. Implement `ServerHandler` behavior.
2. Define capabilities in `get_info()`.
3. Start service with `.serve(transport).await`.
4. Keep process alive with `.waiting().await`.

The initialization sequence is explicit and strict in runtime internals:
- Client sends `initialize` request.
- Server responds with negotiated protocol version + capabilities.
- Client sends `initialized` notification.
- Normal request/notification processing begins.

References:
- [Server handler dispatch](/home/jmagar/workspace/rmcp/crates/rmcp/src/handler/server.rs)
- [Server initialization internals](/home/jmagar/workspace/rmcp/crates/rmcp/src/service/server.rs)
- [Service runtime loop](/home/jmagar/workspace/rmcp/crates/rmcp/src/service.rs)

### Handler defaults
`ServerHandler` provides defaults for most methods (method-not-found or empty result).  
This means we can incrementally implement only what we expose in capabilities.

## 5) Transport Options and Tradeoffs
### `stdio` (recommended v1)
Pros:
- Easiest integration for local MCP clients.
- Lower complexity and operational burden.
- Aligns with RMCP examples and many current MCP integrations.

Cons:
- No direct remote HTTP access.

Reference:
- [stdio helper](/home/jmagar/workspace/rmcp/crates/rmcp/src/transport/io.rs)

### Streamable HTTP (recommended v2)
Pros:
- Multi-client and remote-friendly.
- Session-aware with `Mcp-Session-Id`.
- Good path for future auth and hosted use.

Cons:
- More deployment + auth complexity.

Default config findings:
- `stateful_mode: true`
- `sse_keep_alive: 15s`
- `sse_retry: 3s`

Reference:
- [Streamable HTTP service + config](/home/jmagar/workspace/rmcp/crates/rmcp/src/transport/streamable_http_server/tower.rs)
- [Streamable HTTP example](/home/jmagar/workspace/rmcp/examples/servers/src/counter_streamhttp.rs)

## 6) Capability Surface
### Tools
Defined by `Tool` model with:
- `name`, `description`, `input_schema`
- optional `output_schema`
- optional annotations (`read_only_hint`, `destructive_hint`, etc.)
- optional execution metadata (`task_support`)

References:
- [Tool model](/home/jmagar/workspace/rmcp/crates/rmcp/src/model/tool.rs)
- [Tool router](/home/jmagar/workspace/rmcp/crates/rmcp/src/handler/server/router/tool.rs)
- [Tool name validation constraints](/home/jmagar/workspace/rmcp/crates/rmcp/src/handler/server/tool_name_validation.rs)

### Prompts
First-class prompt support via prompt router/macros and typed arguments.

References:
- [Prompt router](/home/jmagar/workspace/rmcp/crates/rmcp/src/handler/server/router/prompt.rs)
- [Prompt example](/home/jmagar/workspace/rmcp/examples/servers/src/prompt_stdio.rs)

### Resources
Available through `list_resources`, `list_resource_templates`, `read_resource`.
Useful for surfacing Axon state snapshots (`stats`, `domains`, `sources`, job metadata).

Reference:
- [Resource methods on ServerHandler](/home/jmagar/workspace/rmcp/crates/rmcp/src/handler/server.rs)

### Completion
`complete(...)` supports contextual suggestions, ideal for:
- operation enum values
- source type choices
- known domains/collections

Reference:
- [Completion example](/home/jmagar/workspace/rmcp/examples/servers/src/completion_stdio.rs)

### Tasks
RMCP supports task lifecycle and enforces per-tool task rules:
- `Forbidden` (default), `Optional`, `Required`
- validation happens in server dispatch path

References:
- [Task support enforcement](/home/jmagar/workspace/rmcp/crates/rmcp/src/handler/server.rs)
- [Task models and lifecycle](/home/jmagar/workspace/rmcp/crates/rmcp/src/model/tool.rs)
- [Task manager utility](/home/jmagar/workspace/rmcp/crates/rmcp/src/task_manager.rs)

### Sampling
Server can request model generation from capable clients via peer API (`create_message`).
Likely optional for Axon v1.

Reference:
- [Sampling server example](/home/jmagar/workspace/rmcp/examples/servers/src/sampling_stdio.rs)

## 7) RMCP Version and Compatibility Findings
Critical findings:
- Workspace declares `0.16.0`.
- Top-level README examples still show older version strings in snippets.
- Streamable HTTP is the path; old standalone SSE transport was removed.
- 0.15+ and 0.16 include important task and compatibility fixes.

References:
- [Workspace Cargo](/home/jmagar/workspace/rmcp/Cargo.toml)
- [rmcp crate Cargo](/home/jmagar/workspace/rmcp/crates/rmcp/Cargo.toml)
- [Changelog](/home/jmagar/workspace/rmcp/crates/rmcp/CHANGELOG.md)

## 8) Axon Codebase Mapping
Axon is already structured for direct MCP wrapping.

### Existing command dispatch
- Single entrypoint dispatch in [lib.rs](/home/jmagar/workspace/axon_rust/lib.rs)

### Shared runtime config
- Central `Config` model in [types.rs](/home/jmagar/workspace/axon_rust/crates/core/config/types.rs)
- Parsing + env resolution in [parse.rs](/home/jmagar/workspace/axon_rust/crates/core/config/parse.rs)

### Job system APIs (directly reusable for MCP tools)
- Crawl API: [crawl.rs](/home/jmagar/workspace/axon_rust/crates/jobs/crawl.rs)
- Extract API: [extract.rs](/home/jmagar/workspace/axon_rust/crates/jobs/extract.rs)
- Embed API: [embed.rs](/home/jmagar/workspace/axon_rust/crates/jobs/embed.rs)
- Ingest API: [ingest.rs](/home/jmagar/workspace/axon_rust/crates/jobs/ingest.rs)

### RAG/query interfaces
- Query: [query.rs](/home/jmagar/workspace/axon_rust/crates/vector/ops/commands/query.rs)
- Ask: [ask.rs](/home/jmagar/workspace/axon_rust/crates/vector/ops/commands/ask.rs)
- Retrieve/sources/domains/dedupe: [qdrant commands](/home/jmagar/workspace/axon_rust/crates/vector/ops/qdrant/commands.rs)
- Stats and status output:
  - [status command](/home/jmagar/workspace/axon_rust/crates/cli/commands/status.rs)

No existing MCP integration was found in `axon_rust`.

## 9) Proposed Axon MCP API Surface
### Tool set (v1 recommended): single tool `axon`
Expose exactly one MCP tool:
- `axon`

Use domain routing via parameters:
- `action`: top-level domain (`crawl|extract|embed|ingest|rag|discovery|ops`)
- `subaction`: operation within that domain (`start|status|cancel|list|query|ask|retrieve|search|research|map|scrape|stats|doctor|status|sources|domains`)

Why this shape:
- Matches your preferred consolidated action pattern.
- Minimizes tool-list footprint in MCP clients.
- Keeps future expansion cheap (new subactions, same tool contract).

Recommended canonical envelope:
- Required: `action`, `subaction`
- Optional: domain-specific fields (`url`, `urls`, `id`, `query`, `limit`, `collection`, `source_type`, `target`, etc.)

Example request payloads:

```json
{ "action": "crawl", "subaction": "start", "url": "https://example.com", "wait": false }
```

```json
{ "action": "crawl", "subaction": "status", "id": "<job-uuid>" }
```

```json
{ "action": "rag", "subaction": "query", "query": "what changed in parser", "limit": 10 }
```

### Resource set (v1 optional, v1.1 recommended)
- `axon://stats`
- `axon://domains`
- `axon://sources`
- `axon://job/{kind}/{id}`

### Prompt set (v1 optional)
- `axon-debug-triage` (structured debugging flow over status/error data)
- `axon-query-refinement` (user intent to query/search parameterization)

### Completion targets
- `action` values (`crawl|extract|embed|ingest|rag|discovery|ops`)
- `subaction` values based on selected `action`
- ingest `source_type` (`github|reddit|youtube|sessions`)
- known collections/domains where relevant

## 10) Recommended Server Architecture in `axon_rust`
### New binary
Add new binary target:
- `[[bin]] name = "axon-mcp"` with entrypoint e.g. `apps/mcp/main.rs` or `crates/mcp/main.rs`

### Internal structure
- `mcp/server.rs`: server state + `ServerHandler` impl
- `mcp/tools/*.rs`: tool adapters calling existing Axon functions
- `mcp/resources.rs`: resource endpoints
- `mcp/prompts.rs`: optional prompt router
- `mcp/errors.rs`: consistent mapping to MCP `ErrorData`

### Config strategy
- Parse `Config` once at startup from env/CLI.
- Keep in `Arc<Config>` for tool handlers.
- For per-request overrides, use explicit tool parameters and clone+override config safely.

### Task strategy
- Mark long operations as `task_support: Optional` initially.
- For heavy operations, consider `Required` once task endpoints are fully wired.
- Map Axon job UUIDs to MCP task IDs where possible for transparent lifecycle.

### Transport rollout
1. v1: stdio only.
2. v2: streamable HTTP with session manager.
3. v3: auth/OAuth flow if needed.

## 11) Error Handling, Security, and Operational Requirements
### Error mapping
- Validation errors -> `invalid_params`
- Not found -> `resource_not_found` or method-not-found where applicable
- Runtime failures -> `internal_error` with non-secret context

### Secret handling
- Never expose raw values for `OPENAI_API_KEY`, DB URLs, tokens in tool results.
- Reuse existing redaction patterns from Axon config debug behavior.

### Safety hints
Use tool annotations:
- `read_only_hint = true` for query/read/status tools
- `destructive_hint = true` for clear/cancel/cleanup operations
- require explicit confirmation flags for destructive operations

### SSRF and network safety
- Keep existing URL validation (`validate_url`) in any URL-taking tool path.
- Respect existing crawler constraints and defaults.

## 12) Testing and Verification Plan
### Unit tests
- Tool argument schema/validation
- Error mapping
- Task support mode behavior (`Forbidden/Optional/Required`)

### Integration tests
- Start `axon-mcp` over stdio and run initialize/list_tools/call_tool flows
- Verify long-running tool returns task metadata or blocks correctly per configuration
- Verify job status transitions are reflected correctly

### Interop tests
- MCP Inspector against stdio
- Existing local clients that consume MCP

### Regression coverage
- Use deterministic tool listing/order checks (rmcp now deterministic)
- Verify JSON outputs remain stable for machine clients

## 13) Incremental Delivery Plan
### Phase 1 (minimal shippable)
- stdio server
- capabilities: tools only
- tools: `query`, `ask`, `crawl.start`, `crawl.status`, `crawl.cancel`
- baseline tests + inspector validation

### Phase 2
- full job-family tools (`extract`, `embed`, `ingest`)
- `stats`, `status`, `doctor`, `retrieve`, `sources`, `domains`
- initial resources

### Phase 3
- completions + prompts
- stronger task lifecycle integration

### Phase 4
- streamable HTTP transport
- auth if required

## 14) Open Questions and Decisions Needed
1. Should v1 support only stdio, or also streamable HTTP from day one?
2. Do we require all mutating tools to run as MCP tasks, or allow sync mode?
3. How much config override should clients have per tool call versus fixed startup config?
4. Do we want one MCP server exposing all commands, or split read-only and mutating servers?
5. Should job UUID be canonical MCP task ID, or do we maintain a translation layer?

## 15) Proposed Project Structure
Recommended first-class MCP module layout inside `axon_rust`:

```text
axon_rust/
├── Cargo.toml
├── lib.rs
├── main.rs
├── crates/
│   ├── mcp/
│   │   ├── mod.rs
│   │   ├── main.rs                    # axon-mcp binary entrypoint
│   │   ├── server.rs                  # AxonMcpServer + ServerHandler impl
│   │   ├── tools/
│   │   │   ├── mod.rs
│   │   │   ├── crawl.rs               # crawl start/status/cancel/list
│   │   │   ├── extract.rs             # extract start/status/cancel/list
│   │   │   ├── embed.rs               # embed start/status/cancel/list
│   │   │   ├── ingest.rs              # ingest start/status/cancel/list
│   │   │   ├── rag.rs                 # query/ask/retrieve
│   │   │   ├── discovery.rs           # search/research/map/scrape
│   │   │   └── ops.rs                 # stats/status/doctor/sources/domains
│   │   ├── resources.rs               # read-only MCP resources
│   │   ├── completions.rs             # optional complete() implementation
│   │   ├── tasks.rs                   # task lifecycle adapters
│   │   ├── params.rs                  # serde/schemars input structs
│   │   ├── output.rs                  # normalized tool output envelopes
│   │   └── error.rs                   # domain error -> rmcp::ErrorData mapping
│   ├── core/
│   ├── cli/
│   ├── jobs/
│   └── vector/
└── tests/
    ├── mcp_stdio_smoke.rs
    ├── mcp_tasks.rs
    ├── mcp_tools_readonly.rs
    └── mcp_tools_mutating.rs
```

Why this shape:
- Keeps MCP-specific concerns isolated from CLI command formatting.
- Reuses existing domain logic from `crates/jobs/*` and `crates/vector/ops/*`.
- Gives a clear seam for versioned MCP API contracts.

## 16) RMCP Code Snippets (Official Patterns)
This section captures patterns directly aligned with official SDK examples and internals.

### 16.1 Cargo dependency shape
`axon_rust` can add `rmcp` with explicit features for v1 stdio server:

```toml
[dependencies]
rmcp = { version = "0.16.0", features = ["server", "macros", "transport-io", "schemars"] }
schemars = "1.0"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["full"] }
```

For streamable HTTP (v2+), add:

```toml
rmcp = { version = "0.16.0", features = [
  "server",
  "macros",
  "transport-streamable-http-server",
  "schemars"
] }
```

### 16.2 Minimal stdio server skeleton
Pattern from official examples (`calculator_stdio`, `sampling_stdio`) adapted for Axon:

```rust
use rmcp::{ServiceExt, transport::stdio};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cfg = crate::crates::core::config::parse_args();
    let server = AxonMcpServer::new(cfg);
    let service = server.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
```

### 16.3 Tool router with typed params
Pattern from `#[tool_router]` + `#[tool_handler]`:

```rust
use rmcp::{
    ServerHandler, tool, tool_handler, tool_router,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{ServerCapabilities, ServerInfo, CallToolResult, Content},
    ErrorData as McpError,
};

#[derive(Clone)]
pub struct AxonMcpServer {
    tool_router: ToolRouter<Self>,
    state: std::sync::Arc<AppState>,
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct CrawlStartInput {
    pub url: String,
    pub wait: Option<bool>,
}

#[tool_router]
impl AxonMcpServer {
    pub fn new(cfg: crate::crates::core::config::Config) -> Self {
        Self {
            tool_router: Self::tool_router(),
            state: std::sync::Arc::new(AppState::new(cfg)),
        }
    }

    #[tool(name = "axon.crawl.start", description = "Start a crawl job")]
    async fn crawl_start(
        &self,
        Parameters(input): Parameters<CrawlStartInput>,
    ) -> Result<CallToolResult, McpError> {
        let mut cfg = self.state.base_cfg.clone();
        if let Some(wait) = input.wait {
            cfg.wait = wait;
        }
        let job_id = crate::crates::jobs::crawl::start_crawl_job(&cfg, &input.url)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(job_id.to_string())]))
    }
}

#[tool_handler]
impl ServerHandler for AxonMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some("Axon crawler + RAG MCP server".to_string()),
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .build(),
            ..Default::default()
        }
    }
}
```

### 16.4 Structured output tool pattern
From official `structured_output.rs`: return `Json<T>` for machine-safe client consumption.

```rust
use rmcp::Json;

#[derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct JobStarted {
    pub job_id: String,
    pub status: String,
    pub queued: bool,
}

#[tool(name = "axon.crawl.start_json", description = "Start crawl and return structured output")]
async fn crawl_start_json(
    &self,
    Parameters(input): Parameters<CrawlStartInput>,
) -> Result<Json<JobStarted>, McpError> {
    let cfg = self.state.base_cfg.clone();
    let id = crate::crates::jobs::crawl::start_crawl_job(&cfg, &input.url)
        .await
        .map_err(|e| McpError::internal_error(e.to_string(), None))?;
    Ok(Json(JobStarted {
        job_id: id.to_string(),
        status: "pending".to_string(),
        queued: true,
    }))
}
```

### 16.5 Task support declaration pattern
RMCP validates task invocation against tool `execution.taskSupport`.  
Use this for long-running operations:

```rust
use rmcp::model::{ToolExecution, TaskSupport};

// conceptual pattern when building Tool definitions manually:
let tool = rmcp::model::Tool::new("axon.crawl.start", "Start crawl", input_schema)
    .with_execution(
        ToolExecution::new().with_task_support(TaskSupport::Optional)
    );
```

Guidance:
- `Optional` for `crawl.start`, `extract.start`, `embed.start`, `ingest.start`.
- `Forbidden` for read-only quick calls (`sources`, `domains`, `stats`, `retrieve`).

### 16.6 Progress notification pattern
From official progress demo:

```rust
use rmcp::model::{ProgressNotificationParam, ProgressToken, NumberOrString};
use rmcp::service::RequestContext;
use rmcp::RoleServer;

async fn notify_progress(
    ctx: &RequestContext<RoleServer>,
    current: u64,
    message: &str,
) -> Result<(), rmcp::ErrorData> {
    ctx.peer
        .notify_progress(ProgressNotificationParam {
            progress_token: ProgressToken(NumberOrString::Number(current as i64)),
            progress: current as f64,
            total: None,
            message: Some(message.to_string()),
        })
        .await
        .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))
}
```

Use for:
- batch enqueue loops
- long `ask` context assembly
- long `retrieve` full-doc builds

### 16.7 Streamable HTTP bootstrap pattern
Pattern from official streamable HTTP servers:

```rust
use rmcp::transport::streamable_http_server::{
    StreamableHttpService,
    session::local::LocalSessionManager,
};
use rmcp::transport::StreamableHttpServerConfig;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let ct = tokio_util::sync::CancellationToken::new();
    let service = StreamableHttpService::new(
        || Ok(AxonMcpServer::new(load_cfg())),
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig {
            cancellation_token: ct.child_token(),
            ..Default::default()
        },
    );
    let router = axum::Router::new().nest_service("/mcp", service);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:8000").await?;
    axum::serve(listener, router).await?;
    Ok(())
}
```

## 17) Axon Integration Matrix
This matrix maps single-tool `axon` (`action` + `subaction`) to concrete Axon integration points.

| action | subaction | Axon Function(s) | Kind | Task Support | Notes |
|---|---|---|---|---|---|
| `crawl` | `start` | `jobs::crawl::start_crawl_job`, `start_crawl_jobs_batch` | mutating | optional | accepts URL(s), returns job UUID(s) |
| `crawl` | `status` | `jobs::crawl::get_job` | read | forbidden | return full job object |
| `crawl` | `cancel` | `jobs::crawl::cancel_job` | mutating | optional | idempotent cancel boolean |
| `crawl` | `list` | `jobs::crawl::list_jobs` | read | forbidden | limit arg |
| `extract` | `start` | `jobs::extract::start_extract_job` | mutating | optional | URLs + prompt |
| `extract` | `status` | `jobs::extract::get_extract_job` | read | forbidden | |
| `extract` | `cancel` | `jobs::extract::cancel_extract_job` | mutating | optional | |
| `extract` | `list` | `jobs::extract::list_extract_jobs` | read | forbidden | |
| `embed` | `start` | `jobs::embed::start_embed_job` | mutating | optional | input path/url/text |
| `embed` | `status` | `jobs::embed::get_embed_job` | read | forbidden | |
| `embed` | `cancel` | `jobs::embed::cancel_embed_job` | mutating | optional | |
| `embed` | `list` | `jobs::embed::list_embed_jobs` | read | forbidden | |
| `ingest` | `start` | `jobs::ingest::start_ingest_job` | mutating | optional | source + target |
| `ingest` | `status` | `jobs::ingest::get_ingest_job` | read | forbidden | |
| `ingest` | `cancel` | `jobs::ingest::cancel_ingest_job` | mutating | optional | |
| `ingest` | `list` | `jobs::ingest::list_ingest_jobs` | read | forbidden | |
| `rag` | `query` | `vector::ops::run_query_native` or extracted core helper | read | forbidden | favor helper to avoid stdout coupling |
| `rag` | `ask` | `vector::ops::run_ask_native` or extracted helper | read | optional | can be long-running |
| `rag` | `retrieve` | `vector::ops::qdrant::commands::run_retrieve_native` or extracted helper | read | forbidden | |
| `ops` | `sources` | `vector::ops::qdrant::commands::run_sources_native` or helper | read | forbidden | |
| `ops` | `domains` | `vector::ops::qdrant::commands::run_domains_native` or helper | read | forbidden | |
| `ops` | `stats` | `vector::ops::run_stats_native` | read | forbidden | |
| `ops` | `status` | `cli::commands::run_status` or helper | read | forbidden | convert to structured payload |
| `ops` | `doctor` | `cli::commands::run_doctor` | read | forbidden | convert to structured payload |
| `discovery` | `search` | existing search command helper | read | forbidden | Tavily-backed |
| `discovery` | `research` | existing research command helper | read | forbidden | Tavily AI search |
| `discovery` | `map` | existing map command/helper | read | forbidden | URL discovery only |
| `discovery` | `scrape` | existing scrape command/helper | mutating | optional | can embed depending on params |

Integration recommendation:
- Keep one MCP tool (`axon`) and route internally by enum-based `action/subaction`.
- Where command functions only print to stdout, extract reusable helper functions returning structs, then keep CLI command as presentation layer.
- MCP server should consume helper structs and render `CallToolResult` / `Json<T>`.

## 18) Concrete Implementation Blueprint
### 18.1 Add binary target
In [Cargo.toml](/home/jmagar/workspace/axon_rust/Cargo.toml):

```toml
[[bin]]
name = "axon-mcp"
path = "crates/mcp/main.rs"
```

### 18.2 Shared app state
```rust
#[derive(Clone)]
pub struct AppState {
    pub base_cfg: crate::crates::core::config::Config,
}

impl AppState {
    pub fn new(base_cfg: crate::crates::core::config::Config) -> Self {
        Self { base_cfg }
    }
}
```

### 18.3 MCP-safe response envelope
Use a uniform envelope for mutating calls:

```rust
#[derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct MutationAck {
    pub ok: bool,
    pub id: Option<String>,
    pub status: String,
    pub message: String,
}
```

### 18.4 Error mapper
```rust
pub fn to_mcp_error<E: std::fmt::Display>(e: E) -> rmcp::ErrorData {
    rmcp::ErrorData::internal_error(e.to_string(), None)
}

pub fn invalid_params(message: impl Into<String>) -> rmcp::ErrorData {
    rmcp::ErrorData::invalid_params(message.into(), None)
}
```

### 18.5 Capability declaration for v1
```rust
fn get_info(&self) -> rmcp::model::ServerInfo {
    rmcp::model::ServerInfo {
        instructions: Some("Axon MCP server: crawler + RAG operations".to_string()),
        capabilities: rmcp::model::ServerCapabilities::builder()
            .enable_tools()
            .build(),
        ..Default::default()
    }
}
```

### 18.6 Initial tool contract suggestions (single tool `axon`)
```rust
#[derive(Debug, Clone, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AxonAction {
    Crawl,
    Extract,
    Embed,
    Ingest,
    Rag,
    Discovery,
    Ops,
}

#[derive(Debug, Clone, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AxonSubaction {
    Start,
    Status,
    Cancel,
    List,
    Query,
    Ask,
    Retrieve,
    Search,
    Research,
    Map,
    Scrape,
    Sources,
    Domains,
    Stats,
    Doctor,
}

#[derive(Debug, Clone, serde::Deserialize, schemars::JsonSchema)]
pub struct AxonInput {
    pub action: AxonAction,
    pub subaction: AxonSubaction,

    // shared optional fields, validated per route
    pub id: Option<String>,
    pub url: Option<String>,
    pub urls: Option<Vec<String>>,
    pub query: Option<String>,
    pub limit: Option<usize>,
    pub collection: Option<String>,
    pub source_type: Option<String>,
    pub target: Option<String>,
    pub wait: Option<bool>,
}
```

### 18.7 Request-to-config adaptation
Pattern for per-call overrides without mutating global config:

```rust
fn with_overrides(
    base: &crate::crates::core::config::Config,
    collection: Option<String>,
    limit: Option<usize>,
) -> crate::crates::core::config::Config {
    let mut cfg = base.clone();
    if let Some(c) = collection {
        cfg.collection = c;
    }
    if let Some(l) = limit {
        cfg.search_limit = l;
    }
    cfg
}
```

### 18.8 Task-aware enqueue outline
If task support is enabled for long-running tools:
- On `call_tool` with task metadata, enqueue the job and return task creation payload tied to job UUID.
- `tasks/get` and `tasks/result` should read from Axon job tables and map status to MCP task status.

Mapping example:
- Axon `pending|running` -> MCP `working`
- Axon `completed` -> MCP `completed`
- Axon `failed|canceled` -> MCP `failed|canceled`

## 19) Deployment and Client Configuration Examples
### 19.1 Recommended deployment: run `axon-mcp` inside `axon-workers`
For this stack, the MCP server should run in the existing workers container as part of the same runtime boundary.  
No dedicated seventh container. No per-client env duplication.

Rationale:
- `axon-workers` already has the correct service wiring and lifecycle.
- `axon-mcp` should inherit the same container environment used by jobs/CLI.
- Avoids configuration drift between `.env`, Compose, and MCP client config.

### 19.2 Local stdio MCP client config (no env block)
Example `mcp.json` entry:

```json
{
  "servers": {
    "axon": {
      "command": "/usr/local/bin/axon-mcp",
      "args": []
    }
  }
}
```

Notes:
- The MCP client must execute inside the container context (or through `docker exec`) where `axon-mcp` is available.
- Environment is inherited from the container process; do not duplicate `AXON_*`, `QDRANT_URL`, `TEI_URL`, `OPENAI_*` in client config.

### 19.3 Streamable HTTP deployment shape
- Prefer hosting in the same workers runtime if enabled.
- Bind service on internal interface behind reverse proxy.
- Expose `/mcp`.
- Keep session mode enabled unless stateless operation is intentional.
- Add auth middleware before `nest_service("/mcp", ...)`.

### 19.4 Running `axon-mcp` inside `axon-workers` (supervised process model)
Use one container and one shared environment, with a lightweight supervisor launching both worker lanes and MCP stdio process.

`docker-compose.yaml` pattern:

```yaml
services:
  axon-workers:
    build:
      context: .
      dockerfile: docker/Dockerfile
    container_name: axon-workers
    env_file:
      - .env
    depends_on:
      - axon-postgres
      - axon-redis
      - axon-rabbitmq
      - axon-qdrant
    command: ["/usr/local/bin/axon-supervisor"]
```

Supervisor entrypoint pattern:

```bash
#!/usr/bin/env bash
set -euo pipefail

# Start worker lanes in background
/usr/local/bin/axon worker &
worker_pid=$!

# Start MCP stdio server in background
/usr/local/bin/axon-mcp &
mcp_pid=$!

_term() {
  kill -TERM "$worker_pid" "$mcp_pid" 2>/dev/null || true
  wait "$worker_pid" "$mcp_pid" || true
}
trap _term TERM INT

wait -n "$worker_pid" "$mcp_pid"
exit_code=$?
_term
exit "$exit_code"
```

Operational notes:
- Keep a single source of truth for env (`env_file: .env`).
- Do not duplicate service env in MCP client config.
- For local MCP clients, invoke through `docker exec` wrapper if needed.
- If you need healthchecks, check both worker queue processing and MCP process liveness.

### 19.5 Suggested rollout gates
1. Gate A: local stdio smoke + inspector checks.
2. Gate B: internal team use against real crawl jobs.
3. Gate C: streamable HTTP with auth and session observability.

## 20) Source Index
### RMCP core and docs
- [RMCP root README](/home/jmagar/workspace/rmcp/README.md)
- [RMCP crate README](/home/jmagar/workspace/rmcp/crates/rmcp/README.md)
- [RMCP Cargo (workspace)](/home/jmagar/workspace/rmcp/Cargo.toml)
- [RMCP crate Cargo](/home/jmagar/workspace/rmcp/crates/rmcp/Cargo.toml)
- [RMCP changelog](/home/jmagar/workspace/rmcp/crates/rmcp/CHANGELOG.md)
- [Server handler](/home/jmagar/workspace/rmcp/crates/rmcp/src/handler/server.rs)
- [Server runtime](/home/jmagar/workspace/rmcp/crates/rmcp/src/service/server.rs)
- [Transport: streamable HTTP](/home/jmagar/workspace/rmcp/crates/rmcp/src/transport/streamable_http_server/tower.rs)
- [Model capabilities](/home/jmagar/workspace/rmcp/crates/rmcp/src/model/capabilities.rs)
- [Model tool](/home/jmagar/workspace/rmcp/crates/rmcp/src/model/tool.rs)
- [Task manager](/home/jmagar/workspace/rmcp/crates/rmcp/src/task_manager.rs)
- [Tool router](/home/jmagar/workspace/rmcp/crates/rmcp/src/handler/server/router/tool.rs)
- [Prompt router](/home/jmagar/workspace/rmcp/crates/rmcp/src/handler/server/router/prompt.rs)

### RMCP examples used
- [Examples overview](/home/jmagar/workspace/rmcp/examples/servers/README.md)
- [Calculator stdio](/home/jmagar/workspace/rmcp/examples/servers/src/calculator_stdio.rs)
- [Counter streamable HTTP](/home/jmagar/workspace/rmcp/examples/servers/src/counter_streamhttp.rs)
- [Prompt stdio](/home/jmagar/workspace/rmcp/examples/servers/src/prompt_stdio.rs)
- [Completion stdio](/home/jmagar/workspace/rmcp/examples/servers/src/completion_stdio.rs)
- [Sampling stdio](/home/jmagar/workspace/rmcp/examples/servers/src/sampling_stdio.rs)
- [Structured output](/home/jmagar/workspace/rmcp/examples/servers/src/structured_output.rs)
- [Progress demo](/home/jmagar/workspace/rmcp/examples/servers/src/progress_demo.rs)
- [OAuth support doc](/home/jmagar/workspace/rmcp/docs/OAUTH_SUPPORT.md)

### Axon files mapped
- [Command dispatch](/home/jmagar/workspace/axon_rust/lib.rs)
- [Config public module](/home/jmagar/workspace/axon_rust/crates/core/config.rs)
- [Config parser](/home/jmagar/workspace/axon_rust/crates/core/config/parse.rs)
- [Config type model](/home/jmagar/workspace/axon_rust/crates/core/config/types.rs)
- [Scrape command](/home/jmagar/workspace/axon_rust/crates/cli/commands/scrape.rs)
- [Crawl command](/home/jmagar/workspace/axon_rust/crates/cli/commands/crawl.rs)
- [Status command](/home/jmagar/workspace/axon_rust/crates/cli/commands/status.rs)
- [Query command](/home/jmagar/workspace/axon_rust/crates/vector/ops/commands/query.rs)
- [Ask command](/home/jmagar/workspace/axon_rust/crates/vector/ops/commands/ask.rs)
- [Qdrant command ops](/home/jmagar/workspace/axon_rust/crates/vector/ops/qdrant/commands.rs)
- [Crawl jobs API](/home/jmagar/workspace/axon_rust/crates/jobs/crawl.rs)
- [Extract jobs API](/home/jmagar/workspace/axon_rust/crates/jobs/extract.rs)
- [Embed jobs API](/home/jmagar/workspace/axon_rust/crates/jobs/embed.rs)
- [Ingest jobs API](/home/jmagar/workspace/axon_rust/crates/jobs/ingest.rs)
