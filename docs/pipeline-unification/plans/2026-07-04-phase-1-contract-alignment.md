# Phase 1 Contract Alignment Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bring issue #298 Phase 1 into exact alignment with the canonical pipeline-unification contracts for shared DTOs, enum/schema coverage, and removed-surface boundaries.

**Architecture:** Keep Phase 1 transport-neutral: all public wire DTOs and enum projections live in `axon-api`, error projections come from `axon-error`, and schema generation consumes Rust-owned DTOs instead of handwritten mirrors. This plan does not move runtime behavior, delete legacy surfaces, or rewire CLI/MCP/REST handlers except where compile fixes are required after DTO shape changes.

**Tech Stack:** Rust 2024, Cargo workspace, serde, schemars, utoipa, `axon-api`, `axon-error`, `xtask` schema generation, generated docs under `docs/reference/**`.

## Global Constraints

- `CLAUDE.md` is the source of truth for agent memory; do not edit `AGENTS.md` or `GEMINI.md` directly.
- Rust workspace style: `mod_module_files = "deny"`; use `foo.rs` sibling to `foo/` dir, never `foo/mod.rs`.
- Phase 1 scope is transport-neutral DTO/enum/schema alignment only; do not move behavior between runtime crates.
- Clean break: no compatibility aliases for removed CLI commands, MCP actions, REST routes, DTO fields, or config keys.
- Removed DTO/request fields listed in `docs/pipeline-unification/delivery/surface-removal-contract.md` must be absent from generated DTO schemas and generated reference docs.
- Do not add placeholder DTOs to satisfy schema names. A DTO may be added or registered in Phase 1 only when its Rust shape exactly matches the contract table, uses closed enums for stable discriminants, rejects unknown request fields, and has bounded or artifact-backed content.
- Public DTOs must not expose raw secrets, headers, environment values, provider tokens, unbounded prompt bodies, or large document bodies. Use `ArtifactRef`, staged-upload metadata, `ContentRef` only where the contract explicitly allows it, and redacted metadata labels.
- Removed-surface tests must be schema-aware. Do not ban raw tokens such as `"url"`, `"prefix"`, or `"PurgeRequest"` across all generated artifacts when those names are legitimate in non-legacy DTOs.
- Phase 1 API validation must stay targeted: prove API DTO/schema/enum drift and layering. CLI/MCP/REST/config removal is validated in later cutover plans unless a Phase 1 schema change directly touches it.
- Before broad validation, classify the changed-file surface and use the smallest check that proves the change.

---

## Source Of Truth

- `docs/pipeline-unification/foundation/api-contract.md:349-430` defines the required source, ledger, document, job, watch, vector, provider, and boundary DTO field tables.
- `docs/pipeline-unification/foundation/types/dto-contract.md:11-23` defines DTO rules: snake_case JSON, `serde(deny_unknown_fields)` for external requests, typed IDs, no secrets, no transport objects.
- `docs/pipeline-unification/foundation/types/enum-contract.md:4-26` defines closed enum ownership and validation rules.
- `docs/pipeline-unification/foundation/types/stage-result-contract.md:14-220` defines `StageResultHeader`, `StageExecutionResult<T>`, concrete stage result wrappers, and success/degraded/failed fixture expectations.
- `docs/pipeline-unification/schemas/api-dto-schema.md:189-205` defines minimum generated `$defs`.
- `docs/pipeline-unification/delivery/surface-removal-contract.md:132-150` defines removed DTO/request fields and replacements.
- GitHub issue #298 Phase 1 checklist lines 224-244 are the tracker entries this plan must make truthful.

## Engineering Review Corrections

The Lavra engineering review found several plan-level blockers. Address them before implementation:

- Reconcile `PurgeRequest`/`DedupeRequest` with the removal contract before registering schemas. The canonical API schema list may require a current DTO name, but removed-surface enforcement must reject legacy destructive fields and routes, not every occurrence of the type name.
- Replace broad string-token removal checks with JSON-pointer/property assertions scoped to the relevant `$defs`.
- Keep `ResolvedSource.graph` consistent as `Vec<GraphRef>` in tests and implementation.
- Do not select the first adapter candidate when migrating `ResolvedSource`. Resolver output must include an explicit selected adapter or return a typed ambiguity error.
- Export existing `prune` DTOs from `axon_api::source` before registering them in schema generation.
- Add auth/scope metadata coverage for every public request DTO registered in Phase 1. Unknown request DTO/action pairs must fail closed in generated policy data.
- Defer retrieval, discovery, chat, evaluation, diff, screenshot, brand, and extract DTOs unless the implementation copies the exact contract shape with closed enums and explicit content/redaction policy.
- Keep final verification narrow and add `cargo xtask check-layering` because Phase 1 changes public crate ownership.

## File Structure

- Modify `crates/axon-api/src/source/lifecycle.rs`: align `ResolvedSource` with `api-contract.md`, while preserving internally useful identity fields only when represented as explicit optional extension fields or moved to adjacent internal/result DTOs.
- Modify `crates/axon-api/src/source.rs`: export `prune::*` and any newly approved contract-exact DTO modules through `axon_api::source::*`.
- Modify `crates/axon-api/src/source/listing.rs`: add missing listing/runtime DTOs required by the API schema contract, including `WatchDescriptor`.
- Modify `crates/axon-api/src/source/boundary.rs`: add only contract-exact artifact/upload/collection operation DTOs owned by `axon-api`; do not add purge/dedupe placeholders here.
- Modify `crates/axon-api/src/source/provider_io.rs`: register existing contract-exact provider/search DTOs only; defer broad retrieval, discovery, synthesis, and extraction DTO design if exact closed-enum shapes are not already present.
- Modify `crates/axon-api/src/source/prune.rs`: add `PruneExecuteRequest` only if it matches the current prune contract, and keep existing prune request/result DTOs in the source API registry.
- Modify `crates/axon-api/src/source_tests.rs`: add contract-shape tests for `ResolvedSource`, minimum DTO serialization, and legacy request-field rejection.
- Modify `crates/axon-api/src/source_status_tests.rs`: add tests for `WatchDescriptor` and operation/listing DTO strictness where those DTOs naturally fit.
- Modify `xtask/src/schemas/api_defs.rs`: register every Phase 1 `$defs` required by `api-dto-schema.md`.
- Modify `xtask/src/schemas/tests.rs`: assert generated API schemas include required Phase 1 `$defs` and exclude removed request names/fields using schema-aware checks.
- Modify `xtask/src/schemas/registry.rs`: replace broad removed-token bans with path-aware checks where the current contract legitimately still uses a name such as `PurgeRequest` or a property such as `url`.
- Modify the API schema artifact input manifest/checksum logic so every touched API source file participates in `cargo xtask schemas api --check`.
- Modify generated files under `docs/reference/api/` only through `cargo xtask schemas api`; never hand-edit generated JSON or markdown.

## Task 0: Reconcile Removed-Surface Rules Before DTO Registration

**Files:**
- Modify: `xtask/src/schemas/registry.rs`
- Modify: `xtask/src/schemas/tests.rs`
- Test: `cargo test -p xtask removed_surface`

**Interfaces:**
- Consumes: `docs/pipeline-unification/delivery/surface-removal-contract.md`.
- Produces: schema-aware removed-surface checks that do not conflict with legitimate current DTOs.

- [ ] **Step 1: Replace broad token bans with schema-aware rules**

Update removed-surface enforcement so it does not reject every occurrence of `"PurgeRequest"`, `"url"`, `"prefix"`, or `"path_prefix"` in generated API artifacts. Instead, assert these concrete API conditions:

```text
$defs.EmbedRequest is absent
$defs.IngestRequest is absent
$defs.CrawlRequest is absent
$defs.ScrapeRequest is absent
$defs.CodeSearchRequest is absent
if $defs.PurgeRequest exists, properties.target is absent
if $defs.PurgeRequest exists, properties.prefix is absent
if $defs.DedupeRequest exists, it is the current source/prune contract shape, not a legacy route DTO
```

Keep CLI/MCP/REST/config removed-route checks in their later cutover plans unless this Phase 1 work directly regenerates those artifacts.

- [ ] **Step 2: Add a regression test for legacy destructive aliases**

Add a test that parses `docs/reference/api/schemas.json` as JSON and checks the exact `$defs`/property paths above. The test must fail if a legacy purge schema with `target` or `prefix` reappears, while still allowing a contract-exact prune-owned `PurgeRequest` if the API schema contract requires that name.

- [ ] **Step 3: Verify the rule change before adding DTOs**

```bash
cargo test -p xtask removed_surface --no-fail-fast
cargo xtask schemas api --check
```

## Task 1: Lock Phase 1 Contract Failures In Tests

**Files:**
- Modify: `crates/axon-api/src/source_tests.rs`
- Modify: `xtask/src/schemas/tests.rs`
- Test: `cargo test -p axon-api source`
- Test: `cargo test -p xtask api_schema_contains_phase_1_required_defs`
- Test: `cargo test -p xtask removed_surface`

**Interfaces:**
- Consumes: existing `axon_api::source::*` DTO exports.
- Produces: failing tests that define the exact Phase 1 target shape for later tasks.

- [ ] **Step 1: Add a failing `ResolvedSource` contract-shape test**

Add this test to `crates/axon-api/src/source_tests.rs` after `source_request_deserializes_with_defaults_for_minimal_input`:

```rust
#[test]
fn resolved_source_serializes_to_api_contract_shape() {
    let resolved = ResolvedSource {
        source: "https://example.com/docs".to_string(),
        canonical_uri: "https://example.com/docs".to_string(),
        source_id: SourceId::from("src_web_example_docs"),
        source_kind: SourceKind::Web,
        adapter: AdapterRef {
            name: "web".to_string(),
            version: "1".to_string(),
        },
        default_scope: SourceScope::Site,
        available_scopes: vec![SourceScope::Page, SourceScope::Site, SourceScope::Map],
        authority: AuthorityLevel::Official,
        confidence: 0.97,
        reason: "canonical https URL".to_string(),
        graph: Vec::new(),
        warnings: Vec::new(),
        metadata: MetadataMap::new(),
    };

    let value = serde_json::to_value(&resolved).expect("serialize resolved source");

    assert_eq!(value["source"], "https://example.com/docs");
    assert_eq!(value["canonical_uri"], "https://example.com/docs");
    assert_eq!(value["source_kind"], "web");
    assert_eq!(value["adapter"]["name"], "web");
    assert_eq!(value["default_scope"], "site");
    assert_eq!(value["available_scopes"], serde_json::json!(["page", "site", "map"]));
    assert!(value.get("requested_uri").is_none());
    assert!(value.get("candidate_adapters").is_none());
    assert!(value.get("display_name").is_none());
}
```

- [ ] **Step 2: Add a generated `$defs` coverage test**

Add this helper and test to `xtask/src/schemas/tests.rs` near the existing schema tests:

```rust
const PHASE_1_REQUIRED_API_DEFS: &[&str] = &[
    // Keep this list in the same family order as schemas/api-dto-schema.md.
    // Add only contract-exact DTOs. If a required name has no exact Rust DTO yet,
    // add it to PHASE_1_DEFERRED_API_DEFS with the owning follow-up plan.
];

#[test]
fn api_schema_contains_phase_1_required_defs() {
    let artifact = std::fs::read_to_string("docs/reference/api/schemas.json")
        .expect("read generated API schema artifact");
    let schema: serde_json::Value =
        serde_json::from_str(&artifact).expect("parse generated API schema artifact");
    let defs = schema
        .get("$defs")
        .and_then(|value| value.as_object())
        .expect("generated API schema has $defs object");

    for name in PHASE_1_REQUIRED_API_DEFS {
        assert!(defs.contains_key(*name), "missing API schema $defs entry: {name}");
    }
}
```

Populate `PHASE_1_REQUIRED_API_DEFS` from `docs/pipeline-unification/schemas/api-dto-schema.md`, but only include names whose Rust DTOs already match the contract exactly. For required names that are still not contract-exact, add a separate test table named `PHASE_1_DEFERRED_API_DEFS` with an owner plan and reason:

```rust
const PHASE_1_DEFERRED_API_DEFS: &[(&str, &str, &str)] = &[
    ("ChatRequest", "phase-3b-security-error-memory.md", "needs closed ChatRole and prompt/content policy"),
    ("DiffRequest", "phase-5a-surface-drift-generated-artifacts.md", "needs closed DiffMode and generated route policy"),
];

#[test]
fn phase_1_deferred_api_defs_are_documented() {
    for (name, owner, reason) in PHASE_1_DEFERRED_API_DEFS {
        assert!(!name.is_empty(), "deferred API def must have a name");
        assert!(!owner.is_empty(), "deferred API def {name} must have an owner plan");
        assert!(!reason.is_empty(), "deferred API def {name} must have a reason");
    }
}
```

The final required list should include the contract-exact equivalent of these families, but do not copy this literal list blindly:

```rust
[
        "SuccessEnvelope",
        "ErrorEnvelope",
        "Page",
        "PollDescriptor",
        "JobDescriptor",
        "SourceRequest",
        "ResolvedSource",
        "RoutePlan",
        "SourcePlan",
        "SourceResult",
        "SourceManifest",
        "ManifestItem",
        "SourceManifestDiff",
        "SourceGeneration",
        "CleanupDebt",
        "SourceDocument",
        "PreparedDocument",
        "PreparedChunk",
        "DocumentStatus",
        "SourceParseFacts",
        "GraphCandidate",
        "GraphNode",
        "GraphEdge",
        "GraphEvidence",
        "EmbeddingBatch",
        "EmbeddingResult",
        "VectorPointBatch",
        "VectorSearchRequest",
        "VectorSearchResult",
        "QueryRequest",
        "QueryResult",
        "RetrievalRequest",
        "RetrievalResult",
        "AskRequest",
        "AskResult",
        "ChatRequest",
        "ChatResult",
        "EvaluationRequest",
        "EvaluationResult",
        "SuggestRequest",
        "SuggestResult",
        "SearchRequest",
        "SearchResult",
        "ResearchRequest",
        "ResearchResult",
        "SummarizeRequest",
        "SummarizeResult",
        "EndpointDiscoveryRequest",
        "EndpointDiscoveryResult",
        "BrandRequest",
        "BrandResult",
        "DiffRequest",
        "DiffResult",
        "ScreenshotRequest",
        "ScreenshotResult",
        "ExtractRequest",
        "ExtractResult",
        "JobSummary",
        "JobEventPage",
        "WatchRequest",
        "WatchResult",
        "WatchDescriptor",
        "SourceProgressEvent",
        "TraceContext",
        "ArtifactRef",
        "ArtifactListRequest",
        "ArtifactResult",
        "UploadCreateRequest",
        "UploadResult",
        "PruneRequest",
        "PruneExecuteRequest",
        "PrunePlan",
        "PruneResult",
        "DedupeRequest",
        "DedupeResult",
        "PurgeRequest",
        "PurgeResult",
        "CollectionListRequest",
        "CollectionResult",
        "ProviderCapability",
        "HealthReport",
        "ApiError",
        "SourceError",
        "SourceWarning",
]
```

- [ ] **Step 3: Run tests to verify they fail**

Run:

```bash
cargo test -p axon-api source::tests::resolved_source_serializes_to_api_contract_shape
cargo test -p xtask api_schema_contains_phase_1_required_defs
cargo test -p xtask phase_1_deferred_api_defs_are_documented
```

Expected:

```text
resolved_source_serializes_to_api_contract_shape ... FAILED
api_schema_contains_phase_1_required_defs ... FAILED
```

The first failure should complain that `ResolvedSource` has no `source` or `adapter` field. The second failure should name missing `$defs` entries.

- [ ] **Step 4: Commit the failing contract tests**

```bash
git add crates/axon-api/src/source_tests.rs xtask/src/schemas/tests.rs
git commit -m "test: lock phase 1 API contract gaps"
```

## Task 2: Align `ResolvedSource` With `api-contract.md`

**Files:**
- Modify: `crates/axon-api/src/source/lifecycle.rs`
- Modify: `crates/axon-api/src/source_tests.rs`
- Test: `cargo test -p axon-api source::tests::resolved_source_serializes_to_api_contract_shape`

**Interfaces:**
- Consumes: `AdapterRef`, `GraphRef`, `MetadataMap`, `SourceId`, `SourceKind`, `SourceScope`, `AuthorityLevel`, `SourceWarning`.
- Produces: `ResolvedSource` with required wire fields `source`, `canonical_uri`, `source_kind`, `adapter`, `default_scope`, `available_scopes`, `authority`, `confidence`, `reason`, and optional `graph`, `warnings`, `metadata`.

- [ ] **Step 1: Replace the `ResolvedSource` struct**

In `crates/axon-api/src/source/lifecycle.rs`, replace the existing `ResolvedSource` definition with:

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ResolvedSource {
    pub source: String,
    pub canonical_uri: String,
    pub source_id: SourceId,
    pub source_kind: SourceKind,
    pub adapter: AdapterRef,
    pub default_scope: SourceScope,
    pub available_scopes: Vec<SourceScope>,
    pub authority: AuthorityLevel,
    pub confidence: f32,
    pub reason: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub graph: Vec<GraphRef>,
    pub warnings: Vec<SourceWarning>,
    #[serde(default, skip_serializing_if = "MetadataMap::is_empty")]
    pub metadata: MetadataMap,
}
```

The `source_id` and `metadata` fields are retained because downstream ledger/status code needs stable identity and extension data. They are additive fields, not replacements for the required contract fields.

- [ ] **Step 2: Update the test fixture from `graph: None` to an empty vector**

In the test added in Task 1, use this exact field:

```rust
graph: Vec::new(),
```

And keep this assertion:

```rust
assert!(value.get("graph").is_none() || value["graph"].as_array().is_some());
```

- [ ] **Step 3: Fix compile errors at construction sites**

Run:

```bash
cargo check -p axon-api --locked
```

Expected first run:

```text
error[E0560]: struct `ResolvedSource` has no field named `requested_uri`
```

For every failed constructor, map old fields to new fields using this rule:

```rust
ResolvedSource {
    source: old_requested_uri_or_request_source,
    canonical_uri,
    source_id,
    source_kind,
    adapter: selected_adapter_ref,
    default_scope,
    available_scopes,
    authority,
    confidence,
    reason,
    graph: Vec::new(),
    warnings,
    metadata: MetadataMap::new(),
}
```

If a call site only has `candidate_adapters: Vec<AdapterCandidate>`, do not choose `.first()` or otherwise derive public identity from vector ordering. Add an explicit selected route result at the resolver boundary or return a typed `SourceError`/`ApiError` for empty or ambiguous candidates:

```rust
let adapter = selected_adapter.ok_or_else(|| {
    SourceError::ambiguous_source("resolver did not produce a selected adapter")
})?;
```

Add tests for at least these cases:

```text
ambiguous local path vs URL target returns a typed ambiguity error
ambiguous registry vs Git target returns a typed ambiguity error
empty adapter candidates returns a typed resolver error and does not serialize ResolvedSource
```

- [ ] **Step 4: Run focused tests**

```bash
cargo test -p axon-api source::tests::resolved_source_serializes_to_api_contract_shape
cargo test -p axon-api source::tests::enum_wire_values_are_snake_case_and_closed
```

Expected:

```text
test source::tests::resolved_source_serializes_to_api_contract_shape ... ok
test source::tests::enum_wire_values_are_snake_case_and_closed ... ok
```

- [ ] **Step 5: Commit the DTO alignment**

```bash
git add crates/axon-api/src/source/lifecycle.rs crates/axon-api/src/source_tests.rs
git commit -m "fix: align resolved source DTO with phase 1 contract"
```

## Task 3: Add Missing Phase 1 Operation DTOs

**Files:**
- Modify: `crates/axon-api/src/source.rs`
- Modify: `crates/axon-api/src/source/boundary.rs`
- Modify: `crates/axon-api/src/source/listing.rs`
- Modify: `crates/axon-api/src/source/prune.rs`
- Modify: `crates/axon-api/src/source_status_tests.rs`
- Test: `cargo test -p axon-api source::status_tests`

**Interfaces:**
- Consumes: `ArtifactId`, `ArtifactKind`, `ArtifactRef`, `CollectionSpec`, `JobDescriptor`, `JobId`, `LifecycleStatus`, `MetadataMap`, `PruneRequest`, `PruneResult`, `ProviderCapability`, `SourceId`, `SourceWarning`, `WatchId`.
- Produces: missing schema-contract DTOs exported through `axon_api::source::*`.

- [ ] **Step 0: Export existing prune DTOs**

In `crates/axon-api/src/source.rs`, ensure the existing prune module is exported:

```rust
pub mod prune;
pub use prune::*;
```

Run:

```bash
cargo check -p axon-api --locked
```

- [ ] **Step 1: Add artifact/upload/collection DTOs to `boundary.rs`**

Add this import near the other `use super::*` lines in `crates/axon-api/src/source/boundary.rs`:

```rust
use super::vector::CollectionSpec;
```

Append these definitions after `ArtifactReadResult` in `crates/axon-api/src/source/boundary.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ArtifactListRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_id: Option<SourceId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job_id: Option<JobId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<ArtifactKind>,
    pub limit: Option<u32>,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ArtifactResult {
    pub artifacts: Vec<ArtifactRef>,
    pub limit: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    pub warnings: Vec<SourceWarning>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct UploadCreateRequest {
    pub filename: String,
    pub content_type: String,
    pub size_bytes: u64,
    pub purpose: UploadPurpose,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_hint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_id: Option<SourceId>,
    #[serde(default, skip_serializing_if = "MetadataMap::is_empty")]
    pub metadata: MetadataMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct UploadResult {
    pub artifact: ArtifactRef,
    pub status: LifecycleStatus,
    pub warnings: Vec<SourceWarning>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CollectionListRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,
    pub limit: Option<u32>,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CollectionResult {
    pub collections: Vec<CollectionSpec>,
    pub limit: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    pub warnings: Vec<SourceWarning>,
}
```

Do not add `DedupeRequest`, `PurgeRequest`, `DedupeResult`, or `PurgeResult` in `boundary.rs`. If those names are required by `schemas/api-dto-schema.md`, either register an existing contract-exact prune-owned DTO or add them to `PHASE_1_DEFERRED_API_DEFS` with the owning Phase 10/11 plan.

If `UploadPurpose` does not already exist, add a closed enum before `UploadCreateRequest`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum UploadPurpose {
    SourceArtifact,
    Import,
    Evaluation,
}
```

- [ ] **Step 2: Add `WatchDescriptor` to `listing.rs`**

Append this definition after `WatchArtifactListRequest` in `crates/axon-api/src/source/listing.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct WatchDescriptor {
    pub watch_id: WatchId,
    pub source_id: SourceId,
    pub enabled: bool,
    pub schedule: WatchSchedule,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_job: Option<JobDescriptor>,
    pub warnings: Vec<SourceWarning>,
}
```

- [ ] **Step 3: Add `PruneExecuteRequest` to `prune.rs`**

Append this definition after `PruneRequest` in `crates/axon-api/src/source/prune.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PruneExecuteRequest {
    pub plan: PrunePlan,
    pub confirm: bool,
    pub reason: String,
}
```

- [ ] **Step 4: Add DTO strictness tests**

Append this test to `crates/axon-api/src/source_status_tests.rs`:

```rust
#[test]
fn phase_1_operation_dtos_reject_unknown_fields() {
    let upload_err = serde_json::from_value::<UploadCreateRequest>(serde_json::json!({
        "filename": "notes.md",
        "content_type": "text/markdown",
        "size_bytes": 12,
        "purpose": "source_artifact",
        "legacy": true
    }))
    .expect_err("upload request must reject unknown fields");
    assert!(upload_err.to_string().contains("unknown field"), "{upload_err}");

    let watch_err = serde_json::from_value::<WatchDescriptor>(serde_json::json!({
        "watch_id": "watch_1",
        "source_id": "src_1",
        "enabled": true,
        "schedule": { "every_seconds": 3600 },
        "warnings": [],
        "legacy": true
    }))
    .expect_err("watch descriptor must reject unknown fields");
    assert!(watch_err.to_string().contains("unknown field"), "{watch_err}");
}
```

- [ ] **Step 5: Run focused tests**

```bash
cargo test -p axon-api source::status_tests::phase_1_operation_dtos_reject_unknown_fields
cargo test -p axon-api source::status_tests
```

Expected:

```text
test source::status_tests::phase_1_operation_dtos_reject_unknown_fields ... ok
test result: ok
```

- [ ] **Step 6: Commit operation DTOs**

```bash
git add crates/axon-api/src/source.rs crates/axon-api/src/source/boundary.rs crates/axon-api/src/source/listing.rs crates/axon-api/src/source/prune.rs crates/axon-api/src/source_status_tests.rs
git commit -m "feat: add phase 1 operation DTOs"
```

## Task 4: Classify Retrieval And Discovery DTO Gaps Without Placeholder DTOs

**Files:**
- Modify: `crates/axon-api/src/source/provider_io.rs`
- Modify: `crates/axon-api/src/source_status_tests.rs`
- Modify: `xtask/src/schemas/tests.rs`
- Test: `cargo test -p axon-api source::status_tests`
- Test: `cargo test -p xtask phase_1_deferred_api_defs_are_documented`

**Interfaces:**
- Consumes: existing provider/search DTOs, `docs/pipeline-unification/foundation/api-contract.md`, `docs/pipeline-unification/foundation/types/enum-contract.md`, and `docs/pipeline-unification/foundation/types/dto-contract.md`.
- Produces: schema coverage for existing contract-exact DTOs and explicit deferred entries for DTO families that need later policy work.

- [ ] **Step 1: Inventory existing provider DTOs**

Read the current `crates/axon-api/src/source/provider_io.rs` DTOs and compare each public request/result type against `api-contract.md`. Register only DTOs that already satisfy all of these rules:

```text
request DTO has #[serde(deny_unknown_fields)]
stable discriminants use closed enums, not String
large bodies are absent, bounded, or represented by ArtifactRef/ContentRef according to the contract
metadata is approved/redacted, not arbitrary public passthrough
auth/scope policy can be derived from the request/action family
```

Do not add new broad DTOs for chat, evaluation, summarize, diff, endpoint discovery, brand, screenshot, or extract in this task unless the exact contract shape and closed enums already exist.

- [ ] **Step 2: Add missing closed enums before registering DTOs**

If a DTO is otherwise ready but uses a stable raw string, add the closed enum first and update serialization tests:

```text
ChatMessage.role -> ChatRole
EvaluationResult.verdict -> EvaluationVerdict
DiffRequest.mode -> DiffMode
SummarizeRequest.format -> SummaryFormat
ExtractRequest trusted graph behavior -> explicit ExtractPolicy or internal-only field
```

If adding the enum would require runtime behavior or transport policy work, defer that DTO name instead of adding a partial DTO.

- [ ] **Step 3: Replace inline large/sensitive fields with artifact-backed shapes**

Do not expose fields such as `RetrievedDocument.content`, `ChatMessage.content`, `ResearchResult.answer`, `SummarizeResult.summary`, or `ExtractRequest.prompt` as unbounded raw strings in newly added public DTOs. Use one of these patterns:

```text
small text with an explicit max length and redaction policy
ArtifactRef to stored content
ContentRef only where the contract explicitly allows inline content and size bounds
redacted excerpt/citation fields rather than full source body
```

- [ ] **Step 4: Add deferred coverage for non-contract-exact DTOs**

For each API schema name required by `schemas/api-dto-schema.md` that is not contract-exact after Steps 1-3, add an entry to `PHASE_1_DEFERRED_API_DEFS` in `xtask/src/schemas/tests.rs` with:

```text
schema name
owning follow-up plan in docs/pipeline-unification/plans
specific contract gap that blocks Phase 1 registration
```

Expected deferred owners:

```text
retrieval/ask/query gaps -> phase 6 code search and generation cutover plan if generation filters are involved, otherwise phase 3b security/error/memory plan
tool-output/extract policy gaps -> phase 7 parser metadata graph plan
source-family discovery gaps -> phase 9 source families plan
generated removed-surface drift -> phase 5a surface drift plan
reset/prune destructive operation gaps -> phase 5b reset/preflight plan
```

- [ ] **Step 5: Add strictness tests for any DTOs actually registered**

Append tests to `crates/axon-api/src/source_status_tests.rs` only for DTOs actually registered in Phase 1. For example, if `QueryRequest` is contract-exact:

```rust
#[test]
fn phase_1_registered_provider_dtos_reject_unknown_fields() {
    let query_err = serde_json::from_value::<QueryRequest>(serde_json::json!({
        "query": "axon phase 1",
        "limit": 10,
        "legacy": true
    }))
    .expect_err("query request must reject unknown fields");
    assert!(query_err.to_string().contains("unknown field"), "{query_err}");
}
```

- [ ] **Step 6: Run focused tests**

```bash
cargo test -p axon-api source::status_tests
cargo test -p xtask phase_1_deferred_api_defs_are_documented
```

Expected:

```text
test result: ok
test phase_1_deferred_api_defs_are_documented ... ok
```

- [ ] **Step 7: Commit DTO classification**

```bash
git add crates/axon-api/src/source/provider_io.rs crates/axon-api/src/source_status_tests.rs xtask/src/schemas/tests.rs
git commit -m "test: classify phase 1 provider DTO gaps"
```

## Task 5: Register All Phase 1 DTOs In API Schema Generation

**Files:**
- Modify: `xtask/src/schemas/api_defs.rs`
- Modify: API schema source input manifest/checksum code under `xtask/src/schemas/`
- Modify: generated auth/scope metadata if the schema generator already owns request/action scope output
- Modify: generated `docs/reference/api/schemas.json`
- Modify: generated `docs/reference/api/dto.md`
- Modify: generated `docs/reference/api/enums.md`
- Test: `cargo test -p xtask api_schema_contains_phase_1_required_defs`
- Test: `cargo test -p xtask api_request_dtos_have_scope_entries`
- Test: `cargo xtask schemas api --check`

**Interfaces:**
- Consumes: public DTOs exported from `axon_api::source::*`.
- Produces: generated API schemas with all minimum `$defs` required by `api-dto-schema.md`.

- [ ] **Step 1: Extend schema definitions from a single required-name source**

Create or reuse one `PHASE_1_REQUIRED_API_DEFS` constant and use it for:

```text
schema coverage tests
generated markdown DTO name list
missing/extra generated schema assertions
```

Do not maintain separate uncoordinated name lists in tests and generators.

In `xtask/src/schemas/api_defs.rs`, update `api_source_schema_defs()` to include every Phase 1 family:

```rust
pub fn api_source_schema_defs() -> Vec<(&'static str, Value)> {
    let mut defs = source_lifecycle_defs();
    defs.extend(source_document_defs());
    defs.extend(source_job_defs());
    defs.extend(source_status_defs());
    defs.extend(source_operation_defs());
    defs.extend(source_retrieval_defs());
    defs
}
```

Add these helper functions below `source_job_defs()`:

```rust
fn source_status_defs() -> Vec<(&'static str, Value)> {
    vec![
        schema_def::<axon_api::source::SuccessEnvelope<axon_api::source::SourceResult>>("SuccessEnvelope"),
        schema_def::<axon_api::source::ErrorEnvelope>("ErrorEnvelope"),
        schema_def::<axon_api::source::Page<axon_api::source::SourceSummary>>("Page"),
        schema_def::<axon_api::source::SourceProgressEvent>("SourceProgressEvent"),
        schema_def::<axon_api::source::TraceContext>("TraceContext"),
        schema_def::<axon_api::source::SourceStatus>("SourceStatus"),
        schema_def::<axon_api::source::HealthReport>("HealthReport"),
        schema_def::<axon_api::source::ProviderCapability>("ProviderCapability"),
        schema_def::<axon_api::source::ApiError>("ApiError"),
        schema_def::<axon_api::source::SourceError>("SourceError"),
        schema_def::<axon_api::source::SourceWarning>("SourceWarning"),
        schema_def::<axon_api::source::WatchDescriptor>("WatchDescriptor"),
    ]
}

fn source_operation_defs() -> Vec<(&'static str, Value)> {
    vec![
        schema_def::<axon_api::source::ArtifactRef>("ArtifactRef"),
        schema_def::<axon_api::source::ArtifactListRequest>("ArtifactListRequest"),
        schema_def::<axon_api::source::ArtifactResult>("ArtifactResult"),
        schema_def::<axon_api::source::UploadCreateRequest>("UploadCreateRequest"),
        schema_def::<axon_api::source::UploadResult>("UploadResult"),
        schema_def::<axon_api::source::PruneRequest>("PruneRequest"),
        schema_def::<axon_api::source::PruneExecuteRequest>("PruneExecuteRequest"),
        schema_def::<axon_api::source::PrunePlan>("PrunePlan"),
        schema_def::<axon_api::source::PruneResult>("PruneResult"),
        schema_def::<axon_api::source::CollectionListRequest>("CollectionListRequest"),
        schema_def::<axon_api::source::CollectionResult>("CollectionResult"),
    ]
}

fn source_retrieval_defs() -> Vec<(&'static str, Value)> {
    let mut defs = Vec::new();

    // Add entries here only after Task 4 proves the DTO is contract-exact.
    // Examples:
    // defs.push(schema_def::<axon_api::source::QueryRequest>("QueryRequest"));
    // defs.push(schema_def::<axon_api::source::QueryResult>("QueryResult"));

    defs
}
```

Add `DedupeRequest`, `DedupeResult`, `PurgeRequest`, and `PurgeResult` only if the implementation registers contract-exact current DTOs and Task 0's removed-field tests prove the legacy destructive `target`/`prefix` shape is absent. Otherwise keep them in `PHASE_1_DEFERRED_API_DEFS`.

Add retrieval, discovery, chat, evaluation, suggest, summarize, endpoint, brand, diff, screenshot, and extract DTO names only when Task 4 proves their Rust types are contract-exact. Otherwise register the deferred-table test, not placeholder schema definitions.

- [ ] **Step 2: Extend `api_dto_names()`**

Add every contract-exact name from `PHASE_1_REQUIRED_API_DEFS` into the `api_dto_names()` slice. Keep names grouped by family in the same order as `docs/pipeline-unification/schemas/api-dto-schema.md:193-205`. Do not add names that are currently in `PHASE_1_DEFERRED_API_DEFS`.

- [ ] **Step 3: Update API schema source inputs**

Extend the API artifact source manifest/checksum inputs so `cargo xtask schemas api --check` tracks every file touched by Phase 1, including:

```text
crates/axon-api/src/source.rs
crates/axon-api/src/source/common.rs
crates/axon-api/src/source/lifecycle.rs
crates/axon-api/src/source/listing.rs
crates/axon-api/src/source/boundary.rs
crates/axon-api/src/source/provider_io.rs
crates/axon-api/src/source/prune.rs
xtask/src/schemas/api_defs.rs
xtask/src/schemas/registry.rs
xtask/src/schemas/tests.rs
```

Add a check-mode test proving `cargo xtask schemas api --check` does not write any API artifact when these inputs are unchanged.

- [ ] **Step 4: Add request DTO auth/scope coverage**

For every public request DTO registered in Phase 1, add generated or checked policy metadata that maps the DTO/action/subaction to the required scope family. Unknown request DTO/action pairs must fail closed.

At minimum, test:

```text
artifact list/read request DTOs require read or source-owner policy
upload/create request DTOs require write policy
prune/execute request DTOs require admin policy
collection list request DTOs require read policy
unknown request DTO/action pair is rejected during policy generation
```

- [ ] **Step 5: Regenerate API reference artifacts**

```bash
cargo xtask schemas api
```

Expected:

```text
Running `target/debug/xtask schemas api`
```

Generated file content should change under `docs/reference/api/`.

- [ ] **Step 6: Verify generated schema coverage**

```bash
cargo test -p xtask api_schema_contains_phase_1_required_defs
cargo test -p xtask phase_1_deferred_api_defs_are_documented
cargo test -p xtask api_request_dtos_have_scope_entries
cargo xtask schemas api --check
```

Expected:

```text
test api_schema_contains_phase_1_required_defs ... ok
Running `target/debug/xtask schemas api --check`
```

- [ ] **Step 7: Commit schema registration and artifacts**

```bash
git add xtask/src/schemas/api_defs.rs xtask/src/schemas/registry.rs xtask/src/schemas/tests.rs docs/reference/api/schemas.json docs/reference/api/dto.md docs/reference/api/enums.md
git commit -m "feat: complete phase 1 API schema defs"
```

## Task 6: Make Removed-Surface Boundary Explicit

**Files:**
- Modify: `xtask/src/schemas/tests.rs`
- Modify: `xtask/src/schemas/registry.rs`
- Modify: `docs/pipeline-unification/delivery/current-implementation-sweep.md`
- Test: `cargo test -p xtask removed_surface`
- Test: `cargo xtask schemas api --check`

**Interfaces:**
- Consumes: `REMOVED_SURFACE_RULES` in `xtask/src/schemas/registry.rs`.
- Produces: a tested boundary that generated API references reject removed legacy request names/fields without breaking legitimate current DTOs.

- [ ] **Step 1: Add schema-aware removed request assertions**

In `xtask/src/schemas/tests.rs`, replace broad token checks with a JSON parser test:

```rust
#[test]
fn removed_legacy_api_request_shapes_are_absent() {
    let artifact = std::fs::read_to_string("docs/reference/api/schemas.json")
        .expect("read generated API schema artifact");
    let schema: serde_json::Value =
        serde_json::from_str(&artifact).expect("parse generated API schema artifact");
    let defs = schema
        .get("$defs")
        .and_then(|value| value.as_object())
        .expect("generated API schema has $defs object");

    for removed_def in [
        "EmbedRequest",
        "IngestRequest",
        "CrawlRequest",
        "ScrapeRequest",
        "CodeSearchRequest",
    ] {
        assert!(!defs.contains_key(removed_def), "legacy request def leaked: {removed_def}");
    }

    if let Some(purge) = defs.get("PurgeRequest") {
        let properties = purge
            .get("properties")
            .and_then(|value| value.as_object())
            .expect("PurgeRequest schema has properties");
        assert!(!properties.contains_key("target"), "legacy PurgeRequest.target leaked");
        assert!(!properties.contains_key("prefix"), "legacy PurgeRequest.prefix leaked");
    }
}
```

- [ ] **Step 2: Keep generated-surface removal in the proper phase**

Do not add Phase 1 tests that fail on legitimate API properties such as `url` in screenshot/brand/source DTOs, or on `PurgeRequest` when the current API schema contract still requires a prune-owned shape. CLI/MCP/REST/config removed-command checks stay in the Phase 5A/5B cutover plans unless this implementation regenerates those surfaces.

- [ ] **Step 3: Add the transitional note to the sweep doc**

In `docs/pipeline-unification/delivery/current-implementation-sweep.md`, add this exact bullet under the Phase 1 or schema/current-state section:

```markdown
- Phase 1 removed-surface checks apply to generated API DTO schemas and API reference docs. Transitional legacy Rust request structs such as `axon_api::mcp_schema::requests::{CrawlRequest, EmbedRequest, IngestRequest}` may remain until the later surface-cutover deletion phase, but they must not appear in generated Phase 1 API DTO schemas. Legacy destructive purge fields such as `target` and `prefix` are rejected by property-path checks rather than broad string-token bans.
```

- [ ] **Step 4: Run removed-surface tests**

```bash
cargo test -p xtask removed_surface
cargo xtask schemas api --check
```

Expected:

```text
test removed_surface_drift_fails_generation ... ok
test removed_legacy_api_request_shapes_are_absent ... ok
Running `target/debug/xtask schemas api --check`
```

- [ ] **Step 5: Commit removed-surface boundary**

```bash
git add xtask/src/schemas/tests.rs xtask/src/schemas/registry.rs docs/pipeline-unification/delivery/current-implementation-sweep.md
git commit -m "test: clarify phase 1 removed-surface boundary"
```

## Task 7: Reconcile Issue #298 Phase 1 Wording

**Files:**
- Modify: GitHub issue #298 body or add an issue comment after code lands
- No repo file changes unless Jacob requests a checked-in issue-sync artifact
- Test: `gh issue view 298 --json body --jq '.body'`

**Interfaces:**
- Consumes: passing verification from Tasks 1-6.
- Produces: issue checklist wording that accurately reflects the code and contracts.

- [ ] **Step 1: Prepare the issue update text**

Use this body text as either a comment or an edit patch for the Phase 1 section:

```markdown
### Phase 1 alignment update

Phase 1 is now aligned to the current contract packet:

- `ResolvedSource` now uses the `api-contract.md` public shape: `source`, `canonical_uri`, `source_kind`, `adapter`, `default_scope`, `available_scopes`, `authority`, `confidence`, `reason`, optional graph/warnings metadata, plus explicit `source_id` for ledger identity.
- The generated API schema includes the Phase 1 `$defs` that are contract-exact today. Required names that still need closed enums, auth/scope policy, content bounds, or later surface cutover are explicitly listed in `PHASE_1_DEFERRED_API_DEFS` with owner plans.
- Removed legacy API request names and legacy purge fields are rejected from generated API DTO schemas through schema-aware `$defs`/property checks. Transitional legacy Rust structs under `axon_api::mcp_schema::requests` remain tracked for later surface-cutover deletion and are not part of the Phase 1 public DTO catalog.
- Phase 1 proof:
  - `cargo test -p axon-api source`
  - `cargo test -p xtask api_schema_contains_phase_1_required_defs`
  - `cargo test -p xtask phase_1_deferred_api_defs_are_documented`
  - `cargo test -p xtask api_request_dtos_have_scope_entries`
  - `cargo test -p xtask removed_surface`
  - `cargo xtask schemas api --check`
  - `cargo xtask check-layering`
```

- [ ] **Step 2: Inspect current issue body before updating**

```bash
gh issue view 298 --json body --jq '.body' | sed -n '/### Phase 1: Shared DTO And Enum Spine/,/### Phase 2:/p'
```

Expected:

```text
### Phase 1: Shared DTO And Enum Spine
...
### Phase 2: Schema Generator And Drift Checks
```

- [ ] **Step 3: Apply the issue update only after Jacob approves issue mutation**

If Jacob asks for an issue comment:

```bash
gh issue comment 298 --body-file /tmp/phase-1-alignment-update.md
```

If Jacob asks for an issue body edit, write the edited body to `/tmp/issue-298-body.md`, then run:

```bash
gh issue edit 298 --body-file /tmp/issue-298-body.md
```

- [ ] **Step 4: Verify issue text**

```bash
gh issue view 298 --json body,comments --jq '{body: .body, last_comment: (.comments[-1].body // "")}'
```

Expected: the Phase 1 alignment text is present either in the body or the latest comment.

- [ ] **Step 5: Commit status**

No git commit is needed for issue-only mutation. If Jacob requested a checked-in issue-sync artifact, commit that artifact with:

```bash
git add docs/pipeline-unification/delivery/current-implementation-sweep.md
git commit -m "docs: sync phase 1 issue status"
```

## Final Verification

- [ ] Run the Phase 1 API proof:

```bash
cargo test -p axon-api source
```

Expected:

```text
test result: ok
```

- [ ] Run schema coverage and drift checks:

```bash
cargo test -p xtask api_schema_contains_phase_1_required_defs
cargo test -p xtask phase_1_deferred_api_defs_are_documented
cargo test -p xtask api_request_dtos_have_scope_entries
cargo test -p xtask removed_surface
cargo xtask schemas api --check
cargo xtask check-layering
```

Expected:

```text
test api_schema_contains_phase_1_required_defs ... ok
test phase_1_deferred_api_defs_are_documented ... ok
test api_request_dtos_have_scope_entries ... ok
test removed_legacy_api_request_shapes_are_absent ... ok
Running `target/debug/xtask schemas api --check`
Layering check passed
```

- [ ] Run a structural diff check:

```bash
git diff --check
```

Expected:

```text
```

No output means whitespace checks passed.

- [ ] Inspect changed files:

```bash
git status --short
```

Expected: only the Phase 1 plan, Phase 1 DTO/schema/test files, generated API docs, and documented Phase 1 sweep notes are modified.

## Self-Review Notes

- Spec coverage: Tasks 0-6 cover `api-contract.md`, `dto-contract.md`, `enum-contract.md`, `stage-result-contract.md`, `api-dto-schema.md`, and `surface-removal-contract.md`; Task 7 covers issue #298 checklist synchronization after code verification.
- Placeholder control: this plan forbids placeholder DTOs and requires deferred schema names to have owner plans and specific blocking reasons.
- Type consistency: `ResolvedSource`, `WatchDescriptor`, artifact/upload/collection DTOs, `PruneExecuteRequest`, and schema registration names are introduced only when contract-exact and exported before later tasks consume them.
