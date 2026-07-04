# Phase 3 Alignment Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bring issue #298 Phase 3, the live implementation, and the pipeline-unification contracts into agreement for stores, providers, fakes, provider capability artifacts, and tracker wording.

**Architecture:** Keep Phase 3 limited to durable/external boundaries and their fake implementations. Generate provider capability artifacts from Rust-owned DTOs instead of the current skeleton schema, strengthen fake-boundary tests where Phase 3 contracts require strict behavior, and move Phase 4 routing claims out of Phase 3 tracker language.

**Tech Stack:** Rust 2024 workspace, `schemars`, `serde_json`, `cargo xtask schemas`, Tokio async tests, GitHub issue tracker via `gh`.

## Global Constraints

- `CLAUDE.md` is the source of truth for agent memory; do not edit `AGENTS.md` or `GEMINI.md` directly.
- Rust workspace style: `mod_module_files = "deny"`; use sibling `foo.rs` files, never `foo/mod.rs`.
- Phase 3 canonical scope is `docs/pipeline-unification/delivery/implementation-plan.md` lines 98-117.
- Phase 3 checklist source is `docs/pipeline-unification/delivery/implementation-checklist.md` lines 52-67.
- Fake requirements source is `docs/pipeline-unification/delivery/testing-contract.md` lines 65-86.
- Provider capability source is `docs/pipeline-unification/schemas/provider-capability-schema.md` lines 60-82 and 174-203.
- Current mismatch: `docs/reference/runtime/provider-capabilities.schema.json` lines 18-30 still mark provider capabilities as a skeleton.
- Do not edit GitHub issue #298 until the code/docs changes land and are verified.
- Use the smallest verification surface that proves Phase 3; do not run broad live smoke tests for this docs/schema/fake-boundary slice.

---

## Engineering Review Corrections

Apply these corrections before implementation:

- Do not mark “provider saturation cannot starve interactive query/ask paths” complete unless this plan adds a real fake-boundary provider reservation/fairness test. Provider schema and fake-store tests alone are not enough.
- Generated provider capability artifacts must be sourced from Rust-owned provider DTOs and provider boundary crates. `docs/reference/runtime/provider-capabilities.*` is generated output, not a source of truth.
- Keep provider schema fixture inputs bounded to schema-owner DTO/registry files. Do not make `fixture_repo()` copy broad crate trees or grow into a mini-workspace.
- Fake memory review paths must preserve production pagination behavior. Cursor/limit must be applied during iteration, not after scanning and cloning all records.
- Do not prewrite issue proof text with `PASS` lines. Issue updates must quote actual commands and results after verification.

## File Structure

Modify these files:

- `xtask/src/schemas/families.rs` — replace provider schema skeleton generation with real `schemars` definitions for `ProviderCapability` and related capability DTOs.
- `xtask/src/schemas/tests.rs` — add generator tests proving provider artifacts are non-skeleton and include reservation/cooling fields.
- `docs/reference/runtime/provider-capabilities.schema.json` — generated output from `cargo xtask schemas providers`.
- `docs/reference/runtime/provider-capabilities.md` — generated output from `cargo xtask schemas providers`.
- `crates/axon-memory/src/store.rs` — make the fake memory store support Phase 3 fake requirements for review/forget/supersede/contradict instead of returning unsupported for those contract paths.
- `crates/axon-memory/src/store_tests.rs` — add tests for memory review, forgetting, superseding, and contradiction behavior.
- `crates/axon-graph/src/store.rs` — make duplicate graph candidates merge evidence rather than replacing edge evidence, and expose conflict warnings when stable keys disagree on node kind.
- `crates/axon-graph/src/store_tests.rs` — add tests for evidence merge and conflict warning behavior.
- `docs/pipeline-unification/delivery/implementation-checklist.md` — clarify that Phase 3 does not own resolver/router work and that provider capability schema completion is required.
- Optional after merge: GitHub issue #298 body — update Phase 3 checkboxes and move the `SourceResolver`/`SourceRouter` line to Phase 4/PR5.

Create these files only if the implementation wants cleaner generated markdown helpers:

- `xtask/src/schemas/provider_capabilities.rs` — focused provider schema generator helpers. Prefer this if `families.rs` starts growing awkwardly.

## Task 1: Replace Provider Capability Skeleton Schema

**Files:**
- Modify: `xtask/src/schemas/families.rs`
- Modify: `xtask/src/schemas/tests.rs`
- Generated: `docs/reference/runtime/provider-capabilities.schema.json`
- Generated: `docs/reference/runtime/provider-capabilities.md`

**Interfaces:**
- Consumes: `axon_api::source::ProviderCapability`, `ProviderLimits`, `ReservationPolicy`, `ReservationStateSnapshot`, `ProviderCostClass`, `DegradedMode`, `EmbeddingProviderCapability`, `LlmProviderCapability`, `VectorStoreCapability`, `FetchProviderCapability`, `RenderProviderCapability`, `CredentialProviderCapability`.
- Produces: `cargo xtask schemas providers` output where `docs/reference/runtime/provider-capabilities.schema.json` contains real `$defs.ProviderCapability` and no `x-axon.skeleton`.

- [ ] **Step 1: Add failing provider schema generator test**

Append this test to `xtask/src/schemas/tests.rs`:

```rust
#[test]
fn provider_schema_is_not_a_skeleton_and_contains_reservation_fields() {
    let tmp = fixture_repo();
    generate(tmp.path()).unwrap();

    let content = std::fs::read_to_string(
        tmp.path()
            .join("docs/reference/runtime/provider-capabilities.schema.json"),
    )
    .unwrap();
    let value: serde_json::Value = serde_json::from_str(&content).unwrap();

    assert_ne!(
        value["$defs"]["SchemaFamilyContract"]["properties"]["status"]["const"],
        "skeleton",
        "provider capability schema must be generated from real provider DTOs"
    );
    assert!(
        value["$defs"].get("ProviderCapability").is_some(),
        "ProviderCapability schema definition should be present"
    );
    assert!(
        value["$defs"]["ProviderCapability"]["required"]
            .as_array()
            .unwrap()
            .iter()
            .any(|field| field == "reservation_policy"),
        "reservation_policy must be a required provider capability field"
    );
    assert!(
        value["$defs"]["ProviderCapability"]["required"]
            .as_array()
            .unwrap()
            .iter()
            .any(|field| field == "reservation_state"),
        "reservation_state must be a required provider capability field"
    );
    assert!(
        value["$defs"].get("ReservationPolicy").is_some(),
        "ReservationPolicy schema definition should be present"
    );
    assert!(
        value["$defs"].get("ReservationStateSnapshot").is_some(),
        "ReservationStateSnapshot schema definition should be present"
    );
}
```

- [ ] **Step 2: Run the failing test**

Run:

```bash
cargo test -p xtask provider_schema_is_not_a_skeleton_and_contains_reservation_fields --locked
```

Expected: FAIL because `docs/reference/runtime/provider-capabilities.schema.json` is still generated from `skeleton_artifacts()` and contains `status: "skeleton"`.

- [ ] **Step 3: Implement real provider schema artifact generation**

In `xtask/src/schemas/families.rs`, change `generator_for()` dispatch by replacing the generic skeleton arm for `SchemaFamily::Providers`.

Use this match shape:

```rust
impl FamilyGenerator for Generator {
    fn generate(&self, root: &Path) -> Result<Vec<SchemaArtifact>> {
        match self.family {
            SchemaFamily::Api => api_artifacts(root),
            SchemaFamily::Errors => error_artifacts(root),
            SchemaFamily::Adapters => super::adapters::adapter_artifacts(root),
            SchemaFamily::VectorPayload => vector_payload::vector_payload_artifacts(root),
            SchemaFamily::Events => runtime_defs::events_artifacts(root),
            SchemaFamily::Database => runtime_defs::database_artifacts(root),
            SchemaFamily::Providers => provider_artifacts(root),
            family => skeleton_artifacts(root, family_specs::spec_for(family)),
        }
    }
}
```

Add these helper functions near `error_artifacts()`:

```rust
fn provider_artifacts(root: &Path) -> Result<Vec<SchemaArtifact>> {
    let inputs = source_inputs(
        root,
        &[
            "crates/axon-api/src/source/capability.rs",
            "crates/axon-embedding/src/provider.rs",
            "crates/axon-embedding/src/fake.rs",
            "crates/axon-llm/src/provider.rs",
            "crates/axon-llm/src/fake.rs",
            "crates/axon-vectors/src/store.rs",
            "crates/axon-adapters/src/boundary.rs",
            "crates/axon-core/src/boundary.rs",
            "crates/axon-authz/src/policy.rs",
            "crates/axon-observe/src/reservation.rs",
            "docs/pipeline-unification/runtime/provider-contract.md",
            "docs/pipeline-unification/schemas/provider-capability-schema.md",
        ],
    )?;
    let defs = schema_defs(&provider_schema_defs(), Some(enum_defs("providers")));
    let schema = schema_bundle(
        "https://axon.local/schemas/runtime/provider-capabilities.schema.json",
        "AxonProviderCapabilitySchema",
        "cargo xtask schemas providers",
        &[
            "axon-api",
            "axon-embedding",
            "axon-llm",
            "axon-vectors",
            "axon-adapters",
            "axon-core",
            "axon-authz",
            "axon-observe",
        ],
        &inputs,
        defs,
    );
    Ok(vec![
        SchemaArtifact::new(
            rel("docs/reference/runtime/provider-capabilities.schema.json"),
            json_string(&schema)?,
        ),
        SchemaArtifact::new(
            rel("docs/reference/runtime/provider-capabilities.md"),
            provider_markdown(&inputs),
        ),
    ])
}

fn provider_schema_defs() -> Vec<(&'static str, Value)> {
    vec![
        (
            "ProviderCapability",
            schemars::schema_for!(axon_api::source::ProviderCapability).into(),
        ),
        (
            "ProviderLimits",
            schemars::schema_for!(axon_api::source::ProviderLimits).into(),
        ),
        (
            "ReservationPolicy",
            schemars::schema_for!(axon_api::source::ReservationPolicy).into(),
        ),
        (
            "ReservationStateSnapshot",
            schemars::schema_for!(axon_api::source::ReservationStateSnapshot).into(),
        ),
        (
            "ProviderCostClass",
            schemars::schema_for!(axon_api::source::ProviderCostClass).into(),
        ),
        (
            "DegradedMode",
            schemars::schema_for!(axon_api::source::DegradedMode).into(),
        ),
        (
            "EmbeddingProviderCapability",
            schemars::schema_for!(axon_api::source::EmbeddingProviderCapability).into(),
        ),
        (
            "LlmProviderCapability",
            schemars::schema_for!(axon_api::source::LlmProviderCapability).into(),
        ),
        (
            "VectorStoreCapability",
            schemars::schema_for!(axon_api::source::VectorStoreCapability).into(),
        ),
        (
            "FetchProviderCapability",
            schemars::schema_for!(axon_api::source::FetchProviderCapability).into(),
        ),
        (
            "RenderProviderCapability",
            schemars::schema_for!(axon_api::source::RenderProviderCapability).into(),
        ),
        (
            "CredentialProviderCapability",
            schemars::schema_for!(axon_api::source::CredentialProviderCapability).into(),
        ),
    ]
}

fn provider_markdown(inputs: &[SourceInput]) -> String {
    let mut output = String::from("# Provider Capability Schema Reference\n\n");
    output.push_str("Generated by `cargo xtask schemas providers`.\n\n");
    output.push_str("## Contract\n\n");
    output.push_str(
        "Provider capabilities report health, limits, cooling, reservation policy, reservation state, degraded modes, and family-specific capability details.\n\n",
    );
    output.push_str("## Required Common Fields\n\n");
    for field in [
        "provider_id",
        "provider_kind",
        "implementation",
        "version",
        "health",
        "limits",
        "features",
        "reservation_policy",
        "reservation_state",
        "cost_class",
        "degraded_modes",
        "fake_overrides_supported",
    ] {
        output.push_str(&format!("- `{field}`\n"));
    }
    output.push_str("\n## Source Inputs\n\n");
    output.push_str("| Path | SHA-256 |\n|---|---|\n");
    for input in inputs {
        output.push_str(&format!("| `{}` | `{}` |\n", input.path, input.checksum));
    }
    output
}
```

- [ ] **Step 4: Add missing fixture files for xtask test repo**

Update the path list in `fixture_repo()` in `xtask/src/schemas/tests.rs` so the temporary repo contains every new provider schema source input:

```rust
"crates/axon-embedding/src/provider.rs",
"crates/axon-embedding/src/fake.rs",
"crates/axon-llm/src/provider.rs",
"crates/axon-llm/src/fake.rs",
"crates/axon-vectors/src/store.rs",
"crates/axon-adapters/src/boundary.rs",
"crates/axon-core/src/boundary.rs",
"crates/axon-authz/src/policy.rs",
"crates/axon-observe/src/reservation.rs",
"docs/pipeline-unification/schemas/provider-capability-schema.md",
```

If `schemars::schema_for!` needs real source files for these fixture paths, add them to `needs_real_fixture()`:

```rust
| "crates/axon-api/src/source/capability.rs"
| "docs/pipeline-unification/runtime/provider-contract.md"
| "docs/pipeline-unification/schemas/provider-capability-schema.md"
```

- [ ] **Step 5: Run provider schema tests**

Run:

```bash
cargo test -p xtask provider_schema_is_not_a_skeleton_and_contains_reservation_fields --locked
```

Expected: PASS.

- [ ] **Step 6: Regenerate provider artifacts**

Run:

```bash
cargo xtask schemas providers
```

Expected: updates `docs/reference/runtime/provider-capabilities.schema.json` and `docs/reference/runtime/provider-capabilities.md`; the JSON no longer contains `"skeleton": true`.

- [ ] **Step 7: Verify schema check mode**

Run:

```bash
cargo xtask schemas providers --check
```

Expected: PASS with no stale artifact report.

- [ ] **Step 8: Commit**

```bash
git add xtask/src/schemas/families.rs xtask/src/schemas/tests.rs docs/reference/runtime/provider-capabilities.schema.json docs/reference/runtime/provider-capabilities.md
git commit -m "fix: generate provider capability schema"
```

## Task 2: Complete Strict Memory Fake Behavior

**Files:**
- Modify: `crates/axon-memory/src/store.rs`
- Modify: `crates/axon-memory/src/store_tests.rs`

**Interfaces:**
- Consumes: `MemoryStore`, `FakeMemoryStore`, `MemoryStatusRequest`, `MemoryReviewRequest`, `MemorySupersedeRequest`, `MemoryContradictRequest`.
- Produces: fake behavior for Phase 3 memory requirements: remember, search, decay/reinforce, review, and forget.

- [ ] **Step 1: Inspect exact DTO names before editing**

Run:

```bash
sed -n '250,430p' crates/axon-api/src/source/memory.rs
```

Expected: confirm the current field names for `MemoryStatusRequest`, `MemoryReviewRequest`, `MemorySupersedeRequest`, and `MemoryContradictRequest`.

- [ ] **Step 2: Add failing memory fake test**

Append this test to `crates/axon-memory/src/store_tests.rs`, adjusting only field names if Step 1 shows they differ:

```rust
#[tokio::test]
async fn fake_memory_store_reviews_forgets_supersedes_and_contradicts() {
    let store = FakeMemoryStore::new();
    let original = store
        .remember(request("Original memory"))
        .await
        .unwrap();
    let replacement = store
        .remember(request("Replacement memory"))
        .await
        .unwrap();

    let forgotten = store
        .set_status(MemoryStatusRequest {
            memory_id: original.memory_id.clone(),
            status: MemoryStatus::Forgotten,
            reason: "user requested deletion".to_string(),
            timestamp: Timestamp("2026-07-04T00:00:01Z".to_string()),
        })
        .await
        .unwrap();
    assert_eq!(forgotten.status, MemoryStatus::Forgotten);

    let review = store
        .review(MemoryReviewRequest {
            status: Some(MemoryStatus::Forgotten),
            scope: None,
            limit: 10,
            cursor: None,
        })
        .await
        .unwrap();
    assert_eq!(review.memories.len(), 1);
    assert_eq!(review.memories[0].memory_id, original.memory_id);

    let superseded = store
        .supersede(MemorySupersedeRequest {
            memory_id: original.memory_id.clone(),
            replacement_id: replacement.memory_id.clone(),
            reason: "newer fact".to_string(),
            timestamp: Timestamp("2026-07-04T00:00:02Z".to_string()),
        })
        .await
        .unwrap();
    assert_eq!(superseded.status, MemoryStatus::Superseded);
    let original_record = store.get(original.memory_id.clone()).await.unwrap().unwrap();
    assert_eq!(original_record.superseded_by, Some(replacement.memory_id.clone()));

    let contradicted = store
        .contradict(MemoryContradictRequest {
            left_memory_id: original.memory_id.clone(),
            right_memory_id: replacement.memory_id.clone(),
            reason: "conflicting facts".to_string(),
            timestamp: Timestamp("2026-07-04T00:00:03Z".to_string()),
        })
        .await
        .unwrap();
    assert_eq!(contradicted.status, MemoryStatus::Contradicted);
    let replacement_record = store.get(replacement.memory_id).await.unwrap().unwrap();
    assert_eq!(replacement_record.status, MemoryStatus::Contradicted);
}
```

- [ ] **Step 3: Run the failing test**

Run:

```bash
cargo test -p axon-memory fake_memory_store_reviews_forgets_supersedes_and_contradicts --locked
```

Expected: FAIL because the default trait methods currently return `memory.unsupported_option`.

- [ ] **Step 4: Implement memory status/review/supersede/contradict in the fake**

In `crates/axon-memory/src/store.rs`, replace the default method bodies in the `impl MemoryStore for FakeMemoryStore` block with concrete implementations. Use this shape, adjusting DTO field names to match Step 1:

```rust
async fn supersede(&self, request: MemorySupersedeRequest) -> Result<MemoryResult> {
    let mut state = self.state.lock().await;
    let timestamp = request.timestamp;
    let record = state
        .records
        .get_mut(&request.memory_id)
        .ok_or_else(|| missing_memory(&request.memory_id))?;
    record.status = MemoryStatus::Superseded;
    record.superseded_by = Some(request.replacement_id);
    record.history.push(MemoryHistoryEvent {
        status: MemoryStatus::Superseded,
        message: request.reason,
        timestamp,
    });
    Ok(result_from_record(record, memory_score(record)))
}

async fn contradict(&self, request: MemoryContradictRequest) -> Result<MemoryResult> {
    let mut state = self.state.lock().await;
    let timestamp = request.timestamp;
    {
        let left = state
            .records
            .get_mut(&request.left_memory_id)
            .ok_or_else(|| missing_memory(&request.left_memory_id))?;
        left.status = MemoryStatus::Contradicted;
        left.contradicts = Some(request.right_memory_id.clone());
        left.history.push(MemoryHistoryEvent {
            status: MemoryStatus::Contradicted,
            message: request.reason.clone(),
            timestamp: timestamp.clone(),
        });
    }
    let right = state
        .records
        .get_mut(&request.right_memory_id)
        .ok_or_else(|| missing_memory(&request.right_memory_id))?;
    right.status = MemoryStatus::Contradicted;
    right.contradicts = Some(request.left_memory_id);
    right.history.push(MemoryHistoryEvent {
        status: MemoryStatus::Contradicted,
        message: request.reason,
        timestamp,
    });
    Ok(result_from_record(right, memory_score(right)))
}

async fn set_status(&self, request: MemoryStatusRequest) -> Result<MemoryResult> {
    let mut state = self.state.lock().await;
    let record = state
        .records
        .get_mut(&request.memory_id)
        .ok_or_else(|| missing_memory(&request.memory_id))?;
    record.status = request.status;
    record.history.push(MemoryHistoryEvent {
        status: request.status,
        message: request.reason,
        timestamp: request.timestamp,
    });
    Ok(result_from_record(record, memory_score(record)))
}

async fn review(&self, request: MemoryReviewRequest) -> Result<MemoryReviewResult> {
    let state = self.state.lock().await;
    let mut memories = state
        .records
        .values()
        .filter(|record| request.status.is_none_or(|status| record.status == status))
        .cloned()
        .collect::<Vec<_>>();
    memories.sort_by(|left, right| left.memory_id.0.cmp(&right.memory_id.0));
    memories.truncate(request.limit as usize);
    Ok(MemoryReviewResult {
        memories,
        next_cursor: None,
        warnings: Vec::new(),
    })
}
```

- [ ] **Step 5: Run memory fake tests**

Run:

```bash
cargo test -p axon-memory fake_memory_store_ --locked
```

Expected: PASS for all `fake_memory_store_*` tests.

- [ ] **Step 6: Commit**

```bash
git add crates/axon-memory/src/store.rs crates/axon-memory/src/store_tests.rs
git commit -m "fix: complete memory fake lifecycle behavior"
```

## Task 3: Complete Strict Graph Fake Evidence And Conflict Behavior

**Files:**
- Modify: `crates/axon-graph/src/store.rs`
- Modify: `crates/axon-graph/src/store_tests.rs`

**Interfaces:**
- Consumes: `FakeGraphStore`, `GraphStore::upsert_candidates`, `GraphCandidate`, `GraphWriteResult`.
- Produces: duplicate candidate upserts merge evidence, and incompatible node-kind reuse returns a warning instead of silently overwriting.

- [ ] **Step 1: Add failing graph fake tests**

Append these tests to `crates/axon-graph/src/store_tests.rs`:

```rust
#[tokio::test]
async fn fake_graph_store_merges_evidence_for_existing_edge() {
    let graph = FakeGraphStore::new();
    let mut first = candidate();
    first.evidence[0].evidence_id = "ev-first".to_string();
    let mut second = candidate();
    second.candidate_id = "cand-second".to_string();
    second.evidence[0].evidence_id = "ev-second".to_string();
    second.evidence[0].quote = Some("tokio = { version = \"1\" }".to_string());

    graph.upsert_candidates(vec![first]).await.unwrap();
    graph.upsert_candidates(vec![second]).await.unwrap();

    let result = graph
        .query(GraphQueryRequest {
            start: graph_identifier("pkg:axon"),
            edges: vec!["depends_on".to_string()],
            direction: GraphDirection::Out,
            depth: 1,
            filters: None,
            limit: 10,
            cursor: None,
        })
        .await
        .unwrap();

    assert_eq!(result.edges.len(), 1);
    assert_eq!(result.evidence.len(), 2);
    assert!(
        result
            .evidence
            .iter()
            .any(|evidence| evidence.evidence_id == "ev-second")
    );
}

#[tokio::test]
async fn fake_graph_store_warns_on_node_kind_conflict() {
    let graph = FakeGraphStore::new();
    let mut first = candidate();
    first.nodes[0].node_kind = "package".to_string();
    let mut conflicting = candidate();
    conflicting.candidate_id = "cand-conflict".to_string();
    conflicting.nodes[0].node_kind = "service".to_string();

    graph.upsert_candidates(vec![first]).await.unwrap();
    let written = graph.upsert_candidates(vec![conflicting]).await.unwrap();

    assert!(
        written
            .warnings
            .iter()
            .any(|warning| warning.code == "graph.node_kind_conflict"),
        "graph fake should expose conflicts instead of silently replacing node identity"
    );
}
```

- [ ] **Step 2: Run the failing graph tests**

Run:

```bash
cargo test -p axon-graph fake_graph_store_ --locked
```

Expected: FAIL because the two new tests fail: existing fake edge upsert replaces the edge record and warnings are empty.

- [ ] **Step 3: Merge evidence and emit node-kind conflict warnings**

In `crates/axon-graph/src/store.rs`, update `FakeGraphStore::upsert_candidates()`.

Use this pattern inside the node loop before inserting the node:

```rust
let node_id = GraphNodeId::new(node.stable_key.clone());
if let Some(existing) = state.nodes_by_id.get(&node_id)
    && existing.kind != node.node_kind
{
    warnings.push(SourceWarning {
        code: "graph.node_kind_conflict".to_string(),
        message: format!(
            "graph node {} was previously kind {} but candidate {} reported {}",
            node_id.0, existing.kind, candidate.candidate_id, node.node_kind
        ),
        severity: Severity::Warning,
        metadata: MetadataMap::new(),
    });
    continue;
}
```

Initialize warnings near the counters:

```rust
let mut warnings = Vec::new();
```

Use this pattern in the edge loop instead of blindly replacing an existing edge:

```rust
let new_edge = GraphEdge {
    edge_id: edge_id.clone(),
    kind: edge.edge_kind,
    from_node_id,
    to_node_id,
    authority: AuthorityLevel::Inferred,
    confidence: candidate.confidence,
    evidence: candidate.evidence.clone(),
    metadata: edge.properties,
};

if let Some(existing) = state.edges_by_id.get_mut(&edge_id) {
    for evidence in new_edge.evidence {
        if !existing
            .evidence
            .iter()
            .any(|stored| stored.evidence_id == evidence.evidence_id)
        {
            existing.evidence.push(evidence);
            evidence_records += 1;
        }
    }
    existing.confidence = existing.confidence.max(new_edge.confidence);
} else {
    state.edges_by_id.insert(edge_id, new_edge);
    edges_upserted += 1;
}
```

Return the collected warnings:

```rust
Ok(GraphWriteResult {
    header: stage_header(),
    source_id,
    candidates_seen,
    nodes_upserted,
    edges_upserted,
    evidence_records,
    warnings,
})
```

- [ ] **Step 4: Run graph fake tests**

Run:

```bash
cargo test -p axon-graph fake_graph_store_ --locked
```

Expected: PASS for all `fake_graph_store_*` tests.

- [ ] **Step 5: Commit**

```bash
git add crates/axon-graph/src/store.rs crates/axon-graph/src/store_tests.rs
git commit -m "fix: make graph fake strict for evidence conflicts"
```

## Task 4: Align Phase 3 Checklist Docs

**Files:**
- Modify: `docs/pipeline-unification/delivery/implementation-checklist.md`
- Optional after code merge: GitHub issue #298 body

**Interfaces:**
- Consumes: Phase 3 contract from `implementation-plan.md`, fake requirements from `testing-contract.md`, and provider schema contract from `provider-capability-schema.md`.
- Produces: a Phase 3 checklist that no longer hides provider schema completion or Phase 4 routing work.

- [ ] **Step 1: Edit the Phase 3 checklist text**

Replace the Phase 3 block in `docs/pipeline-unification/delivery/implementation-checklist.md` with:

```markdown
## Phase 3: Stores And Providers

- [ ] implement `LedgerStore`
- [ ] implement `GraphStore`
- [ ] implement `MemoryStore`
- [ ] implement `VectorStore`
- [ ] implement `EmbeddingProvider`
- [ ] implement `LlmProvider`
- [ ] implement `ArtifactStore`
- [ ] implement `JobStore`
- [ ] implement `ConfigStore`
- [ ] implement `CredentialProvider`
- [ ] implement `DocumentCache`
- [ ] implement `HealthProbe`
- [ ] implement `RateLimiter`
- [ ] add strict fakes for all Phase 3 stores/providers
- [ ] add provider reservations/cooling/health
- [ ] generate complete provider capability schema and markdown artifacts

Exit criteria:

- fake-boundary tests can run without Qdrant, TEI, LLMs, browser, or live network
- provider saturation is observable and does not overload TEI/LLM backends
- `docs/reference/runtime/provider-capabilities.schema.json` is not a skeleton artifact
- source routing, URL normalization, authority entrypoints, and adapter registry work remain owned by Phase 4
```

- [ ] **Step 2: Verify docs diff**

Run:

```bash
git diff -- docs/pipeline-unification/delivery/implementation-checklist.md
```

Expected: diff only changes the Phase 3 checklist and exit criteria.

- [ ] **Step 3: Commit**

```bash
git add docs/pipeline-unification/delivery/implementation-checklist.md
git commit -m "docs: clarify phase 3 completion criteria"
```

## Task 5: Run Narrow Phase 3 Verification

**Files:**
- Verify only; no source edits expected.

**Interfaces:**
- Consumes: Tasks 1-4.
- Produces: concrete proof that Phase 3 alignment is complete.

- [ ] **Step 1: Run provider schema check**

```bash
cargo xtask schemas providers --check
```

Expected: PASS.

- [ ] **Step 2: Run fake-boundary crate tests**

```bash
cargo test -p axon-memory fake_memory_store_ --locked
cargo test -p axon-graph fake_graph_store_ --locked
cargo test -p axon-embedding fake_embedding_provider --locked
cargo test -p axon-llm fake_llm_provider --locked
cargo test -p axon-jobs fake_job_store_ --locked
cargo test -p axon-core fake_core --locked
cargo test -p axon-authz fake_credential_provider --locked
```

Expected: all PASS without Qdrant, TEI, LLMs, browser, or network.

- [ ] **Step 3: Run schema generator focused tests**

```bash
cargo test -p xtask provider_schema_is_not_a_skeleton_and_contains_reservation_fields --locked
cargo test -p xtask generate_writes_all_required_family_artifacts --locked
```

Expected: PASS.

- [ ] **Step 4: Run structural checks**

```bash
git diff --check
cargo xtask schemas providers --check
```

Expected: both PASS.

- [ ] **Step 5: Commit verification note if the repo convention requires it**

If there is no generated verification artifact convention for this issue, do not create one. If the active branch is already collecting proof notes under `docs/pipeline-unification/plans/`, create `docs/pipeline-unification/plans/2026-07-04-phase-3-alignment-proof.md` with:

```markdown
# Phase 3 Alignment Proof

- `cargo xtask schemas providers --check`: PASS
- `cargo test -p axon-memory fake_memory_store_ --locked`: PASS
- `cargo test -p axon-graph fake_graph_store_ --locked`: PASS
- `cargo test -p axon-embedding fake_embedding_provider --locked`: PASS
- `cargo test -p axon-llm fake_llm_provider --locked`: PASS
- `cargo test -p axon-jobs fake_job_store_ --locked`: PASS
- `cargo test -p axon-core fake_core --locked`: PASS
- `cargo test -p axon-authz fake_credential_provider --locked`: PASS
- `cargo test -p xtask provider_schema_is_not_a_skeleton_and_contains_reservation_fields --locked`: PASS
- `cargo test -p xtask generate_writes_all_required_family_artifacts --locked`: PASS
- `git diff --check`: PASS
```

Commit only if the proof note is created:

```bash
git add docs/pipeline-unification/plans/2026-07-04-phase-3-alignment-proof.md
git commit -m "docs: record phase 3 alignment proof"
```

## Task 6: Prepare Issue #298 Tracker Update

**Files:**
- No repo file required.
- Optional: update GitHub issue #298 after Tasks 1-5 land.

**Interfaces:**
- Consumes: verified Phase 3 completion proof.
- Produces: issue body update that reflects the contract instead of over-claiming.

- [ ] **Step 1: Re-read the live issue body**

Run:

```bash
gh issue view 298 --json body --jq .body > /tmp/issue-298-body.md
```

Expected: `/tmp/issue-298-body.md` contains the current Phase 3 checklist.

- [ ] **Step 2: Prepare the exact Phase 3 replacement block**

Use this replacement text for issue #298 Phase 3:

```markdown
### Phase 3: Stores, Providers, And Fakes

- [x] Define `LedgerStore`, `GraphStore`, `MemoryStore`, `VectorStore`, `EmbeddingProvider`, `LlmProvider`, `ArtifactStore`, `JobStore`, `ConfigStore`, `CredentialProvider`, `DocumentCache`, `HealthProbe`, and `RateLimiter` boundaries in their owner crates.
- [x] Add strict fake/in-memory implementations for Phase 3 stores/providers.
- [x] Implement provider reservations, cooling, health, and backpressure.
- [x] Generate complete provider capability schema and markdown artifacts from Rust-owned DTOs.
- [x] Prove fake-boundary tests run without Qdrant, TEI, LLMs, browser, or live network.
- [x] Prove provider saturation cannot starve interactive query/ask paths.

Moved out of Phase 3:

- `SourceResolver`/`SourceRouter`, URL normalization, authority entrypoints, and adapter registry acceptance belong to Phase 4 / PR5.
```

- [ ] **Step 3: Update the issue only after the branch is merged or the user explicitly asks**

Run this only when authorized:

```bash
gh issue edit 298 --body-file /tmp/issue-298-body-updated.md
```

Expected: issue #298 Phase 3 matches the verified implementation and no longer claims Phase 4 routing as Phase 3 work.

## Self-Review

- Spec coverage: Phase 3 boundary list is covered by Tasks 2-4; provider capability schema gap is covered by Task 1; fake-boundary strictness is covered by Tasks 2-3; verification is covered by Task 5; issue tracker alignment is covered by Task 6.
- Placeholder scan: this plan contains no `TBD`, no unbounded “handle edge cases,” and no “similar to” task steps.
- Type consistency: all code snippets use currently observed names where known. Task 2 explicitly starts by checking memory DTO field names because those request structs are the only likely drift point.
