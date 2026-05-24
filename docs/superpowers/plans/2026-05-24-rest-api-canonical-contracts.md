# REST API Canonical Contracts Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Axon's REST request contracts canonical, truthful in OpenAPI, and reusable by CLI server mode and generated TypeScript clients without silent field drops.

**Architecture:** Add REST-specific canonical request DTOs in `src/services/client_contract.rs` that derive Serde and `utoipa::ToSchema`, but do not expose client-only route preference fields. Active REST handlers in `src/web/server/handlers/{rag,exploration,async_jobs}.rs`, CLI server-mode planning, and OpenAPI all use those DTOs. Security-sensitive embed path validation moves to one shared helper, and scrape+embed orchestration moves back into `services::scrape` so web handlers do not call CLI/vector internals directly.

**Tech Stack:** Rust, Axum, utoipa 5, serde, Qdrant/TEI embedding services, TypeScript, openapi-typescript, openapi-fetch.

---

## File Structure

- Modify `src/services/client_contract.rs`: define canonical `Rest*Request` structs and helper methods. Keep existing `Client*Request` route-planning types only if needed, but do not expose `route_preference` in REST/OpenAPI DTOs.
- Modify `src/services/client_contract_tests.rs`: cover serialization, aliases, absence of `route_preference`, and CLI body deserialization into canonical DTOs.
- Modify `src/services/scrape.rs`: add service-owned scrape+optional-embed orchestration and prepared-doc conversion.
- Modify `src/services/scrape_tests.rs`: cover prepared-doc conversion and embed-skip behavior without web/CLI dependencies.
- Modify `src/core/config/types/overrides.rs`: add missing override fields used by active REST parity, especially `max_sitemaps` and custom headers if needed by crawl/extract.
- Modify `src/web/server/handlers/rag.rs`: replace handler-local DTOs with canonical DTOs and apply supported overrides.
- Modify `src/web/server/handlers/exploration.rs`: replace handler-local DTOs with canonical DTOs and call service-owned scrape+embed orchestration.
- Modify `src/web/server/handlers/async_jobs.rs`: replace handler-local DTOs with canonical DTOs, apply per-family overrides, and use shared embed path validation.
- Modify `src/web/server/openapi.rs`: register canonical DTO schemas instead of handler-local request schemas.
- Modify `src/cli/server_mode/plan.rs` and `src/cli/server_mode/plan_ingest.rs`: serialize canonical DTOs instead of hand-built request JSON for touched endpoints.
- Modify `src/cli/server_mode_tests.rs`: prove server-mode bodies deserialize into canonical DTOs and preserve fields.
- Modify or add `src/web/server_tests.rs`: assert active-router behavior and OpenAPI schema fields for previously drifting fields.
- Modify `src/web/server/handlers/rest_tests.rs` only to quarantine/remove assertions that target inactive DTOs for contract parity.
- Modify `apps/palette-tauri/src/lib/axonClient.ts`: remove weak POST body escape hatch for touched routes and fix ingest body to `target`.
- Modify `apps/palette-tauri/package.json` and README if needed: make API generation use a local OpenAPI artifact in checks, not the deployed remote by default.

---

### Task 1: Canonical REST DTOs

**Files:**
- Modify: `src/services/client_contract.rs`
- Modify: `src/services/client_contract_tests.rs`

- [ ] **Step 1: Write failing tests for REST DTO serialization**

Add tests in `src/services/client_contract_tests.rs`:

```rust
use super::client_contract::{
    ClientRoutePreference, RestCrawlRequest, RestEmbedRequest, RestExtractRequest,
    RestIngestRequest, RestScrapeRequest, RestSearchRequest,
};
use crate::core::config::RenderMode;

#[test]
fn rest_crawl_request_does_not_serialize_route_preference() {
    let req = RestCrawlRequest {
        urls: vec!["https://example.com".to_string()],
        max_pages: Some(10),
        max_depth: Some(2),
        render_mode: Some(RenderMode::Http),
        include_subdomains: Some(false),
        respect_robots: Some(true),
        discover_sitemaps: Some(true),
        max_sitemaps: Some(32),
        sitemap_since_days: Some(7),
        delay_ms: Some(25),
        headers: vec![("x-test".to_string(), "1".to_string())],
        collection: Some("staging".to_string()),
        embed: Some(false),
    };

    let json = serde_json::to_value(&req).expect("serialize crawl request");
    assert_eq!(json["max_sitemaps"], 32);
    assert_eq!(json["collection"], "staging");
    assert_eq!(json["embed"], false);
    assert!(json.get("route_preference").is_none());
}

#[test]
fn rest_extract_accepts_extract_mode_alias() {
    let req: RestExtractRequest = serde_json::from_value(serde_json::json!({
        "urls": ["https://example.com/docs"],
        "prompt": "extract title",
        "extract_mode": "llm",
        "render_mode": "http",
        "embed": false
    }))
    .expect("deserialize extract request");

    assert_eq!(req.urls, vec!["https://example.com/docs"]);
    assert_eq!(req.mode, Some(super::client_contract::ClientExtractMode::Llm));
    assert_eq!(req.render_mode, Some(RenderMode::Http));
    assert_eq!(req.embed, Some(false));
}

#[test]
fn rest_search_accepts_search_time_range_alias() {
    let req: RestSearchRequest = serde_json::from_value(serde_json::json!({
        "query": "rust mcp",
        "search_time_range": "week",
        "limit": 5
    }))
    .expect("deserialize search request");

    assert_eq!(req.query, "rust mcp");
    assert_eq!(req.time_range.as_deref(), Some("week"));
    assert_eq!(req.limit, Some(5));
}

#[test]
fn rest_ingest_uses_source_type_and_target() {
    let req = RestIngestRequest {
        source_type: "github".to_string(),
        target: "MCPJam/inspector".to_string(),
        include_source: Some(true),
    };

    let json = serde_json::to_value(&req).expect("serialize ingest request");
    assert_eq!(json["source_type"], "github");
    assert_eq!(json["target"], "MCPJam/inspector");
    assert!(json.get("repo").is_none());
}
```

- [ ] **Step 2: Run tests and verify they fail**

Run:

```bash
cargo test rest_crawl_request_does_not_serialize_route_preference rest_extract_accepts_extract_mode_alias rest_search_accepts_search_time_range_alias rest_ingest_uses_source_type_and_target --lib
```

Expected: compile failure because `Rest*Request` types do not exist.

- [ ] **Step 3: Add canonical REST DTOs**

In `src/services/client_contract.rs`, add `utoipa::ToSchema` to imports and define REST request structs without `route_preference`:

```rust
use crate::core::config::{RenderMode, ScrapeFormat};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub struct RestCrawlRequest {
    pub urls: Vec<String>,
    pub max_pages: Option<u32>,
    pub max_depth: Option<usize>,
    pub render_mode: Option<RenderMode>,
    pub include_subdomains: Option<bool>,
    pub respect_robots: Option<bool>,
    pub discover_sitemaps: Option<bool>,
    pub max_sitemaps: Option<usize>,
    pub sitemap_since_days: Option<u32>,
    pub delay_ms: Option<u64>,
    #[serde(default)]
    pub headers: Vec<(String, String)>,
    pub collection: Option<String>,
    pub embed: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub struct RestScrapeRequest {
    pub url: Option<String>,
    pub urls: Option<Vec<String>>,
    pub render_mode: Option<RenderMode>,
    pub format: Option<ScrapeFormat>,
    pub embed: Option<bool>,
    pub root_selector: Option<String>,
    pub exclude_selector: Option<String>,
    #[serde(default)]
    pub headers: Vec<(String, String)>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub struct RestExtractRequest {
    pub urls: Vec<String>,
    pub prompt: Option<String>,
    #[serde(alias = "extract_mode")]
    pub mode: Option<ClientExtractMode>,
    pub max_pages: Option<u32>,
    pub render_mode: Option<RenderMode>,
    pub embed: Option<bool>,
    #[serde(default)]
    pub headers: Vec<(String, String)>,
    pub collection: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub struct RestEmbedRequest {
    pub input: String,
    pub source_type: Option<String>,
    pub collection: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub struct RestQueryRequest {
    pub query: String,
    pub collection: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub since: Option<String>,
    pub before: Option<String>,
    pub hybrid_search: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub struct RestRetrieveRequest {
    pub url: String,
    pub collection: Option<String>,
    pub max_points: Option<usize>,
    pub cursor: Option<String>,
    pub token_budget: Option<usize>,
    pub since: Option<String>,
    pub before: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub struct RestSearchRequest {
    pub query: String,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    #[serde(alias = "search_time_range")]
    pub time_range: Option<String>,
    pub wait: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub struct RestSuggestRequest {
    pub focus: Option<String>,
    pub limit: Option<usize>,
    pub collection: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub struct RestEvaluateRequest {
    pub question: String,
    pub collection: Option<String>,
    pub diagnostics: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub struct RestSummarizeRequest {
    pub url: Option<String>,
    pub urls: Option<Vec<String>>,
    pub render_mode: Option<RenderMode>,
    pub root_selector: Option<String>,
    pub exclude_selector: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub struct RestMapRequest {
    pub url: String,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub map_fallback: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub struct RestIngestRequest {
    pub source_type: String,
    #[serde(alias = "repo")]
    pub target: String,
    pub include_source: Option<bool>,
}
```

Also derive `ToSchema` for `ClientExtractMode`.

- [ ] **Step 4: Run tests and verify they pass**

Run:

```bash
cargo test rest_crawl_request_does_not_serialize_route_preference rest_extract_accepts_extract_mode_alias rest_search_accepts_search_time_range_alias rest_ingest_uses_source_type_and_target --lib
```

Expected: PASS.

---

### Task 2: Shared Embed Path Validation

**Files:**
- Modify: `src/mcp/server/common.rs`
- Modify: `src/web/server/handlers/async_jobs.rs`
- Modify: `src/mcp/server/common_tests.rs`
- Modify: `src/web/server_tests.rs`

- [ ] **Step 1: Write failing active REST symlink test**

Add a test in `src/web/server_tests.rs` that posts to active `/v1/embed` through the production router with an allowed directory containing a symlink:

```rust
#[tokio::test]
#[serial]
#[cfg(unix)]
async fn active_embed_rejects_symlink_children_under_allowed_root() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::TempDir::new().expect("tempdir");
    let allowed = temp.path().join("allowed");
    let outside = temp.path().join("secret.md");
    let inside_link = allowed.join("secret.md");
    std::fs::create_dir_all(&allowed).expect("allowed dir");
    std::fs::write(&outside, "secret").expect("outside secret");
    symlink(&outside, &inside_link).expect("symlink");

    let _env = EnvGuard::set_key("AXON_MCP_EMBED_ALLOWED_ROOTS", Some(allowed.to_str().unwrap()));
    let (base, shutdown, handle) = spawn(AuthPolicy::LoopbackDev).await;
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{base}/v1/embed"))
        .json(&serde_json::json!({ "input": allowed }))
        .send()
        .await
        .expect("embed request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    stop(shutdown, handle).await;
}
```

If `EnvGuard` only supports one fixed key today, extend it to `EnvGuard::set_key(key, value)`.

- [ ] **Step 2: Run test and verify it fails**

Run:

```bash
cargo test active_embed_rejects_symlink_children_under_allowed_root --lib
```

Expected: FAIL because active REST validator accepts directory symlink children or compile fails until test helper is adjusted.

- [ ] **Step 3: Extract MCP validator for web reuse**

In `src/mcp/server/common.rs`, make `validate_mcp_embed_input` available as:

```rust
pub(crate) fn validate_embed_input_for_server(input: &str) -> Result<String, String> {
    validate_mcp_embed_input_with_roots(
        input,
        &mcp_embed_allowed_roots_from_env(),
        mcp_embed_max_local_bytes_from_env(),
    )
    .map_err(|err| err.message().to_string())
}
```

If `ErrorData` does not expose `message()`, map with `format!("{err}")` and adjust the expected assertion to status only.

- [ ] **Step 4: Use shared validator in active REST**

Replace `validate_embed_path_sync` in `src/web/server/handlers/async_jobs.rs` with a call to the shared validator:

```rust
fn validate_embed_path_sync(input: &str) -> Result<(), HttpError> {
    crate::mcp::server::common::validate_embed_input_for_server(input)
        .map(|_| ())
        .map_err(|err| HttpError::bad_request(err))
}
```

Remove the duplicated local validator code from `async_jobs.rs`.

- [ ] **Step 5: Run security tests**

Run:

```bash
cargo test active_embed_rejects_symlink_children_under_allowed_root --lib
cargo test mcp_embed_rejects_symlink_inputs --lib
```

Expected: PASS.

---

### Task 3: Service-Owned Scrape Embed Orchestration

**Files:**
- Modify: `src/services/scrape.rs`
- Modify: `src/services/scrape_tests.rs`
- Modify: `src/cli/commands/scrape.rs`
- Modify: `src/web/server/handlers/exploration.rs`

- [ ] **Step 1: Move prepared-doc conversion into services**

Move the logic currently in `src/cli/commands/scrape.rs::scrape_result_to_prepared_doc` into `src/services/scrape.rs`:

```rust
pub fn scrape_result_to_prepared_doc(result: &ScrapeResult) -> crate::vector::ops::PreparedDoc {
    crate::vector::ops::PreparedDoc {
        source: result.url.clone(),
        text: result.markdown.clone(),
        title: result.title.clone(),
        extra: result.extra.clone(),
        extractor_name: result.extractor_name.clone(),
    }
}
```

Adjust field names to match the actual `PreparedDoc` struct if they differ.

- [ ] **Step 2: Add service function for scrape plus optional embed**

In `src/services/scrape.rs`, add:

```rust
pub async fn scrape_batch_with_optional_embed(
    cfg: &Config,
    urls: &[String],
    tx: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<Vec<ScrapeResult>, Box<dyn Error>> {
    let results = scrape_batch(cfg, urls, tx).await?;
    if cfg.embed {
        let docs = results.iter().map(scrape_result_to_prepared_doc).collect();
        crate::vector::ops::embed_prepared_docs(cfg, docs, None).await?;
    }
    Ok(results)
}
```

- [ ] **Step 3: Update CLI scrape to use service conversion**

In `src/cli/commands/scrape.rs`, remove the local conversion body and call `crate::services::scrape::scrape_result_to_prepared_doc(result)`.

- [ ] **Step 4: Update active REST scrape**

In `src/web/server/handlers/exploration.rs`, replace direct conversion and `embed_scrape_docs_sync` with:

```rust
let results = services::scrape::scrape_batch_with_optional_embed(&cfg, &urls, None)
    .await
    .map_err(HttpError::from_box)?;
```

Delete the local `embed_scrape_docs_sync` helper.

- [ ] **Step 5: Run focused scrape tests**

Run:

```bash
cargo test scrape_server_mode_forwards_skip_embed --lib
cargo test test_select_article_body_prefers_full_markdown --lib
cargo test active_scrape_openapi_schema_includes_embed --lib
```

Expected: the first two pass; the OpenAPI test may not exist until Task 6.

---

### Task 4: Active Async Job Parity

**Files:**
- Modify: `src/web/server/handlers/async_jobs.rs`
- Modify: `src/cli/server_mode/plan.rs`
- Modify: `src/cli/server_mode/plan_ingest.rs`
- Modify: `src/cli/server_mode_tests.rs`
- Modify: `src/core/config/types/overrides.rs`
- Modify: `src/core/config/types/overrides_tests.rs`

- [ ] **Step 1: Add override support for max_sitemaps and headers**

In `ConfigOverrides`, add:

```rust
pub max_sitemaps: Option<usize>,
pub custom_headers: Option<Vec<String>>,
```

In `Config::apply_overrides`, apply:

```rust
if let Some(v) = overrides.max_sitemaps {
    cfg.max_sitemaps = v;
}
if let Some(ref v) = overrides.custom_headers {
    cfg.custom_headers = v.clone();
}
```

Add a test in `overrides_tests.rs`:

```rust
#[test]
fn apply_overrides_sets_sitemap_and_headers() {
    let base = Config::test_default();
    let cfg = base.apply_overrides(&ConfigOverrides {
        max_sitemaps: Some(7),
        custom_headers: Some(vec!["x-test: 1".to_string()]),
        ..ConfigOverrides::default()
    });

    assert_eq!(cfg.max_sitemaps, 7);
    assert_eq!(cfg.custom_headers, vec!["x-test: 1"]);
}
```

- [ ] **Step 2: Use canonical DTOs in active async handlers**

Replace `CrawlStartRequest`, `EmbedStartRequest`, and `ExtractStartRequest` local structs with imports:

```rust
use crate::services::client_contract::{RestCrawlRequest, RestEmbedRequest, RestExtractRequest, RestIngestRequest};
```

Update handler signatures and `#[utoipa::path(request_body = ...)]` references.

- [ ] **Step 3: Apply per-family overrides**

In `start_crawl`, build overrides:

```rust
let cfg = cfg.apply_overrides(&ConfigOverrides {
    max_pages: req.max_pages,
    max_depth: req.max_depth,
    include_subdomains: req.include_subdomains,
    respect_robots: req.respect_robots,
    discover_sitemaps: req.discover_sitemaps,
    max_sitemaps: req.max_sitemaps,
    sitemap_since_days: req.sitemap_since_days,
    render_mode: req.render_mode,
    delay_ms: req.delay_ms,
    collection: req.collection.clone(),
    embed: req.embed,
    custom_headers: Some(req.headers.iter().map(|(k, v)| format!("{k}: {v}")).collect()),
    ..ConfigOverrides::default()
});
```

In `start_embed`, apply collection:

```rust
let cfg = cfg.apply_overrides(&ConfigOverrides {
    collection: req.collection.clone(),
    ..ConfigOverrides::default()
});
```

In `start_extract`, apply render/embed/collection/header overrides and `mode` only if existing services already support it. If mode has no service hook, reject non-auto mode with `HttpError::bad_request("extract_mode is not supported by REST yet")` rather than silently ignoring it.

- [ ] **Step 4: Fix ingest request shape**

Make active `/v1/ingest` accept `RestIngestRequest` and map it to the existing MCP request or directly to service source:

```rust
fn ingest_source(req: RestIngestRequest, cfg: &Config) -> Result<services::ingest::IngestSource, HttpError> {
    let mcp_req = crate::mcp::schema::IngestRequest {
        source_type: Some(req.source_type),
        target: Some(req.target),
        include_source: req.include_source,
        ..Default::default()
    };
    services::ingest::source_from_mcp_request(&mcp_req, cfg).map_err(HttpError::bad_request)
}
```

Adjust the exact field names to match `IngestRequest`.

- [ ] **Step 5: Update CLI server-mode plans**

In `src/cli/server_mode/plan.rs`, construct `RestCrawlRequest`, `RestEmbedRequest`, and `RestExtractRequest`, then serialize with `serde_json::to_value(req).expect("serialize REST request")`.

In `plan_ingest.rs`, serialize `RestIngestRequest` with `target` for GitHub instead of `repo`.

- [ ] **Step 6: Run async parity tests**

Run:

```bash
cargo test extract_server_mode_plan_preserves_extract_overrides --lib
cargo test ingest_server_mode_uses_action_api_ingest_contract --lib
cargo test embed_server_mode_plan_fails_clearly_for_host_local_path --lib
cargo test apply_overrides_sets_sitemap_and_headers --lib
```

Expected: PASS after assertions are updated to canonical body shape.

---

### Task 5: RAG And Exploration Parity

**Files:**
- Modify: `src/web/server/handlers/rag.rs`
- Modify: `src/web/server/handlers/exploration.rs`
- Modify: `src/cli/server_mode/plan.rs`
- Modify: `src/cli/server_mode_tests.rs`

- [ ] **Step 1: Use canonical DTOs in RAG handlers**

Replace local `QueryRequest`, `RetrieveRequest`, `EvaluateRequest`, and `SuggestRequest` with canonical `Rest*Request` types.

- [ ] **Step 2: Apply only supported fields**

For `query`, apply `collection`, `limit`, `offset`, `since`, `before`, and `hybrid_search` through `ConfigOverrides` before calling `services::query::query`.

For `retrieve`, apply `collection`, `since`, and `before` before calling `services::query::retrieve`.

For `suggest`, apply `collection` and reject `limit` unless `services::query::suggest` supports it. Use:

```rust
if req.limit.is_some() {
    return Err(HttpError::bad_request("suggest.limit is not supported yet"));
}
```

For `evaluate`, apply `collection`; reject `diagnostics` if no service hook exists.

- [ ] **Step 3: Use canonical DTOs in exploration handlers**

Replace local scrape/summarize/map/search/research request structs with canonical DTOs.

For search/research, include `time_range` from alias-capable DTO. Do not inherit `cfg.wait` from server config for REST by default; only apply `wait` when request explicitly includes it.

For summarize, apply render/root/exclude overrides using `ConfigOverrides`.

For map, reject `map_fallback` unless the service currently supports a request-scoped override.

- [ ] **Step 4: Update CLI server-mode plans for search/research/summarize/map**

Serialize canonical DTOs and include `search_time_range`/`time_range` when configured. Include summarize render/root/exclude controls when configured.

- [ ] **Step 5: Run focused tests**

Run:

```bash
cargo test query_server_mode_uses_rest_contract_body --lib
cargo test retrieve_server_mode_forwards_collection --lib
cargo test search_server_mode_forwards_time_range --lib
cargo test summarize_server_mode_forwards_render_selectors --lib
```

Expected: tests may need to be created in this task and should pass.

---

### Task 6: Active Router And OpenAPI Parity Tests

**Files:**
- Modify: `src/web/server_tests.rs`
- Modify: `src/web/server/openapi.rs`
- Modify: `src/web/server/handlers/rest_tests.rs`

- [ ] **Step 1: Add OpenAPI schema field assertions**

In `src/web/server_tests.rs`, extend the OpenAPI test or add:

```rust
#[tokio::test]
#[serial]
async fn active_openapi_schema_includes_rest_parity_fields() {
    let _env = EnvGuard::set(None);
    let (base, shutdown, handle) = spawn(AuthPolicy::LoopbackDev).await;
    let spec: serde_json::Value = reqwest::get(format!("{base}/api-docs/openapi.json"))
        .await
        .expect("openapi request")
        .json()
        .await
        .expect("openapi json");

    let schemas = &spec["components"]["schemas"];
    assert!(schemas["RestCrawlRequest"]["properties"].get("max_sitemaps").is_some());
    assert!(schemas["RestCrawlRequest"]["properties"].get("collection").is_some());
    assert!(schemas["RestEmbedRequest"]["properties"].get("collection").is_some());
    assert!(schemas["RestExtractRequest"]["properties"].get("render_mode").is_some());
    assert!(schemas["RestExtractRequest"]["properties"].get("embed").is_some());
    assert!(schemas["RestScrapeRequest"]["properties"].get("embed").is_some());
    assert!(schemas["RestRetrieveRequest"]["properties"].get("collection").is_some());

    stop(shutdown, handle).await;
}
```

- [ ] **Step 2: Update OpenAPI components**

In `src/web/server/openapi.rs`, replace handler-local request schemas with canonical schemas:

```rust
crate::services::client_contract::RestQueryRequest,
crate::services::client_contract::RestRetrieveRequest,
crate::services::client_contract::RestEvaluateRequest,
crate::services::client_contract::RestSuggestRequest,
crate::services::client_contract::RestScrapeRequest,
crate::services::client_contract::RestSummarizeRequest,
crate::services::client_contract::RestMapRequest,
crate::services::client_contract::RestSearchRequest,
crate::services::client_contract::RestCrawlRequest,
crate::services::client_contract::RestEmbedRequest,
crate::services::client_contract::RestExtractRequest,
crate::services::client_contract::RestIngestRequest,
```

- [ ] **Step 3: Quarantine inactive REST tests**

In `src/web/server/handlers/rest_tests.rs`, rename or remove tests that claim request-contract parity for inactive DTOs. Keep only auth/scope tests if they still add value, and add comments:

```rust
// This module exercises the retained legacy REST router. Contract parity tests
// for production routes belong in src/web/server_tests.rs against routing.rs.
```

- [ ] **Step 4: Run active OpenAPI tests**

Run:

```bash
cargo test active_openapi_schema_includes_rest_parity_fields --lib
cargo test openapi_docs_are_public_and_list_rest_routes --lib
```

Expected: PASS.

---

### Task 7: Generated Client Cleanup

**Files:**
- Modify: `apps/palette-tauri/src/lib/axonClient.ts`
- Modify: `apps/palette-tauri/package.json`
- Modify: `apps/palette-tauri/README.md`

- [ ] **Step 1: Fix ingest body**

In `apps/palette-tauri/src/lib/axonClient.ts`, change:

```ts
return { source_type: "github", repo: target, include_source: true };
```

to:

```ts
return { source_type: "github", target, include_source: true };
```

- [ ] **Step 2: Add safe bearer-token guard**

Add:

```ts
function canSendBearerToken(serverUrl: string): boolean {
  const url = new URL(serverUrl);
  if (url.protocol === "https:") return true;
  if (url.protocol !== "http:") return false;
  return ["localhost", "127.0.0.1", "::1"].includes(url.hostname);
}
```

Use it in `createAxonClient`:

```ts
if (config.token && !canSendBearerToken(config.serverUrl)) {
  throw new Error("Refusing to send bearer token to non-loopback HTTP Axon server");
}
```

- [ ] **Step 3: Remove weak body escape hatch for touched route**

Replace `postResult` body type with path-specific request body inference:

```ts
type RequestBody<Path extends PostPath> =
  paths[Path]["post"] extends { requestBody: { content: { "application/json": infer Body } } }
    ? Body
    : never;

async function postResult<Path extends PostPath>(
  client: Client,
  path: Path,
  body: RequestBody<Path>,
): Promise<PaletteResult> {
  const { data, error, response } = await client.POST(path, { body } as Parameters<Client["POST"]>[1]);
  return {
    ok: response.ok,
    status: response.status,
    path: String(path),
    method: "POST",
    payload: data ?? error ?? null,
  };
}
```

- [ ] **Step 4: Make API generation local-spec friendly**

In `apps/palette-tauri/package.json`, change `generate:api` to prefer a local file:

```json
"generate:api": "openapi-typescript ${AXON_OPENAPI_URL:-../../docs/openapi/axon.openapi.json} -o src/lib/axon-api.d.ts",
"check:api": "openapi-typescript ${AXON_OPENAPI_URL:-../../docs/openapi/axon.openapi.json} -o src/lib/axon-api.d.ts --check"
```

If `--check` is unsupported by the installed version, replace `check:api` with generation plus `git diff --exit-code src/lib/axon-api.d.ts`.

- [ ] **Step 5: Run TypeScript check**

Run:

```bash
pnpm --dir apps/palette-tauri typecheck
```

Expected: PASS.

---

### Task 8: Verification And Beads Updates

**Files:**
- Modify: Beads issue comments/statuses for `axon_rust-uf1x.*`
- Modify: `docs/API-PARITY.md` if it still claims `/v1/actions` is current

- [ ] **Step 1: Run focused Rust tests**

Run:

```bash
cargo fmt --check
cargo check --bin axon
cargo test client_contract --lib
cargo test active_openapi_schema_includes_rest_parity_fields --lib
cargo test active_embed_rejects_symlink_children_under_allowed_root --lib
cargo test scrape_server_mode_forwards_skip_embed --lib
cargo test extract_server_mode_plan_preserves_extract_overrides --lib
cargo test ingest_server_mode_uses_action_api_ingest_contract --lib
```

Expected: all PASS.

- [ ] **Step 2: Run app checks**

Run:

```bash
pnpm --dir apps/palette-tauri typecheck
```

Expected: PASS.

- [ ] **Step 3: Update Beads**

Add comments to each child bead with the implementation evidence:

```bash
bd comments add axon_rust-uf1x.1 "IMPLEMENTED: canonical REST DTOs are used by active handlers, CLI server-mode plans, and OpenAPI."
bd comments add axon_rust-uf1x.3 "IMPLEMENTED: active REST embed validation rejects symlink children and async job request fields are applied or explicitly rejected."
bd comments add axon_rust-uf1x.4 "IMPLEMENTED: palette ingest uses target and typed generated request bodies; bearer tokens are restricted to HTTPS or loopback HTTP."
```

Close only the child beads whose acceptance criteria are fully satisfied:

```bash
bd close axon_rust-uf1x.1 axon_rust-uf1x.3 axon_rust-uf1x.5 --reason "Implemented and verified in REST canonical contract work"
```

Leave partially deferred beads open with explicit comments rather than over-closing.

---

## Self-Review

Spec coverage:
- Canonical REST DTO source of truth: Tasks 1, 4, 5, 6.
- OpenAPI schema truth: Task 6.
- Generated client usage: Task 7.
- Research/eng-review feedback: Tasks 2, 3, 4, 6, 7.
- Beads updates: Task 8.

Deferred deliberately:
- Full MCP schema unification is not required for this epic unless a concrete mapper bug appears.
- Full `apps/web` generated-client migration can follow after Tauri and OpenAPI are truthful.
- Full deletion of `handlers/rest/*` can follow once active-router coverage is equivalent.

Placeholder scan: no TODO/TBD placeholders remain.

Type consistency: canonical REST DTO names use `Rest*Request`; client-only route preference remains outside REST DTOs.
