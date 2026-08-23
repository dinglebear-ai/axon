# Restored Source Surfaces Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Restore first-class `scrape`, `crawl`, `embed`, `ingest`, and `code-search` operations across CLI, MCP, and REST as batch-capable projections over Axon's existing source and query services.

**Architecture:** `axon-api` owns typed projection DTOs, pure projection rules, the operation registry, and generic batch envelopes. `axon-services` owns whole-batch preflight and execution; `axon-jobs` owns atomic detached admission, idempotency, and durable correlation. CLI, MCP, and REST parse and render only, and all generated contracts derive from the canonical registry.

**Tech Stack:** Rust 2024, Tokio, clap, serde/schemars/utoipa, Axum, rmcp, SQLx/SQLite, Axon xtask generators, Cargo nextest.

**Spec:** `docs/superpowers/specs/2026-08-23-restored-source-surfaces-design.md`

## Global Constraints

- Keep `axon <source>`, `axon source <source>`, MCP `action=source`/`action=query`, and `POST /v1/sources`/`POST /v1/query` working unchanged.
- Do not restore deleted command-specific crates, job kinds, workers, ledgers, acquisition paths, or vector publication paths.
- All five restored operations support the same one-or-many request semantics on CLI, MCP, and REST.
- `scrape` fixes page scope and one-page limits; `crawl` fixes site scope; `embed` forces publication; `ingest` may disable embedding; `code-search` fixes code retrieval.
- Whole-batch validation, routing, idempotency checks, and authorization complete before any side effect.
- Detached source admission is one SQLite transaction: every item job is created or none are.
- Source idempotency is scoped by operation, authenticated caller identity, and caller key; `code-search` rejects idempotency fields.
- The expanded-work ceiling is deterministically partitioned and persisted per item; no in-memory batch coordinator is authoritative.
- Existing scheduler claims, provider reservations, cooldowns, and global worker limits remain authoritative.
- Shared telemetry never records raw inputs, queries, paths, headers, credentials, or idempotency keys.
- Advance the shared REST/MCP contract version to `2026-08-23`.
- Use sibling `_tests.rs` sidecars and never create `mod.rs`.
- Run `cargo xtask generated-contracts refresh` after changing all schema inputs and `cargo xtask generated-contracts check` for final drift proof.
- Do not add a `version` key to `plugins/axon/.claude-plugin/plugin.json`.

---

### Task 1: Canonical Projection DTOs, Rules, and Registry

**Files:**
- Create: `crates/axon-api/src/source/projection.rs`
- Create: `crates/axon-api/src/source/projection_tests.rs`
- Create: `crates/axon-api/src/source/projection_registry.rs`
- Create: `crates/axon-api/src/source/projection_registry_tests.rs`
- Modify: `crates/axon-api/src/source.rs`
- Modify: `crates/axon-api/src/schema_registry.rs`

**Interfaces:**
- Consumes: existing `SourceRequest`, `SourceScope`, `SourceLimits`, `SourceResult`, `ApiError`, `CodeSearchOptions`, and `CodeSearchResult`.
- Produces: `ProjectionOperation`, `SourceProjectionInput`, `QueryProjectionInput`, `BatchRequest<I, P>`, `BatchResult<T>`, `BatchItem<T>`, focused option DTOs, `ProjectionOperationSpec`, `PROJECTION_OPERATIONS`, and `project_*` functions.

- [ ] **Step 1: Write failing DTO and serialization tests**

Add sidecar tests covering non-empty inputs, optional synchronous input echo, source-only idempotency, unknown-field rejection, and stable operation spellings:

```rust
#[test]
fn code_search_rejects_source_idempotency_shape() {
    let value = serde_json::json!({
        "inputs": [{"input": "scheduler", "idempotency_key": "nope"}],
        "options": {"limit": 10}
    });
    assert!(serde_json::from_value::<CodeSearchRequest>(value).is_err());
}

#[test]
fn detached_batch_item_omits_input_echo() {
    let item = BatchItem::<SourceResult>::queued(0, None, descriptor());
    assert!(serde_json::to_value(item).unwrap().get("input").is_none());
}
```

- [ ] **Step 2: Run the focused tests and verify failure**

Run:

```bash
cargo test -p axon-api projection --locked
```

Expected: FAIL because the projection modules and types do not exist.

- [ ] **Step 3: Implement the canonical DTOs and batch envelopes**

Define the exact shared types in `projection.rs`:

```rust
pub const PROJECTION_CONTRACT_VERSION: &str = "2026-08-23";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionOperation { Scrape, Crawl, Embed, Ingest, CodeSearch }

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceProjectionInput {
    pub input: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct QueryProjectionInput { pub input: String }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct BatchRequest<I, P> { pub inputs: Vec<I>, pub options: P }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct BatchResult<T> {
    pub batch_id: BatchId,
    pub status: BatchStatus,
    pub items: Vec<BatchItem<T>>,
    pub summary: BatchSummary,
}
```

Use concrete aliases `ScrapeRequest`, `CrawlRequest`, `EmbedRequest`, `IngestRequest`, and `CodeSearchRequest` so schemars/utoipa register stable schema names.

- [ ] **Step 4: Write failing projection-rule and registry tests**

Cover fixed scope/embedding/limits, caller controls, operation scope/cost/idempotency metadata, and duplicate registry spellings:

```rust
#[test]
fn scrape_projects_page_limits() {
    let requests = project_scrape(&scrape_request("https://example.test")).unwrap();
    assert_eq!(requests[0].scope, Some(SourceScope::Page));
    assert_eq!(requests[0].limits.max_pages, Some(1));
    assert_eq!(requests[0].limits.max_items, Some(1));
}

#[test]
fn projection_registry_has_unique_transport_names() {
    validate_projection_registry(PROJECTION_OPERATIONS).unwrap();
}
```

- [ ] **Step 5: Implement pure projections and registry validation**

Expose functions with stable signatures:

```rust
pub fn project_scrape(request: &ScrapeRequest) -> Result<Vec<SourceRequest>, ApiError>;
pub fn project_crawl(request: &CrawlRequest) -> Result<Vec<SourceRequest>, ApiError>;
pub fn project_embed(request: &EmbedRequest) -> Result<Vec<SourceRequest>, ApiError>;
pub fn project_ingest(request: &IngestRequest) -> Result<Vec<SourceRequest>, ApiError>;
pub fn project_code_search(request: &CodeSearchRequest) -> Result<Vec<CodeSearchPlan>, ApiError>;
pub fn validate_projection_registry(specs: &[ProjectionOperationSpec]) -> Result<(), ApiError>;
```

`CodeSearchPlan` must carry query, collection, limit, offset, cwd, path prefix, language/source filters, hybrid controls, and `ensure_fresh`; it must not include an idempotency key.

- [ ] **Step 6: Run API tests and contract-shape checks**

Run:

```bash
cargo test -p axon-api projection --locked
cargo test -p axon-api schema_registry --locked
cargo fmt --all -- --check
```

Expected: PASS.

- [ ] **Step 7: Commit the API contract source**

```bash
git add crates/axon-api/src/source.rs crates/axon-api/src/source/projection*.rs crates/axon-api/src/schema_registry.rs
git commit -m "feat(api): define restored projection contracts"
```

### Task 2: Executable Contract Fixtures and Generator Family

**Files:**
- Create: `xtask/src/schemas/projections.rs`
- Create: `xtask/src/schemas/projections_tests.rs`
- Modify: `xtask/src/schemas.rs`
- Modify: `xtask/src/schemas/families.rs`
- Modify: `xtask/src/schemas/families/family_specs.rs`
- Modify: `xtask/src/generated_contracts_tests.rs`
- Create: `tests/fixtures/source-projections/*.json`
- Generate: `docs/reference/sources/projections.json`
- Generate: `docs/reference/sources/projections.md`

**Interfaces:**
- Consumes: `PROJECTION_OPERATIONS`, focused DTO schemas, and `project_*` from Task 1.
- Produces: a `SchemaFamily::Projections` generator, canonical JSON fixtures, and drift-owned JSON/Markdown reference artifacts.

- [ ] **Step 1: Add failing generator and fixture tests**

Test that every registry row renders once, fixtures normalize through the declared projection, and the family participates before presentation/docs checks:

```rust
#[test]
fn projection_family_contains_all_operations() {
    let generated = generate_projection_contract().unwrap();
    assert_eq!(generated["operations"].as_array().unwrap().len(), 5);
}
```

- [ ] **Step 2: Verify the generator tests fail**

```bash
cargo test -p xtask projections --locked
```

Expected: FAIL because the family and artifacts do not exist.

- [ ] **Step 3: Implement the generator and fixture loader**

Add `SchemaFamily::Projections` with source provenance rooted in the Task 1 modules and fixture directory. Generate JSON directly from the registry and render Markdown from the same in-memory value; do not parse the Markdown back into data.

- [ ] **Step 4: Add complete canonical fixture cases**

Create named fixtures for each operation's minimal/full request, forbidden overrides, single/batch output, mixed outcomes, detached disclosure, deterministic budget allocation, and `409` idempotency collision. Each fixture must contain `operation`, `transport_input`, `canonical_requests`, and `expected_result` keys.

- [ ] **Step 5: Refresh and verify generated artifacts**

```bash
cargo xtask generated-contracts refresh
cargo xtask generated-contracts check
cargo test -p xtask projections --locked
```

Expected: PASS with `projections.json` and `projections.md` tracked.

- [ ] **Step 6: Commit the executable contract artifacts**

```bash
git add xtask/src tests/fixtures/source-projections docs/reference/sources/projections.*
git commit -m "feat(contracts): generate restored projection registry"
```

### Task 3: Typed Batch Policy Configuration

**Files:**
- Modify: `crates/axon-core/src/config/types/config.rs`
- Modify: `crates/axon-core/src/config/types/config_impls.rs`
- Modify: `crates/axon-core/src/config/parse/toml_config.rs`
- Modify: `crates/axon-core/src/config/parse/toml_config/convert.rs`
- Modify: `crates/axon-core/src/config/parse/toml_config_tests.rs`
- Modify: `crates/axon-core/src/config/parse/build_config_tests.rs`
- Modify: `xtask/src/schemas/config_schema_registry.rs`
- Modify: `xtask/src/schemas/config_schema_registry/env_vars.rs`
- Modify: `config.example.toml`

**Interfaces:**
- Produces: `Config::projection_batch_max_inputs`, `Config::projection_batch_max_concurrency`, and `Config::projection_batch_max_expanded_items`.

- [ ] **Step 1: Add failing default/TOML/env precedence tests**

Use conservative defaults `32`, `4`, and `10_000`, and env names `AXON_PROJECTION_BATCH_MAX_INPUTS`, `AXON_PROJECTION_BATCH_MAX_CONCURRENCY`, and `AXON_PROJECTION_BATCH_MAX_EXPANDED_ITEMS`.

- [ ] **Step 2: Verify tests fail on missing fields**

```bash
cargo test -p axon-core projection_batch --locked
```

- [ ] **Step 3: Implement config defaults, parsing, validation, and schema registration**

Reject zero for every field and reject `max_expanded_items < max_inputs` at config load with the exact field paths in the error.

- [ ] **Step 4: Run config tests and generated config checks**

```bash
cargo test -p axon-core projection_batch --locked
cargo xtask schemas config --check
```

- [ ] **Step 5: Commit batch policy configuration**

```bash
git add crates/axon-core/src/config xtask/src/schemas/config_schema_registry* config.example.toml docs/reference/config
git commit -m "feat(config): add projection batch policy"
```

### Task 4: Durable Batch Correlation and Atomic Job Admission

**Files:**
- Create: `crates/axon-jobs/src/migrations/0009_projection_batch_correlation.sql`
- Modify: `crates/axon-jobs/src/migration-checksums.txt`
- Modify: `crates/axon-api/src/source/job.rs`
- Modify: `crates/axon-api/src/source/job_listing.rs`
- Modify: `crates/axon-api/src/source/lifecycle.rs`
- Modify: `crates/axon-jobs/src/boundary.rs`
- Create: `crates/axon-jobs/src/unified/batch_create.rs`
- Create: `crates/axon-jobs/src/unified/batch_create_tests.rs`
- Modify: `crates/axon-jobs/src/unified.rs`
- Modify: `crates/axon-jobs/src/unified/ops.rs`
- Modify: `crates/axon-jobs/src/unified/schema.rs`
- Modify: `crates/axon-jobs/src/unified_codec.rs`
- Modify: `crates/axon-jobs/src/unified/event_ops.rs`
- Modify: `crates/axon-jobs/src/fake_store.rs`
- Modify: `crates/axon-jobs/src/migrations_tests.rs`

**Interfaces:**
- Consumes: `BatchId`, `JobCreateRequest`, and source-only idempotency fingerprints.
- Produces: `JobCreateRequest.batch_id`, `JobDescriptor.batch_id`, `JobSummary.batch_id`, `JobEvent.batch_id`, caller-scoped idempotency metadata, and `JobStore::create_batch_atomic`.

- [ ] **Step 1: Write failing migration and DTO round-trip tests**

Assert nullable `batch_id` columns on `jobs` and `job_events`, index `idx_axon_jobs_batch_created`, event propagation, and generated database schema visibility.

- [ ] **Step 2: Write failing atomic-admission tests**

```rust
#[tokio::test]
async fn batch_create_rolls_back_every_job_on_collision() {
    let store = sqlite_store().await;
    let result = store.create_batch_atomic(vec![valid(), conflicting()]).await;
    assert!(result.is_err());
    assert_eq!(store.list(all_jobs()).await.unwrap().items.len(), 0);
}
```

Also test identical caller-scoped fingerprints reuse existing jobs, conflicting normalized payloads return `ApiError` with HTTP mapping `409`, and different callers do not collide.

- [ ] **Step 3: Verify focused tests fail**

```bash
cargo test -p axon-jobs batch_create --locked
cargo test -p axon-jobs migrations --locked
```

- [ ] **Step 4: Implement the migration and DTO/codec propagation**

Add nullable `batch_id`, `idempotency_scope`, and `request_fingerprint` columns in migration `0009`; replace the legacy global idempotency uniqueness rule with a partial unique index over `(idempotency_scope, idempotency_key)`; add `idx_axon_jobs_batch_created` over `(batch_id, created_at, job_id)`; update checksum inventory and every DTO/codec constructor. Preserve `SourceProgressEvent.batch_id` instead of forcing it to `None` in observe/terminal paths. Existing non-projection callers continue to use their current globally scoped behavior through an explicit legacy scope value.

- [ ] **Step 5: Implement transactional bulk creation**

Extend the boundary:

```rust
#[async_trait]
pub trait JobStore: Send + Sync {
    async fn create(&self, request: JobCreateRequest) -> Result<JobDescriptor>;
    async fn create_batch_atomic(
        &self,
        requests: Vec<JobCreateRequest>,
    ) -> Result<Vec<JobDescriptor>>;
}
```

`SqliteUnifiedJobStore` must open one SQLx transaction, validate all `(operation, authenticated caller identity, caller key)` scoped idempotency rows and normalized request fingerprints, insert/reuse in input order, and commit once. A matching fingerprint reuses its job; a different fingerprint returns the typed collision mapped to HTTP `409`. The fake store must emulate all-or-nothing behavior for service tests.

- [ ] **Step 6: Run durable job and schema tests**

```bash
cargo test -p axon-jobs batch_create --locked
cargo test -p axon-jobs migrations --locked
cargo xtask schemas database --check
```

- [ ] **Step 7: Commit durable admission**

```bash
git add crates/axon-api/src/source/job* crates/axon-api/src/source/lifecycle.rs crates/axon-jobs/src docs/reference/runtime/database-schema.*
git commit -m "feat(jobs): admit projection batches atomically"
```

### Task 5: Whole-Batch Preflight and Deterministic Budgets

**Files:**
- Create: `crates/axon-services/src/projections.rs`
- Create: `crates/axon-services/src/projections/preflight.rs`
- Create: `crates/axon-services/src/projections/preflight_tests.rs`
- Create: `crates/axon-services/src/projections/budget.rs`
- Create: `crates/axon-services/src/projections/budget_tests.rs`
- Modify: `crates/axon-services/src/lib.rs`
- Modify: `crates/axon-services/src/source/authorize.rs`
- Modify: `crates/axon-services/src/source/routing.rs`

**Interfaces:**
- Consumes: Task 1 projection requests, Task 3 config limits, canonical source router, and `AuthSnapshot`.
- Produces: `ProjectionPreflight`, `PreparedSourceItem`, `PreparedCodeSearchItem`, `preflight_source_batch`, `preflight_code_search_batch`, and `partition_expanded_budget`.

- [ ] **Step 1: Write failing deterministic budget tests**

```rust
#[test]
fn partitions_remainder_by_stable_input_order() {
    assert_eq!(partition_expanded_budget(10, 3).unwrap(), vec![4, 3, 3]);
}

#[test]
fn rejects_budget_smaller_than_input_count() {
    assert!(partition_expanded_budget(2, 3).is_err());
}
```

- [ ] **Step 2: Write failing side-effect-free preflight tests**

Use fake acquisition/provider/job capabilities that panic on access. Test that an invalid or unauthorized final input produces no enqueue, reservation, artifact, ledger, or vector call for earlier items.

- [ ] **Step 3: Verify tests fail**

```bash
cargo test -p axon-services projection_preflight --locked
cargo test -p axon-services projection_budget --locked
```

- [ ] **Step 4: Implement budget partitioning and request persistence**

Return deterministic `Vec<u32>` quotas, apply them to `SourceLimits.max_pages`/`max_items` or code-search result limits, and ensure the effective limits serialize inside each canonical request stored in a detached job.

- [ ] **Step 5: Implement whole-batch preflight**

Expose:

```rust
pub fn preflight_source_batch(
    operation: ProjectionOperation,
    requests: Vec<SourceRequest>,
    auth: Option<&AuthSnapshot>,
    policy: &ProjectionBatchPolicy,
    access: &SourceAccessPolicy,
) -> Result<ProjectionPreflight<PreparedSourceItem>, ApiError>;

pub fn preflight_code_search_batch(
    plans: Vec<CodeSearchPlan>,
    policy: &ProjectionBatchPolicy,
) -> Result<ProjectionPreflight<PreparedCodeSearchItem>, ApiError>;
```

The source function may classify, route, and authorize only. It must not invoke async acquisition or obtain provider handles.

- [ ] **Step 6: Run service preflight/security tests**

```bash
cargo test -p axon-services projection_preflight --locked
cargo test -p axon-services source_security --locked
```

- [ ] **Step 7: Commit preflight**

```bash
git add crates/axon-services/src/projections* crates/axon-services/src/lib.rs crates/axon-services/src/source/{authorize,routing}.rs
git commit -m "feat(services): preflight projection batches"
```

### Task 6: Shared Batch Execution and Observability

**Files:**
- Create: `crates/axon-services/src/projections/execute.rs`
- Create: `crates/axon-services/src/projections/execute_tests.rs`
- Create: `crates/axon-services/src/projections/events.rs`
- Create: `crates/axon-services/src/projections/events_tests.rs`
- Modify: `crates/axon-services/src/source/enqueue.rs`
- Modify: `crates/axon-services/src/query/code_search.rs`
- Modify: `crates/axon-services/src/query/code_search_tests.rs`
- Modify: `crates/axon-observe/src/schema_registry.rs`
- Modify: `crates/axon-observe/src/metric.rs`

**Interfaces:**
- Consumes: `ProjectionPreflight<T>`, `JobStore::create_batch_atomic`, `index_source_with_auth`, and `query::code_search`.
- Produces: `execute_source_projection_batch`, `enqueue_source_projection_batch`, `execute_code_search_projection_batch`, `CodeSearchCaller::Rest`, and redacted batch lifecycle events.

- [ ] **Step 1: Add failing inline/mixed/detached executor tests**

Test stable output order under concurrency, configured concurrency ceiling, mixed runtime results returning `BatchStatus::CompletedDegraded`, synchronous input echo, detached omission, and `202` only after atomic admission.

- [ ] **Step 2: Add failing observability-redaction tests**

Assert accepted/started/completed events contain batch ID, operation, counts, duration, and exhaustion flags while excluding raw URLs, queries, paths, headers, and idempotency keys.

- [ ] **Step 3: Verify tests fail**

```bash
cargo test -p axon-services projection_execute --locked
cargo test -p axon-services projection_events --locked
```

- [ ] **Step 4: Implement bounded ordered execution**

Use `futures::stream::iter(items).buffer_unordered(policy.max_concurrency)` with each future still entering existing scheduler/provider reservation paths. Collect `(index, result)`, sort by index, and build one `BatchResult<T>`.

- [ ] **Step 5: Implement atomic detached projection admission**

Build every `JobCreateRequest` with the shared `batch_id`, persisted effective limits, scoped idempotency fingerprint metadata, and canonical auth/config snapshots; call `create_batch_atomic` once.

- [ ] **Step 6: Implement code-search plans over the existing service**

Map `PreparedCodeSearchItem` into the existing `CodeSearchOptions`, add `CodeSearchCaller::Rest` with the same explicit-root safety rules as MCP, add any missing language/source filters at the service request boundary, and do not reintroduce `code-search-watch` or a separate indexer.

- [ ] **Step 7: Run service and observe tests**

```bash
cargo test -p axon-services projection_ --locked
cargo test -p axon-observe batch --locked
```

- [ ] **Step 8: Commit shared execution**

```bash
git add crates/axon-services/src/projections* crates/axon-services/src/source/enqueue.rs crates/axon-services/src/query/code_search* crates/axon-observe/src
git commit -m "feat(services): execute restored projection batches"
```

### Task 7: CLI Commands and Output Safety

**Files:**
- Modify: `crates/axon-core/src/config/cli.rs`
- Modify: `crates/axon-core/src/config/cli_tests.rs`
- Modify: `crates/axon-core/src/config/parse/build_config/command_dispatch.rs`
- Modify: `crates/axon-core/src/config/parse/build_config_tests.rs`
- Modify: `crates/axon-core/src/config/types/enums.rs`
- Create: `crates/axon-cli/src/commands/projections.rs`
- Create: `crates/axon-cli/src/commands/projections_tests.rs`
- Modify: `crates/axon-cli/src/commands.rs`
- Modify: `crates/axon-cli/src/lib.rs`
- Modify: `crates/axon-cli/src/commands/source.rs`
- Modify: `crates/axon-cli/src/commands/source/batch.rs`
- Modify: `crates/axon-cli/src/commands/query.rs`
- Modify: `crates/axon-cli/src/schema_registry.rs`
- Modify: `crates/axon-cli/src/scrape_map_source_projection_tests.rs`

**Interfaces:**
- Consumes: focused Task 1 DTOs and Task 6 execution functions.
- Produces: real clap commands `scrape`, `crawl`, `embed`, `ingest`, and `code-search`, all rendering `BatchResult<T>`.

- [ ] **Step 1: Add failing parser/help tests for all commands**

Assert focused flags, repeated positional inputs, fixed-option rejection, `code-search` hyphen spelling, and absence from the removed-command registry.

- [ ] **Step 2: Add failing keyed-input pairing tests**

Cover valid repeated `--input/--idempotency-key` groups; reject leading keys, duplicate keys, unpaired inputs, positional/keyed mixing, and all code-search idempotency flags.

- [ ] **Step 3: Add failing output-path tests**

Cover one-input `--output`, multi-input rejection, `--output-dir`, `{index}`/`{input_hash}` templates, normalized collision rejection, JSON batch stdout, and stderr-only progress.

- [ ] **Step 4: Verify CLI tests fail**

```bash
cargo test -p axon-core projection --locked
cargo test -p axon-cli projection --locked
```

- [ ] **Step 5: Implement clap/config dispatch**

Add `CommandKind::{Crawl, Embed, Ingest, CodeSearch}` and focused argument structs. Preserve current scrape inline/file behavior for a single item while routing its request construction through Task 1.

- [ ] **Step 6: Implement CLI execution and rendering**

`run_projection` builds the focused DTO, calls the shared service function, prints exactly one JSON envelope under `--json`, and delegates human output to focused render helpers. Do not loop over the single-source CLI executor.

- [ ] **Step 7: Run CLI checks**

```bash
cargo test -p axon-core projection --locked
cargo test -p axon-cli projection --locked
cargo xtask schemas cli --check
```

- [ ] **Step 8: Commit CLI surfaces**

```bash
git add crates/axon-core/src/config crates/axon-cli/src docs/reference/cli
git commit -m "feat(cli): restore focused source commands"
```

### Task 8: MCP Actions, Scope Gates, and Schema

**Files:**
- Modify: `crates/axon-api/src/mcp_schema.rs`
- Modify: `crates/axon-api/src/mcp_schema/requests.rs`
- Modify: `crates/axon-api/src/mcp_schema_tests.rs`
- Create: `crates/axon-mcp/src/server/handlers_projections.rs`
- Create: `crates/axon-mcp/src/server/handlers_projections_tests.rs`
- Modify: `crates/axon-mcp/src/server.rs`
- Modify: `crates/axon-mcp/src/server/authz.rs`
- Modify: `crates/axon-mcp/src/server/tasks.rs`
- Modify: `crates/axon-mcp/src/server/tool_schema.rs`
- Modify: `crates/axon-mcp/src/server/tool_schema_tests.rs`
- Modify: `crates/axon-mcp/src/schema_registry.rs`
- Modify: `crates/axon-mcp/tests/golden/tool-schema.json`

**Interfaces:**
- Consumes: Task 1 DTOs and Task 6 execution functions.
- Produces: `action=scrape|crawl|embed|ingest|code_search`, with write/read scopes and canonical batch envelopes.

- [ ] **Step 1: Add failing MCP parse/schema/dispatch tests**

Assert the five actions parse, removed guidance no longer rejects them, schemas reference canonical DTOs, action help/capabilities derive from the registry, and `code_search` rejects idempotency.

- [ ] **Step 2: Add failing authorization/preflight tests**

Test write scope for four source actions, read scope for code search, local/execute fine-grained scopes for every source item, and zero execution when the last item is denied.

- [ ] **Step 3: Verify MCP tests fail**

```bash
cargo test -p axon-api mcp_schema --locked
cargo test -p axon-mcp projections --locked
```

- [ ] **Step 4: Implement enum/registry/authz changes**

Add the five `AxonRequest` variants and dispatch arms. Remove only these names from `removed_action_guidance`; keep `vertical_scrape`, `code_search_watch`, purge, and dedupe removed.

- [ ] **Step 5: Implement thin MCP handlers**

Handlers obtain the caller auth snapshot, invoke whole-batch preflight once, then call the shared service executor. They must not resolve or authorize individual items after execution begins.

- [ ] **Step 6: Run MCP schema and smoke-shape checks**

```bash
cargo test -p axon-mcp projections --locked
cargo test -p axon-mcp tool_schema --locked
cargo xtask schemas mcp --check
```

- [ ] **Step 7: Commit MCP surfaces**

```bash
git add crates/axon-api/src/mcp_schema* crates/axon-mcp/src crates/axon-mcp/tests/golden docs/reference/mcp
git commit -m "feat(mcp): restore focused projection actions"
```

### Task 9: REST Routes, OpenAPI, and Loopback Guard

**Files:**
- Create: `crates/axon-web/src/server/handlers/projections.rs`
- Create: `crates/axon-web/src/server/handlers/projections_tests.rs`
- Modify: `crates/axon-web/src/server/handlers.rs`
- Modify: `crates/axon-web/src/server/handlers/sources.rs`
- Modify: `crates/axon-web/src/server/handlers/rag.rs`
- Modify: `crates/axon-web/src/server/routing.rs`
- Modify: `crates/axon-web/src/server/routing_loopback_guard.rs`
- Modify: `crates/axon-web/src/server/routing_loopback_guard_tests.rs`
- Modify: `crates/axon-web/src/schema_registry.rs`
- Modify: `crates/axon-web/src/server/openapi.rs`
- Modify: `crates/axon-web/src/server_tests.rs`

**Interfaces:**
- Produces: `POST /v1/scrape`, `/v1/crawl`, `/v1/embed`, `/v1/ingest`, and `/v1/code-search` with operation IDs `scrapeSources`, `crawlSources`, `embedSources`, `ingestSources`, and `codeSearch`.

- [ ] **Step 1: Add failing route/OpenAPI tests**

Assert every route mounts, accepts single/batch DTOs, pins the exact operation ID, appears in the schema registry, and maps preflight/runtime statuses to `4xx`/`409`/`200`/`202`/`5xx` as specified.

- [ ] **Step 2: Add failing auth and loopback tests**

Verify source routes use broad write plus per-target fine-grained checks, code search uses read, loopback guard treats all mutating projection routes like `/v1/sources`, and a denied final item leaves no jobs.

- [ ] **Step 3: Verify web tests fail**

```bash
cargo test -p axon-web projections --locked
cargo test -p axon-web routing_loopback_guard --locked
```

- [ ] **Step 4: Extract shared source/query handler seams**

Keep transport mechanics in web while moving no domain logic into handlers. `sources.rs` should expose an internal function accepting an already-preflighted batch/service context; `rag.rs` should expose the equivalent query seam.

- [ ] **Step 5: Implement routes and explicit utoipa operation IDs**

Each handler deserializes the Task 1 DTO, resolves auth once, runs whole-batch preflight, calls Task 6, and returns the shared envelope without an internal HTTP call.

- [ ] **Step 6: Run web/OpenAPI checks**

```bash
cargo test -p axon-web projections --locked
cargo test -p axon-web server_tests --locked
cargo xtask schemas openapi --check
```

- [ ] **Step 7: Commit REST surfaces**

```bash
git add crates/axon-web/src docs/reference/rest apps/web/openapi/axon.json
git commit -m "feat(rest): add focused projection endpoints"
```

### Task 10: Cross-Surface Parity, Layering, Clients, and Living Docs

**Files:**
- Create: `tests/source_projection_contract.rs`
- Create: `tests/source_projection_security.rs`
- Modify: `tests/cross_surface_operation_matrix.rs`
- Modify: `tests/cross_surface_scope_matrix.rs`
- Modify: `tests/workflow_shapes.rs`
- Modify: `xtask/src/checks/layering.rs` and its focused modules/tests
- Modify: `xtask/src/schemas/removed_registry.rs`
- Modify: `crates/axon-cli/src/CLAUDE.md`
- Modify: `crates/axon-mcp/src/CLAUDE.md`
- Modify: `crates/axon-web/src/CLAUDE.md`
- Modify: `CLAUDE.md`
- Modify generated web/Palette client files selected by OpenAPI generation

**Interfaces:**
- Consumes: all transport parsers and Task 2 fixtures.
- Produces: byte-normalized cross-surface parity proof and enforced transport/domain dependency boundaries.

- [ ] **Step 1: Add failing cross-surface normalization tests**

For every minimal/full/single/batch fixture, parse CLI, MCP JSON, and REST JSON, normalize to the canonical request list, and assert equality. Compare MCP/REST JSON and CLI `--json` against the same golden `BatchResult<T>`.

- [ ] **Step 2: Add failing layering tests**

Reject restored transport modules importing `axon-adapters`, `axon-embedding`, `axon-vectors`, `axon-ledger`, concrete job stores, acquisition clients, or internal `::ops::*` modules. Permit only `axon-api`, `axon-core` transport config, and `axon-services` facades.

- [ ] **Step 3: Add failing removed-registry tests**

Assert the five restored names are absent from removed registries while `vertical_scrape`, `code-search-watch`, purge, dedupe, refresh, and fresh remain rejected.

- [ ] **Step 4: Verify contract tests fail, then update generated clients/docs**

```bash
cargo test --test source_projection_contract --locked
cargo test --test source_projection_security --locked
cargo xtask check-layering
```

Update living docs to call the five names supported projections. Leave dated historical pipeline-unification delivery documents unchanged except generator-owned projections.

- [ ] **Step 5: Refresh every generated contract and run drift checks**

```bash
cargo xtask generated-contracts refresh
cargo xtask generated-contracts check
```

- [ ] **Step 6: Run cross-surface and layering tests**

```bash
cargo test --test source_projection_contract --locked
cargo test --test source_projection_security --locked
cargo test --test cross_surface_operation_matrix --locked
cargo xtask check-layering
```

- [ ] **Step 7: Commit parity and generated outputs**

```bash
git add tests xtask/src/checks xtask/src/schemas/removed_registry.rs crates/*/src/CLAUDE.md CLAUDE.md docs/reference apps/web apps/palette-tauri
git commit -m "test: enforce restored projection parity"
```

### Task 11: Full Verification and Runtime Smoke

**Files:**
- Modify only if verification exposes a defect; keep fixes scoped to the owning task files and add a regression test beside each fix.

**Interfaces:**
- Produces: local contract/build/test evidence and safe configured-runtime proof. Hosted CI and deployed runtime remain separate claims.

- [ ] **Step 1: Run formatting and static contract gates**

```bash
cargo fmt --all -- --check
cargo xtask generated-contracts check
cargo xtask check-layering
cargo xtask check-no-mod-rs
cargo xtask check-fetch-divergence
```

Expected: PASS.

- [ ] **Step 2: Run focused crate suites**

```bash
cargo nextest run --locked -p axon-api -p axon-core -p axon-jobs -p axon-services -p axon-cli -p axon-mcp -p axon-web
```

Expected: PASS.

- [ ] **Step 3: Run compile/lint gates that cover test sidecars**

```bash
cargo test --no-run --workspace --features test-helpers --locked
cargo clippy --all-targets --locked -- -D warnings
```

Expected: PASS.

- [ ] **Step 4: Run the repository pre-PR gate**

```bash
just verify
```

Expected: PASS. If a failure is unrelated or environmental, record the exact command, exit status, and boundary instead of masking it.

- [ ] **Step 5: Run safe runtime smoke against configured providers**

Use bounded disposable inputs and machine-readable output:

```bash
./scripts/axon scrape --input https://example.com --json
./scripts/axon crawl --input https://example.com --json --wait true
./scripts/axon embed --input ./README.md --json --wait true
./scripts/axon ingest --input ./README.md --no-embed --json --wait true
./scripts/axon code-search --input "projection registry" --json
```

Then invoke equivalent MCP and REST batches against the configured local server and compare their normalized envelopes to CLI output. Do not claim production/deployed proof from these local calls.

- [ ] **Step 6: Inspect final worktree and commit verification fixes**

```bash
git status --short
git diff --check
git log --oneline --decorate -12
```

If verification required fixes, commit each with its regression test. Otherwise leave the already committed implementation history intact.

- [ ] **Step 7: Update the Bead only after every required local gate passes**

```bash
bd close axon_rust-twfx7
```

Record remaining hosted-CI, deployment, or fresh-client boundaries explicitly in the closeout rather than calling them complete.
