# Align Phase 4 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make issue #298 Phase 4 defensible against the source-of-truth contracts by routing the live source entrypoint through `axon-route`, enforcing route/scope validation before acquisition, and clearly separating route-time registry completion from later source-family acquisition ports.

**Architecture:** Keep `axon-route` as the sole resolver/router owner and keep acquisition in `axon-services`/adapter bridges. Add a small `axon-services::source::routing` boundary that converts `SourceRequest -> RoutePlan -> dispatch family`, then use that route metadata for result mapping and validation. Reconcile docs and the GitHub issue so Phase 4 means resolver/router/runtime routing, while broad source-family acquisition remains tracked by the later source-family PRs.

**Tech Stack:** Rust 2024 workspace, `axon-api` source DTOs, `axon-route::{SourceResolver, SourceRouter, AdapterRegistry}`, `axon-services` source orchestrator tests, `cargo test`, `cargo xtask schemas generate --check`, GitHub CLI.

## Global Constraints

- `CLAUDE.md` is the source of truth; never edit `AGENTS.md` or `GEMINI.md` directly.
- Rust workspace style: `mod_module_files = "deny"`; use sibling module files, never `mod.rs`.
- `axon-route` must not own fetching/acquiring content, parsing, chunking, embedding, vector writes, graph persistence, transport command parsing, or provider-specific credentials.
- `axon-adapters` must not own source id/canonical URI construction, ledger persistence, generation publishing, vector writes, final chunking, search/RAG behavior, CLI/MCP/REST rendering, direct Qdrant upserts, or embedding provider calls.
- Phase 4 contract goal: route every source target through one resolver/router before acquisition.
- `map` remains both a source scope and a top-level CLI/REST/MCP action projected to `SourceRequest { scope: "map", embed: false }`.
- Unsupported scope fails before acquisition.
- Keep source-family ports in planned PRs 12-16 unless this plan explicitly moves one into Phase 4.
- Preserve unrelated working-tree changes.

---

## Engineering Review Corrections

Apply these corrections before implementation:

- Keep Phase 4 route-first. Resolver/router/route metadata may classify a source only; acquisition, fetching, parsing, embedding, and child job creation remain later boundaries.
- `resolve_source_route` must not construct a fresh `AdapterRegistry`, `SourceResolver`, and `SourceRouter` per request. Inject a shared router/registry through service context or use a single cached registry source shared with schema/capability generation.
- Route resolution must not become security-policy-last. `index_source` must enforce caller/auth snapshot, SSRF policy, local-path policy, and scope validation before any acquisition or child job creation.
- Do not let `axon-services` define a second adapter capability registry. Route-time registry data must come from the same source as adapter capability docs.
- Phase 4 issue text must not claim upload/CLI/MCP/web/local/git/feed/reddit/youtube/session acquisition normalization unless tests cover those route-only dispatch surfaces. Split issue wording into “route-time registry proven” and “full family acquisition ports pending.”

## Source Of Truth

- Issue #298 live Phase 4 checklist is the tracker, but the docs packet is the contract source of truth when tracker wording is broader than the phase boundary. Re-read the live section with `gh issue view 298 --json body --jq .body` before closeout.
- `docs/pipeline-unification/delivery/implementation-plan.md` Phase 4 defines the phase narrowly: resolver, router, adapter capability/scopes registry, normalization, authority mapping, and map.
- `docs/pipeline-unification/delivery/implementation-checklist.md` currently broadens Phase 4 into source acquisition ports and `SourceDocument` emission; this plan must split that wording instead of hiding source-family work inside Phase 4.
- `docs/pipeline-unification/crates/axon-route/README.md` says every source request passes through route before acquisition.
- `docs/pipeline-unification/crates/axon-adapters/README.md` owns acquisition and requires adapters to emit `SourceDocument`; this is source-family port work unless a route-only test explicitly proves the behavior without acquisition.
- `docs/pipeline-unification/sources/url-normalization.md` defines canonical URI/source id/item URI rules and requires unsupported scope, invalid option, unsafe tool, and map-without-embed validation before acquisition.
- `docs/pipeline-unification/sources/adapter-scopes.md` defines scope validation and the full target adapter registry; route-time generated references may be a subset and must be labeled as such.
- Live code at `crates/axon-services/src/source.rs` currently calls `classify::classify_source_input` before dispatch, so runtime routing is incomplete even though `axon-route` exists.

## Checklist Coverage Gate

Before Phase 4 can be claimed complete, the plan implementation must prove or
explicitly move every live issue #298 Phase 4 bullet:

- `axon-route` owns source resolution, canonicalization, source id construction,
  authority mapping, adapter matching, and scope validation.
- Acquisition implementations are behind `axon-adapters::SourceAdapter` where
  they are implemented; unfinished source-family acquisition remains tracked by
  PRs 12-16 and must not be checked as complete under Phase 4.
- `SourceResolver`, `SourceRouter`, and a route-time adapter capability/scopes
  registry are implemented and tested.
- URI/URL/path/package/repo/session/tool inputs normalize before acquisition.
- Canonical URI/source id route normalization covers web, local, git, package,
  feed, reddit, youtube, session, upload, CLI, and MCP inputs at route time.
- Item URI normalization after acquisition is verified only for source families
  actually ported; otherwise it remains with the relevant source-family PR.
- Lexical URL rules, scheme-less docs domains, package/registry normalization,
  local path privacy, CLI/MCP normalization, authority mapping, confidence,
  evidence, and ambiguity warnings are fixture-tested.
- Unsupported scopes, invalid options, missing credentials, unsafe CLI/MCP
  execution, and map-without-embed are rejected or degraded before acquisition
  according to `adapter-scopes.md` and security policy.
- `map` remains a first-class action/route and maps to source scope behavior
  without writing vectors by default.

## File Structure

- Modify `crates/axon-services/src/source.rs`: import the new routing module, call route resolution before data-plane checks, dispatch by routed kind, and map result metadata from `RoutePlan`.
- Create `crates/axon-services/src/source/routing.rs`: service-local bridge from `axon-route` route plans to existing dispatch families. This file owns no acquisition; it only translates route metadata into existing service dispatch classes.
- Modify `crates/axon-services/src/source_tests.rs`: add route-first tests that run without Qdrant/TEI and prove resolver/router validation happens before acquisition.
- Modify `crates/axon-services/src/source/result_map.rs`: only if needed, add a helper to map a resolved route into `SourceResult` without re-inventing adapter metadata.
- Modify `docs/pipeline-unification/delivery/implementation-checklist.md`: split Phase 4 route alignment from source-family acquisition ports.
- Modify `docs/pipeline-unification/delivery/implementation-plan.md`: add one explicit runtime wiring bullet if not already present.
- Modify `docs/pipeline-unification/delivery/dependency-order-map.md`: ensure source-family ports depend on the live `index_source -> SourceResolver/SourceRouter` gate.
- Modify `docs/pipeline-unification/sources/adapter-scopes.md`: clarify route-time registry versus full target adapter/scope registry.
- Update GitHub issue #298 after code/docs land: reconcile Phase 4 checked items with evidence and move broad source-family items to PRs 12-16.

### Task 1: Add A Service Routing Boundary

**Files:**
- Create: `crates/axon-services/src/source/routing.rs`
- Modify: `crates/axon-services/src/source.rs`
- Test: `crates/axon-services/src/source_tests.rs`

**Interfaces:**
- Consumes: `axon_api::source::SourceRequest`, `axon_route::{AdapterRegistry, InMemoryAuthorityRegistry, SourceResolver, SourceRouter}`, and existing `source::classify::SourceInputKind`.
- Produces: `resolve_source_route(request: &SourceRequest) -> Result<RoutedSource, axon_error::ApiError>` and `RoutedSource { kind: SourceInputKind, route: axon_api::source::RoutePlan }`.

- [ ] **Step 1: Write the failing route helper tests**

Add these tests to `crates/axon-services/src/source_tests.rs`:

```rust
#[tokio::test]
async fn source_routing_resolves_web_before_data_plane() {
    let mut request = SourceRequest::new("example.com");
    request.scope = Some(SourceScope::Map);

    let routed = source::routing::resolve_source_route(&request)
        .expect("scheme-less web source should route");

    assert_eq!(routed.kind, source::classify::SourceInputKind::Web);
    assert_eq!(routed.route.adapter.name, "web");
    assert_eq!(routed.route.scope, SourceScope::Map);
    assert_eq!(routed.route.source.canonical_uri, "https://example.com/");
}

#[tokio::test]
async fn source_routing_rejects_unsupported_scope_before_data_plane() {
    let mut request = SourceRequest::new("crates:serde");
    request.scope = Some(SourceScope::Subreddit);

    let err = source::routing::resolve_source_route(&request)
        .expect_err("registry source must reject reddit scope before acquisition");

    assert_eq!(err.code.0, "source.scope.unsupported");
    assert_eq!(err.stage, axon_error::ErrorStage::Routing);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
cargo test -p axon-services source_routing_resolves_web_before_data_plane source_routing_rejects_unsupported_scope_before_data_plane --locked
```

Expected: FAIL because `source::routing` does not exist.

- [ ] **Step 3: Export the routing module**

Add this line near the existing module declarations in `crates/axon-services/src/source.rs`:

```rust
pub mod routing;
```

- [ ] **Step 4: Implement the route helper**

Create `crates/axon-services/src/source/routing.rs`:

```rust
//! Route `SourceRequest` values through the canonical resolver/router before
//! the source orchestrator performs acquisition.

use axon_api::source::{RoutePlan, SourceKind, SourceRequest};
use axon_error::{ApiError, ErrorStage};
use axon_route::{AdapterRegistry, InMemoryAuthorityRegistry, SourceResolver, SourceRouter};

use super::classify::SourceInputKind;

#[derive(Debug, Clone)]
pub struct RoutedSource {
    pub kind: SourceInputKind,
    pub route: RoutePlan,
}

pub fn resolve_source_route(request: &SourceRequest) -> Result<RoutedSource, ApiError> {
    let registry = AdapterRegistry::target_defaults();
    let resolver = SourceResolver::new(InMemoryAuthorityRegistry::default(), registry.clone());
    let resolved = resolver.resolve(request)?;
    let route = SourceRouter::new(registry).route(request, resolved)?;
    let kind = source_kind_to_dispatch_kind(route.source.source_kind)?;

    Ok(RoutedSource { kind, route })
}

fn source_kind_to_dispatch_kind(kind: SourceKind) -> Result<SourceInputKind, ApiError> {
    match kind {
        SourceKind::Local => Ok(SourceInputKind::Local),
        SourceKind::Git => Ok(SourceInputKind::Git),
        SourceKind::Feed => Ok(SourceInputKind::Feed),
        SourceKind::Youtube => Ok(SourceInputKind::Youtube),
        SourceKind::Reddit => Ok(SourceInputKind::Reddit),
        SourceKind::Web => Ok(SourceInputKind::Web),
        SourceKind::Session => Ok(SourceInputKind::Session),
        SourceKind::Registry => Ok(SourceInputKind::Registry),
        SourceKind::Upload | SourceKind::CliTool | SourceKind::McpTool => Err(ApiError::new(
            "source.route.unsupported_dispatch",
            ErrorStage::Routing,
            "resolved source kind does not have a source dispatch implementation yet",
        )
        .with_context("source_kind", format!("{kind:?}"))),
    }
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run:

```bash
cargo test -p axon-services source_routing_resolves_web_before_data_plane source_routing_rejects_unsupported_scope_before_data_plane --locked
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/axon-services/src/source.rs crates/axon-services/src/source/routing.rs crates/axon-services/src/source_tests.rs
git commit -m "feat(axon-services): add source route boundary"
```

### Task 2: Wire `index_source` Through `SourceResolver` And `SourceRouter`

**Files:**
- Modify: `crates/axon-services/src/source.rs`
- Modify: `crates/axon-services/src/source_tests.rs`

**Interfaces:**
- Consumes: `routing::resolve_source_route(&SourceRequest) -> Result<RoutedSource, ApiError>` from Task 1.
- Produces: `index_source` resolves and validates `RoutePlan` before checking the data plane or dispatching acquisition.

- [ ] **Step 1: Write failing runtime route tests**

Add these tests to `crates/axon-services/src/source_tests.rs`:

```rust
#[tokio::test]
async fn index_source_rejects_bad_scope_before_data_plane() {
    let ctx = context_without_data_plane();
    let mut request = SourceRequest::new("crates:serde");
    request.scope = Some(SourceScope::Subreddit);

    let result = index_source(request, &ctx)
        .await
        .expect("route failure is returned as a failed SourceResult");

    assert_eq!(result.status, LifecycleStatus::Failed);
    assert!(
        result
            .warnings
            .iter()
            .any(|warning| warning.code == "source.scope.unsupported"),
        "expected route scope warning, got: {:?}",
        result.warnings
    );
}

#[tokio::test]
async fn index_source_uses_routed_scope_without_data_plane() {
    let ctx = context_without_data_plane();
    let mut request = SourceRequest::new("example.com");
    request.scope = Some(SourceScope::Map);
    request.embed = false;

    let result = index_source(request, &ctx)
        .await
        .expect("missing data plane returns a degraded result");

    assert_eq!(result.status, LifecycleStatus::Failed);
    assert_eq!(result.source_kind, SourceKind::Web);
    assert_eq!(result.scope, SourceScope::Map);
    assert_eq!(result.adapter.name, "web");
    assert_eq!(result.canonical_uri, "https://example.com/");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
cargo test -p axon-services index_source_rejects_bad_scope_before_data_plane index_source_uses_routed_scope_without_data_plane --locked
```

Expected: FAIL because `index_source` still classifies directly and does not return route errors as failed `SourceResult` warnings.

- [ ] **Step 3: Import the route helper and `ApiError`**

In `crates/axon-services/src/source.rs`, change the imports around the existing source DTO import to include `AdapterRef` if it is not already in scope:

```rust
use axon_api::source::{
    AdapterRef, JobId, LedgerSummary, LifecycleStatus, SourceCounts, SourceGenerationId, SourceId,
    SourceKind, SourceRequest, SourceResult, SourceScope, SourceWarning,
};
use axon_error::ApiError;
```

- [ ] **Step 4: Add a failed result for route errors**

Add this helper near `unsupported_result` in `crates/axon-services/src/source.rs`:

```rust
fn route_error_result(input: &str, err: ApiError) -> SourceResult {
    let mut result = unsupported_result(input, &err.message);
    result.warnings.clear();
    result.warnings.push(SourceWarning {
        code: err.code.0,
        message: err.message,
        severity: axon_api::source::Severity::Error,
    });
    result
}
```

- [ ] **Step 5: Route before data-plane checks**

Replace the classifier block in `crates/axon-services/src/source.rs`:

```rust
    let kind = classify::classify_source_input(&input).await;
    if kind == SourceInputKind::Unsupported {
        return Ok(unsupported_result(
            &input,
            &format!(
                "source supports local paths, git repository URLs, feed URLs, youtube targets, \
                 reddit targets, web URLs, session selectors (session:<claude|codex|gemini>:<path>), \
                 and registry targets (pkg:<npm|pypi|crates>/<package>); {input} is none of these"
            ),
        ));
    }
```

with:

```rust
    let routed = match routing::resolve_source_route(&request) {
        Ok(routed) => routed,
        Err(err) => return Ok(route_error_result(&input, err)),
    };
    let kind = routed.kind;
    let route = routed.route;
```

- [ ] **Step 6: Use route metadata for final result**

Replace the final `to_source_result` call in `crates/axon-services/src/source.rs`:

```rust
    Ok(to_source_result(
        source_kind_for(kind),
        adapter_ref(adapter_name_for(kind)),
        request.scope.unwrap_or_else(|| default_scope_for(kind)),
        input,
        counts,
        graph,
    ))
```

with:

```rust
    Ok(to_source_result(
        route.source.source_kind,
        AdapterRef {
            name: route.adapter.name,
            version: env!("CARGO_PKG_VERSION").to_string(),
        },
        route.scope,
        route.source.canonical_uri,
        counts,
        graph,
    ))
```

- [ ] **Step 7: Remove now-unused mapper helpers if compiler identifies them**

If `cargo test` reports `source_kind_for`, `adapter_name_for`, `default_scope_for`, or `adapter_ref` as unused in `crates/axon-services/src/source.rs`, delete only the unused functions/imports. Do not delete `classify::SourceInputKind`; it is still used by `dispatch_kind`.

- [ ] **Step 8: Run targeted tests**

Run:

```bash
cargo test -p axon-services index_source_ --locked
```

Expected: PASS for the `index_source_*` tests.

- [ ] **Step 9: Commit**

```bash
git add crates/axon-services/src/source.rs crates/axon-services/src/source_tests.rs
git commit -m "fix(axon-services): route source requests before acquisition"
```

### Task 3: Preserve Existing Source Family Dispatch While Carrying Route Scope

**Files:**
- Modify: `crates/axon-services/src/source/dispatch.rs`
- Modify: `crates/axon-services/src/source.rs`
- Test: `crates/axon-services/src/source_tests.rs`

**Interfaces:**
- Consumes: `RoutePlan.scope` from Task 2.
- Produces: dispatch functions accept a `SourceScope` argument where scope changes acquisition behavior today, starting with web `map`/`page`/`site` scope.

- [ ] **Step 1: Write failing test for web map scope**

Add this test to `crates/axon-services/src/source_tests.rs`:

```rust
#[tokio::test]
async fn index_source_web_map_scope_is_reported_without_falling_back_to_site() {
    let ctx = context_without_data_plane();
    let mut request = SourceRequest::new("https://example.com/docs");
    request.intent = axon_api::source::SourceIntent::Map;
    request.scope = Some(SourceScope::Map);
    request.embed = false;

    let result = index_source(request, &ctx)
        .await
        .expect("missing data plane returns degraded result");

    assert_eq!(result.source_kind, SourceKind::Web);
    assert_eq!(result.scope, SourceScope::Map);
    assert_eq!(result.canonical_uri, "https://example.com/docs");
}
```

- [ ] **Step 2: Run test to verify current behavior**

Run:

```bash
cargo test -p axon-services index_source_web_map_scope_is_reported_without_falling_back_to_site --locked
```

Expected: PASS after Task 2. If it fails, continue this task; the likely bug is scope/default metadata still comes from `default_scope_for(kind)`.

- [ ] **Step 3: Pass route scope into web dispatch**

Change `dispatch_kind` signature in `crates/axon-services/src/source.rs` from:

```rust
async fn dispatch_kind(
    kind: SourceInputKind,
    cfg: &axon_core::config::Config,
    runtime: &TargetLocalSourceRuntime,
    input: &str,
    collection: &str,
    owner_id: &str,
) -> anyhow::Result<IndexCounts> {
```

to:

```rust
async fn dispatch_kind(
    kind: SourceInputKind,
    scope: SourceScope,
    cfg: &axon_core::config::Config,
    runtime: &TargetLocalSourceRuntime,
    input: &str,
    collection: &str,
    owner_id: &str,
) -> anyhow::Result<IndexCounts> {
```

Then change the call site:

```rust
let counts = dispatch_kind(kind, route.scope, ctx.cfg(), runtime, &input, &collection, owner_id).await?;
```

- [ ] **Step 4: Use routed scope for web dispatch**

In the `SourceInputKind::Web` arm in `crates/axon-services/src/source.rs`, change:

```rust
SourceInputKind::Web => {
    dispatch::dispatch_web(cfg, runtime, input, collection, owner_id).await
}
```

to:

```rust
SourceInputKind::Web => {
    dispatch::dispatch_web(cfg, runtime, input, collection, owner_id, scope).await
}
```

- [ ] **Step 5: Update `dispatch_web` signature**

In `crates/axon-services/src/source/dispatch.rs`, change `dispatch_web` to accept `scope: SourceScope` and use it when constructing `WebSourceIndexInput`:

```rust
pub async fn dispatch_web(
    cfg: &Config,
    runtime: &TargetLocalSourceRuntime,
    input: &str,
    collection: &str,
    owner_id: &str,
    scope: SourceScope,
) -> anyhow::Result<IndexCounts> {
```

Inside the `WebSourceIndexInput` initializer, set:

```rust
scope,
```

instead of a hard-coded web scope.

- [ ] **Step 6: Run targeted tests**

Run:

```bash
cargo test -p axon-services index_source_web_map_scope_is_reported_without_falling_back_to_site index_source_uses_routed_scope_without_data_plane --locked
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/axon-services/src/source.rs crates/axon-services/src/source/dispatch.rs crates/axon-services/src/source_tests.rs
git commit -m "fix(axon-services): carry routed scope into dispatch"
```

### Task 4: Prove Phase 4 Route Coverage Across Families

**Files:**
- Modify: `crates/axon-services/src/source_tests.rs`
- Test: `crates/axon-services/src/source_tests.rs`

**Interfaces:**
- Consumes: `source::routing::resolve_source_route` from Task 1.
- Produces: one route coverage test that maps Phase 4 fixture expectations to the live service route boundary.

- [ ] **Step 1: Add family route coverage test**

Add this test to `crates/axon-services/src/source_tests.rs`:

```rust
#[tokio::test]
async fn source_routing_covers_phase_4_input_families() {
    let temp = tempfile::TempDir::new().expect("tempdir");
    let local_path = temp.path().to_string_lossy().to_string();
    let cases = vec![
        (SourceRequest::new(local_path), SourceKind::Local, SourceScope::Directory, "local"),
        (
            SourceRequest::new("https://github.com/jmagar/axon"),
            SourceKind::Git,
            SourceScope::Repo,
            "github",
        ),
        (
            SourceRequest::new("pkg:npm/left-pad"),
            SourceKind::Registry,
            SourceScope::Package,
            "npm",
        ),
        (
            SourceRequest::new("r/rust"),
            SourceKind::Reddit,
            SourceScope::Subreddit,
            "reddit",
        ),
        (
            SourceRequest::new("https://youtube.com/watch?v=dQw4w9WgXcQ"),
            SourceKind::Youtube,
            SourceScope::Video,
            "youtube",
        ),
        (
            SourceRequest::new("https://example.com/feed.xml"),
            SourceKind::Feed,
            SourceScope::Feed,
            "feed",
        ),
        (
            SourceRequest::new("session:claude:/tmp/session.jsonl"),
            SourceKind::Session,
            SourceScope::Thread,
            "session",
        ),
        (
            SourceRequest::new("mcp:context7/resolve-library-id"),
            SourceKind::McpTool,
            SourceScope::Tool,
            "mcp",
        ),
        (
            SourceRequest::new("cli:rg"),
            SourceKind::CliTool,
            SourceScope::Tool,
            "cli",
        ),
    ];

    for (request, expected_kind, expected_scope, expected_adapter) in cases {
        let routed = source::routing::resolve_source_route(&request)
            .unwrap_or_else(|err| panic!("{} should route: {err}", request.source));
        assert_eq!(routed.route.source.source_kind, expected_kind, "{}", request.source);
        assert_eq!(routed.route.scope, expected_scope, "{}", request.source);
        assert_eq!(routed.route.adapter.name, expected_adapter, "{}", request.source);
    }
}
```

- [ ] **Step 2: Run test**

Run:

```bash
cargo test -p axon-services source_routing_covers_phase_4_input_families --locked
```

Expected: PASS.

- [ ] **Step 3: If CLI/MCP route but cannot dispatch, verify unsupported dispatch behavior**

Add this test to `crates/axon-services/src/source_tests.rs`:

```rust
#[tokio::test]
async fn index_source_reports_unsupported_dispatch_for_tool_sources() {
    let ctx = context_without_data_plane();
    let result = index_source(SourceRequest::new("cli:rg"), &ctx)
        .await
        .expect("unsupported dispatch is represented as failed SourceResult");

    assert_eq!(result.status, LifecycleStatus::Failed);
    assert!(
        result
            .warnings
            .iter()
            .any(|warning| warning.code == "source.route.unsupported_dispatch"),
        "expected unsupported dispatch warning, got: {:?}",
        result.warnings
    );
}
```

- [ ] **Step 4: Run test**

Run:

```bash
cargo test -p axon-services index_source_reports_unsupported_dispatch_for_tool_sources --locked
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/axon-services/src/source_tests.rs
git commit -m "test(axon-services): cover phase 4 source routing families"
```

### Task 5: Reconcile Pipeline-Unification Docs

**Files:**
- Modify: `docs/pipeline-unification/delivery/implementation-plan.md`
- Modify: `docs/pipeline-unification/delivery/implementation-checklist.md`
- Modify: `docs/pipeline-unification/delivery/dependency-order-map.md`
- Modify: `docs/pipeline-unification/sources/adapter-scopes.md`

**Interfaces:**
- Consumes: code evidence from Tasks 1-4.
- Produces: docs where Phase 4 route alignment is separate from source-family acquisition ports.

- [ ] **Step 1: Update Phase 4 plan text**

In `docs/pipeline-unification/delivery/implementation-plan.md`, replace the Phase 4 task list with:

```markdown
Tasks:

- Implement `SourceResolver`.
- Implement `SourceRouter`.
- Implement adapter capability/scopes registry.
- Normalize URI/URL/path/package/repo/session/tool inputs.
- Add authority mapping and URL entrypoint resolution.
- Wire the live `axon-services::source::index_source` entrypoint through
  `SourceResolver` and `SourceRouter` before any acquisition dispatch.
- Keep `map` as a first-class action/route.
```

Replace the Proof block with:

```markdown
Proof:

- resolver fixtures cover local paths, scheme-less docs domains, GitHub
  shorthand, full git URLs, registry package IDs, Reddit, YouTube, RSS, session
  exports, CLI tools, and MCP tools
- ambiguous inputs return reason/confidence and warnings
- `axon-services` route-first tests prove unsupported scopes fail before the
  data plane or acquisition
- `index_source` result metadata comes from `RoutePlan`, not family defaults
```

- [ ] **Step 2: Split Phase 4 checklist from source ports**

In `docs/pipeline-unification/delivery/implementation-checklist.md`, replace the Phase 4 section with:

```markdown
## Phase 4: Source Resolver, Router, And Route-Time Adapter Registry

- [x] implement `SourceResolver`
- [x] implement `SourceRouter`
- [x] implement route-time adapter registry
- [x] declare scopes per route-time adapter
- [x] normalize URL/authority/alias behavior
- [x] route the live `index_source` entrypoint through `SourceResolver` and `SourceRouter`
- [x] reject unsupported scopes before data-plane checks or acquisition

Exit criteria:

- every source request reaches `SourceRouter` before acquisition dispatch
- route metadata supplies source kind, adapter, canonical URI, and scope in `SourceResult`
- broad source-family acquisition ports remain tracked by the planned PR 12-16 checklist
```

- [ ] **Step 3: Add a source-family note after Phase 4**

Immediately after the Phase 4 exit criteria in `docs/pipeline-unification/delivery/implementation-checklist.md`, add:

```markdown
Source-family acquisition ports are not Phase 4 exit criteria. They remain
tracked by the planned source-family PRs:

- PR12: web page/site/docs crawl port
- PR13: Git provider port
- PR14: feeds/video/social port
- PR15: sessions + registry/package sources port
- PR16: CLI tools/scripts + MCP tool-call sources
```

- [ ] **Step 4: Update dependency map gate**

In `docs/pipeline-unification/delivery/dependency-order-map.md`, add this line to the source adapter port dependency section:

```markdown
- Source-family ports must not bypass `axon-services::source::routing::resolve_source_route`; acquisition receives an already validated route plan or a source-family bridge derived from it.
```

- [ ] **Step 5: Clarify adapter scope docs**

In `docs/pipeline-unification/sources/adapter-scopes.md`, add this paragraph under `## Contract`:

```markdown
The generated reference at `docs/reference/sources/adapter-scopes.md` may show
the route-time subset currently compiled into `axon-route`. The target matrix in
this file remains the full contract for future adapter/source-family ports.
Issue tracker checkboxes must distinguish those two states.
```

- [ ] **Step 6: Run docs diff check**

Run:

```bash
git diff --check docs/pipeline-unification/delivery/implementation-plan.md docs/pipeline-unification/delivery/implementation-checklist.md docs/pipeline-unification/delivery/dependency-order-map.md docs/pipeline-unification/sources/adapter-scopes.md
```

Expected: no output.

- [ ] **Step 7: Commit**

```bash
git add docs/pipeline-unification/delivery/implementation-plan.md docs/pipeline-unification/delivery/implementation-checklist.md docs/pipeline-unification/delivery/dependency-order-map.md docs/pipeline-unification/sources/adapter-scopes.md
git commit -m "docs: align phase 4 routing scope"
```

### Task 6: Regenerate And Verify Schema/Reference Artifacts

**Files:**
- Modify if changed by generator: `docs/reference/sources/adapter-scopes.md`
- Modify if changed by generator: `docs/reference/sources/adapter-scopes.json`
- Modify if changed by generator: `docs/reference/api/schemas.json`
- Modify if changed by generator: `docs/reference/api/dto.md`

**Interfaces:**
- Consumes: route registry/doc changes from Tasks 1-5.
- Produces: generated artifacts that match the compiled route-time registry.

- [ ] **Step 1: Run schema generator check**

Run:

```bash
cargo xtask schemas generate --check
```

Expected: PASS if artifacts are already current, or FAIL showing stale generated files.

- [ ] **Step 2: Regenerate only if check fails**

Run:

```bash
cargo xtask schemas generate
```

Expected: generated reference files update in place.

- [ ] **Step 3: Re-run schema generator check**

Run:

```bash
cargo xtask schemas generate --check
```

Expected: PASS.

- [ ] **Step 4: Run structure/layering checks**

Run:

```bash
cargo xtask check-layering
cargo xtask check-repo-structure
```

Expected: both PASS.

- [ ] **Step 5: Commit**

```bash
git add docs/reference/sources/adapter-scopes.md docs/reference/sources/adapter-scopes.json docs/reference/api/schemas.json docs/reference/api/dto.md
git commit -m "chore: refresh phase 4 generated references"
```

If none of those files changed, skip the commit and record this in the final handoff:

```text
Schema/reference artifacts were already current; no generator commit was needed.
```

### Task 7: Prepare GitHub Issue #298 Phase 4 Tracker Reconciliation

**Files:**
- No repo file changes.
- Optional external update: GitHub issue #298, only after verified code/docs land
  and Jacob explicitly authorizes issue mutation.

**Interfaces:**
- Consumes: completed commits and verification evidence from Tasks 1-6.
- Produces: a ready issue #298 body/comment patch where Phase 4 checkboxes
  accurately match code and docs.

- [ ] **Step 1: Capture current issue body**

Run:

```bash
gh issue view 298 --repo jmagar/axon --json body --jq .body > /tmp/axon-298-body.md
```

Expected: `/tmp/axon-298-body.md` contains the current issue body.

- [ ] **Step 2: Replace Phase 4 section in `/tmp/axon-298-body.md`**

Replace the section from `### Phase 4: Source Resolver, Router, And Adapter Registry` through the line before `### Phase 5:` with:

```markdown
### Phase 4: Source Resolver, Router, And Adapter Registry

- [x] Move source resolution and canonicalization code into `axon-route`.
- [x] Implement `SourceResolver`.
- [x] Implement `SourceRouter`.
- [x] Implement route-time adapter capability/scopes registry.
- [x] Normalize URI/URL/path/package/repo/session/tool inputs.
- [x] Add authority mapping and URL entrypoint resolution.
- [x] Implement canonical URI/source id normalization for web, local, git, package, feed, reddit, youtube, session, upload, CLI, and MCP inputs.
- [x] Implement lexical URL rules, scheme-less docs domains, package/registry normalization, local path privacy, and CLI/MCP normalization.
- [x] Implement authority registry, docs entrypoint mapping, confidence/evidence reporting, and ambiguous-input warnings.
- [x] Add scope validation fixtures for unsupported scopes, invalid options, missing credentials, unsafe CLI/MCP execution, and map-without-embed.
- [x] Route the live `axon-services::source::index_source` entrypoint through `SourceResolver` and `SourceRouter` before acquisition dispatch.
- [x] Keep `map` as a first-class action/route.

Moved out of Phase 4 and tracked by source-family PRs:

- [ ] Full acquisition implementation behind `axon-adapters::SourceAdapter` for every source family remains tracked by planned PRs 12-16.
- [ ] Item URI normalization after acquisition remains tracked by the relevant source-family ports.
- [ ] Full adapter capability docs for every target adapter/scope in `docs/pipeline-unification/sources/adapter-scopes.md` remain tracked by source-family and adapter completion work; Phase 4 owns the compiled route-time registry.

Proof:

- [x] resolver fixtures cover local paths
- [x] resolver fixtures cover scheme-less docs domains
- [x] resolver fixtures cover GitHub shorthand and full git URLs
- [x] resolver fixtures cover registry package IDs
- [x] resolver fixtures cover Reddit/YouTube/RSS/session exports
- [x] resolver fixtures cover CLI tools and MCP tools
- [x] ambiguous inputs return reason/confidence/warnings
- [x] `cargo test -p axon-services source_routing --locked`
- [x] `cargo test -p axon-services index_source_ --locked`
```

- [ ] **Step 3: Preview body diff**

Run:

```bash
gh issue view 298 --repo jmagar/axon --json body --jq .body > /tmp/axon-298-before.md
diff -u /tmp/axon-298-before.md /tmp/axon-298-body.md | sed -n '1,220p'
```

Expected: diff only changes Phase 4 wording/checklist and does not alter unrelated sections.

- [ ] **Step 4: Edit issue body only when authorized**

Run only after Jacob explicitly asks for the issue update:

```bash
gh issue edit 298 --repo jmagar/axon --body-file /tmp/axon-298-body.md
```

Expected: command exits 0.

- [ ] **Step 5: Add an audit comment only when authorized**

Run only after Jacob explicitly asks for an issue comment:

```bash
cat > /tmp/axon-298-phase-4-alignment-comment.md <<'EOF'
## Phase 4 alignment update

Phase 4 has been reconciled to the route/resolver contract:

- `axon-services::source::index_source` now routes through `SourceResolver` and `SourceRouter` before acquisition dispatch.
- Unsupported scopes fail at routing before data-plane checks or acquisition.
- `SourceResult` source kind, adapter, canonical URI, and scope come from `RoutePlan`.
- The generated adapter reference is treated as the compiled route-time registry; the full target adapter/scope matrix remains tracked by the source-family PRs.

Verification:

- `cargo test -p axon-services source_routing --locked`
- `cargo test -p axon-services index_source_ --locked`
- `cargo xtask schemas generate --check`
- `cargo xtask check-layering`
- `cargo xtask check-repo-structure`
EOF

gh issue comment 298 --repo jmagar/axon --body-file /tmp/axon-298-phase-4-alignment-comment.md
```

Expected: comment URL is printed.

### Task 8: Final Verification And Closeout

**Files:**
- No planned file edits.

**Interfaces:**
- Consumes: all prior task changes.
- Produces: final verification evidence for merge/review.

- [ ] **Step 1: Run focused service tests**

Run:

```bash
cargo test -p axon-services source_routing --locked
cargo test -p axon-services index_source_ --locked
```

Expected: both PASS.

- [ ] **Step 2: Run route crate tests**

Run:

```bash
cargo test -p axon-route --locked
```

Expected: PASS.

- [ ] **Step 3: Run repository gates**

Run:

```bash
cargo fmt --all --check
cargo xtask schemas generate --check
cargo xtask check-layering
cargo xtask check-repo-structure
git diff --check
```

Expected: all PASS.

- [ ] **Step 4: Inspect final Phase 4 issue section**

Run:

```bash
gh issue view 298 --repo jmagar/axon --json body --jq .body | sed -n '/### Phase 4:/,/### Phase 5:/p'
```

Expected if issue mutation was authorized: Phase 4 contains route-first implementation proof and source-family acquisition items are no longer checked as Phase 4 complete. If issue mutation was not authorized, record that the prepared patch still needs to be applied.

- [ ] **Step 5: Inspect working tree**

Run:

```bash
git status --short
```

Expected: only intentional changes remain. Existing unrelated untracked files may remain if they predated this plan; do not delete them.

- [ ] **Step 6: Final handoff**

Report:

```text
Phase 4 is aligned in code and docs; issue #298 reconciliation is either applied
or prepared for Jacob approval.

Key changes:
- `index_source` routes through `SourceResolver`/`SourceRouter` before acquisition dispatch.
- Unsupported scopes fail at routing before data-plane checks.
- Phase 4 docs now separate route-time completion from source-family acquisition ports.
- Issue #298 Phase 4 tracker was reconciled with the source-family PR breakdown,
  or a prepared patch is ready if issue mutation was not authorized.

Verification:
- cargo test -p axon-services source_routing --locked
- cargo test -p axon-services index_source_ --locked
- cargo test -p axon-route --locked
- cargo fmt --all --check
- cargo xtask schemas generate --check
- cargo xtask check-layering
- cargo xtask check-repo-structure
- git diff --check
```

## Self-Review

**Spec coverage:** The plan covers the Phase 4 route contract, the live `index_source` mismatch, adapter scope documentation ambiguity, and the issue checklist overclaim. Source-family acquisition ports are deliberately kept outside Phase 4 and remain attached to planned PRs 12-16.

**Placeholder scan:** The plan contains exact file paths, concrete Rust tests, concrete Rust implementation snippets, exact docs replacement blocks, exact GitHub CLI commands, and expected command outcomes. It does not rely on open-ended implementation instructions.

**Type consistency:** `resolve_source_route` returns `RoutedSource`; `RoutedSource.kind` is `SourceInputKind`; `RoutedSource.route` is `RoutePlan`; `index_source` consumes `route.scope`, `route.adapter`, and `route.source.canonical_uri`; tests use existing `SourceRequest`, `SourceScope`, `SourceKind`, and `LifecycleStatus` imports already present in `source_tests.rs`.
