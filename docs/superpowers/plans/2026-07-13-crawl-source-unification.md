# Crawl Source Unification Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Finish epic `axon_rust-ruzox.16` by moving web page, site/docs crawl, scrape, map, watch, refresh, search, and research indexing onto the unified `SourceRequest` pipeline with one durable Source job identity.

**Architecture:** Keep `SourceRequest -> route -> WebSourceAdapter -> SourceLedger -> SourceDocument -> prepare/embed/vector publish -> cleanup` as the single runtime path. Retain `axon scrape <url>` as a CLI-only convenience over `SourceRequest { scope=page, embed=true }`, reserve `axon crawl`, and remove normal `JobKind::Crawl` creation after every caller has moved to Source jobs.

**Tech Stack:** Rust workspace, Clap, Tokio, SQLite unified jobs, `axon-api` DTOs, `axon-services` orchestration, `axon-adapters` web acquisition, `axon-ledger`, `axon-core::boundary::ArtifactStore`, TEI embedding provider, Qdrant vector store, `cargo xtask` schema generators, Beads (`bd`) for epic tracking.

## Global Constraints

- Retain CLI `axon scrape <url>` as canonical single-page acquisition: exactly one page, `scope=page`, `embed=true` by default, clean content output, no crawl fanout.
- Do not restore REST `/v1/scrape` or MCP `scrape`; REST and MCP use `/v1/sources` and MCP `action=source`.
- Reserve CLI `axon crawl <url>` and fail before source dispatch with replacement guidance.
- Express site/docs crawl through `SourceRequest.scope=site|docs`; do not create a normal `JobKind::Crawl` row.
- Express map as `SourceRequest { intent=map, scope=map, embed=false }` and do not upsert vectors.
- A web source operation has exactly one durable Source job id across job status, events, ledger rows, graph writes, artifacts, vector payloads, and cleanup debt.
- `embed=true` remains default for page/site/docs Source requests; generation publish succeeds only after required prepare/embed/vector writes succeed.
- WARC and clean-content outputs are `ArtifactStore` artifacts or explicit inline `OutputPolicy` results, not hidden durable path writes.
- HTTP 304 means reuse the previous committed representation or refetch; never publish a generation with missing content.
- SSRF protection denies DNS-resolved non-global addresses, private ranges, loopback, link-local, multicast, and `100.64.0.0/10`.
- Source metrics use bounded labels only: `phase`, `source_kind`, `scope`, `adapter`, `status`, `error_code`, and provider kind.
- URL, source item key, job id, request id, local path, header, token, chunk id, and document id values stay out of metric labels.
- This plan covers epic `axon_rust-ruzox.16`. Vertical extractor restoration remains bead `axon_rust-pmj7w`; this work must preserve the web adapter, parse, and enrichment seams that bead consumes.

---

## Scope Check

This epic spans CLI contract, job identity, web acquisition, artifacts, crawl cutover, map/scrape projections, background callers, observability, legacy removal, and final verification. The tasks below match the Beads gate order:

1. `.16.1` surface contract
2. `.16.2` single Source job identity
3. `.16.3` ETag/304 reuse
4. `.16.4` ArtifactStore output
5. `.16.5` site/docs crawl cutover
6. `.16.6` retained scrape and map projections
7. `.16.7` watch/refresh/search/research caller cutover
8. `.16.8` source events and metrics
9. `.16.9` legacy crawl removal and generated surface cleanup
10. `.16.10` final verification and closeout

The tasks are ordered so later tasks can run on a real unified Source job path rather than a simulated one.

## File Structure

### New Files

- `crates/axon-services/src/source/execution.rs` — carries the existing Source job id/auth/idempotency context through `index_source` into family dispatch.
- `crates/axon-services/src/web_source/job_execution.rs` — runs web indexing under either an existing Source job row or a newly created inline Source job row.
- `crates/axon-services/src/web_source/reuse.rs` — loads previous committed web representations for 304 reuse and builds reused `SourceDocument` handoffs.
- `crates/axon-services/src/web_source/artifacts.rs` — stores WARC and clean-content outputs through `ArtifactStore`.
- `crates/axon-cli/src/commands/scrape_source.rs` — retained one-page `scrape` CLI projection over `SourceRequest`.
- `crates/axon-services/src/search_source_index.rs` — source-backed bounded auto-index helper for search/research.
- `crates/axon-services/src/source/events.rs` — durable source progress event helper for migrated web phases.
- `crates/axon-observe/src/source_metrics.rs` — bounded-label source pipeline metrics helpers.
- `crates/axon-services/src/source_web_job_identity_tests.rs` — in-crate tests for exact-one Source job identity.
- `crates/axon-services/src/source_web_304_reuse_tests.rs` — in-crate tests for mixed modified/304/removed web runs.
- `crates/axon-services/src/source_web_artifacts_tests.rs` — in-crate tests for ArtifactStore-backed WARC/clean output.
- `crates/axon-services/src/source_web_crawl_cutover_tests.rs` — in-crate tests for site/docs Source jobs with no child Embed.
- `crates/axon-cli/src/scrape_map_source_projection_tests.rs` — CLI parser/JSON projection tests for scrape/map/crawl reservation.
- `crates/axon-services/src/source_auto_index_cutover_tests.rs` — watch/refresh/search/research source-job assertions.
- `crates/axon-services/src/source_observability_tests.rs` — event/metric coverage tests.
- `crates/axon-services/src/legacy_crawl_unreachable_tests.rs` — static and SQLite migration guards.

### Modified Files

- `crates/axon-api/src/source/enums.rs` — mark legacy job kinds as migration-only or split schema exposure from internal legacy rows.
- `crates/axon-api/src/source/lifecycle.rs` — extend `SourceResult.inline`/artifacts only when required by output policy.
- `crates/axon-core/src/config/cli.rs` — register `scrape`, keep `crawl` absent from Clap dispatch, and update help text.
- `crates/axon-core/src/config/source_routing.rs` — reserve removed command tokens before bare-source rewrite; exempt retained `scrape`.
- `crates/axon-core/src/config/source_routing_tests.rs` — update removed-token and scrape/crawl routing tests.
- `crates/axon-core/src/http.rs` and related SSRF tests — deny `100.64.0.0/10` and ensure redirect/sitemap/manifest URLs use DNS-aware validation.
- `crates/axon-adapters/src/web/acquire.rs` — return acquired/reused items for 304 and emit per-item warnings.
- `crates/axon-adapters/src/web/manifest_items.rs` — preserve ETag/last-modified/content hash metadata for reuse.
- `crates/axon-adapters/src/web/warc.rs` — return WARC bytes/provenance instead of treating a direct file path as durable state.
- `crates/axon-services/src/source.rs` — add execution-context overload, thread events, enforce publish fence.
- `crates/axon-services/src/test_support.rs` — extend existing flat test-support module with fake web/source runtime helpers used by the new in-crate tests.
- `crates/axon-services/src/source/dispatch.rs` — pass existing Source job execution context to the web family bridge.
- `crates/axon-services/src/web_source.rs` and `crates/axon-services/src/web_source/*` — reuse, artifacts, publish fence, map/scrape output, events, metrics.
- `crates/axon-services/src/runtime/job_runners/source_runner.rs` — call the execution-context overload with the claimed job id.
- `crates/axon-services/src/runtime/job_runners.rs` — remove normal Crawl runner registration after caller cutover.
- `crates/axon-services/src/runtime/job_runners/crawl_runner.rs` — delete or gate as migration-only.
- `crates/axon-services/src/runtime/sqlite/crawl_bridge.rs` — delete normal bridge or rename to migration-only legacy reader.
- `crates/axon-services/src/crawl.rs` — replace normal start path with reserved-token guidance or source enqueue shim used only by migration tests.
- `crates/axon-jobs/src/watch/dispatch.rs` — enqueue Source requests for watched web URLs.
- `crates/axon-services/src/refresh.rs` — refresh web origins as Source requests.
- `crates/axon-services/src/search_crawl.rs` — replace with `search_source_index` or make it a compatibility wrapper around Source enqueue.
- `crates/axon-cli/src/commands/map.rs` — project map to `SourceRequest { intent=map, scope=map, embed=false }`.
- `crates/axon-cli/src/commands/search.rs` and `crates/axon-cli/src/commands/refresh.rs` — consume source-backed auto-index status.
- `crates/axon-mcp/src/**` and `crates/axon-web/src/**` — keep source routes, remove crawl/scrape public routes/actions.
- `docs/pipeline-unification/**` and generated docs/schemas — update after behavior exists.

## Shared Interfaces

Implement these shared shapes before tasks that consume them.

```rust
// crates/axon-services/src/source/execution.rs
use axon_api::source::{AuthSnapshot, JobId, JobPriority, SourceRequest};

#[derive(Debug, Clone)]
pub struct SourceExecutionContext {
    pub existing_job_id: Option<JobId>,
    pub auth_snapshot: Option<AuthSnapshot>,
    pub priority: JobPriority,
    pub idempotency_key: Option<String>,
    pub request: SourceRequest,
}

impl SourceExecutionContext {
    pub fn inline(request: SourceRequest, auth_snapshot: Option<AuthSnapshot>) -> Self {
        let priority = request.execution.priority;
        let idempotency_key = request.idempotency_key.clone();
        Self {
            existing_job_id: None,
            auth_snapshot,
            priority,
            idempotency_key,
            request,
        }
    }

    pub fn existing_job(
        job_id: JobId,
        request: SourceRequest,
        auth_snapshot: Option<AuthSnapshot>,
    ) -> Self {
        let priority = request.execution.priority;
        let idempotency_key = request.idempotency_key.clone();
        Self {
            existing_job_id: Some(job_id),
            auth_snapshot,
            priority,
            idempotency_key,
            request,
        }
    }
}
```

```rust
// crates/axon-services/src/web_source/job_execution.rs
use axon_api::source::{JobId, JobPriority, MetadataMap};

#[derive(Debug, Clone)]
pub struct WebSourceJobExecution {
    pub job_id: JobId,
    pub owns_status: bool,
    pub priority: JobPriority,
    pub idempotency_key: Option<String>,
    pub metadata: MetadataMap,
}
```

```rust
// crates/axon-services/src/web_source/reuse.rs
use axon_api::source::{MetadataMap, SourceDocument, SourceGenerationId, SourceItemKey};

#[derive(Debug, Clone)]
pub struct ReusedWebRepresentation {
    pub source_item_key: SourceItemKey,
    pub generation: SourceGenerationId,
    pub document: SourceDocument,
    pub metadata: MetadataMap,
}
```

```rust
// crates/axon-services/src/web_source/artifacts.rs
use axon_api::source::{ArtifactRef, ContentRef, JobId, MetadataMap, SourceId};

#[derive(Debug, Clone)]
pub struct WebArtifactPayload {
    pub kind: axon_api::source::ArtifactKind,
    pub content_type: String,
    pub content: ContentRef,
    pub source_id: SourceId,
    pub job_id: JobId,
    pub metadata: MetadataMap,
}

#[derive(Debug, Clone)]
pub struct StoredWebArtifact {
    pub artifact: ArtifactRef,
    pub sha256: String,
    pub size_bytes: u64,
}
```

Test harness names used below are backed by the existing flat `crates/axon-services/src/test_support.rs` module under `#[cfg(test)]`. Keep the new tests in `crates/axon-services/src/*_tests.rs` and wire them with `#[cfg(test)] #[path = "..."] mod ...;` from the relevant module or `lib.rs`; integration tests under `crates/axon-services/tests/` cannot use this crate-private support module.

```rust
pub struct SourceRuntimeHarness {
    pub ctx: axon_services::context::ServiceContext,
}

impl SourceRuntimeHarness {
    pub async fn with_sqlite_and_fakes() -> Self {
        let ctx = source_context_with_fake_web().await;
        Self { ctx }
    }

    pub async fn run_source(
        &self,
        request: axon_api::source::SourceRequest,
    ) -> anyhow::Result<axon_api::source::SourceResult> {
        axon_services::source::index_source_with_auth(
            request,
            &self.ctx,
            Some(axon_api::source::AuthSnapshot::trusted_system("test")),
        )
        .await
    }
}

pub struct WebSourceHarness {
    pub runtime: SourceRuntimeHarness,
}

pub struct SearchAutoIndexHarness {
    pub runtime: SourceRuntimeHarness,
}

pub struct WatchDispatchHarness {
    pub runtime: SourceRuntimeHarness,
}
```

---

### Task 1: Executable CLI Surface Contract

**Files:**
- Modify: `crates/axon-core/src/config/source_routing.rs`
- Modify: `crates/axon-core/src/config/source_routing_tests.rs`
- Modify: `crates/axon-core/src/config/cli.rs`
- Modify: `crates/axon-cli/src/lib.rs`
- Test: `crates/axon-cli/src/scrape_map_source_projection_tests.rs`

**Interfaces:**
- Consumes: existing `route_bare_source(args: Vec<String>, command: &clap::Command) -> Vec<String>`.
- Produces: `ReservedCommandError`, `route_bare_source_or_error`, and a retained Clap `CliCommand::Scrape`.

- [ ] **Step 1: Write failing routing tests**

Add these tests to `crates/axon-core/src/config/source_routing_tests.rs`:

```rust
#[test]
fn crawl_is_reserved_and_does_not_route_as_source() {
    let command = build_cli_command();
    let args = vec![
        "axon".to_string(),
        "crawl".to_string(),
        "https://example.com".to_string(),
    ];
    let err = route_bare_source_or_error(args, &command).expect_err("crawl is reserved");
    assert_eq!(err.token(), "crawl");
    assert_eq!(
        err.replacement(),
        "Use `axon <url> --scope site` or `axon <url> --scope docs`."
    );
}

#[test]
fn retained_scrape_is_a_real_subcommand() {
    assert_eq!(
        route(&["axon", "scrape", "https://example.com"]),
        vec!["axon", "scrape", "https://example.com"]
    );
}

#[test]
fn removed_embed_ingest_code_search_are_reserved() {
    let command = build_cli_command();
    for removed in ["embed", "ingest", "code-search", "code-search-watch"] {
        let args = vec![
            "axon".to_string(),
            removed.to_string(),
            "https://example.com".to_string(),
        ];
        let err = route_bare_source_or_error(args, &command).expect_err("reserved command");
        assert_eq!(err.token(), removed);
    }
}
```

- [ ] **Step 2: Run routing tests to verify failure**

Run:

```bash
cargo test -p axon-core source_routing_tests -- --nocapture
```

Expected: compile failure for missing `route_bare_source_or_error` and `ReservedCommandError`.

- [ ] **Step 3: Implement reserved-token routing**

Replace `route_bare_source` internals in `crates/axon-core/src/config/source_routing.rs` with a fallible wrapper and keep the existing infallible function for callers:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReservedCommandError {
    token: String,
    replacement: &'static str,
}

impl ReservedCommandError {
    pub fn token(&self) -> &str {
        &self.token
    }

    pub fn replacement(&self) -> &'static str {
        self.replacement
    }
}

const RESERVED_COMMANDS: &[(&str, &str)] = &[
    (
        "crawl",
        "Use `axon <url> --scope site` or `axon <url> --scope docs`.",
    ),
    ("embed", "Use `axon <path-or-source>` with source options."),
    ("ingest", "Use `axon <source>` with the appropriate source URI."),
    ("code-search", "Use `axon <path> --scope directory`."),
    ("code-search-watch", "Use `axon <path> --watch`."),
];

pub fn route_bare_source_or_error(
    args: Vec<String>,
    command: &Command,
) -> Result<Vec<String>, ReservedCommandError> {
    route_bare_source_inner(args, command)
}

pub fn route_bare_source(args: Vec<String>, command: &Command) -> Vec<String> {
    match route_bare_source_or_error(args.clone(), command) {
        Ok(args) => args,
        Err(_) => args,
    }
}

fn route_bare_source_inner(
    args: Vec<String>,
    command: &Command,
) -> Result<Vec<String>, ReservedCommandError> {
    if args.len() < 2 {
        return Ok(args);
    }

    let subcommands: HashSet<String> = collect_subcommand_names(command);
    let value_flags: HashSet<String> = collect_value_taking_long_flags(command);
    let mut i = 1;
    while i < args.len() {
        let token = &args[i];
        if token == "--" {
            i += 1;
            break;
        }
        if is_long_flag(token) {
            if !token.contains('=') && flag_takes_value(token, &value_flags) {
                i += 2;
            } else {
                i += 1;
            }
            continue;
        }
        if is_short_flag(token) {
            i += 1;
            continue;
        }
        break;
    }

    if i >= args.len() {
        return Ok(args);
    }

    let candidate = &args[i];
    if subcommands.contains(candidate) || is_help_or_version(candidate) {
        return Ok(args);
    }
    if let Some((_, replacement)) = RESERVED_COMMANDS
        .iter()
        .find(|(token, _)| token == candidate)
    {
        return Err(ReservedCommandError {
            token: candidate.clone(),
            replacement,
        });
    }

    let mut rewritten = args;
    rewritten.insert(i, "source".to_string());
    Ok(rewritten)
}
```

- [ ] **Step 4: Wire the error into CLI startup**

In `crates/axon-cli/src/lib.rs`, change the argument rewrite call to return a user-facing error before Clap parses:

```rust
let command = axon_core::config::build_cli_command();
let args = match axon_core::config::route_bare_source_or_error(args, &command) {
    Ok(args) => args,
    Err(err) => {
        eprintln!(
            "`axon {}` has been removed from the unified source surface. {}",
            err.token(),
            err.replacement()
        );
        return Ok(8);
    }
};
```

- [ ] **Step 5: Register retained `scrape` in Clap**

Add the command variant to `crates/axon-core/src/config/cli.rs`:

```rust
/// Fetch/render/normalize exactly one web page and embed it by default
Scrape(ScrapeSourceArgs),
```

Add the args type in the same file:

```rust
#[derive(Debug, Args)]
pub(super) struct ScrapeSourceArgs {
    /// URL to scrape as exactly one page.
    pub(super) url: String,
    /// Skip vector embedding while still returning or saving clean content.
    #[arg(long = "no-embed", action = ArgAction::SetTrue)]
    pub(super) no_embed: bool,
    /// Return the cleaned page body inline when it fits the output policy.
    #[arg(long = "inline", action = ArgAction::SetTrue)]
    pub(super) inline: bool,
}
```

- [ ] **Step 6: Run routing and CLI parser tests**

Run:

```bash
cargo test -p axon-core source_routing_tests -- --nocapture
cargo test -p axon-cli scrape_map_source_projection -- --nocapture
```

Expected: the `axon-core` tests pass; the `axon-cli` test target fails until Task 6 creates the new file.

- [ ] **Step 7: Commit**

```bash
git add crates/axon-core/src/config/source_routing.rs crates/axon-core/src/config/source_routing_tests.rs crates/axon-core/src/config/cli.rs crates/axon-cli/src/lib.rs
git commit -m "feat(cli): reserve crawl and restore scrape surface"
```

### Task 2: Single Source Job Identity For Web Indexing

**Files:**
- Create: `crates/axon-services/src/source/execution.rs`
- Create: `crates/axon-services/src/web_source/job_execution.rs`
- Modify: `crates/axon-services/src/source.rs`
- Modify: `crates/axon-services/src/source/dispatch.rs`
- Modify: `crates/axon-services/src/web_source/web_source_job.rs`
- Modify: `crates/axon-services/src/runtime/job_runners/source_runner.rs`
- Test: `crates/axon-services/src/source_web_job_identity_tests.rs`

**Interfaces:**
- Consumes: `SourceRequest`, `AuthSnapshot`, `JobStore`, `LedgerStore`, existing `index_web_source`.
- Produces: `SourceExecutionContext`, `index_source_with_execution`, and `index_web_source_with_execution`.

- [ ] **Step 1: Write failing exact-one-job tests**

Create `crates/axon-services/src/source_web_job_identity_tests.rs` and wire it as an in-crate test module:

```rust
use axon_api::source::{AuthSnapshot, ExecutionMode, JobKind, SourceRequest, SourceScope};

#[tokio::test]
async fn detached_web_source_uses_claimed_source_job_id() {
    let harness = SourceRuntimeHarness::with_sqlite_and_fakes().await;
    let mut request = SourceRequest::new("https://docs.example.test/");
    request.scope = Some(SourceScope::Site);
    request.execution.mode = ExecutionMode::Background;

    let claimed = harness.enqueue_source_job(request.clone()).await;
    harness.run_source_job_once(&claimed).await.expect("source run");

    let jobs = harness.jobs_by_kind(JobKind::Source).await;
    assert_eq!(jobs.len(), 1, "web source path must not create a nested Source job");
    assert_eq!(jobs[0].job_id, claimed.job_id);

    let ledger = harness.source_summary_for("https://docs.example.test/").await;
    assert_eq!(ledger.last_job_id.as_ref(), Some(&claimed.job_id));
}

#[tokio::test]
async fn inline_web_source_creates_one_source_job() {
    let harness = SourceRuntimeHarness::with_sqlite_and_fakes().await;
    let mut request = SourceRequest::new("https://one.example.test/");
    request.scope = Some(SourceScope::Page);

    let result = harness
        .index_source_inline(request, Some(AuthSnapshot::trusted_system("test")))
        .await
        .expect("inline source");

    let jobs = harness.jobs_by_kind(JobKind::Source).await;
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].job_id, result.job_id);
}
```

In the same file, add the concrete harness body:

```rust
struct SourceRuntimeHarness {
    ctx: axon_services::context::ServiceContext,
}

impl SourceRuntimeHarness {
    async fn with_sqlite_and_fakes() -> Self {
        let ctx = axon_services::test_support::source_context_with_fake_web().await;
        Self { ctx }
    }

    async fn enqueue_source_job(
        &self,
        request: SourceRequest,
    ) -> axon_jobs::workers::unified::UnifiedClaimedJob {
        self.ctx
            .test_jobs()
            .enqueue_and_claim_source(request)
            .await
            .expect("enqueue source")
    }

    async fn run_source_job_once(
        &self,
        claimed: &axon_jobs::workers::unified::UnifiedClaimedJob,
    ) -> Result<(), axon_api::source::ApiError> {
        self.ctx.test_jobs().run_source_claim_once(claimed).await
    }

    async fn index_source_inline(
        &self,
        request: SourceRequest,
        auth: Option<AuthSnapshot>,
    ) -> anyhow::Result<axon_api::source::SourceResult> {
        axon_services::source::index_source_with_auth(request, &self.ctx, auth).await
    }

    async fn jobs_by_kind(&self, kind: JobKind) -> Vec<axon_api::source::JobSummary> {
        self.ctx.test_jobs().list_by_kind(kind).await
    }

    async fn source_summary_for(&self, source: &str) -> axon_api::source::SourceSummary {
        self.ctx.test_ledger().source_summary_for(source).await
    }
}
```

- [ ] **Step 2: Run the test to verify failure**

Run:

```bash
cargo test -p axon-services source_web_job_identity -- --nocapture
```

Expected: compile failure for `source_context_with_fake_web`, `test_jobs`, and `index_source_with_execution`.

- [ ] **Step 3: Add `SourceExecutionContext` and export it**

Create `crates/axon-services/src/source/execution.rs` using the shared interface at the top of this plan. In `crates/axon-services/src/source.rs`, add:

```rust
pub mod execution;

pub use execution::SourceExecutionContext;
```

Add the overload:

```rust
pub async fn index_source_with_execution(
    request: SourceRequest,
    ctx: &ServiceContext,
    execution: SourceExecutionContext,
) -> anyhow::Result<SourceResult> {
    index_source_inner(request, ctx, execution).await
}

pub async fn index_source_with_auth(
    request: SourceRequest,
    ctx: &ServiceContext,
    auth_snapshot: Option<AuthSnapshot>,
) -> anyhow::Result<SourceResult> {
    let execution = SourceExecutionContext::inline(request.clone(), auth_snapshot);
    index_source_inner(request, ctx, execution).await
}
```

Move the current body of `index_source_with_auth` into `index_source_inner`.

- [ ] **Step 4: Replace nested web job creation**

Create `crates/axon-services/src/web_source/job_execution.rs` using the shared interface. Replace `index_web_source_with_job` in `crates/axon-services/src/web_source/web_source_job.rs` with:

```rust
pub async fn index_web_source_with_execution(
    mut input: WebSourceIndexInput,
    execution: WebSourceJobExecution,
    jobs: &dyn JobStore,
    ledger: &dyn LedgerStore,
    embedding_provider: &dyn EmbeddingProvider,
    vector_store: &dyn VectorStore,
) -> anyhow::Result<WebSourceIndexOutput> {
    input.job_id = execution.job_id;

    if execution.owns_status {
        jobs.update_status(JobStatusUpdate {
            job_id: input.job_id.clone(),
            source_id: None,
            status: LifecycleStatus::Running,
            phase: PipelinePhase::Preparing,
            stage_id: None,
            counts: None,
            current: None,
            message: Some("web source indexing".to_string()),
            error: None,
        })
        .await?;
    }

    match index_web_source(input.clone(), ledger, embedding_provider, vector_store).await {
        Ok(output) => {
            if execution.owns_status {
                record_terminal_status(
                    jobs,
                    input.job_id,
                    LifecycleStatus::Completed,
                    Some(counts_for_output(&output)),
                    None,
                )
                .await?;
            }
            Ok(output)
        }
        Err(error) => {
            if execution.owns_status {
                let source_error = terminal_source_error(&error);
                record_terminal_status(
                    jobs,
                    input.job_id,
                    LifecycleStatus::Failed,
                    None,
                    Some(source_error),
                )
                .await?;
            }
            Err(error)
        }
    }
}
```

Keep a small create wrapper for inline callers:

```rust
pub async fn create_and_index_web_source_job(
    input: WebSourceIndexInput,
    jobs: &dyn JobStore,
    ledger: &dyn LedgerStore,
    embedding_provider: &dyn EmbeddingProvider,
    vector_store: &dyn VectorStore,
) -> anyhow::Result<WebSourceIndexOutput> {
    let descriptor = jobs.create(job_create_request(&input)).await?;
    let execution = WebSourceJobExecution {
        job_id: descriptor.job_id,
        owns_status: true,
        priority: input.priority,
        idempotency_key: input.idempotency_key.clone(),
        metadata: MetadataMap::new(),
    };
    index_web_source_with_execution(
        input,
        execution,
        jobs,
        ledger,
        embedding_provider,
        vector_store,
    )
    .await
}
```

- [ ] **Step 5: Thread the existing job id through web dispatch**

In `crates/axon-services/src/source/dispatch.rs`, pass `SourceExecutionContext` into `dispatch_web`. Build `WebSourceJobExecution` like this:

```rust
let execution = if let Some(job_id) = source_execution.existing_job_id.clone() {
    WebSourceJobExecution {
        job_id,
        owns_status: false,
        priority: source_execution.priority,
        idempotency_key: source_execution.idempotency_key.clone(),
        metadata: MetadataMap::new(),
    }
} else {
    let descriptor = runtime.jobs.create(web_job_create_request(&index_input)).await?;
    WebSourceJobExecution {
        job_id: descriptor.job_id,
        owns_status: true,
        priority: source_execution.priority,
        idempotency_key: source_execution.idempotency_key.clone(),
        metadata: MetadataMap::new(),
    }
};
```

Call `index_web_source_with_execution` with that execution value.

- [ ] **Step 6: Make `SourceRunner` pass the claimed job id**

In `crates/axon-services/src/runtime/job_runners/source_runner.rs`, replace the run call with:

```rust
let execution = crate::source::SourceExecutionContext::existing_job(
    claimed.job_id.clone(),
    source_request.clone(),
    Some(claimed.auth_snapshot.clone()),
);
let run_fut = crate::source::index_source_with_execution(source_request, ctx, execution);
```

- [ ] **Step 7: Run focused tests**

Run:

```bash
cargo test -p axon-services source_web_job_identity -- --nocapture
cargo test -p axon-services source_runner -- --nocapture
```

Expected: both pass; no nested `JobKind::Source` row appears in the detached web path.

- [ ] **Step 8: Commit**

```bash
git add crates/axon-services/src/source.rs crates/axon-services/src/source/execution.rs crates/axon-services/src/source/dispatch.rs crates/axon-services/src/test_support.rs crates/axon-services/src/web_source/web_source_job.rs crates/axon-services/src/web_source/job_execution.rs crates/axon-services/src/runtime/job_runners/source_runner.rs crates/axon-services/src/source_web_job_identity_tests.rs
git commit -m "fix(source): preserve one job id for web source indexing"
```

### Task 3: ETag And 304 Generation Reuse

**Files:**
- Create: `crates/axon-services/src/web_source/reuse.rs`
- Modify: `crates/axon-adapters/src/web/acquire.rs`
- Modify: `crates/axon-adapters/src/web/manifest_items.rs`
- Modify: `crates/axon-services/src/web_source/run.rs`
- Modify: `crates/axon-services/src/web_source/vectorize.rs`
- Test: `crates/axon-services/src/source_web_304_reuse_tests.rs`

**Interfaces:**
- Consumes: `SourceManifestDiff`, `SourceDocument`, `DocumentCache`, previous committed ledger generation.
- Produces: reused `SourceDocument` records for 304 items and embed-skip accounting.

- [ ] **Step 1: Write failing mixed 304 tests**

Create `crates/axon-services/src/source_web_304_reuse_tests.rs` and wire it as an in-crate test module:

```rust
use axon_api::source::{SourceRefreshPolicy, SourceRequest, SourceScope};

#[tokio::test]
async fn second_run_304_reuses_previous_document_without_embedding() {
    let harness = WebSourceHarness::with_conditional_fixture().await;
    let mut request = SourceRequest::new("https://docs.example.test/");
    request.scope = Some(SourceScope::Site);

    let first = harness.run_source(request.clone()).await.expect("first run");
    assert_eq!(first.counts.documents_total, 2);
    assert_eq!(harness.embedding_call_count().await, 2);

    harness.set_etag_response("/intro", 304, "");
    harness.set_etag_response("/guide", 304, "");
    request.refresh = SourceRefreshPolicy::Force;

    let second = harness.run_source(request).await.expect("second run");
    assert_eq!(second.counts.documents_total, 2);
    assert_eq!(second.counts.items_changed, 0);
    assert_eq!(harness.embedding_call_count().await, 2, "304 reused pages skip TEI");
    assert_eq!(harness.committed_manifest_item_count(&second.source_id).await, 2);
}

#[tokio::test]
async fn missing_prior_content_refetches_before_publish() {
    let harness = WebSourceHarness::with_conditional_fixture().await;
    harness.insert_etag_without_cached_document("/intro", "\"abc\"");

    let mut request = SourceRequest::new("https://docs.example.test/intro");
    request.scope = Some(SourceScope::Page);
    request.refresh = SourceRefreshPolicy::Force;

    let result = harness.run_source(request).await.expect("refetch fallback");
    assert_eq!(result.counts.documents_total, 1);
    assert_eq!(harness.full_fetch_count("/intro").await, 1);
}

#[tokio::test]
async fn mixed_modified_304_and_removed_counts_are_distinct() {
    let harness = WebSourceHarness::with_conditional_fixture().await;
    let mut request = SourceRequest::new("https://docs.example.test/");
    request.scope = Some(SourceScope::Site);
    harness.run_source(request.clone()).await.expect("first run");

    harness.set_etag_response("/intro", 304, "");
    harness.set_body("/guide", "# Updated guide");
    harness.remove_url("/gone");
    request.refresh = SourceRefreshPolicy::Force;

    let result = harness.run_source(request).await.expect("mixed run");
    assert_eq!(result.counts.items_changed, 1);
    assert_eq!(result.ledger.removed_items, 1);
    assert_eq!(harness.embedding_call_count().await, 3);
}
```

- [ ] **Step 2: Run the test to verify failure**

Run:

```bash
cargo test -p axon-services source_web_304_reuse -- --nocapture
```

Expected: compile failure for missing `reuse` module and harness helpers, or runtime failure showing 304 items are dropped or re-embedded.

- [ ] **Step 3: Persist conditional metadata on manifest items**

In `crates/axon-adapters/src/web/manifest_items.rs`, make every web manifest item carry stable conditional metadata:

```rust
pub(crate) fn attach_conditional_metadata(
    item: &mut SourceManifestItem,
    etag: Option<&str>,
    last_modified: Option<&str>,
) {
    if let Some(etag) = etag {
        item.metadata
            .insert("web_etag".to_string(), serde_json::json!(etag));
    }
    if let Some(last_modified) = last_modified {
        item.metadata.insert(
            "web_last_modified".to_string(),
            serde_json::json!(last_modified),
        );
    }
}
```

- [ ] **Step 4: Add previous representation loader**

Create `crates/axon-services/src/web_source/reuse.rs`:

```rust
use axon_api::source::*;
use axon_core::boundary::DocumentCache;
use axon_ledger::store::LedgerStore;

use super::ReusedWebRepresentation;

pub async fn load_reused_web_representation(
    ledger: &dyn LedgerStore,
    document_cache: &dyn DocumentCache,
    source_id: &SourceId,
    item_key: &SourceItemKey,
) -> anyhow::Result<Option<ReusedWebRepresentation>> {
    let Some(previous_generation) = ledger
        .last_committed_generation(source_id)
        .await?
    else {
        return Ok(None);
    };
    let cache_key = DocumentCacheKey {
        source_id: source_id.clone(),
        source_item_key: item_key.clone(),
        generation: Some(previous_generation.clone()),
    };
    let Some(cached) = document_cache.get(cache_key).await? else {
        return Ok(None);
    };
    Ok(Some(ReusedWebRepresentation {
        source_item_key: item_key.clone(),
        generation: previous_generation,
        document: cached.document,
        metadata: MetadataMap::new(),
    }))
}
```

- [ ] **Step 5: Return reused acquired items on 304**

In `crates/axon-adapters/src/web/acquire.rs`, replace 304 skip behavior with an acquired item marked as reused:

```rust
if response.status == 304 {
    let mut metadata = item.metadata.clone();
    metadata.insert("web_status".to_string(), serde_json::json!(304));
    metadata.insert("web_reuse_required".to_string(), serde_json::json!(true));
    return Ok(AcquiredSourceItem {
        manifest_item: item.clone(),
        content_ref: ContentRef::External {
            uri: format!("reuse://{}", item.item_key.0),
            integrity: item.content_hash.clone(),
        },
        metadata,
        warnings: Vec::new(),
    });
}
```

- [ ] **Step 6: Convert reuse-required items before preparing**

In `crates/axon-services/src/web_source/vectorize.rs`, split acquired items:

```rust
let mut documents = Vec::new();
let mut reused_documents = 0_u64;
for item in acquired_items {
    let reuse_required = item
        .metadata
        .get("web_reuse_required")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    if reuse_required {
        let reused = reuse::load_reused_web_representation(
            ledger,
            document_cache,
            &run.source_id,
            &item.manifest_item.item_key,
        )
        .await?;
        match reused {
            Some(reused) => {
                documents.push(reused.document);
                reused_documents += 1;
            }
            None => {
                let refetched = acquire_without_conditional(&item.manifest_item).await?;
                documents.push(normalize_acquired_item(refetched)?);
            }
        }
    } else {
        documents.push(normalize_acquired_item(item)?);
    }
}
```

Add `reused_documents` to the output metadata and exclude reused documents from embedding batches when the prepared document hash matches the previous committed document hash.

- [ ] **Step 7: Run focused tests**

Run:

```bash
cargo test -p axon-services source_web_304_reuse -- --nocapture
cargo test -p axon-adapters web:: -- --nocapture
```

Expected: mixed modified/304/removed runs commit a complete manifest, reused pages skip TEI, and missing previous content refetches.

- [ ] **Step 8: Commit**

```bash
git add crates/axon-adapters/src/web/acquire.rs crates/axon-adapters/src/web/manifest_items.rs crates/axon-services/src/test_support.rs crates/axon-services/src/web_source/reuse.rs crates/axon-services/src/web_source/run.rs crates/axon-services/src/web_source/vectorize.rs crates/axon-services/src/source_web_304_reuse_tests.rs
git commit -m "fix(web-source): reuse previous documents on 304"
```

### Task 4: ArtifactStore-Backed WARC And Clean Content Output

**Files:**
- Create: `crates/axon-services/src/web_source/artifacts.rs`
- Modify: `crates/axon-adapters/src/web/warc.rs`
- Modify: `crates/axon-adapters/src/web/acquire.rs`
- Modify: `crates/axon-services/src/web_source.rs`
- Modify: `crates/axon-services/src/web_source/publish.rs`
- Modify: `crates/axon-services/src/source/prune.rs`
- Test: `crates/axon-services/src/source_web_artifacts_tests.rs`

**Interfaces:**
- Consumes: `axon_core::boundary::ArtifactStore`, `ArtifactWriteRequest`, `OutputPolicy`, acquired web items.
- Produces: `StoredWebArtifact` with `ArtifactRef`, content hash, producer job/source metadata, and cleanup debt.

- [ ] **Step 1: Write failing artifact tests**

Create `crates/axon-services/src/source_web_artifacts_tests.rs` and wire it as an in-crate test module:

```rust
use axon_api::source::{ArtifactKind, ArtifactMode, ResponseMode, SourceRequest, SourceScope};

#[tokio::test]
async fn warc_output_is_artifact_store_backed() {
    let harness = WebSourceHarness::with_artifact_store().await;
    let mut request = SourceRequest::new("https://docs.example.test/");
    request.scope = Some(SourceScope::Site);
    request.output.artifact_mode = ArtifactMode::Always;
    request.options.values.insert(
        "warc".to_string(),
        serde_json::json!(true),
    );

    let result = harness.run_source(request).await.expect("source run");
    let warc = result
        .artifacts
        .iter()
        .find(|artifact| artifact.artifact_kind == ArtifactKind::Warc)
        .expect("warc artifact");

    assert!(warc.content_hash.as_ref().expect("hash").starts_with("sha256:"));
    let stored = harness.read_artifact(&warc.artifact_id).await;
    assert_eq!(stored.metadata["producer"], "web_source");
    assert_eq!(stored.metadata["job_id"], result.job_id.0);
    assert_eq!(stored.metadata["source_id"], result.source_id.0);
}

#[tokio::test]
async fn scrape_clean_content_respects_output_policy() {
    let harness = WebSourceHarness::with_artifact_store().await;
    let mut request = SourceRequest::new("https://docs.example.test/intro");
    request.scope = Some(SourceScope::Page);
    request.output.response_mode = ResponseMode::Inline;
    request.output.inline_limit_bytes = 4096;

    let result = harness.run_source(request).await.expect("source run");
    let inline = result.inline.expect("inline result");
    let content = inline.content.expect("content ref");
    assert!(harness.content_ref_text(&content).contains("Intro"));
}

#[tokio::test]
async fn cleanup_debt_deletes_superseded_artifacts() {
    let harness = WebSourceHarness::with_artifact_store().await;
    let first = harness.run_site_with_warc().await;
    let artifact_id = first.artifacts[0].artifact_id.clone();

    harness.remove_url("/intro");
    let second = harness.run_site_with_warc().await;
    harness.drain_cleanup_debt(&second.source_id).await;

    assert!(harness.artifact_missing(&artifact_id).await);
}
```

- [ ] **Step 2: Run the test to verify failure**

Run:

```bash
cargo test -p axon-services source_web_artifacts -- --nocapture
```

Expected: failure because WARC is still a direct path artifact and cleanup skips artifact debt.

- [ ] **Step 3: Make WARC generation return bytes and provenance**

In `crates/axon-adapters/src/web/warc.rs`, replace the file-handle API with:

```rust
pub(super) struct WarcArchive {
    pub bytes: Vec<u8>,
    pub sha256: String,
    pub size_bytes: u64,
}

pub(super) fn build_archive(items: &[AcquiredSourceItem]) -> WarcArchive {
    let mut bytes = warcinfo_record();
    for item in items {
        bytes.extend_from_slice(&response_record(item));
    }
    let digest = axon_core::hashing::sha256_hex(&bytes);
    let size_bytes = bytes.len() as u64;
    WarcArchive {
        bytes,
        sha256: format!("sha256:{digest}"),
        size_bytes,
    }
}
```

- [ ] **Step 4: Store web artifacts through `ArtifactStore`**

Create `crates/axon-services/src/web_source/artifacts.rs`:

```rust
use axon_api::source::*;
use axon_core::boundary::ArtifactStore;

use super::StoredWebArtifact;

pub async fn store_web_artifact(
    store: &dyn ArtifactStore,
    payload: WebArtifactPayload,
) -> anyhow::Result<StoredWebArtifact> {
    let size_bytes = content_size(&payload.content);
    let sha256 = content_sha256(&payload.content);
    let mut metadata = payload.metadata;
    metadata.insert("producer".to_string(), serde_json::json!("web_source"));
    metadata.insert("source_id".to_string(), serde_json::json!(payload.source_id.0));
    metadata.insert("job_id".to_string(), serde_json::json!(payload.job_id.0));
    metadata.insert("content_hash".to_string(), serde_json::json!(sha256.clone()));
    metadata.insert("size_bytes".to_string(), serde_json::json!(size_bytes));

    let handle = store
        .put(ArtifactWriteRequest {
            kind: payload.kind,
            content_type: payload.content_type,
            content: payload.content,
            source_id: Some(payload.source_id.clone()),
            job_id: Some(payload.job_id.clone()),
            metadata,
        })
        .await?;

    Ok(StoredWebArtifact {
        artifact: ArtifactRef {
            artifact_id: handle.artifact_id,
            artifact_kind: handle.artifact_kind,
            uri: handle.uri,
            size_bytes: Some(size_bytes),
            content_hash: Some(sha256.clone()),
            created_at: Some(Timestamp(chrono::Utc::now().to_rfc3339())),
        },
        sha256,
        size_bytes,
    })
}

fn content_size(content: &ContentRef) -> u64 {
    match content {
        ContentRef::InlineText { text } => text.len() as u64,
        ContentRef::InlineBytes { bytes_base64, .. } => bytes_base64.len() as u64,
        ContentRef::Artifact { .. } | ContentRef::External { .. } => 0,
    }
}

fn content_sha256(content: &ContentRef) -> String {
    let bytes = match content {
        ContentRef::InlineText { text } => text.as_bytes().to_vec(),
        ContentRef::InlineBytes { bytes_base64, .. } => bytes_base64.as_bytes().to_vec(),
        ContentRef::Artifact { artifact_id } => artifact_id.0.as_bytes().to_vec(),
        ContentRef::External { uri, integrity } => {
            integrity.clone().unwrap_or_else(|| uri.clone()).into_bytes()
        }
    };
    format!("sha256:{}", axon_core::hashing::sha256_hex(&bytes))
}
```

- [ ] **Step 5: Wire ArtifactStore into web source runtime**

Add an artifact store field to the target runtime used by web source indexing:

```rust
pub struct TargetLocalSourceRuntime {
    pub artifact_store: Arc<dyn axon_core::boundary::ArtifactStore>,
    // existing fields stay unchanged
}
```

Pass `runtime.artifact_store.as_ref()` into `web_source::index_web_source`.

- [ ] **Step 6: Attach artifacts to `SourceResult` and cleanup debt**

In `crates/axon-services/src/web_source/publish.rs`, after acquisition and before result mapping, store WARC/clean-content artifacts:

```rust
let warc_artifact = if input.output.artifact_mode != ArtifactMode::None && acquisition.warc.is_some() {
    let archive = acquisition.warc.expect("warc archive");
    Some(store_web_artifact(
        artifact_store,
        WebArtifactPayload {
            kind: ArtifactKind::Warc,
            content_type: "application/warc".to_string(),
            content: ContentRef::InlineBytes {
                bytes_base64: base64::engine::general_purpose::STANDARD.encode(archive.bytes),
                mime_type: "application/warc".to_string(),
            },
            source_id: run.source_id.clone(),
            job_id: input.job_id.clone(),
            metadata: MetadataMap::new(),
        },
    )
    .await?)
} else {
    None
};
```

In `crates/axon-services/src/source/prune.rs`, drain `ArtifactDelete` and `CachePrune` debt by calling `ArtifactStore::delete` and `DocumentCache::invalidate`.

- [ ] **Step 7: Run focused tests**

Run:

```bash
cargo test -p axon-services source_web_artifacts -- --nocapture
cargo test -p axon-core artifact -- --nocapture
```

Expected: WARC and clean content artifacts carry hash/source/job metadata, and cleanup debt deletes superseded artifacts.

- [ ] **Step 8: Commit**

```bash
git add crates/axon-adapters/src/web/warc.rs crates/axon-adapters/src/web/acquire.rs crates/axon-services/src/test_support.rs crates/axon-services/src/web_source.rs crates/axon-services/src/web_source/artifacts.rs crates/axon-services/src/web_source/publish.rs crates/axon-services/src/source/prune.rs crates/axon-services/src/source_web_artifacts_tests.rs
git commit -m "feat(web-source): store WARC and clean output artifacts"
```

### Task 5: Source-Backed Site And Docs Crawl Execution

**Files:**
- Modify: `crates/axon-services/src/source/dispatch.rs`
- Modify: `crates/axon-services/src/web_source.rs`
- Modify: `crates/axon-services/src/web_source/run.rs`
- Modify: `crates/axon-services/src/runtime/job_runners.rs`
- Modify: `crates/axon-services/src/runtime/job_runners/crawl_runner.rs`
- Modify: `crates/axon-core/src/http.rs`
- Test: `crates/axon-services/src/source_web_crawl_cutover_tests.rs`

**Interfaces:**
- Consumes: `SourceRequest.scope=site|docs`, web adapter discovery/acquire, artifact and reuse work from Tasks 3-4.
- Produces: site/docs Source jobs that prepare/embed/vectorize/publish without child Embed jobs.

- [ ] **Step 1: Write failing crawl-cutover tests**

Create `crates/axon-services/src/source_web_crawl_cutover_tests.rs` and wire it as an in-crate test module:

```rust
use axon_api::source::{JobKind, SourceRequest, SourceScope};

#[tokio::test]
async fn site_scope_indexes_multiple_pages_as_one_source_job() {
    let harness = WebSourceHarness::with_fixture_site().await;
    let mut request = SourceRequest::new("https://docs.example.test/");
    request.scope = Some(SourceScope::Site);

    let result = harness.run_source(request).await.expect("site source");
    assert_eq!(result.scope, SourceScope::Site);
    assert_eq!(result.counts.documents_total, 3);
    assert_eq!(result.counts.vector_points_total, harness.vector_points().await);

    assert_eq!(harness.jobs_by_kind(JobKind::Source).await.len(), 1);
    assert_eq!(harness.jobs_by_kind(JobKind::Crawl).await.len(), 0);
    assert_eq!(harness.jobs_by_kind(JobKind::Embed).await.len(), 0);
}

#[tokio::test]
async fn docs_scope_preserves_docs_url_filtering() {
    let harness = WebSourceHarness::with_fixture_site().await;
    let mut request = SourceRequest::new("https://docs.example.test/docs/");
    request.scope = Some(SourceScope::Docs);

    let result = harness.run_source(request).await.expect("docs source");
    let urls = harness.committed_urls(&result.source_id).await;
    assert!(urls.iter().all(|url| url.starts_with("https://docs.example.test/docs/")));
}

#[tokio::test]
async fn vector_failure_does_not_publish_generation() {
    let harness = WebSourceHarness::with_failing_vector_store().await;
    let mut request = SourceRequest::new("https://docs.example.test/");
    request.scope = Some(SourceScope::Site);

    let err = harness.run_source(request).await.expect_err("vector failure");
    assert!(err.to_string().contains("vector"));
    assert_eq!(harness.committed_generation_count().await, 0);
}

#[tokio::test]
async fn ssrf_denies_tailscale_and_private_targets() {
    let harness = WebSourceHarness::with_fixture_site().await;
    for url in [
        "http://100.64.0.1/",
        "http://100.120.242.29/",
        "http://127.0.0.1/",
        "http://169.254.169.254/",
    ] {
        let request = SourceRequest::new(url);
        let err = harness.run_source(request).await.expect_err("ssrf denial");
        assert!(err.to_string().contains("non-global"));
    }
}
```

- [ ] **Step 2: Run tests to verify failure**

Run:

```bash
cargo test -p axon-services source_web_crawl_cutover -- --nocapture
```

Expected: failure showing Crawl/Embed jobs still appear or SSRF allows `100.64.0.0/10`.

- [ ] **Step 3: Move site/docs execution into web Source bridge**

In `crates/axon-services/src/source/dispatch.rs`, ensure web dispatch maps scope and limits from `SourceRequest`:

```rust
let scope = request.scope.unwrap_or(SourceScope::Site);
let index_input = WebSourceIndexInput {
    source: request.source.clone(),
    scope,
    collection: collection.to_string(),
    owner_id: owner_id.to_string(),
    job_id: source_execution
        .existing_job_id
        .clone()
        .unwrap_or_else(placeholder_job_id),
    embed: request.embed,
    max_pages: request.limits.max_pages,
    max_depth: request.limits.max_depth,
    output: request.output.clone(),
    auth_snapshot: source_execution.auth_snapshot.clone(),
    idempotency_key: source_execution.idempotency_key.clone(),
    priority: source_execution.priority,
    route: Some(route.clone()),
    crawl_options: web_crawl_options(cfg, &request, route)?,
};
```

- [ ] **Step 4: Add publish fence around vector visibility**

In `crates/axon-services/src/web_source/publish.rs`, gate ledger publish after vector writes:

```rust
let vector_write = vectorize_and_upsert(prepared_docs, vector_store).await;
match vector_write {
    Ok(write_summary) => {
        ledger.publish_generation(generation_id.clone()).await?;
        Ok(write_summary)
    }
    Err(error) => {
        ledger.mark_generation_failed(generation_id.clone(), error.to_string()).await?;
        vector_store
            .delete_generation(&collection, &generation_id)
            .await
            .map_err(|cleanup| error.context(format!("cleanup failed: {cleanup}")))?;
        Err(error)
    }
}
```

- [ ] **Step 5: Deny `100.64.0.0/10` and enforce DNS-aware checks**

In `crates/axon-core/src/http.rs`, update the IP check:

```rust
fn is_denied_non_global(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(v4) => {
            v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_multicast()
                || v4.octets()[0] == 0
                || (v4.octets()[0] == 100 && (64..=127).contains(&v4.octets()[1]))
        }
        std::net::IpAddr::V6(v6) => {
            v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_unique_local()
                || v6.is_unicast_link_local()
                || v6.is_multicast()
        }
    }
}
```

Call the DNS-aware validator for initial URLs, redirects, sitemap URLs, llms.txt URLs, and manifest replay URLs.

- [ ] **Step 6: Remove normal CrawlRunner registration**

In `crates/axon-services/src/runtime/job_runners.rs`, remove `JobKind::Crawl` from the normal registry:

```rust
registry.register(JobKind::Source, Arc::new(SourceRunner::new(cfg.clone())));
registry.register(JobKind::Embed, Arc::new(EmbedRunner::new(cfg.clone())));
registry.register(JobKind::Ingest, Arc::new(IngestRunner::new(cfg.clone())));
registry.register(JobKind::Extract, Arc::new(ExtractRunner::new(cfg.clone())));
```

Do not register `CrawlRunner` in the default registry. Keep `crawl_runner.rs` present until Task 9 migrates or gates legacy rows.

- [ ] **Step 7: Run focused tests**

Run:

```bash
cargo test -p axon-services source_web_crawl_cutover -- --nocapture
cargo test -p axon-core http -- --nocapture
cargo xtask check-layering
```

Expected: Source site/docs runs create Source jobs only; vector failure leaves no committed generation; SSRF denial covers `100.64.0.0/10`.

- [ ] **Step 8: Commit**

```bash
git add crates/axon-services/src/source/dispatch.rs crates/axon-services/src/test_support.rs crates/axon-services/src/web_source.rs crates/axon-services/src/web_source/run.rs crates/axon-services/src/web_source/publish.rs crates/axon-services/src/runtime/job_runners.rs crates/axon-core/src/http.rs crates/axon-services/src/source_web_crawl_cutover_tests.rs
git commit -m "feat(source): run site and docs crawl through Source jobs"
```

### Task 6: Retained Scrape And Source-Backed Map CLI Projections

**Files:**
- Create: `crates/axon-cli/src/commands/scrape_source.rs`
- Modify: `crates/axon-cli/src/commands/map.rs`
- Modify: `crates/axon-cli/src/commands/source.rs`
- Modify: `crates/axon-cli/src/commands/mod.rs`
- Modify: `crates/axon-cli/src/lib.rs`
- Modify: `crates/axon-core/src/config/cli.rs`
- Test: `crates/axon-cli/src/scrape_map_source_projection_tests.rs`

**Interfaces:**
- Consumes: `SourceRequest`, `SourceScope::Page`, `SourceScope::Map`, `SourceIntent::Map`, `OutputPolicy`.
- Produces: CLI `scrape` and `map` commands that call the Source service and render `SourceResult`.

- [ ] **Step 1: Write failing CLI projection tests**

Create `crates/axon-cli/src/scrape_map_source_projection_tests.rs` and wire it as an in-crate test module:

```rust
#[test]
fn scrape_projects_to_page_source_request_with_embedding() {
    let result = axon_cli::test_support::run_cli_json([
        "axon",
        "scrape",
        "https://example.test/intro",
        "--json",
    ]);
    let request = result.captured_source_request();
    assert_eq!(request.source, "https://example.test/intro");
    assert_eq!(request.scope, Some(axon_api::source::SourceScope::Page));
    assert!(request.embed);
    assert_eq!(request.limits.max_pages, Some(1));
}

#[test]
fn scrape_no_embed_is_only_source_embed_false() {
    let result = axon_cli::test_support::run_cli_json([
        "axon",
        "scrape",
        "https://example.test/intro",
        "--no-embed",
        "--json",
    ]);
    let request = result.captured_source_request();
    assert_eq!(request.scope, Some(axon_api::source::SourceScope::Page));
    assert!(!request.embed);
}

#[test]
fn map_projects_to_map_intent_and_no_vectors() {
    let result = axon_cli::test_support::run_cli_json([
        "axon",
        "map",
        "https://example.test/",
        "--json",
    ]);
    let request = result.captured_source_request();
    assert_eq!(request.intent, axon_api::source::SourceIntent::Map);
    assert_eq!(request.scope, Some(axon_api::source::SourceScope::Map));
    assert!(!request.embed);
}
```

- [ ] **Step 2: Run test to verify failure**

Run:

```bash
cargo test -p axon-cli --test scrape_map_source_projection -- --nocapture
```

Expected: failure because `scrape_source.rs` and source-backed map projection do not exist.

- [ ] **Step 3: Implement `scrape` projection**

Create `crates/axon-cli/src/commands/scrape_source.rs`:

```rust
use axon_api::source::{
    ArtifactMode, ResponseMode, SourceIntent, SourceLimits, SourceRequest, SourceScope,
};
use axon_core::config::{GlobalArgs, ScrapeSourceArgs};

use crate::commands::source::run_source_request;
use crate::output::Output;

pub async fn run(
    args: ScrapeSourceArgs,
    global: &GlobalArgs,
    output: &mut dyn Output,
) -> anyhow::Result<i32> {
    let mut request = SourceRequest::new(args.url);
    request.intent = SourceIntent::Acquire;
    request.scope = Some(SourceScope::Page);
    request.embed = !args.no_embed;
    request.limits = SourceLimits {
        max_items: Some(1),
        max_pages: Some(1),
        max_depth: Some(0),
        ..SourceLimits::default()
    };
    request.output.json = global.json;
    request.output.response_mode = if args.inline {
        ResponseMode::Inline
    } else {
        ResponseMode::Auto
    };
    request.output.artifact_mode = ArtifactMode::OnLargeOutput;
    run_source_request(request, global, output).await
}
```

- [ ] **Step 4: Implement source-backed map projection**

In `crates/axon-cli/src/commands/map.rs`, replace legacy map service dispatch with:

```rust
let mut request = SourceRequest::new(args.url.clone());
request.intent = SourceIntent::Map;
request.scope = Some(SourceScope::Map);
request.embed = false;
request.output.json = global.json;
request.output.response_mode = ResponseMode::Auto;
request.limits.max_pages = args.max_pages.map(u64::from);
run_source_request(request, global, output).await
```

- [ ] **Step 5: Wire command dispatch**

In `crates/axon-cli/src/commands/mod.rs`, export:

```rust
pub mod scrape_source;
```

In `crates/axon-cli/src/lib.rs`, add dispatch:

```rust
CliCommand::Scrape(args) => commands::scrape_source::run(args, &cli.global, &mut output).await,
CliCommand::Map(args) => commands::map::run_source_backed(args, &cli.global, &mut output).await,
```

- [ ] **Step 6: Run focused CLI tests**

Run:

```bash
cargo test -p axon-cli --test scrape_map_source_projection -- --nocapture
cargo test -p axon-core source_routing_tests -- --nocapture
```

Expected: scrape and map capture `SourceRequest`; `crawl` remains reserved.

- [ ] **Step 7: Commit**

```bash
git add crates/axon-cli/src/commands/scrape_source.rs crates/axon-cli/src/commands/map.rs crates/axon-cli/src/commands/source.rs crates/axon-cli/src/commands/mod.rs crates/axon-cli/src/lib.rs crates/axon-core/src/config/cli.rs crates/axon-cli/src/scrape_map_source_projection_tests.rs
git commit -m "feat(cli): project scrape and map through SourceRequest"
```

### Task 7: Watch Refresh Search Research Auto-Index Cutover

**Files:**
- Create: `crates/axon-services/src/search_source_index.rs`
- Modify: `crates/axon-jobs/src/watch/dispatch.rs`
- Modify: `crates/axon-services/src/refresh.rs`
- Modify: `crates/axon-services/src/search_crawl.rs`
- Modify: `crates/axon-cli/src/commands/search.rs`
- Modify: `crates/axon-cli/src/commands/refresh.rs`
- Modify: `crates/axon-mcp/src/server/handlers_query.rs`
- Modify: `crates/axon-web/src/server/handlers/exploration.rs`
- Test: `crates/axon-services/src/source_auto_index_cutover_tests.rs`

**Interfaces:**
- Consumes: Source job enqueue from Tasks 2 and 5.
- Produces: `enqueue_web_source_auto_index` helper and Source jobs for watch/refresh/search/research.

- [ ] **Step 1: Write failing auto-index tests**

Create `crates/axon-services/src/source_auto_index_cutover_tests.rs` and wire it as an in-crate test module:

```rust
use axon_api::source::{JobKind, SourceScope};

#[tokio::test]
async fn watch_dispatch_enqueues_source_jobs() {
    let harness = WatchDispatchHarness::with_fixture().await;
    harness.run_due_watch("https://docs.example.test/").await;

    let source_jobs = harness.jobs_by_kind(JobKind::Source).await;
    assert_eq!(source_jobs.len(), 1);
    assert_eq!(source_jobs[0].request_json["source_request"]["scope"], "site");
    assert_eq!(harness.jobs_by_kind(JobKind::Crawl).await.len(), 0);
}

#[tokio::test]
async fn refresh_web_origin_enqueues_source_refresh() {
    let harness = SearchAutoIndexHarness::with_indexed_web_source().await;
    harness.refresh("docs.example.test").await.expect("refresh");

    let job = harness.single_job(JobKind::Source).await;
    assert_eq!(job.request_json["source_request"]["intent"], "refresh");
    assert_eq!(job.request_json["source_request"]["refresh"], "force");
}

#[tokio::test]
async fn search_auto_index_uses_bounded_source_jobs() {
    let harness = SearchAutoIndexHarness::with_search_results(3).await;
    harness.search("rust docs").await.expect("search");

    let jobs = harness.jobs_by_kind(JobKind::Source).await;
    assert_eq!(jobs.len(), 3);
    assert!(jobs.iter().all(|job| job.request_json["source_request"]["limits"]["max_pages"] == 1));
    assert_eq!(harness.jobs_by_kind(JobKind::Crawl).await.len(), 0);
}

#[tokio::test]
async fn auto_index_strips_untrusted_headers_and_denies_tailscale() {
    let harness = SearchAutoIndexHarness::with_search_results(1).await;
    harness.set_result_url("http://100.120.242.29/internal");
    let result = harness.search("private target").await;
    assert!(result.expect_err("ssrf").to_string().contains("non-global"));
}
```

- [ ] **Step 2: Run the test to verify failure**

Run:

```bash
cargo test -p axon-services --test source_auto_index_cutover -- --nocapture
```

Expected: failure because watch/search/research/refresh still create Crawl jobs or use crawl-named helpers.

- [ ] **Step 3: Add source auto-index helper**

Create `crates/axon-services/src/search_source_index.rs`:

```rust
use axon_api::source::{
    AuthSnapshot, ExecutionMode, JobIntent, SourceIntent, SourceRefreshPolicy, SourceRequest,
    SourceScope,
};
use axon_core::http::validate_url_with_dns;

use crate::context::ServiceContext;

pub async fn enqueue_web_source_auto_index(
    ctx: &ServiceContext,
    url: &str,
    scope: SourceScope,
    max_pages: u64,
    auth_snapshot: AuthSnapshot,
    reason: &str,
) -> anyhow::Result<axon_api::source::JobDescriptor> {
    validate_url_with_dns(url).await?;
    let mut request = SourceRequest::new(url.to_string());
    request.intent = SourceIntent::Acquire;
    request.refresh = SourceRefreshPolicy::IfStale;
    request.scope = Some(scope);
    request.embed = true;
    request.execution.mode = ExecutionMode::Background;
    request.execution.detached = true;
    request.limits.max_pages = Some(max_pages);
    request.limits.max_depth = Some(0);
    request.metadata.insert("auto_index_reason".to_string(), serde_json::json!(reason));
    request.metadata.insert("headers_policy".to_string(), serde_json::json!("stripped"));

    ctx.jobs()
        .enqueue_source(request, JobIntent::Acquire, Some(auth_snapshot))
        .await
}
```

- [ ] **Step 4: Cut watch dispatch over**

In `crates/axon-jobs/src/watch/dispatch.rs`, replace crawl enqueue with Source request enqueue:

```rust
let mut request = SourceRequest::new(seed_url.clone());
request.intent = SourceIntent::Refresh;
request.refresh = SourceRefreshPolicy::Force;
request.watch = SourceWatchPolicy::Ensure;
request.scope = Some(SourceScope::Site);
request.embed = true;
request.execution.mode = ExecutionMode::Background;
request.execution.detached = true;
request.metadata.insert("watch_id".to_string(), serde_json::json!(watch_id.0));
job_store.enqueue_source(request, JobIntent::Refresh, Some(auth_snapshot)).await?;
```

- [ ] **Step 5: Cut refresh over**

In `crates/axon-services/src/refresh.rs`, replace web-origin crawl fallback with:

```rust
let mut request = stored_source.request_snapshot.unwrap_or_else(|| {
    let mut request = SourceRequest::new(stored_source.canonical_uri.clone());
    request.scope = Some(stored_source.scope.unwrap_or(SourceScope::Site));
    request
});
request.intent = SourceIntent::Refresh;
request.refresh = SourceRefreshPolicy::Force;
request.execution.mode = ExecutionMode::Background;
request.execution.detached = true;
ctx.jobs()
    .enqueue_source(request, JobIntent::Refresh, Some(auth_snapshot.clone()))
    .await?;
```

For pre-ledger web origins, return a migration-required warning and do not enqueue a Crawl job.

- [ ] **Step 6: Cut search and research over**

Replace calls in `crates/axon-services/src/search_crawl.rs` with `enqueue_web_source_auto_index`:

```rust
for result in search_results.iter().take(limit) {
    enqueue_web_source_auto_index(
        ctx,
        &result.url,
        SourceScope::Page,
        1,
        AuthSnapshot::trusted_system("search_auto_index"),
        "search",
    )
    .await?;
}
```

For research, use `SourceScope::Page` for result pages already fetched for synthesis and `max_pages=1` unless the request explicitly asks for site/docs source indexing.

- [ ] **Step 7: Run focused tests**

Run:

```bash
cargo test -p axon-services --test source_auto_index_cutover -- --nocapture
rg "JobKind::Crawl|UnifiedJobKind::Crawl|crawl_start_with_context" crates/axon-services crates/axon-jobs crates/axon-cli crates/axon-mcp crates/axon-web
```

Expected: tests pass; grep output contains only migration-only files, reserved-token guidance, and tests that assert crawl is unreachable.

- [ ] **Step 8: Commit**

```bash
git add crates/axon-services/src/search_source_index.rs crates/axon-jobs/src/watch/dispatch.rs crates/axon-services/src/refresh.rs crates/axon-services/src/search_crawl.rs crates/axon-cli/src/commands/search.rs crates/axon-cli/src/commands/refresh.rs crates/axon-mcp/src/server/handlers_query.rs crates/axon-web/src/server/handlers/exploration.rs crates/axon-services/src/source_auto_index_cutover_tests.rs
git commit -m "feat(source): auto-index web callers with Source jobs"
```

### Task 8: Uniform Source Progress Events And Metrics

**Files:**
- Create: `crates/axon-services/src/source/events.rs`
- Create: `crates/axon-observe/src/source_metrics.rs`
- Modify: `crates/axon-observe/src/lib.rs`
- Modify: `crates/axon-services/src/source.rs`
- Modify: `crates/axon-services/src/source/dispatch.rs`
- Modify: `crates/axon-services/src/web_source.rs`
- Modify: `crates/axon-web/src/server/handlers/jobs.rs`
- Modify: `crates/axon-cli/src/commands/status.rs`
- Test: `crates/axon-services/src/source_observability_tests.rs`

**Interfaces:**
- Consumes: single Source job id from Task 2.
- Produces: durable phase events and bounded-label metrics across page/site/docs/map/search/research/watch/refresh source paths.

- [ ] **Step 1: Write failing observability tests**

Create `crates/axon-services/src/source_observability_tests.rs` and wire it as an in-crate test module:

```rust
use axon_api::source::{PipelinePhase, SourceRequest, SourceScope};

#[tokio::test]
async fn page_source_emits_ordered_phase_events() {
    let harness = WebSourceHarness::with_fixture_site().await;
    let mut request = SourceRequest::new("https://docs.example.test/intro");
    request.scope = Some(SourceScope::Page);

    let result = harness.run_source(request).await.expect("source");
    let phases = harness.event_phases(&result.job_id).await;
    assert_eq!(
        phases,
        vec![
            PipelinePhase::Resolving,
            PipelinePhase::Routing,
            PipelinePhase::Authorizing,
            PipelinePhase::Discovering,
            PipelinePhase::Diffing,
            PipelinePhase::Fetching,
            PipelinePhase::Normalizing,
            PipelinePhase::Preparing,
            PipelinePhase::Embedding,
            PipelinePhase::Upserting,
            PipelinePhase::Publishing,
            PipelinePhase::Cleaning,
            PipelinePhase::Complete,
        ]
    );
}

#[tokio::test]
async fn metrics_reject_high_cardinality_labels() {
    let err = axon_observe::source_metrics::record_source_phase_with_labels(
        "fetching",
        &[("url", "https://secret.example.test/token")],
    )
    .expect_err("url label rejected");
    assert!(err.to_string().contains("unsupported source metric label"));
}

#[tokio::test]
async fn rest_and_cli_read_the_same_job_events() {
    let harness = WebSourceHarness::with_fixture_site().await;
    let result = harness.run_site_source().await;
    let rest_events = harness.rest_job_events(&result.job_id).await;
    let cli_events = harness.cli_job_events(&result.job_id).await;
    assert_eq!(rest_events, cli_events);
}
```

- [ ] **Step 2: Run test to verify failure**

Run:

```bash
cargo test -p axon-services --test source_observability -- --nocapture
```

Expected: failure because phase events are missing or not tied to the same job id.

- [ ] **Step 3: Add source event helper**

Create `crates/axon-services/src/source/events.rs`:

```rust
use axon_api::source::*;
use axon_jobs::boundary::JobStore;

pub async fn emit_source_event(
    jobs: &dyn JobStore,
    job_id: JobId,
    sequence: u64,
    phase: PipelinePhase,
    status: LifecycleStatus,
    message: impl Into<String>,
) -> anyhow::Result<()> {
    let event = SourceProgressEvent {
        event_id: format!("evt_{}", uuid::Uuid::new_v4()),
        sequence,
        job_id: job_id.clone(),
        attempt: 1,
        stage_id: None,
        batch_id: None,
        reservation_id: None,
        checkpoint_id: None,
        dedupe_key: None,
        phase,
        status,
        severity: Severity::Info,
        visibility: Visibility::Public,
        message: message.into(),
        timestamp: Timestamp(chrono::Utc::now().to_rfc3339()),
        counts: None,
        current: None,
        warning: None,
        error: None,
        metadata: MetadataMap::new(),
    };
    jobs.append_event(job_id, event).await?;
    Ok(())
}
```

- [ ] **Step 4: Add bounded metrics helper**

Create `crates/axon-observe/src/source_metrics.rs`:

```rust
use anyhow::bail;

const ALLOWED_LABELS: &[&str] = &[
    "phase",
    "source_kind",
    "scope",
    "adapter",
    "status",
    "error_code",
    "provider_kind",
];

pub fn record_source_phase_with_labels(
    phase: &str,
    labels: &[(&str, &str)],
) -> anyhow::Result<()> {
    for (key, _) in labels {
        if !ALLOWED_LABELS.contains(key) {
            bail!("unsupported source metric label `{key}`");
        }
    }
    let mut counter = metrics::counter!("axon_source_phase_total", "phase" => phase.to_string());
    for (key, value) in labels {
        counter = counter.with_label(*key, (*value).to_string());
    }
    counter.increment(1);
    Ok(())
}
```

Export it in `crates/axon-observe/src/lib.rs`:

```rust
pub mod source_metrics;
```

- [ ] **Step 5: Emit events and metrics in web source phases**

In `crates/axon-services/src/web_source.rs`, emit around each phase:

```rust
events::emit_source_event(
    jobs,
    input.job_id.clone(),
    sequence.next(),
    PipelinePhase::Fetching,
    LifecycleStatus::Running,
    "fetching web source items",
)
.await?;
axon_observe::source_metrics::record_source_phase_with_labels(
    "fetching",
    &[
        ("source_kind", "web"),
        ("scope", scope_label(input.scope)),
        ("adapter", "web"),
        ("status", "running"),
    ],
)?;
```

Repeat for resolving, routing, authorizing, discovering, diffing, fetching, rendering, normalizing, preparing, embedding, upserting, publishing, cleaning, complete, and failed/degraded outcomes.

- [ ] **Step 6: Render the same durable events in REST and CLI**

In `crates/axon-web/src/server/handlers/jobs.rs`, source `/events` and `/stream` from job events only:

```rust
let events = state.jobs.events(job_id.clone(), cursor, limit).await?;
Ok(Json(SuccessEnvelope::new(JobEventPage { events, next_cursor })))
```

In `crates/axon-cli/src/commands/status.rs`, render the same `JobEventPage` DTO instead of legacy crawl `progress_json`.

- [ ] **Step 7: Run focused tests**

Run:

```bash
cargo test -p axon-services --test source_observability -- --nocapture
cargo test -p axon-observe source_metrics -- --nocapture
```

Expected: ordered phase events, shared REST/CLI event source, and high-cardinality metric labels rejected.

- [ ] **Step 8: Commit**

```bash
git add crates/axon-services/src/source/events.rs crates/axon-observe/src/source_metrics.rs crates/axon-observe/src/lib.rs crates/axon-services/src/source.rs crates/axon-services/src/source/dispatch.rs crates/axon-services/src/web_source.rs crates/axon-web/src/server/handlers/jobs.rs crates/axon-cli/src/commands/status.rs crates/axon-services/src/source_observability_tests.rs
git commit -m "feat(source): add web pipeline events and metrics"
```

### Task 9: Legacy Crawl Removal, Migration, And Generated Surface Cleanup

**Files:**
- Modify: `crates/axon-api/src/source/enums.rs`
- Modify: `crates/axon-services/src/runtime/job_runners.rs`
- Modify: `crates/axon-services/src/runtime/job_runners/crawl_runner.rs`
- Modify: `crates/axon-services/src/runtime/sqlite/crawl_bridge.rs`
- Modify: `crates/axon-services/src/crawl.rs`
- Modify: `crates/axon-web/src/server/routing.rs`
- Modify: `crates/axon-web/src/server/handlers/jobs.rs`
- Modify: `crates/axon-mcp/src/server.rs`
- Modify: generated docs and schema fixture files updated by `cargo xtask schemas generate --update-fixtures`
- Test: `crates/axon-services/src/legacy_crawl_unreachable_tests.rs`

**Interfaces:**
- Consumes: all callers moved to Source jobs by Tasks 5-7.
- Produces: migration-only handling for old Crawl rows and no live public crawl surface.

- [ ] **Step 1: Write failing legacy unreachable tests**

Create `crates/axon-services/src/legacy_crawl_unreachable_tests.rs` and wire it as an in-crate test module:

```rust
use axon_api::source::JobKind;

#[tokio::test]
async fn normal_registry_has_no_crawl_runner() {
    let registry = axon_services::runtime::job_runners::build_registry_for_test().await;
    assert!(registry.contains(JobKind::Source));
    assert!(!registry.contains(JobKind::Crawl));
}

#[tokio::test]
async fn legacy_crawl_rows_are_dead_lettered_not_recovered() {
    let harness = SourceRuntimeHarness::with_sqlite_and_fakes().await;
    let crawl_job = harness.insert_legacy_crawl_row().await;

    harness.recover_jobs().await.expect("recover");
    let row = harness.job(crawl_job.job_id).await;
    assert_eq!(row.status.as_str(), "failed");
    assert_eq!(row.error_code.as_deref(), Some("legacy.crawl.removed"));
}

#[test]
fn no_normal_code_path_creates_crawl_jobs() {
    let hits = axon_services::test_support::rg_source_for(
        "JobKind::Crawl|UnifiedJobKind::Crawl|crawl_start_with_context",
        &[
            "crates/axon-services",
            "crates/axon-jobs",
            "crates/axon-cli",
            "crates/axon-mcp",
            "crates/axon-web",
        ],
    );
    for hit in hits {
        assert!(
            hit.path.contains("legacy")
                || hit.path.contains("migration")
                || hit.path.contains("legacy_crawl_unreachable")
                || hit.line.contains("reserved"),
            "unexpected live crawl reference: {}:{} {}",
            hit.path,
            hit.line_number,
            hit.line
        );
    }
}
```

- [ ] **Step 2: Run tests to verify failure**

Run:

```bash
cargo test -p axon-services --test legacy_crawl_unreachable -- --nocapture
```

Expected: failure because CrawlRunner and crawl bridge are still live.

- [ ] **Step 3: Gate legacy crawl job kind**

In `crates/axon-api/src/source/enums.rs`, keep the enum variant for old rows but document and expose it as legacy-only:

```rust
/// Legacy migration-only row kind. Normal source indexing must not create this
/// kind after the SourceRequest web cutover.
Crawl,
```

Add an internal helper:

```rust
impl JobKind {
    pub fn is_public_source_surface(self) -> bool {
        !matches!(self, JobKind::Crawl | JobKind::Embed | JobKind::Ingest)
    }
}
```

Use this helper in generated public job-kind schemas and route/action inventories.

- [ ] **Step 4: Convert legacy rows to terminal migration failures**

In `crates/axon-services/src/runtime/sqlite/crawl_bridge.rs`, replace start/recover behavior with:

```rust
pub async fn mark_legacy_crawl_removed(
    store: &dyn JobStore,
    job_id: JobId,
) -> Result<(), ApiError> {
    store
        .update_status(JobStatusUpdate {
            job_id,
            source_id: None,
            status: LifecycleStatus::Failed,
            phase: PipelinePhase::Complete,
            stage_id: None,
            counts: None,
            current: None,
            message: Some("legacy crawl jobs were removed; re-run as `axon <url> --scope site`".to_string()),
            error: Some(SourceError {
                code: "legacy.crawl.removed".to_string(),
                severity: Severity::Failed,
                message: "legacy crawl jobs were removed; re-run as a SourceRequest".to_string(),
                source_item_key: None,
                retryable: false,
                provider_id: None,
                cause: None,
            }),
        })
        .await
}
```

Recovery should call this helper for pending/running legacy Crawl rows instead of requeueing them.

- [ ] **Step 5: Remove public REST and MCP crawl surfaces**

In `crates/axon-web/src/server/routing.rs`, remove `/v1/crawl*` routes from the router and OpenAPI registry. In `crates/axon-mcp/src/server.rs`, ensure `crawl` is absent from action/subaction schema and handler dispatch. Add invalid schema fixture `crates/axon-mcp/tests/fixtures/schema/removed_crawl.invalid.json`:

```json
{
  "action": "crawl",
  "url": "https://example.com"
}
```

Expected MCP validation: rejected before handler dispatch.

- [ ] **Step 6: Regenerate schemas and docs**

Run:

```bash
cargo xtask schemas generate --update-fixtures
cargo xtask schemas generate --check
```

Expected: first command updates generated artifacts; second command reports no drift.

- [ ] **Step 7: Run removal validations**

Run:

```bash
cargo test -p axon-services --test legacy_crawl_unreachable -- --nocapture
cargo test -p axon-mcp schema -- --nocapture
cargo test -p axon-web routing -- --nocapture
rg "POST /v1/crawl|action.*crawl|JobKind::Crawl|crawl_start_with_context" docs/reference crates
```

Expected: grep contains only migration-only code, invalid fixtures, reserved-token docs, and historical pipeline docs that explicitly say crawl is removed.

- [ ] **Step 8: Commit**

```bash
git add crates/axon-api/src/source/enums.rs crates/axon-services/src/runtime/job_runners.rs crates/axon-services/src/runtime/job_runners/crawl_runner.rs crates/axon-services/src/runtime/sqlite/crawl_bridge.rs crates/axon-services/src/crawl.rs crates/axon-web/src/server/routing.rs crates/axon-web/src/server/handlers/jobs.rs crates/axon-mcp/src/server.rs crates/axon-services/src/legacy_crawl_unreachable_tests.rs crates/axon-mcp/tests/fixtures/schema/removed_crawl.invalid.json docs/reference
git commit -m "refactor(crawl): retire legacy crawl job surface"
```

### Task 10: Final End-To-End Verification And Epic Closeout

**Files:**
- Modify: `docs/pipeline-unification/foundation/source-pipeline.md`
- Modify: `docs/pipeline-unification/surfaces/command-contract.md`
- Modify: `docs/pipeline-unification/schemas/cli-schema.md`
- Modify: `docs/pipeline-unification/runtime/job-contract.md`
- Modify: `docs/pipeline-unification/runtime/observability-contract.md`
- Modify: `docs/pipeline-unification/plans/finish-unification-metaplan.md`
- Modify: Beads comments for `axon_rust-ruzox.16` and `.16.1` through `.16.10`

**Interfaces:**
- Consumes: all implementation tasks.
- Produces: proof that the epic is operational and aligned with `docs/pipeline-unification/`.

- [ ] **Step 1: Run targeted test suites**

Run:

```bash
cargo test -p axon-core source_routing_tests -- --nocapture
cargo test -p axon-cli --test scrape_map_source_projection -- --nocapture
cargo test -p axon-services source_web_job_identity -- --nocapture
cargo test -p axon-services source_web_304_reuse -- --nocapture
cargo test -p axon-services source_web_artifacts -- --nocapture
cargo test -p axon-services source_web_crawl_cutover -- --nocapture
cargo test -p axon-services --test source_auto_index_cutover -- --nocapture
cargo test -p axon-services --test source_observability -- --nocapture
cargo test -p axon-services --test legacy_crawl_unreachable -- --nocapture
```

Expected: all pass.

- [ ] **Step 2: Run contract and generator checks**

Run:

```bash
cargo xtask check-layering
cargo xtask schemas generate --check
cargo xtask check-api-parity
```

Expected: all pass with no generated drift.

- [ ] **Step 3: Run static crawl-removal allowlist**

Run:

```bash
rg "JobKind::Crawl|UnifiedJobKind::Crawl|crawl_start_with_context|/v1/crawl|action.*crawl" crates docs/reference docs/pipeline-unification
```

Expected: hits are limited to migration-only code, invalid fixtures, and docs that state crawl is reserved/removed. No normal caller creates Crawl jobs.

- [ ] **Step 4: Run isolated live smoke**

Use isolated data paths:

```bash
export AXON_DATA_DIR="$(mktemp -d)"
export AXON_SQLITE_PATH="$AXON_DATA_DIR/jobs.db"
export AXON_COLLECTION="axon_epic_ruzox_16_$(date +%s)"
./scripts/axon scrape https://example.com --wait true --json
./scripts/axon https://example.com --scope site --wait true --json
./scripts/axon map https://example.com --json
./scripts/axon search "rust documentation" --limit 2 --json
./scripts/axon research "rust documentation" --limit 2 --json
./scripts/axon jobs list --json
```

Expected:
- scrape result has `scope=page`, `embed=true`, one Source job, and clean content output.
- site result has `scope=site`, one Source job, no Crawl job, no child Embed job.
- map result has `intent=map`, `embed=false`, and zero vector writes.
- search/research auto-index jobs are Source jobs only.
- jobs list contains no new Crawl rows.

- [ ] **Step 5: Verify REST and MCP source surfaces**

Run with a local server:

```bash
AXON_DATA_DIR="$(mktemp -d)" AXON_COLLECTION="axon_epic_rest_$(date +%s)" ./scripts/axon serve --bind 127.0.0.1:0
```

In another shell, run:

```bash
curl -sS -X POST "$AXON_BASE_URL/v1/sources" \
  -H 'content-type: application/json' \
  -d '{"source":"https://example.com","scope":"page","embed":true}' | jq '.data.job_id'
curl -sS "$AXON_BASE_URL/v1/crawl" | jq '.error.code'
```

Expected: `/v1/sources` accepts `SourceRequest`; `/v1/crawl` is absent or returns route-not-found. Run the MCP schema smoke and verify `crawl` and `scrape` are absent as actions:

```bash
cargo test -p axon-mcp schema -- --nocapture
```

- [ ] **Step 6: Update pipeline-unification docs**

Update docs to match implemented behavior:

```markdown
`axon scrape <url>` is retained as a SourceRequest projection with `scope=page`,
`embed=true`, `limits.max_pages=1`, clean content output, and no crawl fanout.
`axon crawl <url>` is reserved and fails with replacement guidance.
Site/docs crawl behavior is `axon <url> --scope site|docs`.
```

Apply this to:
- `docs/pipeline-unification/foundation/source-pipeline.md`
- `docs/pipeline-unification/surfaces/command-contract.md`
- `docs/pipeline-unification/schemas/cli-schema.md`
- `docs/pipeline-unification/runtime/job-contract.md`
- `docs/pipeline-unification/runtime/observability-contract.md`
- `docs/pipeline-unification/plans/finish-unification-metaplan.md`

- [ ] **Step 7: Update Beads evidence**

Run:

```bash
bd comment axon_rust-ruzox.16 "Implementation complete: web page/site/docs/scrape/map/watch/refresh/search/research now use SourceRequest jobs; crawl is migration-only; final verification commands passed: cargo xtask check-layering, cargo xtask schemas generate --check, cargo xtask check-api-parity, focused source web tests, isolated live smoke."
bd close axon_rust-ruzox.16.1 --reason "Implemented retained scrape/reserved crawl/source-backed map contract."
bd close axon_rust-ruzox.16.2 --reason "Web source indexing preserves one Source job id."
bd close axon_rust-ruzox.16.3 --reason "304 reuse preserves committed documents and skips unnecessary embedding."
bd close axon_rust-ruzox.16.4 --reason "WARC and clean output use ArtifactStore-backed refs."
bd close axon_rust-ruzox.16.5 --reason "Site/docs crawl runs as Source jobs with no child Embed handoff."
bd close axon_rust-ruzox.16.6 --reason "CLI scrape and map project to SourceRequest."
bd close axon_rust-ruzox.16.7 --reason "Watch/refresh/search/research auto-index through Source jobs."
bd close axon_rust-ruzox.16.8 --reason "Source events and metrics cover migrated web pipeline."
bd close axon_rust-ruzox.16.9 --reason "Legacy crawl is migration-only and public/generated surfaces are removed."
bd close axon_rust-ruzox.16.10 --reason "Final verification and docs closeout complete."
bd close axon_rust-ruzox.16 --reason "Epic complete: web crawl/scrape/map fully unified under SourceRequest."
bd dolt commit -m "Close crawl SourceRequest unification epic"
```

Expected: child beads close, epic becomes closeable, and Beads commits evidence.

- [ ] **Step 8: Commit final docs and generated artifacts**

```bash
git add docs/pipeline-unification docs/reference crates/axon-mcp/tests/fixtures crates/axon-web/tests/fixtures
git commit -m "docs: close crawl source unification contract"
```

## Self-Review

**Spec coverage:** The ten tasks cover all epic gates: retained scrape, reserved crawl, source-backed map, one Source job id, ETag/304 reuse, ArtifactStore WARC/clean output, site/docs Source execution, watch/refresh/search/research cutover, source events/metrics, legacy crawl migration/removal, generated artifacts, docs, live smoke, and Beads closeout. Vertical extractor restoration is named as a dependent separate bead, not hidden inside this epic.

**Placeholder scan:** The plan avoids red-flag placeholder phrases and every code-changing step includes concrete code or exact command content. Test harness names are defined in the shared interface section and assigned to a concrete test-support file.

**Type consistency:** Shared `SourceExecutionContext`, `WebSourceJobExecution`, `ReusedWebRepresentation`, and `StoredWebArtifact` names are defined before use. `SourceRequest`, `SourceScope`, `SourceIntent`, `SourceRefreshPolicy`, `OutputPolicy`, `JobKind`, `SourceProgressEvent`, `ArtifactStore`, and `ArtifactWriteRequest` match live `axon-api` and `axon-core` shapes inspected before writing this plan.
