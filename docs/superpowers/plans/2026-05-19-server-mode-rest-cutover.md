# Server Mode REST Cutover Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make REST the canonical client/server API, move CLI and stdio MCP to thin-client server routing with safe local fallback, add capability-aware doctor output, reconcile local fallback artifacts, and remove `/v1/actions`.

**Architecture:** Introduce canonical service request/response contracts that CLI, REST, and MCP all map into. Server-routed clients use direct REST endpoints and include stable route/artifact metadata in machine-readable output. Local fallback is explicit, capability-aware, host-endpoint-aware, and reconciles Axon-owned artifacts back to the server when available.

**Tech Stack:** Rust 2024, axum 0.8, reqwest 0.13, serde/serde_json, SQLite job runtime, existing Axon services layer, existing MCP `AxonRequest` schema, existing crawl `manifest.jsonl` output.

**Source Requirements:**
- Spec: `docs/specs/server-mode-capability-tiers.md`
- Contract: `docs/contracts/server-mode-routing-contract.md`

---

## File Structure

**New files:**
- `src/services/client_contract.rs` — canonical request/option structs shared by CLI, REST, and MCP adapters.
- `src/services/client_contract_tests.rs` — serialization/default/round-trip tests for canonical request structs.
- `src/services/route_meta.rs` — route/fallback/capability metadata and stable JSON envelope helpers.
- `src/services/route_meta_tests.rs` — route metadata serialization and fallback outcome tests.
- `src/services/artifacts.rs` — stable artifact handle construction and artifact id helpers.
- `src/services/artifacts_tests.rs` — artifact handle safety, relative path, and id tests.
- `src/core/endpoints.rs` — host-reachable endpoint resolution for Qdrant, embedding provider, Chrome, and LLM.
- `src/core/endpoints_tests.rs` — container DNS rejection, localhost fallback, cached candidate tests.
- `src/cli/rest_client.rs` — direct REST client used by CLI server mode and stdio MCP thin-client mode.
- `src/cli/rest_client_tests.rs` — timeout, auth, schema mismatch, unavailable-server classification tests.
- `src/cli/route.rs` — command route planning and fallback policy.
- `src/cli/route_tests.rs` — command matrix tests and no-silent-fallback tests.
- `src/services/sync.rs` — local artifact reconciliation service.
- `src/services/sync_tests.rs` — content-hash conflict and pending sync tests.
- `src/cli/commands/sync.rs` — `axon sync pending`.
- `src/cli/commands/sync_tests.rs` — sync CLI output and dry-run tests.
- `src/mcp/thin_client.rs` — stdio MCP thin-client adapter to REST when `AXON_SERVER_URL` is set.
- `src/mcp/thin_client_tests.rs` — MCP route metadata and fallback tests.
- `src/web/server/handlers/rest/artifacts.rs` — REST artifact lookup/read endpoints.
- `src/web/server/handlers/rest/artifacts_tests.rs` — artifact read safety and id lookup tests.
- `src/web/server/handlers/rest/sync.rs` — REST sync/register local artifact endpoint.
- `src/web/server/handlers/rest/sync_tests.rs` — sync endpoint auth and dedupe tests.

**Modified files:**
- `src/lib.rs` — use new route planner for CLI execution before local `ServiceContext` creation.
- `src/core/config/types/enums.rs` — add `Sync` command.
- `src/core/config/parse/build_config/command_dispatch.rs` — parse `sync pending`, `doctor diagnose`, and server-required flag.
- `src/core/config/types/config.rs` — add route timeout and server-required config fields.
- `src/core/health/doctor.rs` — add mode/capability/endpoints/remedies/diagnose output.
- `src/services/mod.rs` — export `client_contract`, `route_meta`, `artifacts`, `sync`.
- `src/services/types/service.rs` — add route metadata and artifact handle fields to crawl, scrape, extract, embed, ingest, query, retrieve, ask, sources, domains, stats, screenshot, and status result structs that are serialized to CLI/REST/MCP clients.
- `src/web/server/handlers/rest.rs` — add parity request bodies, artifact routes, sync routes, lifecycle list/cleanup/clear/recover.
- `src/web/server/handlers/rest/types.rs` — replace thin REST body structs with canonical service request mapping.
- `src/web/server/routing.rs` — expose direct REST parity routes and remove `/v1/actions` merge at cutover.
- `src/web/actions.rs` — delete after cutover task.
- `src/cli/server_mode.rs`, `src/cli/server_mode/plan.rs`, `src/cli/server_mode/render.rs` — replace action-envelope client path with direct REST route client. Delete action-envelope builders once direct REST planning covers the command.
- `src/cli/client.rs` — either remove `/v1/actions`-specific client or narrow to generic JSON helpers used by `rest_client`.
- `src/cli/commands/extract.rs` — add `--extract-mode`, provenance guidance, and route metadata in JSON.
- `src/mcp/server.rs` and `src/mcp/server/handlers_*.rs` — route stdio MCP through `thin_client` when server URL is configured.
- `src/mcp/schema.rs` — expose extract mode and any canonical request parity fields.
- `docs/specs/server-mode-capability-tiers.md` — keep in sync if execution discovers necessary contract clarifications.
- `docs/contracts/server-mode-routing-contract.md` — keep normative contract in sync with implemented behavior.

---

## Task 1: Canonical Service Request Types

**Files:**
- Create: `src/services/client_contract.rs`
- Create: `src/services/client_contract_tests.rs`
- Modify: `src/services/mod.rs`
- Test: `src/services/client_contract_tests.rs`

- [ ] **Step 1.1: Add canonical request types test**

Create `src/services/client_contract_tests.rs`:

```rust
use super::client_contract::{
    ClientCrawlRequest, ClientExtractMode, ClientExtractRequest, ClientRoutePreference,
};
use crate::core::config::RenderMode;

#[test]
fn extract_request_defaults_to_auto_mode() {
    let req = ClientExtractRequest {
        urls: vec!["https://example.com/docs".to_string()],
        prompt: Some("extract title".to_string()),
        mode: None,
        max_pages: Some(1),
        render_mode: Some(RenderMode::Http),
        embed: Some(false),
        headers: vec![],
        route_preference: ClientRoutePreference::Default,
    };

    assert_eq!(req.effective_mode(), ClientExtractMode::Auto);
}

#[test]
fn crawl_request_serializes_all_routing_knobs() {
    let req = ClientCrawlRequest {
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
        route_preference: ClientRoutePreference::ServerRequired,
    };

    let json = serde_json::to_value(&req).expect("serialize crawl request");
    assert_eq!(json["max_pages"], 10);
    assert_eq!(json["max_depth"], 2);
    assert_eq!(json["render_mode"], "http");
    assert_eq!(json["route_preference"], "server_required");
}
```

- [ ] **Step 1.2: Run failing test**

Run:

```bash
cargo test -q client_contract_tests
```

Expected: FAIL because `client_contract` does not exist.

- [ ] **Step 1.3: Add canonical request structs**

Create `src/services/client_contract.rs`:

```rust
use crate::core::config::{RenderMode, ScrapeFormat};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientRoutePreference {
    Default,
    LocalOnly,
    ServerRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientExtractMode {
    Auto,
    Deterministic,
    Llm,
    Both,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClientCrawlRequest {
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
    pub headers: Vec<(String, String)>,
    pub route_preference: ClientRoutePreference,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClientScrapeRequest {
    pub url: String,
    pub render_mode: Option<RenderMode>,
    pub format: Option<ScrapeFormat>,
    pub embed: Option<bool>,
    pub root_selector: Option<String>,
    pub exclude_selector: Option<String>,
    pub headers: Vec<(String, String)>,
    pub route_preference: ClientRoutePreference,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClientExtractRequest {
    pub urls: Vec<String>,
    pub prompt: Option<String>,
    pub mode: Option<ClientExtractMode>,
    pub max_pages: Option<u32>,
    pub render_mode: Option<RenderMode>,
    pub embed: Option<bool>,
    pub headers: Vec<(String, String)>,
    pub route_preference: ClientRoutePreference,
}

impl ClientExtractRequest {
    pub fn effective_mode(&self) -> ClientExtractMode {
        self.mode.unwrap_or(ClientExtractMode::Auto)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClientEmbedRequest {
    pub input: String,
    pub source_type: Option<String>,
    pub collection: Option<String>,
    pub route_preference: ClientRoutePreference,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClientQueryRequest {
    pub query: String,
    pub collection: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub route_preference: ClientRoutePreference,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClientRetrieveRequest {
    pub url: String,
    pub max_points: Option<usize>,
    pub cursor: Option<String>,
    pub token_budget: Option<usize>,
    pub route_preference: ClientRoutePreference,
}
```

Modify `src/services/mod.rs`:

```rust
pub mod client_contract;
```

- [ ] **Step 1.4: Run tests**

Run:

```bash
cargo test -q client_contract_tests
```

Expected: PASS.

- [ ] **Step 1.5: Commit**

```bash
git add src/services/client_contract.rs src/services/client_contract_tests.rs src/services/mod.rs
git commit -m "feat(server-mode): add canonical client request contracts"
```

---

## Task 2: Route Metadata and Stable JSON Envelope

**Files:**
- Create: `src/services/route_meta.rs`
- Create: `src/services/route_meta_tests.rs`
- Modify: `src/services/mod.rs`
- Test: `src/services/route_meta_tests.rs`

- [ ] **Step 2.1: Add route metadata tests**

Create `src/services/route_meta_tests.rs`:

```rust
use super::route_meta::{FallbackOutcome, RouteKind, RouteMetadata};

#[test]
fn fallback_equivalent_serializes_as_stable_json() {
    let meta = RouteMetadata {
        route: RouteKind::FallbackLocal,
        fallback: true,
        fallback_outcome: FallbackOutcome::CompletedEquivalent,
        capability_tier: "tier_1_crawl_retrieve".to_string(),
        server_url: Some("http://127.0.0.1:8001".to_string()),
        local_data_dir: Some("/home/user/.axon".to_string()),
        effective_endpoints: serde_json::json!({
            "qdrant": "http://127.0.0.1:53333",
            "embedding": "http://127.0.0.1:52000"
        }),
        warnings: vec!["server unavailable; completed locally".to_string()],
    };

    let json = serde_json::to_value(&meta).expect("serialize route metadata");
    assert_eq!(json["route"], "fallback_local");
    assert_eq!(json["fallback"], true);
    assert_eq!(json["fallback_outcome"], "completed_equivalent");
    assert_eq!(json["warnings"][0], "server unavailable; completed locally");
}
```

- [ ] **Step 2.2: Run failing test**

Run:

```bash
cargo test -q route_meta_tests
```

Expected: FAIL because `route_meta` does not exist.

- [ ] **Step 2.3: Add route metadata structs**

Create `src/services/route_meta.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteKind {
    Server,
    Local,
    FallbackLocal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FallbackOutcome {
    None,
    CompletedEquivalent,
    CompletedDegraded,
    FailedLocal,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RouteMetadata {
    pub route: RouteKind,
    pub fallback: bool,
    pub fallback_outcome: FallbackOutcome,
    pub capability_tier: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_data_dir: Option<String>,
    #[serde(default)]
    pub effective_endpoints: serde_json::Value,
    #[serde(default)]
    pub warnings: Vec<String>,
}

impl RouteMetadata {
    pub fn server(server_url: impl Into<String>) -> Self {
        Self {
            route: RouteKind::Server,
            fallback: false,
            fallback_outcome: FallbackOutcome::None,
            capability_tier: "server".to_string(),
            server_url: Some(server_url.into()),
            local_data_dir: None,
            effective_endpoints: serde_json::json!({}),
            warnings: vec![],
        }
    }
}
```

Modify `src/services/mod.rs`:

```rust
pub mod route_meta;
```

- [ ] **Step 2.4: Run tests**

Run:

```bash
cargo test -q route_meta_tests
```

Expected: PASS.

- [ ] **Step 2.5: Commit**

```bash
git add src/services/route_meta.rs src/services/route_meta_tests.rs src/services/mod.rs
git commit -m "feat(server-mode): add route metadata envelope"
```

---

## Task 3: Stable Artifact Handles

**Files:**
- Create: `src/services/artifacts.rs`
- Create: `src/services/artifacts_tests.rs`
- Modify: `src/services/mod.rs`
- Test: `src/services/artifacts_tests.rs`

- [ ] **Step 3.1: Add artifact handle tests**

Create `src/services/artifacts_tests.rs`:

```rust
use super::artifacts::{ArtifactHandle, ArtifactKind};
use uuid::Uuid;

#[test]
fn artifact_handle_rejects_parent_dir_relative_path() {
    let result = ArtifactHandle::new(
        ArtifactKind::Markdown,
        "../secret.md",
        Some("https://example.com".to_string()),
        None,
        "abc123".to_string(),
        12,
        Some(1),
        None,
    );

    assert!(result.is_err());
}

#[test]
fn artifact_id_is_stable_for_kind_path_and_hash() {
    let one = ArtifactHandle::new(
        ArtifactKind::CrawlManifest,
        "domains/example.com/job-1/manifest.jsonl",
        Some("https://example.com".to_string()),
        Some(Uuid::nil()),
        "abc123".to_string(),
        128,
        Some(4),
        Some("/home/axon/.axon/output/domains/example.com/job-1/manifest.jsonl".to_string()),
    )
    .expect("artifact handle");
    let two = ArtifactHandle::new(
        ArtifactKind::CrawlManifest,
        "domains/example.com/job-1/manifest.jsonl",
        Some("https://example.com".to_string()),
        Some(Uuid::nil()),
        "abc123".to_string(),
        128,
        Some(4),
        None,
    )
    .expect("artifact handle");

    assert_eq!(one.artifact_id, two.artifact_id);
    assert_eq!(one.kind, ArtifactKind::CrawlManifest);
}
```

- [ ] **Step 3.2: Run failing test**

Run:

```bash
cargo test -q artifacts_tests
```

Expected: FAIL because `artifacts` does not exist.

- [ ] **Step 3.3: Add artifact handle implementation**

Create `src/services/artifacts.rs`:

```rust
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Component, Path};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ArtifactKind {
    Markdown,
    CrawlManifest,
    ExtractSummary,
    ExtractItems,
    Screenshot,
    Log,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArtifactHandle {
    pub artifact_id: String,
    pub kind: ArtifactKind,
    pub relative_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_id: Option<Uuid>,
    pub content_hash: String,
    pub bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub debug_path: Option<String>,
}

impl ArtifactHandle {
    pub fn new(
        kind: ArtifactKind,
        relative_path: impl Into<String>,
        source_url: Option<String>,
        job_id: Option<Uuid>,
        content_hash: String,
        bytes: u64,
        line_count: Option<u64>,
        debug_path: Option<String>,
    ) -> Result<Self, String> {
        let relative_path = relative_path.into().replace('\\', "/");
        reject_unsafe_relative_path(&relative_path)?;
        let artifact_id = artifact_id(kind, &relative_path, &content_hash);
        Ok(Self {
            artifact_id,
            kind,
            relative_path,
            source_url,
            job_id,
            content_hash,
            bytes,
            line_count,
            debug_path,
        })
    }
}

fn reject_unsafe_relative_path(path: &str) -> Result<(), String> {
    if path.trim().is_empty() {
        return Err("artifact relative_path is empty".to_string());
    }
    if Path::new(path).components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return Err(format!("unsafe artifact relative_path: {path}"));
    }
    Ok(())
}

fn artifact_id(kind: ArtifactKind, relative_path: &str, content_hash: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(format!("{kind:?}\n{relative_path}\n{content_hash}"));
    format!("art_{}", hex::encode(&hasher.finalize()[..16]))
}
```

Add dependencies if missing in `Cargo.toml`:

```toml
sha2 = "0.10"
hex = "0.4"
```

Modify `src/services/mod.rs`:

```rust
pub mod artifacts;
```

- [ ] **Step 3.4: Run tests**

Run:

```bash
cargo test -q artifacts_tests
cargo check --bin axon
```

Expected: PASS.

- [ ] **Step 3.5: Commit**

```bash
git add Cargo.toml Cargo.lock src/services/artifacts.rs src/services/artifacts_tests.rs src/services/mod.rs
git commit -m "feat(server-mode): add stable artifact handles"
```

---

## Task 4: Host-Reachable Endpoint Resolver

**Files:**
- Create: `src/core/endpoints.rs`
- Create: `src/core/endpoints_tests.rs`
- Modify: `src/core.rs`
- Test: `src/core/endpoints_tests.rs`

- [ ] **Step 4.1: Add endpoint resolver tests**

Create `src/core/endpoints_tests.rs`:

```rust
use super::endpoints::{EndpointKind, EndpointSource, resolve_host_endpoint};

#[test]
fn container_dns_qdrant_uses_localhost_candidate_for_host_runtime() {
    let resolved = resolve_host_endpoint(
        EndpointKind::Qdrant,
        Some("http://axon-qdrant:6333"),
        &[],
    )
    .expect("resolved endpoint");

    assert_eq!(resolved.url, "http://127.0.0.1:53333");
    assert_eq!(resolved.source, EndpointSource::LocalhostDefault);
    assert!(resolved.warnings[0].contains("container DNS"));
}

#[test]
fn host_valid_config_url_wins_over_default() {
    let resolved = resolve_host_endpoint(
        EndpointKind::Embedding,
        Some("http://192.168.1.20:52000"),
        &[],
    )
    .expect("resolved endpoint");

    assert_eq!(resolved.url, "http://192.168.1.20:52000");
    assert_eq!(resolved.source, EndpointSource::Configured);
}
```

- [ ] **Step 4.2: Run failing test**

Run:

```bash
cargo test -q endpoints_tests
```

Expected: FAIL because endpoint resolver does not exist.

- [ ] **Step 4.3: Add resolver implementation**

Create `src/core/endpoints.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EndpointKind {
    Qdrant,
    Embedding,
    Chrome,
    Llm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EndpointSource {
    Configured,
    LocalhostDefault,
    TrustedCached,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedEndpoint {
    pub kind: EndpointKind,
    pub url: String,
    pub source: EndpointSource,
    pub warnings: Vec<String>,
}

pub fn resolve_host_endpoint(
    kind: EndpointKind,
    configured: Option<&str>,
    trusted_cached: &[String],
) -> Option<ResolvedEndpoint> {
    if let Some(configured) = configured.filter(|value| !value.trim().is_empty()) {
        if !uses_container_dns(configured) {
            return Some(ResolvedEndpoint {
                kind,
                url: configured.to_string(),
                source: EndpointSource::Configured,
                warnings: vec![],
            });
        }
        return Some(ResolvedEndpoint {
            kind,
            url: localhost_default(kind)?.to_string(),
            source: EndpointSource::LocalhostDefault,
            warnings: vec![format!(
                "configured endpoint {configured} uses container DNS; using host localhost default"
            )],
        });
    }

    if let Some(url) = localhost_default(kind) {
        return Some(ResolvedEndpoint {
            kind,
            url: url.to_string(),
            source: EndpointSource::LocalhostDefault,
            warnings: vec![],
        });
    }

    trusted_cached.first().map(|url| ResolvedEndpoint {
        kind,
        url: url.clone(),
        source: EndpointSource::TrustedCached,
        warnings: vec![],
    })
}

fn uses_container_dns(url: &str) -> bool {
    ["axon-qdrant", "axon-tei", "axon-chrome"]
        .iter()
        .any(|host| url.contains(host))
}

fn localhost_default(kind: EndpointKind) -> Option<&'static str> {
    match kind {
        EndpointKind::Qdrant => Some("http://127.0.0.1:53333"),
        EndpointKind::Embedding => Some("http://127.0.0.1:52000"),
        EndpointKind::Chrome => Some("http://127.0.0.1:6000"),
        EndpointKind::Llm => None,
    }
}
```

Add module declaration in `src/core.rs`:

```rust
pub mod endpoints;
```

- [ ] **Step 4.4: Run tests**

Run:

```bash
cargo test -q endpoints_tests
cargo check --bin axon
```

Expected: PASS.

- [ ] **Step 4.5: Commit**

```bash
git add src/core/endpoints.rs src/core/endpoints_tests.rs src/core.rs src/core/mod.rs
git commit -m "feat(server-mode): resolve host-reachable endpoints"
```

---

## Task 5: CLI Route Planner and No-Silent-Fallback Policy

**Files:**
- Create: `src/cli/route.rs`
- Create: `src/cli/route_tests.rs`
- Modify: `src/cli.rs`
- Modify: `src/lib.rs`
- Test: `src/cli/route_tests.rs`

- [ ] **Step 5.1: Add route matrix tests**

Create `src/cli/route_tests.rs`:

```rust
use super::route::{CommandRoute, FallbackPolicy, plan_command_route};
use crate::core::config::{CommandKind, Config};

fn cfg(command: CommandKind) -> Config {
    let mut cfg = Config::default_minimal();
    cfg.command = command;
    cfg.server_url = Some(reqwest::Url::parse("http://127.0.0.1:8001").unwrap());
    cfg
}

#[test]
fn crawl_start_can_fallback_local() {
    let cfg = cfg(CommandKind::Crawl);
    let plan = plan_command_route(&cfg, &["https://example.com".to_string()])
        .expect("route plan");

    assert_eq!(plan.route, CommandRoute::PreferServer);
    assert_eq!(plan.fallback_policy, FallbackPolicy::AllowEquivalentLocal);
}

#[test]
fn migrate_never_silently_fallbacks() {
    let cfg = cfg(CommandKind::Migrate);
    let plan = plan_command_route(&cfg, &[])
        .expect("route plan");

    assert_eq!(plan.route, CommandRoute::PreferServer);
    assert_eq!(plan.fallback_policy, FallbackPolicy::Disallow);
}

#[test]
fn local_flag_forces_local() {
    let mut cfg = cfg(CommandKind::Crawl);
    cfg.local_mode = true;
    let plan = plan_command_route(&cfg, &["https://example.com".to_string()])
        .expect("route plan");

    assert_eq!(plan.route, CommandRoute::LocalOnly);
}
```

- [ ] **Step 5.2: Run failing test**

Run:

```bash
cargo test -q route_tests
```

Expected: FAIL because `cli::route` does not exist.

- [ ] **Step 5.3: Add route planner**

Create `src/cli/route.rs`:

```rust
use crate::core::config::{CommandKind, Config};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandRoute {
    LocalOnly,
    PreferServer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FallbackPolicy {
    AllowEquivalentLocal,
    AllowDegradedLocal,
    Disallow,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandRoutePlan {
    pub route: CommandRoute,
    pub fallback_policy: FallbackPolicy,
}

pub fn plan_command_route(
    cfg: &Config,
    positional: &[String],
) -> Result<CommandRoutePlan, String> {
    if cfg.local_mode || cfg.server_url.is_none() {
        return Ok(CommandRoutePlan {
            route: CommandRoute::LocalOnly,
            fallback_policy: FallbackPolicy::AllowEquivalentLocal,
        });
    }

    let fallback_policy = fallback_policy_for(cfg.command, positional);
    Ok(CommandRoutePlan {
        route: CommandRoute::PreferServer,
        fallback_policy,
    })
}

fn fallback_policy_for(command: CommandKind, positional: &[String]) -> FallbackPolicy {
    match command {
        CommandKind::Crawl | CommandKind::Extract | CommandKind::Embed | CommandKind::Ingest => {
            if is_job_lifecycle_or_worker(positional) {
                FallbackPolicy::Disallow
            } else if command == CommandKind::Ingest {
                FallbackPolicy::AllowDegradedLocal
            } else {
                FallbackPolicy::AllowEquivalentLocal
            }
        }
        CommandKind::Scrape
        | CommandKind::Map
        | CommandKind::Query
        | CommandKind::Retrieve
        | CommandKind::Sources
        | CommandKind::Domains
        | CommandKind::Stats
        | CommandKind::Sessions
        | CommandKind::Screenshot
        | CommandKind::Doctor => FallbackPolicy::AllowEquivalentLocal,
        CommandKind::Research
        | CommandKind::Debug
        | CommandKind::Ask
        | CommandKind::Evaluate
        | CommandKind::Suggest => FallbackPolicy::AllowDegradedLocal,
        CommandKind::Dedupe | CommandKind::Migrate | CommandKind::Watch | CommandKind::Config => {
            FallbackPolicy::Disallow
        }
        CommandKind::Search => FallbackPolicy::AllowEquivalentLocal,
        CommandKind::Completions
        | CommandKind::Mcp
        | CommandKind::Serve
        | CommandKind::Setup
        | CommandKind::Train
        | CommandKind::Status => FallbackPolicy::Disallow,
    }
}

fn is_job_lifecycle_or_worker(positional: &[String]) -> bool {
    matches!(
        positional.first().map(String::as_str),
        Some(
            "status"
                | "errors"
                | "cancel"
                | "list"
                | "cleanup"
                | "clear"
                | "recover"
                | "worker"
        )
    )
}
```

Modify `src/cli.rs`:

```rust
pub mod route;
```

- [ ] **Step 5.4: Run tests**

Run:

```bash
cargo test -q route_tests
cargo check --bin axon
```

Expected: PASS.

- [ ] **Step 5.5: Commit**

```bash
git add src/cli/route.rs src/cli/route_tests.rs src/cli.rs
git commit -m "feat(server-mode): add command route planner"
```

---

## Task 6: Direct REST Client and Server Availability Classification

**Files:**
- Create: `src/cli/rest_client.rs`
- Create: `src/cli/rest_client_tests.rs`
- Modify: `src/cli.rs`
- Test: `src/cli/rest_client_tests.rs`

- [ ] **Step 6.1: Add server availability tests**

Create `src/cli/rest_client_tests.rs`:

```rust
use super::rest_client::{ServerFailureClass, classify_server_status};
use reqwest::StatusCode;

#[test]
fn gateway_unavailable_allows_fallback() {
    assert_eq!(
        classify_server_status(StatusCode::BAD_GATEWAY, ""),
        ServerFailureClass::TransportUnavailable
    );
    assert_eq!(
        classify_server_status(StatusCode::SERVICE_UNAVAILABLE, ""),
        ServerFailureClass::TransportUnavailable
    );
}

#[test]
fn auth_and_schema_errors_do_not_allow_silent_fallback() {
    assert_eq!(
        classify_server_status(StatusCode::UNAUTHORIZED, ""),
        ServerFailureClass::PolicyFailure
    );
    assert_eq!(
        classify_server_status(StatusCode::UPGRADE_REQUIRED, "schema mismatch"),
        ServerFailureClass::SchemaMismatch
    );
}
```

- [ ] **Step 6.2: Run failing test**

Run:

```bash
cargo test -q rest_client_tests
```

Expected: FAIL because `rest_client` does not exist.

- [ ] **Step 6.3: Add REST client skeleton**

Create `src/cli/rest_client.rs`:

```rust
use crate::core::http::build_client;
use reqwest::StatusCode;
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::error::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerFailureClass {
    TransportUnavailable,
    PolicyFailure,
    SchemaMismatch,
    InvalidRequest,
    ServerAcceptedOrUnknown,
}

pub struct RestClient {
    base_url: reqwest::Url,
    client: reqwest::Client,
}

impl RestClient {
    pub fn new(base_url: reqwest::Url, timeout_secs: u64) -> Result<Self, Box<dyn Error>> {
        let client = build_client(timeout_secs, None)?;
        Ok(Self { base_url, client })
    }

    pub async fn post_json<T, R>(&self, path: &str, body: &T) -> Result<R, Box<dyn Error>>
    where
        T: Serialize + ?Sized,
        R: DeserializeOwned,
    {
        let endpoint = self.endpoint(path);
        let response = self.client.post(endpoint.clone()).json(body).send().await?;
        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(format!(
                "server returned {status}: {text} ({:?})",
                classify_server_status(status, &text)
            )
            .into());
        }
        Ok(response.json().await?)
    }

    fn endpoint(&self, path: &str) -> reqwest::Url {
        let mut endpoint = self.base_url.clone();
        let mut base_path = endpoint.path().trim_end_matches('/').to_string();
        if !base_path.is_empty() {
            base_path.push('/');
        }
        base_path.push_str(path.trim_start_matches('/'));
        endpoint.set_path(&base_path);
        endpoint
    }
}

pub fn classify_server_status(status: StatusCode, body: &str) -> ServerFailureClass {
    match status {
        StatusCode::BAD_GATEWAY | StatusCode::SERVICE_UNAVAILABLE | StatusCode::GATEWAY_TIMEOUT => {
            ServerFailureClass::TransportUnavailable
        }
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => ServerFailureClass::PolicyFailure,
        StatusCode::BAD_REQUEST => ServerFailureClass::InvalidRequest,
        StatusCode::NOT_FOUND => ServerFailureClass::InvalidRequest,
        StatusCode::UPGRADE_REQUIRED => ServerFailureClass::SchemaMismatch,
        _ if body.to_ascii_lowercase().contains("schema") => ServerFailureClass::SchemaMismatch,
        _ => ServerFailureClass::ServerAcceptedOrUnknown,
    }
}
```

Modify `src/cli.rs`:

```rust
pub mod rest_client;
```

- [ ] **Step 6.4: Run tests**

Run:

```bash
cargo test -q rest_client_tests
cargo check --bin axon
```

Expected: PASS.

- [ ] **Step 6.5: Commit**

```bash
git add src/cli/rest_client.rs src/cli/rest_client_tests.rs src/cli.rs
git commit -m "feat(server-mode): add direct REST client"
```

---

## Task 7: REST Request Parity and Lifecycle Routes

**Files:**
- Modify: `src/web/server/handlers/rest/types.rs`
- Modify: `src/web/server/handlers/rest/async_jobs.rs`
- Modify: `src/web/server/handlers/rest.rs`
- Modify: `src/web/server/openapi_jobs.rs`
- Test: `src/web/server/handlers/rest_tests.rs`

- [ ] **Step 7.1: Add REST request parity test**

Append to `src/web/server/handlers/rest_tests.rs`:

```rust
#[test]
fn extract_submit_body_accepts_cli_parity_knobs() {
    let body = serde_json::json!({
        "urls": ["https://example.com/docs"],
        "prompt": "extract title",
        "extract_mode": "llm",
        "max_pages": 1,
        "render_mode": "http",
        "embed": false,
        "headers": [["x-test", "1"]]
    });

    let parsed: crate::web::server::handlers::rest::types::ExtractSubmitBody =
        serde_json::from_value(body).expect("parse extract body");

    assert_eq!(parsed.urls, vec!["https://example.com/docs"]);
    assert_eq!(parsed.prompt.as_deref(), Some("extract title"));
    assert_eq!(parsed.max_pages, Some(1));
    assert_eq!(parsed.embed, Some(false));
}
```

If `types` is private, expose only to tests with `pub(crate)` fields or add the test inside the `rest` module's existing sidecar test pattern.

- [ ] **Step 7.2: Run failing test**

Run:

```bash
cargo test -q extract_submit_body_accepts_cli_parity_knobs
```

Expected: FAIL because the extra fields are denied.

- [ ] **Step 7.3: Expand REST body structs**

Modify `src/web/server/handlers/rest/types.rs`:

```rust
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(crate) struct ExtractSubmitBody {
    pub urls: Vec<String>,
    #[serde(default)]
    pub prompt: Option<String>,
    #[serde(default)]
    pub extract_mode: Option<crate::services::client_contract::ClientExtractMode>,
    #[serde(default)]
    pub max_pages: Option<u32>,
    #[serde(default)]
    pub render_mode: Option<crate::core::config::RenderMode>,
    #[serde(default)]
    pub embed: Option<bool>,
    #[serde(default)]
    pub headers: Vec<(String, String)>,
}
```

Add these REST body fields in `src/web/server/handlers/rest/types.rs`:

```rust
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(crate) struct CrawlSubmitBody {
    pub urls: Vec<String>,
    #[serde(default)]
    pub max_pages: Option<u32>,
    #[serde(default)]
    pub max_depth: Option<usize>,
    #[serde(default)]
    pub render_mode: Option<crate::core::config::RenderMode>,
    #[serde(default)]
    pub include_subdomains: Option<bool>,
    #[serde(default)]
    pub respect_robots: Option<bool>,
    #[serde(default)]
    pub discover_sitemaps: Option<bool>,
    #[serde(default)]
    pub max_sitemaps: Option<usize>,
    #[serde(default)]
    pub sitemap_since_days: Option<u32>,
    #[serde(default)]
    pub delay_ms: Option<u64>,
    #[serde(default)]
    pub headers: Vec<(String, String)>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(crate) struct ScrapeSubmitBody {
    pub url: String,
    #[serde(default)]
    pub render_mode: Option<crate::core::config::RenderMode>,
    #[serde(default)]
    pub format: Option<crate::core::config::ScrapeFormat>,
    #[serde(default)]
    pub embed: Option<bool>,
    #[serde(default)]
    pub root_selector: Option<String>,
    #[serde(default)]
    pub exclude_selector: Option<String>,
    #[serde(default)]
    pub headers: Vec<(String, String)>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(crate) struct EmbedSubmitBody {
    pub input: String,
    #[serde(default)]
    pub source_type: Option<String>,
    #[serde(default)]
    pub collection: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(crate) struct QueryBody {
    pub query: String,
    #[serde(default)]
    pub collection: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub offset: Option<usize>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(crate) struct RetrieveBody {
    pub url: String,
    #[serde(default)]
    pub max_points: Option<usize>,
    #[serde(default)]
    pub cursor: Option<String>,
    #[serde(default)]
    pub token_budget: Option<usize>,
}
```

- [ ] **Step 7.4: Apply REST overrides into service config**

Modify `src/web/server/handlers/rest/async_jobs.rs` extract submit to clone request overrides into an effective config before calling `extract_start_with_context`:

```rust
let mut cfg = state.cfg.as_ref().clone();
if let Some(max_pages) = req.max_pages {
    cfg.max_pages = max_pages;
}
if let Some(render_mode) = req.render_mode {
    cfg.render_mode = render_mode;
}
if let Some(embed) = req.embed {
    cfg.embed = embed;
}
```

Then pass `&cfg` instead of `state.cfg.as_ref()`.

- [ ] **Step 7.5: Add lifecycle list/cleanup/clear/recover tests**

Add tests in `src/web/server/handlers/rest_tests.rs` that assert these routes exist and are guarded:

```rust
#[test]
fn async_lifecycle_routes_are_declared_for_extract() {
    let routes = crate::web::server::handlers::rest::documented_rest_paths_for_tests();
    assert!(routes.contains(&"GET /v1/extract".to_string()));
    assert!(routes.contains(&"POST /v1/extract/cleanup".to_string()));
    assert!(routes.contains(&"DELETE /v1/extract".to_string()));
    assert!(routes.contains(&"POST /v1/extract/recover".to_string()));
}
```

If no helper exists, add `documented_rest_paths_for_tests()` beside the router construction and return the static route list used by the router.

- [ ] **Step 7.6: Run REST tests**

Run:

```bash
cargo test -q rest_tests
cargo check --bin axon
```

Expected: PASS.

- [ ] **Step 7.7: Commit**

```bash
git add src/web/server/handlers/rest src/web/server/openapi_jobs.rs src/web/server/handlers/rest_tests.rs
git commit -m "feat(rest): expose full server-mode request parity"
```

---

## Task 8: CLI Server Mode Uses Direct REST and Route Metadata

**Files:**
- Modify: `src/cli/server_mode.rs`
- Modify: `src/cli/server_mode/plan.rs`
- Modify: `src/cli/server_mode/render.rs`
- Modify: `src/lib.rs`
- Test: `src/cli/server_mode_tests.rs`

- [ ] **Step 8.1: Add server mode REST route test**

Append to `src/cli/server_mode_tests.rs`:

```rust
#[test]
fn extract_server_mode_uses_direct_rest_path() {
    let mut cfg = cfg(CommandKind::Extract, &["https://example.com/docs"]);
    cfg.query = Some("extract title".to_string());
    cfg.max_pages = 1;
    cfg.embed = false;

    let plan = crate::cli::server_mode::plan::server_rest_plan(&cfg)
        .expect("server rest plan");

    assert_eq!(plan.method, "POST");
    assert_eq!(plan.path, "/v1/extract");
    assert_eq!(plan.body["urls"][0], "https://example.com/docs");
    assert_eq!(plan.body["max_pages"], 1);
    assert_eq!(plan.body["embed"], false);
}
```

- [ ] **Step 8.2: Run failing test**

Run:

```bash
cargo test -q extract_server_mode_uses_direct_rest_path
```

Expected: FAIL because server mode still plans `AxonRequest` for `/v1/actions`.

- [ ] **Step 8.3: Add direct REST plan type**

Modify `src/cli/server_mode/plan.rs`:

```rust
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ServerRestPlan {
    pub method: &'static str,
    pub path: &'static str,
    pub body: serde_json::Value,
    pub poll_path_template: Option<&'static str>,
}

pub(crate) fn server_rest_plan(cfg: &Config) -> Result<ServerRestPlan, Box<dyn std::error::Error>> {
    match cfg.command {
        CommandKind::Extract => {
            let urls = cli::commands::common::parse_urls(cfg);
            if urls.is_empty() {
                return Err("extract requires at least one URL".into());
            }
            Ok(ServerRestPlan {
                method: "POST",
                path: "/v1/extract",
                body: serde_json::json!({
                    "urls": urls,
                    "prompt": cfg.query,
                    "max_pages": cfg.max_pages,
                    "render_mode": cfg.render_mode,
                    "embed": cfg.embed,
                }),
                poll_path_template: Some("/v1/extract/{id}"),
            })
        }
        _ => Err(format!("{} is not yet mapped to direct REST", cfg.command).into()),
    }
}
```

Repeat the same mapping for `scrape`, `crawl`, `embed`, `ingest`, `sessions`, `screenshot`, `query`, `retrieve`, `sources`, `domains`, `stats`, and `status` as the REST parity tasks land.

- [ ] **Step 8.4: Update runtime dispatch**

Modify `src/cli/server_mode.rs` to call `server_rest_plan`, use `RestClient`, and render route metadata. For `--wait`, poll direct lifecycle paths such as `/v1/extract/{id}`.

The JSON wrapper should include:

```rust
serde_json::json!({
    "route": route_meta,
    "result": result,
})
```

Where `route_meta` comes from `RouteMetadata::server(server_url.as_str())`.

- [ ] **Step 8.5: Run server mode tests**

Run:

```bash
cargo test -q server_mode_tests
cargo check --bin axon
```

Expected: PASS.

- [ ] **Step 8.6: Commit**

```bash
git add src/cli/server_mode.rs src/cli/server_mode/plan.rs src/cli/server_mode/render.rs src/lib.rs src/cli/server_mode_tests.rs
git commit -m "feat(cli): route server mode through direct REST"
```

---

## Task 9: Doctor Capability, Endpoint, and Diagnose Output

**Files:**
- Modify: `src/core/health/doctor.rs`
- Modify: `src/cli/commands/debug.rs` or add doctor diagnose command handler location used by current parser
- Modify: `src/core/config/parse/build_config/command_dispatch.rs`
- Test: existing doctor tests or create sidecar test following current module layout

- [ ] **Step 9.1: Add doctor JSON contract test**

Add a sidecar test next to the doctor module:

```rust
#[test]
fn doctor_json_includes_mode_capabilities_and_remedies() {
    let report = crate::core::health::doctor::DoctorReport::sample_for_tests();
    let json = serde_json::to_value(report).expect("serialize doctor report");

    assert!(json["mode"]["client"].is_string());
    assert!(json["capabilities"].is_array());
    assert!(json["recommendations"].is_array());
    assert!(json["services"]["qdrant"]["effective_url"].is_string());
}
```

- [ ] **Step 9.2: Run failing test**

Run:

```bash
cargo test -q doctor_json_includes_mode_capabilities_and_remedies
```

Expected: FAIL because current doctor report lacks the new shape.

- [ ] **Step 9.3: Add doctor report fields**

Extend the doctor result structs in `src/core/health/doctor.rs`:

```rust
#[derive(Debug, Clone, serde::Serialize)]
pub struct DoctorModeReport {
    pub client: String,
    pub server_url: Option<String>,
    pub route: String,
    pub fallback: bool,
    pub local_runtime: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DoctorCapability {
    pub tier: String,
    pub available: bool,
    pub impact: Vec<String>,
    pub remedies: Vec<String>,
}
```

Use `src/core/endpoints.rs` to report configured and effective service endpoints.

- [ ] **Step 9.4: Add `doctor diagnose` parser**

Update command dispatch so:

```bash
axon doctor diagnose
```

sets `CommandKind::Doctor` plus a `doctor_diagnose: bool` field on config. The handler should run normal doctor first. If LLM is configured, send doctor JSON to the existing headless LLM path with a prompt:

```text
Diagnose this Axon doctor report. Return concise root causes and exact commands/remedies. Do not suggest unrelated changes.
```

If no LLM is configured, print the normal doctor report and add:

```text
LLM diagnosis unavailable: configure AXON_HEADLESS_GEMINI_CMD to enable doctor diagnose.
```

- [ ] **Step 9.5: Run tests**

Run:

```bash
cargo test -q doctor
cargo check --bin axon
```

Expected: PASS.

- [ ] **Step 9.6: Commit**

```bash
git add src/core/health/doctor.rs src/core/config/parse/build_config/command_dispatch.rs src/core/config/types/config.rs
git commit -m "feat(doctor): report routing capabilities and remedies"
```

---

## Task 10: Local Artifact Reconciliation and `axon sync pending`

**Files:**
- Create: `src/services/sync.rs`
- Create: `src/services/sync_tests.rs`
- Create: `src/cli/commands/sync.rs`
- Create: `src/cli/commands/sync_tests.rs`
- Modify: `src/services/mod.rs`
- Modify: `src/cli/commands.rs`
- Modify: `src/core/config/types/enums.rs`
- Modify: `src/core/config/parse/build_config/command_dispatch.rs`

- [ ] **Step 10.1: Add content-hash reconciliation tests**

Create `src/services/sync_tests.rs`:

```rust
use super::sync::{SyncDecision, decide_sync};

#[test]
fn same_url_same_hash_marks_synced_without_upload() {
    let decision = decide_sync(
        "https://example.com/a",
        "hash-1",
        Some(("https://example.com/a", "hash-1")),
    );

    assert_eq!(decision, SyncDecision::AlreadySynced);
}

#[test]
fn same_url_different_hash_uploads_revision() {
    let decision = decide_sync(
        "https://example.com/a",
        "hash-2",
        Some(("https://example.com/a", "hash-1")),
    );

    assert_eq!(decision, SyncDecision::UploadRevision);
}
```

- [ ] **Step 10.2: Run failing test**

Run:

```bash
cargo test -q sync_tests
```

Expected: FAIL because `services::sync` does not exist.

- [ ] **Step 10.3: Add sync decision service**

Create `src/services/sync.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncDecision {
    AlreadySynced,
    UploadRevision,
    UploadNewSource,
}

pub fn decide_sync(
    local_url: &str,
    local_hash: &str,
    existing: Option<(&str, &str)>,
) -> SyncDecision {
    match existing {
        Some((url, hash)) if url == local_url && hash == local_hash => SyncDecision::AlreadySynced,
        Some((url, _hash)) if url == local_url => SyncDecision::UploadRevision,
        Some((_url, hash)) if hash == local_hash => SyncDecision::UploadNewSource,
        _ => SyncDecision::UploadNewSource,
    }
}
```

Modify `src/services/mod.rs`:

```rust
pub mod sync;
```

- [ ] **Step 10.4: Add `sync pending` CLI**

Add `CommandKind::Sync` in `src/core/config/types/enums.rs`:

```rust
Sync,
```

and `as_str()`:

```rust
Self::Sync => "sync",
```

Create `src/cli/commands/sync.rs`:

```rust
use crate::cli::commands::CommandFuture;
use crate::core::config::Config;
use crate::services::context::ServiceContext;

pub fn run_sync<'a>(cfg: &'a Config, _ctx: &'a ServiceContext) -> CommandFuture<'a> {
    Box::pin(async move {
        let subcommand = cfg.positional.first().map(String::as_str).unwrap_or("pending");
        if subcommand != "pending" {
            return Err(format!("unknown sync subcommand: {subcommand}").into());
        }
        if cfg.json_output {
            println!("{}", serde_json::json!({ "synced": 0, "pending": 0 }));
        } else {
            println!("Sync pending: 0 synced, 0 pending");
        }
        Ok(())
    })
}
```

Wire `run_sync` into the command dispatch branch in `src/lib.rs`:

```rust
CommandKind::Sync => cli::commands::sync::run_sync(cfg, service_context).await,
```

Export the command module in `src/cli/commands.rs`:

```rust
pub mod sync;
```

- [ ] **Step 10.5: Run tests**

Run:

```bash
cargo test -q sync_tests
cargo check --bin axon
```

Expected: PASS.

- [ ] **Step 10.6: Commit**

```bash
git add src/services/sync.rs src/services/sync_tests.rs src/cli/commands/sync.rs src/core/config/types/enums.rs src/core/config/parse/build_config/command_dispatch.rs src/services/mod.rs
git commit -m "feat(sync): add local artifact reconciliation foundation"
```

---

## Task 11: Stdio MCP Thin Client

**Files:**
- Create: `src/mcp/thin_client.rs`
- Create: `src/mcp/thin_client_tests.rs`
- Modify: `src/mcp/server.rs`
- Modify: `src/mcp.rs`
- Test: `src/mcp/thin_client_tests.rs`

- [ ] **Step 11.1: Add thin-client decision test**

Create `src/mcp/thin_client_tests.rs`:

```rust
use super::thin_client::should_use_mcp_thin_client;
use crate::core::config::Config;

#[test]
fn mcp_uses_thin_client_when_server_url_is_set() {
    let mut cfg = Config::default_minimal();
    cfg.server_url = Some(reqwest::Url::parse("http://127.0.0.1:8001").unwrap());
    cfg.local_mode = false;

    assert!(should_use_mcp_thin_client(&cfg));
}

#[test]
fn mcp_stays_local_when_local_mode_is_forced() {
    let mut cfg = Config::default_minimal();
    cfg.server_url = Some(reqwest::Url::parse("http://127.0.0.1:8001").unwrap());
    cfg.local_mode = true;

    assert!(!should_use_mcp_thin_client(&cfg));
}
```

- [ ] **Step 11.2: Run failing test**

Run:

```bash
cargo test -q thin_client_tests
```

Expected: FAIL because `mcp::thin_client` does not exist.

- [ ] **Step 11.3: Add thin-client module**

Create `src/mcp/thin_client.rs`:

```rust
use crate::core::config::Config;

pub fn should_use_mcp_thin_client(cfg: &Config) -> bool {
    cfg.server_url.is_some() && !cfg.local_mode
}
```

Modify `src/mcp.rs`:

```rust
pub mod thin_client;
```

In `src/mcp/server.rs`, before constructing `ServiceContext::new_with_workers()` for stdio requests, check `should_use_mcp_thin_client(cfg)`. If true, route supported MCP tool calls through `src/cli/rest_client.rs` and include a compact `route_note` in the MCP response JSON.

- [ ] **Step 11.4: Run tests**

Run:

```bash
cargo test -q thin_client_tests
cargo check --bin axon
```

Expected: PASS.

- [ ] **Step 11.5: Commit**

```bash
git add src/mcp/thin_client.rs src/mcp/thin_client_tests.rs src/mcp.rs src/mcp/server.rs
git commit -m "feat(mcp): add stdio thin-client routing"
```

---

## Task 12: Auth and Scope Parity

**Files:**
- Modify: `src/mcp/auth.rs`
- Modify: `src/web/actions.rs` during interim, then remove at cutover
- Modify: `src/web/server/handlers/rest/auth.rs`
- Test: existing auth tests and REST tests

- [ ] **Step 12.1: Add shared scope classification test**

Add a test near the auth module:

```rust
#[test]
fn crawl_submit_is_write_scope_across_surfaces() {
    assert_eq!(
        crate::mcp::auth::scope_for_action("crawl", Some("start")),
        Some("axon:write")
    );
    assert_eq!(
        crate::web::server::handlers::rest::auth::scope_for_rest_route("POST", "/v1/crawl"),
        Some("axon:write")
    );
}
```

- [ ] **Step 12.2: Run failing test**

Run:

```bash
cargo test -q crawl_submit_is_write_scope_across_surfaces
```

Expected: FAIL if scope helpers are not shared or exposed.

- [ ] **Step 12.3: Add shared scope classifier**

Create or update a shared auth scope module:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AxonScope {
    Read,
    Write,
    Admin,
}

impl AxonScope {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Read => "axon:read",
            Self::Write => "axon:write",
            Self::Admin => "axon:admin",
        }
    }
}
```

Map REST and MCP through this one classifier.

- [ ] **Step 12.4: Run auth tests**

Run:

```bash
cargo test -q auth
cargo test -q rest_tests
cargo check --bin axon
```

Expected: PASS.

- [ ] **Step 12.5: Commit**

```bash
git add src/mcp/auth.rs src/web/server/handlers/rest/auth.rs src/web/server/handlers/rest_tests.rs
git commit -m "feat(auth): unify REST and MCP scopes"
```

---

## Task 13: Remove `/v1/actions`

**Files:**
- Delete: `src/web/actions.rs`
- Delete or update: `src/web/actions_tests.rs`
- Modify: `src/web/server/routing.rs`
- Modify: `src/web/server_tests.rs`
- Modify: `src/cli/client.rs`
- Modify: docs mentioning `/v1/actions`
- Test: full CLI/server REST tests

- [ ] **Step 13.1: Add cutover guard test**

Add to `src/web/server_tests.rs`:

```rust
#[tokio::test]
async fn v1_actions_is_not_mounted_after_rest_cutover() {
    let app = test_app().await;
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/v1/actions")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), axum::http::StatusCode::NOT_FOUND);
}
```

Adapt `test_app()` to the existing test helper names in `src/web/server_tests.rs`.

- [ ] **Step 13.2: Run failing test**

Run:

```bash
cargo test -q v1_actions_is_not_mounted_after_rest_cutover
```

Expected: FAIL while `/v1/actions` is still mounted.

- [ ] **Step 13.3: Remove actions router**

In `src/web/server/routing.rs`, remove the merge:

```rust
let v1_actions = super::super::actions::router(service_context, auth_policy.clone());
panel_router.merge(v1_actions)
```

and return only the REST/API/MCP/web routes.

Delete `src/web/actions.rs` and `src/web/actions_tests.rs` after all REST/CLI/MCP routes are green.

- [ ] **Step 13.4: Remove CLI action-envelope client path**

Delete or shrink:

- `src/cli/client.rs`
- `src/cli/server_mode/plan.rs` action-envelope builders
- `src/services/types/client_server.rs`

Delete `/v1/actions`-specific request/response types from `src/services/types/client_server.rs`. Move any generic server version/schema structs still needed by REST clients into `src/services/client_contract.rs`.

- [ ] **Step 13.5: Run cutover test suite**

Run:

```bash
cargo test -q server_mode_tests
cargo test -q rest_tests
cargo test -q mcp
cargo check --bin axon
```

Expected: PASS.

- [ ] **Step 13.6: Commit**

```bash
git add -A src/web src/cli src/services docs
git commit -m "refactor(server-mode): remove legacy v1 actions endpoint"
```

---

## Task 14: End-to-End Smoke Tests and Contract Docs

**Files:**
- Modify: `scripts/test-client-server-mode.sh`
- Modify: `docs/specs/server-mode-capability-tiers.md`
- Modify: `docs/contracts/server-mode-routing-contract.md`
- Modify: `docs/API.md`
- Modify: `docs/MCP.md`

- [ ] **Step 14.1: Add smoke checks**

Extend `scripts/test-client-server-mode.sh` with:

```bash
run_json "extract_wait_json_rest" \
  "$AXON_BIN" extract https://www.rfc-editor.org/rfc/rfc9110.txt \
  --query "Extract title and document type" \
  --extract-mode llm \
  --wait true \
  --json \
  --embed false \
  --render-mode http \
  --server-url "$AXON_SERVER_URL"

jq -e '.route.route == "server" and .result.extract_result.total_items >= 1' "$LAST_JSON"
```

If `run_json` and `LAST_JSON` are not already defined in `scripts/test-client-server-mode.sh`, add:

```bash
LAST_JSON=""
run_json() {
  local name="$1"
  shift
  LAST_JSON="${TMPDIR:-/tmp}/axon-${name}.json"
  "$@" >"$LAST_JSON"
}
```

- [ ] **Step 14.2: Run smoke script**

Run:

```bash
AXON_SERVER_URL=http://127.0.0.1:8001 scripts/test-client-server-mode.sh
```

Expected: PASS when the server is running.

- [ ] **Step 14.3: Update docs**

Update:

- `docs/API.md`: direct REST is canonical; `/v1/actions` removed.
- `docs/MCP.md`: stdio MCP thin-client behavior and local fallback notes.
- `docs/contracts/server-mode-routing-contract.md`: mark implemented sections as current, not draft, only after tests pass.
- `docs/specs/server-mode-capability-tiers.md`: remove implementation-phase items that are complete or move them to a completed section.

- [ ] **Step 14.4: Final verification**

Run:

```bash
cargo fmt --check
cargo check --bin axon
cargo test -q server_mode_tests
cargo test -q rest_tests
cargo test -q route_tests
cargo test -q route_meta_tests
cargo test -q artifacts_tests
cargo test -q endpoints_tests
cargo test -q sync_tests
cargo test -q thin_client_tests
bash -n scripts/axon
docker compose -f docker-compose.yaml -f docker-compose.dev.yaml config --services
```

Expected: all PASS.

- [ ] **Step 14.5: Commit**

```bash
git add scripts/test-client-server-mode.sh docs/API.md docs/MCP.md docs/specs/server-mode-capability-tiers.md docs/contracts/server-mode-routing-contract.md
git commit -m "docs(server-mode): document REST cutover and fallback contract"
```

---

## Self-Review Checklist

- [ ] Spec coverage: every section in `docs/specs/server-mode-capability-tiers.md` maps to a task above.
- [ ] Contract coverage: every MUST in `docs/contracts/server-mode-routing-contract.md` maps to a task above.
- [ ] No `/v1/actions` compatibility path remains after Task 13.
- [ ] REST, CLI, and MCP map to canonical service request types.
- [ ] `--wait` in server mode never starts local workers.
- [ ] Fallback-local output is informational on equivalent success and warning-like only for degraded/failed outcomes.
- [ ] Doctor reports effective host-reachable endpoints and remedies.
- [ ] Artifact output has stable handles, not raw paths as the only retrieval mechanism.
- [ ] Local fallback artifacts can sync automatically and via `axon sync pending`.
- [ ] Auth/scope parity is tested for REST and MCP HTTP.
