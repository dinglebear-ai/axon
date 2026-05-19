# API Implementation Plan for Axon Rust

## Objective
Transform the `axon_rust` CLI tool into a dual-purpose application by adding a REST API. This API will expose the existing RAG and scraping pipelines (crawl, batch, extract, embed, query, ask) over HTTP, allowing external services to trigger jobs and retrieve results.

## Architecture
*   **Web Framework:** `axum` (integrates seamlessly with the existing `tokio` runtime).
*   **Middleware:** `tower-http` for CORS and request tracing.
*   **State Management:** The existing `crates::core::config::Config` struct will be wrapped in an `Arc` and used as the Axum `AppState`. This allows the API to inherit all environment variables, defaults, and database connections already configured for the CLI.
*   **Error Handling:** A custom `AppError` wrapper around `Box<dyn std::error::Error>` that implements Axum's `IntoResponse` to return standardized JSON error payloads (e.g., HTTP 400/500).

## Implementation Steps

### Phase 1: Dependencies & CLI Wiring
1.  **Update `Cargo.toml`:**
    *   Add `axum = "0.7"`
    *   Add `tower-http = { version = "0.5", features = ["cors", "trace"] }`
2.  **Update CLI Parser (`crates/core/config/cli.rs` & `types.rs`):**
    *   Add `Serve(ServeArgs)` to `CliCommand`.
    *   Add `Serve` to `CommandKind`.
    *   Create `ServeArgs` struct with a `--port` flag (default: `8080`).
3.  **Create Command Entrypoint (`crates/cli/commands/serve.rs`):**
    *   Implement `pub async fn run_serve(cfg: &Config) -> Result<(), Box<dyn Error>>`.
    *   Wire it into the main `run()` dispatch in `crates/cli/mod.rs` (or wherever the main dispatch lives).

### Phase 2: API Core Infrastructure
1.  **Create Module Structure:**
    *   `crates/api/mod.rs`
    *   `crates/api/state.rs`
    *   `crates/api/error.rs`
    *   `crates/api/server.rs`
    *   `crates/api/routes/mod.rs`
2.  **Implement State (`crates/api/state.rs`):**
    *   Define `AppState { pub base_config: Arc<Config> }`.
3.  **Implement Error Handling (`crates/api/error.rs`):**
    *   Define `AppError` and implement `IntoResponse` to map internal errors to HTTP status codes and JSON responses.

### Phase 3: MVP Routes (Crawl Jobs V2)
1.  **Create `crates/api/routes/crawl.rs`:**
    *   Define `StartCrawlRequest` (Deserialize) to allow overriding `Config` fields (e.g., `url`, `max_pages`, `max_depth`).
2.  **Implement Handlers:**
    *   `POST /api/v1/crawl`: Clones `AppState` config, applies overrides, calls `start_crawl_job()`, returns Job ID.
    *   `GET /api/v1/crawl/:id`: Calls `get_job()`, returns job status/result.
    *   `GET /api/v1/crawl`: Calls `list_jobs()`.
    *   `DELETE /api/v1/crawl/:id`: Calls `cancel_job()`.

### Phase 4: Server Initialization
1.  **Implement `crates/api/server.rs`:**
    *   Build the Axum `Router`.
    *   Attach routes, `CorsLayer`, and `.with_state(AppState)`.
    *   Bind to `0.0.0.0:<port>` and start `axum::serve`.

### Phase 5: Expansion & Polish (Post-MVP)
1.  **Additional Job Routes:** Implement `/api/v1/batch`, `/api/v1/extract`, and `/api/v1/embed` following the crawl pattern.
2.  **Synchronous Routes:** Implement `/api/v1/query`, `/api/v1/ask`, and `/api/v1/search` for immediate RAG interactions.
3.  **OpenAPI/Swagger:** Integrate `utoipa` to auto-generate documentation at `/api/docs`.
4.  **Docker Integration:** Update `docker-compose.yaml` to expose the API port and potentially add a dedicated `axon-api` service.

## Monolith Policy Compliance
*   Keep all new files under 500 lines.
*   Keep functions under 80-120 lines.
*   Ensure clear separation of concerns: API handlers only parse requests and format responses; business logic remains in `crates/jobs` and `crates/vector`.