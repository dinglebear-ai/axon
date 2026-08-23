# Restored Source Surfaces Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Restore first-class `scrape`, `crawl`, `embed`, `ingest`, and `code-search` operations across CLI, MCP, and REST as batch-capable projections over Axon's existing source and query services.

**Architecture:** `axon-api` owns typed projection DTOs, pure projection rules, the operation registry, and generic batch envelopes. `axon-services` owns whole-batch preflight and execution; `axon-jobs` owns job-first atomic admission for foreground and detached calls, idempotency, and durable correlation. CLI, MCP, and REST parse and render only, and generated metadata is validated against the canonical registry.

**Tech Stack:** Rust 2024, Tokio, clap, serde/schemars/utoipa, Axum, rmcp, SQLx/SQLite, Axon xtask generators, Cargo nextest.

**Spec:** `docs/superpowers/specs/2026-08-23-restored-source-surfaces-design.md`

## Global Constraints

- Keep `axon <source>`, `axon source <source>`, MCP `action=source`/`action=query`, and `POST /v1/sources`/`POST /v1/query` working unchanged.
- Do not restore deleted command-specific crates, job kinds, workers, ledgers, acquisition paths, or vector publication paths.
- All five restored operations support the same one-or-many request semantics on CLI, MCP, and REST.
- `scrape` fixes page scope and one-page limits; `crawl` fixes site scope; `embed` forces publication; `ingest` may disable embedding; `code-search` fixes code retrieval.
- Whole-batch validation, routing, idempotency checks, and authorization complete before any side effect.
- Source admission is one SQLite transaction in both modes: every item is
  inserted/reused and correlated, or none are.
- Source idempotency is scoped by operation, opaque authenticated principal,
  and caller key; `code-search` rejects idempotency fields.
- Persist operation/stage-specific effective limits; never claim one generic unit bounds pages, files, chunks, bytes, vectors, and query hits.
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
    let item = BatchItem::<SourceResult> {
        index: 0,
        input: None,
        outcome: BatchOutcome::Queued(descriptor()),
    };
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(tag = "status", content = "data", rename_all = "snake_case")]
pub enum BatchOutcome<T> {
    Completed(T),
    Queued(JobDescriptor),
    Failed(SanitizedApiError),
    Canceled,
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

`CodeSearchPlan` carries query, collection, limit, offset, path prefix,
language/source filters, and hybrid controls. It must not include cwd-driven
refresh, `ensure_fresh`, or idempotency; projection forces committed-state
retrieval so `axon:read` cannot mutate local/vector state.

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

- [ ] **Step 4: Add bounded canonical fixture cases**

Create one minimal and one boundary fixture per operation plus tagged outcome,
detached disclosure, effective-limit, and `409` collision fixtures. Each fixture
contains `operation`, `transport_input`, `canonical_requests`, and
`expected_result`. Add the semantic registry-to-transport bijection harness now;
later transport tasks plug their pure parsers into it before dispatch wiring.
Compare canonical semantic fields while excluding principal/cwd/runtime context,
not byte-for-byte incidental transport state.

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

### Task 3: Typed Admission and Stage-Limit Configuration

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
- Produces: conservative count/body/input/key/result ceilings plus owning
  page, manifest-item, prepared-byte, document, chunk, vector-point, redirect,
  elapsed-time, and query-window ceilings. No generic expanded-item or
  per-request concurrency knob is introduced.

- [ ] **Step 1: Add failing default/TOML/env precedence tests**

Cover conservative defaults for maximum inputs, encoded request bytes,
per-input/query/idempotency-key bytes, aggregate decoded bytes, response bytes,
and each operation/stage unit. Include global caller/request admission/rate
limits. Pin every TOML/env name in the generated registry rather than inventing
one polymorphic ceiling.

- [ ] **Step 2: Verify tests fail on missing fields**

```bash
cargo test -p axon-core projection_batch --locked
```

- [ ] **Step 3: Implement config defaults, parsing, validation, and schema registration**

Reject zero, arithmetic overflow, and unsafe/inverted owning limits with exact
field paths. Enforce HTTP body bytes before deserialization and Unicode byte
length after decoding. Request values may only clamp downward using
`min(caller, fixed, server)`.

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
- Create: `crates/axon-jobs/src/unified/projection_admission.rs`
- Create: `crates/axon-jobs/src/unified/projection_admission_tests.rs`
- Modify: `crates/axon-jobs/src/unified.rs`
- Modify: `crates/axon-jobs/src/unified/ops.rs`
- Modify: `crates/axon-jobs/src/unified/schema.rs`
- Modify: `crates/axon-jobs/src/unified_codec.rs`
- Modify: `crates/axon-jobs/src/unified/event_ops.rs`
- Modify: `crates/axon-jobs/src/fake_store.rs`
- Modify: `crates/axon-jobs/src/migrations_tests.rs`

**Interfaces:**
- Consumes: `BatchId`, `JobCreateRequest`, opaque projection keys, and
  `RequestFingerprintV1`.
- Produces: `projection_batch_items`, principal-authorized ordered batch lookup,
  originating batch metadata in event JSON, and
  `JobStore::admit_projection_batch_atomic`.

- [ ] **Step 1: Write failing migration and DTO round-trip tests**

Assert `projection_batch_items(batch_id,item_index,job_id,operation,reused,
principal_id,created_at)`, unique ordered membership, job foreign key, lookup
index, event JSON propagation, principal-filtered lookup, and generated schema
visibility. Assert no physical `job_events.batch_id` column is introduced.

- [ ] **Step 2: Write failing atomic-admission tests**

```rust
#[tokio::test]
async fn projection_admission_rolls_back_every_job_on_collision() {
    let store = sqlite_store().await;
    store.create(conflict_owner()).await.unwrap();
    let before = store.list(all_jobs()).await.unwrap().items.len();
    let result = store
        .admit_projection_batch_atomic(batch(vec![valid(), conflicting()]))
        .await;
    assert!(result.is_err());
    assert_eq!(store.list(all_jobs()).await.unwrap().items.len(), before);
    assert!(store.find_by_key("new-key").await.unwrap().is_none());
}
```

Also test same-key/same-fingerprint duplicates within one request, same-key/
different-fingerprint rollback, mixed new/reused ordered descriptors, cross-batch
reuse membership, different principals, unauthorized lookup, cancellation before
commit, retry after committed response loss, and two-pool contention.

- [ ] **Step 3: Verify focused tests fail**

```bash
cargo test -p axon-jobs projection_admission --locked
cargo test -p axon-jobs migrations --locked
```

- [ ] **Step 4: Implement the migration and DTO/codec propagation**

Add only the projection association table/index in migration `0009`; preserve
the legacy global `jobs.idempotency_key` uniqueness and watch/recovery behavior.
Projection storage keys are opaque hashes of
`IdempotencyScopeV1(operation, principal)` plus the bounded caller key;
`RequestFingerprintV1` is stored in bounded metadata and compared with a
constant-time helper. Lifetime is explicitly the retained canonical job's
lifetime. Persist only opaque principal/scopes and minimum authorization
decisions—never email, token, secret config, or raw caller key. Preserve batch
metadata in existing event JSON instead of adding a redundant event column.

- [ ] **Step 5: Implement transactional bulk creation**

Extend the boundary:

```rust
#[async_trait]
pub trait JobStore: Send + Sync {
    async fn create(&self, request: JobCreateRequest) -> Result<JobDescriptor>;
    async fn admit_projection_batch_atomic(
        &self, admission: ProjectionBatchAdmission,
    ) -> Result<ProjectionBatchAdmissionResult>;
}
```

Validate/serialize bounded requests, allocate IDs, and compute keys/fingerprints
before `BEGIN IMMEDIATE`. Fetch all existing keys set-wise, compare in memory,
and bulk-insert jobs/stages/associations in bind-safe chunks within one short
transaction using the existing whole-operation busy/snapshot retry boundary.
A matching fingerprint reuses its job and adds current-batch membership; a
different fingerprint returns typed `409`. The fake store emulates atomicity.
Migration regression tests cover populated legacy/watch keys and prove ordinary
`create`, retry, recovery, and watch scheduling retain prior semantics. Add an
optional batch filter to the canonical jobs lookup backed by the new association
and assert its query plan uses the index.

- [ ] **Step 6: Run durable job and schema tests**

```bash
cargo test -p axon-jobs projection_admission --locked
cargo test -p axon-jobs migrations --locked
cargo xtask schemas database --check
```

- [ ] **Step 7: Commit durable admission**

```bash
git add crates/axon-api/src/source/job* crates/axon-api/src/source/lifecycle.rs crates/axon-jobs/src docs/reference/runtime/database-schema.*
git commit -m "feat(jobs): admit projection batches atomically"
```

### Task 5: Whole-Batch Preflight, Prepared Identity, and Enforceable Limits

**Files:**
- Create: `crates/axon-services/src/projections.rs`
- Create: `crates/axon-services/src/projections/preflight.rs`
- Create: `crates/axon-services/src/projections/preflight_tests.rs`
- Create: `crates/axon-services/src/projections/limits.rs`
- Create: `crates/axon-services/src/projections/limits_tests.rs`
- Modify: `crates/axon-services/src/lib.rs`
- Modify: `crates/axon-services/src/source/authorize.rs`
- Modify: `crates/axon-services/src/source/routing.rs`

**Interfaces:**
- Consumes: Task 1 projection requests, Task 3 config limits, canonical source router, and `AuthSnapshot`.
- Produces: prepared canonical targets and authorization evidence,
  operation-specific effective limits, `preflight_source_batch`, and
  `preflight_code_search_batch`.

- [ ] **Step 1: Write failing effective-limit tests**

```rust
#[test]
fn effective_limit_never_raises_caller_or_fixed_limit() {
    assert_eq!(effective_limit(Some(2), Some(1), 100).unwrap(), 1);
}

#[test]
fn oversized_unicode_input_is_measured_in_bytes() {
    assert!(validate_input_bytes("🦀🦀", 7).is_err());
}
```

- [ ] **Step 2: Write failing side-effect-free preflight tests**

Use panic-on-access fakes. An invalid/unauthorized final input creates no job,
idempotency claim, reservation, artifact, ledger, or vector call. Add
redirect-to-metadata, IPv6-mapped-private, DNS-change, `..`, absolute escape,
and symlink-swap cases across web/local/git/tool families.

- [ ] **Step 3: Verify tests fail**

```bash
cargo test -p axon-services projection_preflight --locked
cargo test -p axon-services projection_limits --locked
```

- [ ] **Step 4: Implement owning limits and request persistence**

Compute `min(caller, fixed, server)` per owning unit and persist it in each
canonical job. Thread limits to the stage creating crawl pages, local/git/feed
manifest items, prepared bytes/documents, chunks/vector points, and query
windows/results; stop before unit `limit + 1`. Separately bound redirects,
elapsed time, fetched/decompressed bytes, and serialized results. Fail preflight
for an adapter that cannot enforce its declared unit.

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

The source function classifies, routes, authorizes, and constructs an immutable
prepared identity only. Execution consumes it without permissive rerouting and
rechecks each redirect/connect/open through canonical SSRF and allowed-root/
no-follow boundaries. Code search has no cwd refresh path.

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
- Consumes: prepared identities, `JobStore::admit_projection_batch_atomic`, the
  canonical worker/wait path, and committed-state `query::code_search`.
- Produces: `execute_source_projection_batch`,
  `enqueue_source_projection_batch`, `execute_code_search_projection_batch`,
  and redacted batch lifecycle events.

- [ ] **Step 1: Add failing inline/mixed/detached executor tests**

Test job-first foreground execution, stable sequential wait order, mixed runtime
outcomes, tagged outcome validity, synchronous input echo, detached omission,
and `202` only after atomic admission. Barrier tests using two service contexts
prove concurrent identical calls execute once and conflicting calls perform no
acquisition/provider/ledger/vector work. Add panic, timeout, cancellation, and
client-disconnect/response-loss recovery cases.

- [ ] **Step 2: Add failing observability-redaction tests**

Assert accepted/started/completed events contain safe fields and exclude raw
URLs, queries, paths, headers, tokens, secret config, and caller keys. Accepted
appears only after commit; an observe-store failure does not fail successful
work and increments a dropped-event counter. Sentinel-secret tests inspect
logs, traces, metrics, errors, SQLite, and artifacts.

- [ ] **Step 3: Verify tests fail**

```bash
cargo test -p axon-services projection_execute --locked
cargo test -p axon-services projection_events --locked
```

- [ ] **Step 4: Implement job-first ordered execution**

Admit every mutating item atomically, then either return queued descriptors or
wait sequentially for the admitted/reused canonical jobs in input order. Do not
add a per-request concurrency lane; the existing global scheduler/provider
reservations own fairness after restart. Build `Vec<Option<BatchItem<T>>>` by
index to avoid sorting/copying large results, and enforce aggregate serialized
response bytes.

- [ ] **Step 5: Implement shared foreground/detached admission**

Build one `ProjectionBatchAdmission` with bounded canonical requests, persisted
effective limits, opaque principal/scopes, opaque storage keys, and versioned
fingerprints. Never persist bearer tokens or secret config. Call
`admit_projection_batch_atomic` once for both modes and authorize every later
batch/job/event read against the association principal.

- [ ] **Step 6: Implement code-search plans over the existing service**

Map `PreparedCodeSearchItem` into `CodeSearchOptions` with `ensure_fresh=false`
and no cwd refresh controls. Add missing language/source filters, clamp
`offset + limit` to the canonical search window, and group identical normalized
plans so one embedding/Qdrant read fans out to duplicate ordered items. Do not
reintroduce an indexer/watch or a `CodeSearchCaller::Rest` mutation mode.

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

- [ ] **Step 2: Add failing self-contained item/request-file tests**

Cover repeatable `--item '{"input":...,"idempotency_key":...}'` and
`--request-file` using the canonical DTO. Reject unknown/duplicate fields,
malformed JSON, item/request-file/positional mixing, and every code-search
idempotency field. Avoid adjacency-sensitive clap occurrence pairing.

- [ ] **Step 3: Add failing output-path tests**

Cover one-input `--output`, multi-input rejection, `--output-dir`, restricted
`{index}`/`{input_hash}` templates, canonical-root containment, `../`/absolute
escape, symlink/hardlink swaps, case/Unicode collisions, existing targets,
ENOSPC/rename cleanup, JSON batch stdout, and stderr-only progress.

- [ ] **Step 4: Verify CLI tests fail**

```bash
cargo test -p axon-core projection --locked
cargo test -p axon-cli projection --locked
```

- [ ] **Step 5: Implement clap/config dispatch**

Add `CommandKind::{Crawl, Embed, Ingest, CodeSearch}` and focused argument structs. Preserve current scrape inline/file behavior for a single item while routing its request construction through Task 1.

- [ ] **Step 6: Implement CLI execution and rendering**

`run_projection` builds the focused DTO, completes output-path preflight before
admission, calls the shared service facade, prints exactly one JSON envelope
under `--json`, and delegates human output to focused render helpers. File
writes use create-new/no-follow temp files and atomic no-clobber rename. Do not
loop over the single-source CLI executor.

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

Assert the five actions parse, removed guidance no longer rejects them, schemas
reference canonical DTOs, action metadata matches the registry bijectively, and
`code_search` rejects idempotency, cwd, and freshness controls.

- [ ] **Step 2: Add failing authorization/preflight tests**

Test write scope for four source actions, read-only committed-state code search,
local/execute fine-grained scopes for every source item, principal-bound
batch/job lookup, and zero admission/execution when the last item is denied.

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

Assert every route mounts, accepts canonical one-or-many DTOs, pins the exact
operation ID, matches registry metadata, enforces body size before
deserialization, and maps statuses to `4xx`/`409`/`413`/`429`/`200`/`202`/`5xx`.

- [ ] **Step 2: Add failing auth and loopback tests**

Verify source routes use broad write plus per-target checks, code search uses
read and cannot refresh/index, loopback guard treats mutations like
`/v1/sources`, retrieval is principal-bound, and a denied final item leaves no
jobs or idempotency reservation.

- [ ] **Step 3: Verify web tests fail**

```bash
cargo test -p axon-web projections --locked
cargo test -p axon-web routing_loopback_guard --locked
```

- [ ] **Step 4: Keep orchestration in the public services facade**

Handlers call `axon-services::projections` directly. Transport-local helpers may
map typed errors/status/body only; do not expose source/rag handler-to-handler
seams or let web modules become a pseudo-service layer.

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

Complete the Task 2 semantic harness: each explicit CLI/MCP/REST declaration
must match registry name, mutation class, schema type, and result type
bijectively. Parse representative minimal/boundary requests into the same
canonical semantic fields while excluding legitimate auth/cwd/runtime context.
Compare tagged batch outcomes, not incidental byte serialization context.

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

### Task 11: Failure-Mode Matrix, Full Verification, and Runtime Smoke

**Files:**
- Modify only if verification exposes a defect; keep fixes scoped to the owning task files and add a regression test beside each fix.

**Interfaces:**
- Produces: local contract/build/test evidence and safe configured-runtime proof. Hosted CI and deployed runtime remain separate claims.

- [ ] **Step 1: Complete the production failure-mode matrix**

Add focused regression rows/tests for projection validation; principal
resolution; URL/path validation at use; owning-limit exhaustion; admission
reuse/collision/busy/disk-full/cancel/commit ambiguity; worker panic/timeout/
cancel; response serialization/disconnect; telemetry failure; CLI create/write/
rename; MCP/REST body/auth/rate limiting. Every row records rescue, test,
user-visible typed outcome, and safe logging. No silent unrescued/untested row
may remain.

- [ ] **Step 2: Run formatting and static contract gates**

```bash
cargo fmt --all -- --check
cargo xtask generated-contracts check
cargo xtask check-layering
cargo xtask check-no-mod-rs
cargo xtask check-fetch-divergence
```

Expected: PASS.

- [ ] **Step 3: Run focused crate suites**

```bash
cargo nextest run --locked -p axon-api -p axon-core -p axon-jobs -p axon-services -p axon-cli -p axon-mcp -p axon-web
```

Expected: PASS.

- [ ] **Step 4: Run compile/lint gates that cover test sidecars**

```bash
cargo test --no-run --workspace --features test-helpers --locked
cargo clippy --all-targets --locked -- -D warnings
```

Expected: PASS.

- [ ] **Step 5: Run the repository pre-PR gate**

```bash
just verify
```

Expected: PASS. If a failure is unrelated or environmental, record the exact command, exit status, and boundary instead of masking it.

- [ ] **Step 6: Run safe runtime smoke against configured providers**

Use bounded disposable inputs and machine-readable output:

```bash
./scripts/axon scrape --input https://example.com --json
./scripts/axon crawl --input https://example.com --json --wait true
./scripts/axon embed --input ./README.md --json --wait true
./scripts/axon ingest --input ./README.md --no-embed --json --wait true
./scripts/axon code-search --input "projection registry" --json
```

Then invoke equivalent MCP and REST batches against the configured local server and compare their normalized envelopes to CLI output. Do not claim production/deployed proof from these local calls.

- [ ] **Step 7: Inspect final worktree and commit verification fixes**

```bash
git status --short
git diff --check
git log --oneline --decorate -12
```

If verification required fixes, commit each with its regression test. Otherwise leave the already committed implementation history intact.

- [ ] **Step 8: Update the Bead only after every required local gate passes**

```bash
bd close axon_rust-twfx7
```

Record remaining hosted-CI, deployment, or fresh-client boundaries explicitly in the closeout rather than calling them complete.
