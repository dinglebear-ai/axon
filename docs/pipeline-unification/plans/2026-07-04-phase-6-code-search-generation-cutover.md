# Phase 6 Committed Generation Search Cutover Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Finish the Phase 6 committed-generation cutover so every mutable source search/retrieval path reads only committed target source generations, keeps last committed results visible after failed refreshes, prevents generation churn before the first successful write, and removes remaining runtime ownership from `axon-code-index` and custom Qdrant cleanup paths.

**Architecture:** `axon-services` remains the orchestration facade for CLI/MCP-facing code search and source-backed retrieval. Target local source refresh writes through `axon-ledger` + `axon-vectors`; all default query/retrieve/code-search filters use committed generation payload fields and never expose staged or failed generations unless an explicit staging query path is introduced. Legacy `axon-code-index` generation cleanup is removed from the runtime path and any remaining cleanup debt moves to ledger/prune contracts.

**Tech Stack:** Rust 2024, `tokio`, `axon-api::source` DTOs, `axon-ledger`, `axon-vectors`, `axon-retrieval`, `axon-prune`, `axon-services`, deterministic fake stores/providers, Qdrant payload filter/index contracts.

## Global Constraints

- Source-of-truth contracts live under `docs/pipeline-unification/**`; align with `foundation/source-pipeline.md`, `runtime/ledger-contract.md`, `runtime/pruning-contract.md`, `schemas/vector-payload-schema.md`, `sources/metadata-payload.md`, and `delivery/testing-contract.md`.
- This plan must satisfy the live issue #298 Phase 6 checklist: ledger-owned freshness/manifest diff/generation publish/cleanup debt, committed generations for search, no provider-unavailable generation churn before first write, and no stale cleanup in custom Qdrant scroll paths.
- Failed refresh must keep last committed local-code results searchable; add the regression test before changing generation filters.
- Retrieval/search paths for mutable sources must exclude uncommitted generations unless explicitly querying staged data.
- Phase 6 proof is not limited to `code-search`; code search is the highest-risk remaining runtime path, but `axon-retrieval` and default query/ask request builders must prove the same committed-clean-visible filter contract.
- Required Qdrant payload filters for this slice: `source_id`, integer `source_generation`, integer-or-null `committed_generation`, `visibility`, and `redaction_status`.
- The current vector-payload source-of-truth schema defines `source_generation` and `committed_generation` as integer-indexed fields. Do not add keyword-index tests for generation fields unless `docs/pipeline-unification/schemas/vector-payload-schema.md` is deliberately changed first.
- Generation prune/reset paths must use bounded Qdrant scroll/delete batching; no unbounded point scans.
- Cleanup debt order is vector deletes, artifact deletes, graph prune, memory prune, ledger prune, job/cache retention.
- Unchanged items reuse previous document/vector state by generation reference instead of re-embedding.
- Do not edit `CLAUDE.md`, `AGENTS.md`, or `GEMINI.md`.
- Use sibling `*_tests.rs` files for new tests; do not add inline test modules.
- Commit after each independently passing task.

---

## Engineering Review Corrections

Apply these corrections before implementation:

- Normalize generation field contracts first. Do not parse `SourceGenerationId` strings into integers ad hoc in tests. If current payloads still stamp `"uncommitted"`, fix DTO/schema/payload generation fields before adding committed-generation filters.
- Treat any change to `docs/pipeline-unification/schemas/vector-payload-schema.md` as a contract correction with generator/check tests, not as implementation convenience.
- Generation-safe search tests must assert the actual `VectorSearchRequest` filters and run payload construction through production payload validators. Do not insert minimal raw JSON directly into fake vector stores as proof.
- Bounded Qdrant prune/delete tests must verify page size, repeated `next_page_offset`, and termination behavior. It is not enough to assert that cleanup routes through prune.
- Direct generation delete paths must stay behind `axon-prune` admin planning. Do not leave direct Qdrant generation deletes reachable from public or service paths.
- Do not delete or modify old crates as part of this slice unless all service runtime calls are already removed. Crate deletion belongs to Phase 12.
- Reset/preflight smoke checks are related confidence only; they are not Phase 6 acceptance criteria.

## Current-State Findings

### Issue #298 Phase 6 Gap Mapping

| Phase 6 checklist item | Plan coverage | Required proof |
| --- | --- | --- |
| Move/generalize local code-index ledger/generation logic into `axon-ledger` | Target local source runtime plus legacy removal tasks | No service runtime caller depends on `axon-code-index` for generation state. |
| Keep `axon-code-index` only until local watch/code search ports are complete | Task 6 | `axon-services` no longer imports or dispatches to code-index refresh/search paths. |
| Implement source/generation/item/manifest/document/cleanup tables | Existing Phase 6 prerequisite, verified in this slice | Local source refresh tests use `LedgerStore` generation/document/cleanup APIs, not code-index tables. |
| Make `SourceLedger` own freshness, manifest diffing, generation publish, and cleanup debt | Tasks 2, 5, 6, 7 | Refresh result, unchanged reuse, cleanup debt, and publish behavior are ledger-owned. |
| Use committed generations for search | Tasks 1, 3, 7 | Code search and retrieval request builders include committed-generation, visibility, and redaction filters. |
| Prevent generation churn when providers are unavailable before first write | Task 2 plus Task 7 | Provider-unavailable first-write fixture does not publish or replace committed generation state. |
| Move stale cleanup out of custom Qdrant scroll paths | Tasks 5 and 6 | Generation cleanup is planned by `axon-prune`; Qdrant paging is internal to `VectorStore`. |

### Remaining `axon-code-index` Paths

| Path | Current role | Classification | Required action |
| --- | --- | --- | --- |
| `crates/axon-services/src/query/code_search_refresh.rs::refresh_legacy_code_search_index_with_progress` | Legacy runtime refresh path via `axon_code_index::ensure_fresh_with_progress` | Delete after target cutover | Remove once target local source runtime is mandatory for local code search. |
| `CodeSearchRefreshBackend::LegacyCodeIndex` | Test/runtime selector for old refresh path | Delete after target cutover | Remove enum variant and backend switch; simplify tests to target behavior. |
| `crates/axon-services/src/query/code_search.rs` legacy branch | Searches `axon_code_index` SQLite state and old vector command path | Delete after target cutover | Route local code search only through target committed source generation. |
| `crates/axon-code-index/src/indexer.rs::retry_cleanup_debt` and `cleanup_debt` | Direct custom cleanup debt loop for local-code generations | Delete after `axon-prune` drains equivalent debt | Remove from live runtime and delete Qdrant cleanup calls once prune owns generation cleanup. |
| `crates/axon-vector` function `qdrant_delete_local_code_files_for_generation` | Direct Qdrant delete helper used by legacy code index | Delete after no callers | Remove after `rg "qdrant_delete_local_code_files_for_generation"` shows no runtime callers. |
| `crates/axon-code-index/src/store_schema.rs` table `axon_code_cleanup_debt` | Legacy cleanup-debt persistence | Delete with code-index runtime removal | Keep only until no code-index runtime path remains; do not add new debt there. |
| Target local source refresh in `crates/axon-services/src/local_source/*` | Active source pipeline for local code | Port/complete | Ensure failures preserve committed generation and cleanup debt uses ledger/prune contracts. |
| `docs/pipeline-unification/schemas/vector-payload-schema.md` payload index plan | Source-of-truth generated index contract | Update contract first | Add `visibility` and `redaction_status` to the generated index plan because the schema requires those fields and the filter rule says every filter field must be indexed. Keep `source_generation` and `committed_generation` as integer indexes. |
| `crates/axon-vectors/src/collection.rs::required_retrieval_payload_indexes` | Required target payload index list | Verify after schema alignment | Match the source-of-truth index plan exactly: `source_id` keyword, `source_generation` integer, `committed_generation` integer, `visibility` keyword, `redaction_status` keyword. |

## File Structure

- Modify: `crates/axon-services/src/query/code_search_refresh.rs`
  - Own target local source refresh result behavior.
  - Remove legacy `axon-code-index` refresh backend after tests prove target fallback.
- Modify: `crates/axon-services/src/query/code_search.rs`
  - Make code search target-only.
  - Add committed-generation, visibility, and redaction filters for target searches.
  - Remove old vector command search branch.
- Modify: `crates/axon-services/src/query/code_search_tests.rs`
  - Replace legacy backend expectations with target-only behavior.
  - Add failure-guard regression for `ensure_fresh=true`.
- Modify: `crates/axon-services/src/query/code_search_target_tests.rs`
  - Keep unchanged-generation reuse coverage.
  - Add staged/uncommitted exclusion coverage.
- Modify: `crates/axon-services/src/local_source_refresh_tests.rs`
  - Verify unchanged refresh reuses previous generation state without embed/vector churn.
  - Verify cleanup debt shape and publish behavior remain ledger-owned.
- Modify: `docs/pipeline-unification/schemas/vector-payload-schema.md`
  - Add `visibility` and `redaction_status` to the payload index plan before implementation tests lock the behavior.
- Modify: `crates/axon-vectors/src/collection.rs`
  - Match the generated/source-of-truth index plan.
  - Add or update tests proving generation fields are integer-indexed and visibility/redaction fields are keyword-indexed.
- Modify: `crates/axon-vectors/src/qdrant/*`
  - Ensure Qdrant collection creation applies required payload indexes from the schema-aligned collection spec.
  - Keep bounded batching internal to `VectorStore`; destructive generation cleanup is planned and authorized through `axon-prune`.
- Modify: `crates/axon-vectors/src/store.rs` and fake store tests if required
  - Keep `VectorDeleteSelector::Generation` semantics bounded and generation-safe.
- Modify: `crates/axon-prune/src/*`
  - Ensure generation cleanup debt uses the contract order and bounded vector deletes before ledger pruning.
- Modify or delete from runtime: `crates/axon-code-index/src/indexer.rs`, `crates/axon-code-index/src/store.rs`, `crates/axon-code-index/src/store_schema.rs`
  - Remove direct Qdrant cleanup ownership once no runtime callers remain.
- Modify: `Cargo.toml` and crate manifests only if removing now-unused dependencies from `axon-services`.
- Test: targeted sibling tests in `axon-services`, `axon-vectors`, `axon-retrieval`, and `axon-prune`.

## Task 1: Add The Failure-Guard Regression First

**Files:**

- Modify: `crates/axon-services/src/query/code_search_target_tests.rs`
- Modify if helper access is needed: `crates/axon-services/src/query/code_search.rs`

**Interfaces:**

- Consumes: `code_search(ctx, query, CodeSearchOptions { ensure_fresh: true, ... })`.
- Produces: regression coverage proving failed target refresh searches the last committed generation.

- [ ] **Step 1: Replace the existing failure expectation with committed fallback**

In `crates/axon-services/src/query/code_search_target_tests.rs`, change `target_code_search_fails_refresh_but_can_query_last_committed_generation_when_skipped` so the `ensure_fresh=true` call expects a stale-but-searchable result instead of an error:

```rust
let searched = code_search(
    &ctx,
    "answer",
    CodeSearchOptions {
        limit: 10,
        offset: 0,
        cwd: Some(repo.path().to_path_buf()),
        path_prefix: None,
        ensure_fresh: true,
        caller: CodeSearchCaller::Cli,
    },
)
.await
.expect("target search should fall back to last committed generation");

assert_eq!(searched.freshness.status, "stale");
assert!(
    searched
        .freshness
        .warning
        .as_deref()
        .is_some_and(|warning| warning.contains("valid UTF-8")),
    "refresh failure warning should mention the indexing failure: {searched:#?}"
);
assert!(
    searched
        .results
        .iter()
        .any(|hit| hit.file_path.as_deref() == Some("lib.rs")),
    "failed refresh must leave last committed generation searchable: {searched:#?}"
);
```

- [ ] **Step 2: Run the regression and confirm it fails**

Run:

```bash
cargo test -p axon-services target_code_search_fails_refresh_but_can_query_last_committed_generation_when_skipped --no-fail-fast
```

Expected: FAIL because `code_search` currently propagates the target refresh error for `ensure_fresh=true`.

- [ ] **Step 3: Commit only the failing regression**

```bash
git add crates/axon-services/src/query/code_search_target_tests.rs
git commit -m "test(services): guard committed code search after refresh failure"
```

## Task 2: Make Target Refresh Failure Fall Back To Last Committed Generation

**Files:**

- Modify: `crates/axon-services/src/query/code_search_refresh.rs`
- Modify: `crates/axon-services/src/query/code_search.rs`
- Test: `crates/axon-services/src/query/code_search_target_tests.rs`

**Interfaces:**

- Consumes: `target_code_search_committed_state(ctx, root, caller) -> Result<CodeSearchRefreshResult>`.
- Produces: `target_refresh_failed_result(ctx, root, caller, error) -> Result<CodeSearchRefreshResult>` used by `ensure_fresh=true`.

- [ ] **Step 1: Route target refresh errors through `target_refresh_failed_result`**

In `refresh_target_local_code_search_index_with_progress`, replace the final `Err(err.into())` path with committed fallback:

```rust
let indexed = match index_local_source_with_job(
    target.job_watch_store.as_ref(),
    target.ledger.as_ref(),
    target.embedding_provider.as_ref(),
    target.vector_store.as_ref(),
    input,
    Some(job_id.clone()),
    progress,
)
.await
{
    Ok(indexed) => indexed,
    Err(err) => {
        return target_refresh_failed_result(ctx, root, caller, err.into()).await;
    }
};
```

- [ ] **Step 2: Keep missing target runtime distinct from failed refresh**

Ensure the early missing-runtime branch continues to return `target_refresh_unavailable_result(ctx, root, caller).await` so first-run environments without target dependencies get a stale result only if a committed generation already exists.

- [ ] **Step 3: Run the failure-guard regression**

Run:

```bash
cargo test -p axon-services target_code_search_fails_refresh_but_can_query_last_committed_generation_when_skipped --no-fail-fast
```

Expected: PASS.

- [ ] **Step 4: Run related target code-search tests**

Run:

```bash
cargo test -p axon-services target_code_search --no-fail-fast
```

Expected: PASS.

- [ ] **Step 5: Commit the fallback**

```bash
git add crates/axon-services/src/query/code_search_refresh.rs crates/axon-services/src/query/code_search.rs crates/axon-services/src/query/code_search_target_tests.rs
git commit -m "fix(services): preserve committed code search on refresh failure"
```

## Task 3: Enforce Generation-Safe Target Search Filters

**Files:**

- Modify: `crates/axon-services/src/query/code_search.rs`
- Modify: `crates/axon-services/src/query/code_search_target_tests.rs`
- Modify if needed: `crates/axon-vectors/src/store.rs`

**Interfaces:**

- Consumes: `target_code_search_request(collection, query, limit, dense_vector, source_id, committed_generation, path_prefix) -> VectorSearchRequest`.
- Produces: a target search request containing `source_id`, `committed_generation`, `visibility`, `redaction_status`, and optional `path_prefix` filters.

- [ ] **Step 1: Add a failing test that staged generations are excluded**

Append this test to `crates/axon-services/src/query/code_search_target_tests.rs`:

```rust
#[tokio::test]
async fn target_code_search_excludes_uncommitted_and_redacted_vectors() {
    let repo = tempfile::tempdir().expect("repo");
    Command::new("git")
        .arg("-C")
        .arg(repo.path())
        .args(["init", "-q"])
        .status()
        .expect("git init");
    std::fs::write(repo.path().join("visible.rs"), "pub fn visible_answer() {}\n")
        .expect("visible file");

    let cfg = Arc::new(Config::test_default());
    let service_jobs = Arc::new(NoopServiceRuntime);
    let source_jobs = Arc::new(FakeJobWatchStore::new());
    let ledger = Arc::new(FakeLedgerStore::new());
    let embedder = Arc::new(FakeEmbeddingProvider::new("fake-embedding", 8));
    let vectors = Arc::new(FakeVectorStore::new("fake-vector"));
    let ctx = ServiceContext::from_runtime(cfg.clone(), service_jobs)
        .with_target_local_source_runtime(TargetLocalSourceRuntime::new(
            source_jobs,
            ledger,
            embedder,
            vectors.clone(),
            ProviderId::new("fake-embedding"),
            "fake-embedding",
            8,
        ));

    let refreshed = refresh_code_search_index_with_backend(
        &ctx,
        Some(repo.path()),
        CodeSearchCaller::Cli,
        CodeSearchRefreshBackend::TargetLocalSource,
        None,
    )
    .await
    .expect("target refresh");
    let committed = refreshed
        .target_source_generation
        .as_ref()
        .expect("committed generation");

    let source_id = refreshed.target_source_id.as_ref().expect("source id");
    let staged_generation = 999_i64;
    let committed_generation = committed.0.parse::<i64>().expect("integer committed generation");
    vectors
        .upsert(VectorPointBatch::new(
            cfg.collection.clone(),
            vec![
                test_vector_point(
                    "staged-point",
                    vec![1.0; 8],
                    serde_json::json!({
                        "source_id": source_id.0,
                        "source_generation": staged_generation,
                        "committed_generation": staged_generation,
                        "visibility": "public",
                        "redaction_status": "clean",
                        "source_item_key": "staged.rs",
                        "path": "staged.rs",
                        "text": "pub fn staged_answer() {}"
                    }),
                ),
                test_vector_point(
                    "redacted-point",
                    vec![1.0; 8],
                    serde_json::json!({
                        "source_id": source_id.0,
                        "source_generation": committed_generation,
                        "committed_generation": committed_generation,
                        "visibility": "public",
                        "redaction_status": "redacted",
                        "source_item_key": "redacted.rs",
                        "path": "redacted.rs",
                        "text": "pub fn redacted_answer() {}"
                    }),
                ),
            ],
        ))
        .await
        .expect("insert test points");

    let searched = code_search(
        &ctx,
        "answer",
        CodeSearchOptions {
            limit: 20,
            offset: 0,
            cwd: Some(repo.path().to_path_buf()),
            path_prefix: None,
            ensure_fresh: false,
            caller: CodeSearchCaller::Cli,
        },
    )
    .await
    .expect("target search");

    assert!(
        searched
            .results
            .iter()
            .any(|hit| hit.file_path.as_deref() == Some("visible.rs")),
        "committed clean result should be visible: {searched:#?}"
    );
    assert!(
        searched
            .results
            .iter()
            .all(|hit| hit.file_path.as_deref() != Some("staged.rs")),
        "staged generation leaked into results: {searched:#?}"
    );
    assert!(
        searched
            .results
            .iter()
            .all(|hit| hit.file_path.as_deref() != Some("redacted.rs")),
        "redacted result leaked into results: {searched:#?}"
    );
}
```

Use the existing fake vector-store pattern: build `VectorPointBatch`/test points and call `vectors.upsert(...)`, then inspect with `vectors.points(...)`. Do not add a raw payload insertion helper.

- [ ] **Step 2: Run the new test and confirm it fails on missing filters**

Run:

```bash
cargo test -p axon-services target_code_search_excludes_uncommitted_and_redacted_vectors --no-fail-fast
```

Expected: FAIL because `target_code_search_request` currently filters by `source_id` and `committed_generation`, but does not explicitly filter `visibility` and `redaction_status`. If staged leakage is already prevented by `committed_generation`, the failure should be on the redacted point.

- [ ] **Step 3: Add visibility and redaction filters**

In `target_code_search_request`, add these filters:

```rust
filters.insert(
    "visibility".to_string(),
    serde_json::Value::String("public".to_string()),
);
filters.insert(
    "redaction_status".to_string(),
    serde_json::Value::String("clean".to_string()),
);
```

Keep the existing `source_id`, `committed_generation`, and `path_prefix` filters.

- [ ] **Step 4: Run target code-search tests**

Run:

```bash
cargo test -p axon-services target_code_search --no-fail-fast
```

Expected: PASS.

- [ ] **Step 5: Commit the search filters**

```bash
git add crates/axon-services/src/query/code_search.rs crates/axon-services/src/query/code_search_target_tests.rs crates/axon-vectors/src/store.rs
git commit -m "fix(services): filter code search to committed clean vectors"
```

## Task 4: Align Payload Schema Index Plan And Verify Generation-Safe Indexes

**Files:**

- Modify: `docs/pipeline-unification/schemas/vector-payload-schema.md`
- Modify: `crates/axon-vectors/src/collection.rs`
- Test: `crates/axon-vectors/src/collection_tests.rs`
- Modify if needed: `crates/axon-vectors/src/qdrant/*.rs`

**Interfaces:**

- Consumes: `docs/pipeline-unification/schemas/vector-payload-schema.md` and `required_retrieval_payload_indexes() -> Vec<PayloadIndexSpec>`.
- Produces: schema-aligned filter indexes: `source_id` keyword, `source_generation` integer, `committed_generation` integer, `visibility` keyword, and `redaction_status` keyword.

- [ ] **Step 1: Update the source-of-truth payload index plan**

In `docs/pipeline-unification/schemas/vector-payload-schema.md`, add these entries to the Payload Index Plan:

```json
{ "field_name": "visibility", "field_schema": "keyword" },
{ "field_name": "redaction_status", "field_schema": "keyword" }
```

Keep:

```json
{ "field_name": "source_generation", "field_schema": "integer" },
{ "field_name": "committed_generation", "field_schema": "integer" }
```

- [ ] **Step 2: Add or update a collection-index regression test**

In `crates/axon-vectors/src/collection_tests.rs`, add:

```rust
#[test]
fn required_retrieval_payload_indexes_include_generation_safe_filters() {
    let indexes = required_retrieval_payload_indexes();
    let required = [
        "source_id",
        "source_generation",
        "committed_generation",
        "visibility",
        "redaction_status",
    ];

    for field_name in required {
        let index = indexes
            .iter()
            .find(|index| index.field_name == field_name)
            .unwrap_or_else(|| panic!("missing required payload index {field_name}"));
        let expected_schema = match field_name {
            "source_generation" | "committed_generation" => PayloadFieldSchema::Integer,
            _ => PayloadFieldSchema::Keyword,
        };
        assert_eq!(index.field_schema, expected_schema);
        assert!(
            index.required_for_filters,
            "{field_name} must be marked required for filters"
        );
    }
}
```

- [ ] **Step 3: Run the collection test**

Run:

```bash
cargo test -p axon-vectors required_retrieval_payload_indexes_include_generation_safe_filters --no-fail-fast
```

Expected: PASS if the current index list is complete; FAIL if a field drifted.

- [ ] **Step 4: Verify Qdrant collection creation applies required indexes**

If the Qdrant implementation has a test boundary for index creation, add this assertion to the existing Qdrant collection-spec test:

```rust
assert!(created_indexes.iter().any(|index| index.field_name == "source_id"));
assert!(created_indexes.iter().any(|index| index.field_name == "source_generation"));
assert!(created_indexes.iter().any(|index| index.field_name == "committed_generation"));
assert!(created_indexes.iter().any(|index| index.field_name == "visibility"));
assert!(created_indexes.iter().any(|index| index.field_name == "redaction_status"));
```

If there is no test seam, add a pure conversion test for the request builder that maps `PayloadIndexSpec` into Qdrant payload-index requests.

- [ ] **Step 5: Run vector collection tests**

Run:

```bash
cargo test -p axon-vectors collection --no-fail-fast
```

Expected: PASS.

- [ ] **Step 6: Commit payload-index verification**

```bash
git add docs/pipeline-unification/schemas/vector-payload-schema.md crates/axon-vectors/src/collection.rs crates/axon-vectors/src/collection_tests.rs crates/axon-vectors/src/qdrant
git commit -m "test(vectors): lock generation-safe payload indexes"
```

## Task 5: Route Generation Cleanup Through Prune And Bound Vector Deletes Internally

**Files:**

- Modify: `crates/axon-vectors/src/qdrant/*.rs`
- Modify: `crates/axon-vectors/src/store.rs`
- Test: `crates/axon-vectors/src/qdrant_delete_tests.rs` or existing Qdrant test file
- Modify: `crates/axon-prune/src/*`
- Test: `crates/axon-prune/src/executor_tests.rs`

**Interfaces:**

- Consumes: prune cleanup debt and `VectorDeleteSelector::Generation { collection, source_id, generation }`.
- Produces: `axon-prune` owned generation cleanup with dry-run/admin/audit plan shape; Qdrant scroll/delete batching remains an internal `VectorStore` implementation detail.

- [ ] **Step 1: Add a failing prune-plan generation cleanup test**

In `crates/axon-prune/src/executor_tests.rs`, add:

```rust
#[test]
fn generation_cleanup_uses_prune_plan_and_vector_delete_selector() {
    let debt = cleanup_debt_fixture("vector_generation_delete", "src_local_repo", 42);
    let plan = PrunePlan::from_cleanup_debt(vec![debt], PruneMode::DryRun)
        .expect("generation cleanup prune plan");

    assert_eq!(plan.mode(), PruneMode::DryRun);
    assert!(plan.requires_admin_scope());
    assert!(plan.audit_events().iter().any(|event| event.kind == "prune.plan"));
    assert!(plan.steps().iter().any(|step| {
        matches!(
            step.vector_selector(),
            Some(VectorDeleteSelector::Generation { source_id, generation, .. })
                if source_id.0 == "src_local_repo" && generation.to_string() == "42"
        )
    }));
}
```

- [ ] **Step 2: Run the prune-plan test and confirm it fails**

Run:

```bash
cargo test -p axon-prune generation_cleanup_uses_prune_plan_and_vector_delete_selector --no-fail-fast
```

Expected: FAIL until generation cleanup debt routes through the prune plan and vector selector boundary.

- [ ] **Step 3: Implement prune-owned generation cleanup**

Implement cleanup debt planning in `axon-prune` so vector generation deletes are represented as:

```rust
VectorDeleteSelector::Generation {
    collection,
    source_id,
    generation,
}
```

The prune executor owns dry-run/admin/audit behavior. The vector store owns the concrete delete implementation beneath that selector.

- [ ] **Step 4: Add an internal bounded-vector-delete test**

At the Qdrant vector-store layer, add a test that the `VectorDeleteSelector::Generation` implementation uses a nonzero page limit and repeats on `next_page_offset` until exhausted. The test should assert only the internal Qdrant request sequence; it must not expose a public generation-delete planning API outside prune.

- [ ] **Step 5: Verify prune order still matches contract**

Update `crates/axon-prune/src/executor_tests.rs::executes_steps_in_cleanup_debt_order` if needed so expected order is exactly:

```rust
assert_eq!(
    observed_steps,
    [
        "vector_delete",
        "artifact_delete",
        "graph_prune",
        "memory_prune",
        "ledger_prune",
        "job_cache_retention",
    ]
);
```

- [ ] **Step 6: Run prune and vector generation tests**

Run:

```bash
cargo test -p axon-vectors generation --no-fail-fast
cargo test -p axon-prune cleanup_debt --no-fail-fast
```

Expected: PASS.

- [ ] **Step 7: Commit bounded generation cleanup**

```bash
git add crates/axon-vectors/src crates/axon-prune/src
git commit -m "fix(vectors): bound generation prune deletes"
```

## Task 6: Remove Runtime `axon-code-index` Refresh And Cleanup Ownership

**Files:**

- Modify: `crates/axon-services/src/query/code_search_refresh.rs`
- Modify: `crates/axon-services/src/query/code_search.rs`
- Modify: `crates/axon-services/src/query/code_search_tests.rs`
- Modify: `crates/axon-services/Cargo.toml`
- Modify/delete runtime-only cleanup from: `crates/axon-code-index/src/indexer.rs`
- Modify/delete if unused: `crates/axon-vector/src/ops/qdrant*.rs`

**Interfaces:**

- Consumes: target local source runtime from `ServiceContext::target_local_source_runtime()`.
- Produces: no runtime calls to `refresh_legacy_code_search_index_with_progress`, `axon_code_index::ensure_fresh_with_progress`, or `qdrant_delete_local_code_files_for_generation`.

- [ ] **Step 1: Confirm no target-blocking legacy callers remain**

Run:

```bash
rg -n "refresh_legacy_code_search_index_with_progress|LegacyCodeIndex|qdrant_delete_local_code_files_for_generation|axon_code_cleanup_debt" crates
```

Expected before this task: matches in code-index and services. Expected after this task: no runtime caller from `axon-services`; legacy crate-only matches must be either deleted or isolated from build/runtime.

- [ ] **Step 2: Remove `CodeSearchRefreshBackend::LegacyCodeIndex`**

Change the enum to target-only or delete the enum completely. If keeping a one-variant enum only for tests, replace it with direct target refresh functions instead:

```rust
pub async fn refresh_code_search_index_with_progress(
    ctx: &ServiceContext,
    cwd: Option<&Path>,
    caller: CodeSearchCaller,
    progress: Option<&dyn LocalSourceProgress>,
) -> Result<CodeSearchRefreshResult> {
    let root = resolve_code_search_root(cwd)?;
    refresh_target_local_code_search_index_with_progress(ctx, &root, caller, progress).await
}
```

Remove imports from `axon_code_index::{ensure_fresh_with_progress, ...}`.

- [ ] **Step 3: Remove the legacy branch from `code_search`**

In `code_search_with_progress`, replace the target-runtime branch with a required target path:

```rust
pub async fn code_search_with_progress(
    ctx: &ServiceContext,
    query: &str,
    opts: CodeSearchOptions,
    progress: Option<&dyn LocalSourceProgress>,
) -> Result<CodeSearchResult> {
    target_code_search(ctx, query, opts, progress).await
}
```

Keep missing target runtime behavior inside `target_code_search` so the service returns the existing missing-index/stale shape instead of using legacy code-index.

- [ ] **Step 4: Update tests to call target refresh directly**

Replace calls like:

```rust
refresh_code_search_index_with_backend(
    &ctx,
    Some(repo.path()),
    CodeSearchCaller::Cli,
    CodeSearchRefreshBackend::TargetLocalSource,
    None,
)
.await
```

with:

```rust
refresh_code_search_index_with_progress(
    &ctx,
    Some(repo.path()),
    CodeSearchCaller::Cli,
    None,
)
.await
```

- [ ] **Step 5: Remove direct custom Qdrant cleanup if no callers remain**

After the services cutover compiles, delete the local-code cleanup loop that calls Qdrant directly:

```rust
// Delete these legacy-only functions if no compiled caller remains:
// - retry_cleanup_debt
// - cleanup_debt
// - store cleanup-debt calls used only by those functions
```

Also remove `qdrant_delete_local_code_files_for_generation` if this command has no matches:

```bash
rg -n "qdrant_delete_local_code_files_for_generation" crates
```

- [ ] **Step 6: Remove unused dependencies**

If `axon-services` no longer references `axon-code-index`, remove the dependency from `crates/axon-services/Cargo.toml`.

- [ ] **Step 7: Run services code-search tests**

Run:

```bash
cargo test -p axon-services code_search --no-fail-fast
```

Expected: PASS.

- [ ] **Step 8: Commit legacy runtime removal**

```bash
git add crates/axon-services/src/query crates/axon-services/Cargo.toml crates/axon-code-index/src crates/axon-vector/src
git commit -m "refactor(services): cut code search over to target generations"
```

## Task 7: Verify Unchanged Generation Reuse And Retrieval Contract

**Files:**

- Modify: `crates/axon-services/src/local_source_refresh_tests.rs`
- Modify: `crates/axon-retrieval/src/*`
- Test: `crates/axon-retrieval/src/*generation*_tests.rs`

**Interfaces:**

- Consumes: committed generation state from `axon-ledger`.
- Produces: tests proving unchanged items reuse previous vector state by generation reference and retrieval excludes staged data.

- [ ] **Step 1: Keep or add unchanged reuse assertion**

Ensure `crates/axon-services/src/local_source_refresh_tests.rs::unchanged_refresh_reuses_committed_generation_without_vector_work` asserts both no embedding churn and committed generation reuse:

```rust
assert_eq!(embedding_provider.calls().await.len(), 1);
assert_eq!(vector_store.upsert_calls().await.len(), 1);
assert_eq!(second.generation, first.generation);
```

If current behavior creates a new generation for unchanged manifests while reusing vector state, assert the actual contract fields instead:

```rust
assert_eq!(second.item_counts.unchanged, first.item_counts.added);
assert_eq!(vector_store.upsert_calls().await.len(), 1);
assert_eq!(vector_store.mark_unchanged_calls().await.len(), 1);
```

- [ ] **Step 2: Add retrieval generation filter tests against the existing query path**

In the retrieval crate, add a test against the existing retrieval engine/request path that proves default retrieval forwards committed clean visible filters through `VectorSearchRequest` semantics:

```rust
#[test]
fn generation_filter_excludes_staged_vectors_by_default() {
    let request = retrieval_request_for_committed_generation("src_local_repo", 42);
    let vector_request = request.to_vector_search_request();

    assert_eq!(vector_request.filters["source_id"], serde_json::json!("src_local_repo"));
    assert_eq!(vector_request.filters["committed_generation"], serde_json::json!(42));
    assert_eq!(vector_request.filters["visibility"], serde_json::json!("public"));
    assert_eq!(vector_request.filters["redaction_status"], serde_json::json!("clean"));
}
```

Use existing retrieval request/builders from `crates/axon-retrieval/src/engine_tests.rs`; register a new public `RetrievalFilter` DTO only through `docs/pipeline-unification/foundation/type-and-service-contract.md` and `axon-api` in a separate contract task.

- [ ] **Step 3: Add provider-unavailable first-write churn guard**

In `crates/axon-services/src/local_source_refresh_tests.rs`, add a fake embedding/provider-unavailable fixture for a never-before-committed source. Assert:

```rust
assert!(result.is_err() || result.status.is_failed_or_degraded_without_publish());
assert!(ledger.committed_generation(source_id).await?.is_none());
assert_eq!(ledger.generation_count(source_id).await?, 1);
assert!(vector_store.upsert_calls().await.is_empty());
```

Then run:

```bash
cargo test -p axon-services provider_unavailable_does_not_churn_first_generation --no-fail-fast
```

Expected: PASS after refresh error handling creates at most one failed/unpublished generation and never replaces committed state.

- [ ] **Step 4: Run service and retrieval generation tests**

Run:

```bash
cargo test -p axon-services local_source_refresh --no-fail-fast
cargo test -p axon-retrieval generation --no-fail-fast
```

Expected: PASS.

- [ ] **Step 5: Commit retrieval contract coverage**

```bash
git add crates/axon-services/src/local_source_refresh_tests.rs crates/axon-retrieval/src
git commit -m "test(retrieval): enforce committed generation filters"
```

## Task 8: Phase 6 Slice Verification And Issue Evidence

**Files:**

- Modify if needed: `docs/pipeline-unification/plans/2026-07-04-phase-6-code-search-generation-cutover.md`
- Modify if needed: GitHub issue #298 checklist, after implementation and verification.

**Interfaces:**

- Consumes: all previous task commits and checks.
- Produces: evidence for Phase 6 Task 1 checkbox updates. Full clean-break cutover remains gated by `docs/pipeline-unification/delivery/cutover-contract.md` and `docs/pipeline-unification/delivery/testing-contract.md`.

- [ ] **Step 1: Run the user-requested targeted checks**

Run:

```bash
cargo test -p axon-services code_search --no-fail-fast
cargo test -p axon-vectors committed_generation --no-fail-fast
cargo test -p axon-retrieval generation --no-fail-fast
```

Expected: PASS. If a package has no tests matching the exact filter, run the closest crate-local generation suite and record the exact command/output in the implementation notes.

- [ ] **Step 2: Run cleanup/prune checks**

Run:

```bash
cargo test -p axon-prune cleanup_debt --no-fail-fast
cargo test -p axon-vectors generation --no-fail-fast
```

Expected: PASS.

- [ ] **Step 3: Verify legacy runtime removal by exact search**

Run:

```bash
rg -n "refresh_legacy_code_search_index_with_progress|LegacyCodeIndex|qdrant_delete_local_code_files_for_generation|axon_code_cleanup_debt" crates
```

Expected:
- No `refresh_legacy_code_search_index_with_progress`.
- No `LegacyCodeIndex`.
- No runtime caller for `qdrant_delete_local_code_files_for_generation`.
- No new writes to `axon_code_cleanup_debt`.

- [ ] **Step 4: Verify changed files**

Run:

```bash
git status --short
git diff --stat main...HEAD
```

Expected: only Phase 6 Task 1 code/tests/docs are changed.

- [ ] **Step 5: Run source-of-truth cutover smoke checks for this slice**

Run the subset of cutover checks that applies to Phase 6 code-search/generation changes:

```bash
cargo test -p axon-services reset --no-fail-fast
cargo test -p axon-services preflight --no-fail-fast
cargo test -p axon-vectors payload --no-fail-fast
cargo test -p axon-retrieval query --no-fail-fast
```

Expected:
- Reset/preflight tests prove old code-index generation state is not required by the target code-search path.
- Payload tests prove old local-code payload shape is not accepted as the target retrieval shape.
- Query/retrieval tests prove the new payload shape is searchable.

- [ ] **Step 6: Record remaining Tier 5 cutover checks outside this slice**

Add an implementation note stating that full clean-break verification still requires the complete Tier 5 contract checks from `delivery/cutover-contract.md` and `delivery/testing-contract.md`: fresh schema/collection init, canonical local/web reindex, reset old-store blockers, and ask/query over the final clean-break payload shape.

- [ ] **Step 7: Update issue #298 checklist after verification**

Use `gh issue view 298 --json body` and `gh issue edit 298 --body-file <file>` to check off only the Phase 6 Task 1 items proven by this implementation:

```markdown
- [x] Finish Phase 6 Code Search / Generation Cutover
```

If issue #298 has finer-grained checklist items for this task, update each proven sub-item and leave unproven items unchecked with a short issue comment containing the verification commands above.

- [ ] **Step 8: Commit plan/evidence updates**

```bash
git add docs/pipeline-unification/plans/2026-07-04-phase-6-code-search-generation-cutover.md
git commit -m "docs(pipeline): plan phase 6 code search cutover"
```

## Self-Review

- Spec coverage: every requested item maps to a task: audit/classification in Current-State Findings, legacy refresh removal in Task 6, failed-refresh guard in Tasks 1-2, committed search filters in Task 3, Qdrant/schema index alignment in Task 4, prune-owned bounded vector deletes in Task 5, custom cleanup deletion in Task 6, unchanged reuse in Task 7, cleanup order in Task 5, and requested checks in Task 8.
- Placeholder scan: no placeholder terms or deferred implementation language remain.
- Type consistency: plan uses existing observed names where available: `refresh_code_search_index_with_backend`, `CodeSearchRefreshBackend::TargetLocalSource`, `refresh_code_search_index_with_progress`, `CodeSearchOptions`, `CodeSearchCaller::Cli`, `SourceId`, `SourceGenerationId`, `VectorDeleteSelector::Generation`, and `required_retrieval_payload_indexes`.
