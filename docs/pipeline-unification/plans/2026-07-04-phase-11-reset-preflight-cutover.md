# Reset Preflight Cutover Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Finish Phase 11 by making reset, prune, preflight, startup blockers, and repo-structure checks enforce the empty-store clean-break cutover.

**Architecture:** Reset and prune are plan-first, admin-confirmed, chunked, resumable jobs with durable receipts. Startup/doctor/preflight detect incompatible non-empty stores before workers perform side effects, while config rewrite remains explicit and never silently discards `.env` or `config.toml`.

**Tech Stack:** Rust 2024, `axon-cli`, `axon-services`, `axon-jobs`, `axon-prune`, `axon-ledger`, `axon-memory`, `axon-graph`, `axon-vectors`, SQLite, Qdrant fakes, ArtifactStore, `xtask check-repo-structure`.

## Engineering Review Corrections

The Lavra engineering review found that reset execution needed stronger plan binding and safer dry-run semantics. This revision binds destructive execution to an `AuthSnapshot`, config snapshot, store inventory checksum, selectors, estimates, and TTL; separates `reset plan` from `reset --dry-run`; requires re-estimation before mutation; caps Qdrant estimates; adds startup preflight timeouts/caching; and keeps reset and prune as separate destructive flows.

## Global Constraints

- Source of truth: live issue #298 Phase 11, `docs/pipeline-unification/delivery/cutover-contract.md`, `delivery/testing-contract.md`, `delivery/surface-removal-contract.md`, `runtime/pruning-contract.md`, `runtime/job-contract.md`, `runtime/auth-contract.md`, `runtime/storage-contract.md`, `runtime/schema-contract.md`, `runtime/ledger-contract.md`, `runtime/observability-contract.md`, `configuration/env-contract.md`, and `configuration/config-contract.md`.
- `axon reset --dry-run`, `axon reset --yes`, reset receipts, and incompatible-store blockers must be proven before old storage code is deleted.
- Move cleanup/dedupe/purge behavior into `axon-prune`; reset remains a separate destructive clean-slate flow owned by reset services.
- Remove old split crates and root domain modules only after replacement paths pass and reset/preflight blockers are implemented: `axon-vector`, `axon-code-index`, `axon-crawl`, `axon-ingest`, `axon-extract`, and non-bootstrap root `src/*` modules.
- Destructive reset/prune execution requires `axon:admin`; dry-run may be read-only.
- Reset/prune execution binds to a reusable plan id and explicit confirmation.
- Destructive execution must also bind the enqueue-time `AuthSnapshot`, config snapshot id, store inventory epoch/checksum, selectors, estimates, and plan TTL. Re-estimate before the first destructive chunk and fail if stores changed.
- Reset/prune dry-run reports cardinality estimates for SQLite rows, artifacts, and Qdrant points before mutation.
- Execution is chunked, resumable, and receipt-driven.
- Config is preserved or rewritten intentionally only.
- Separate `reset plan` from `reset --dry-run`: dry-run reports without store mutation; reusable execution plans may be stored only as explicit plan artifacts/receipts and must prove no data/schema deletion occurred.
- Qdrant count estimates must use bounded count APIs or capped estimates. Never full-scroll points for dry-run counts.
- Startup preflight vector-store inspection must have strict timeouts and cacheable results so normal startup does not block indefinitely on slow Qdrant inspection.
- Reset and prune are separate destructive flows. Implement reset first; prune receipts/audit can follow unless old-crate deletion directly depends on prune.
- Reset should be receipt-driven and idempotent. Chunked resumability is required for large prune and for reset only where a backend cannot delete a whole store atomically.
- Fresh SQLite schema and fresh Qdrant collection/payload/index shape must be recreated and validated after reset. Old job-family tables, old code-index generations, and old vector payload shape must be absent.
- `axon doctor`, `axon preflight --config`, worker startup, REST reset/preflight routes, and CLI reset/preflight commands must report the same blockers and remediation guidance.
- Use forward-only schema migrations after the new schema lands. Do not add old-data migrations, payload backfills, compatibility readers, dual-write old/new stores, or route tombstones.

---

## File Structure

- Modify: `crates/axon-cli/src/commands/reset.rs`
- Modify: `crates/axon-cli/src/commands/preflight.rs`
- Modify: `crates/axon-cli/src/commands/setup.rs`
- Modify: `crates/axon-services/src/reset.rs`
- Modify: `crates/axon-services/src/prune.rs`
- Modify: `crates/axon-services/src/preflight.rs`
- Modify: `crates/axon-jobs/src/workers/reset.rs` or create it if absent.
- Modify: `crates/axon-prune/src/**`
- Modify: `crates/axon-web/src/**/reset*.rs`
- Modify: `xtask/src/check_repo_structure.rs`
- Test: reset/preflight/prune tests in `axon-cli`, `axon-services`, `axon-jobs`, `axon-web`, and `xtask`.

---

### Task 1: Reset Plan And Receipt DTOs

**Files:**
- Modify: `crates/axon-api/src/reset.rs`
- Modify: `crates/axon-services/src/reset.rs`
- Test: `crates/axon-api/src/reset_tests.rs`

**Interfaces:**
- Produces: `ResetPlan`, `ResetPlanId`, `ResetEstimate`, `ResetReceipt`, `ResetStore`, `ResetExecutionState`.

- [ ] **Step 1: Add failing DTO tests**

```rust
#[test]
fn reset_plan_contains_cutover_inventory() {
    let plan = reset_plan_fixture();
    assert!(plan.stores.contains(&ResetStore::Jobs));
    assert!(plan.stores.contains(&ResetStore::Ledger));
    assert!(plan.stores.contains(&ResetStore::CodeIndex));
    assert!(plan.stores.contains(&ResetStore::Watch));
    assert!(plan.stores.contains(&ResetStore::Memory));
    assert!(plan.stores.contains(&ResetStore::Graph));
    assert!(plan.stores.contains(&ResetStore::Vectors));
    assert!(plan.stores.contains(&ResetStore::Artifacts));
    assert!(plan.estimates.sqlite_rows > 0);
    assert!(plan.receipt_path.is_some());
}
```

- [ ] **Step 2: Run DTO tests**

Run: `cargo test -p axon-api reset_plan --no-fail-fast`

Expected: missing reset inventory fields fail.

- [ ] **Step 3: Implement DTOs**

Reset plan includes selected stores, SQLite table row estimates, Qdrant collection/point estimates, artifact file estimates, config validation results, OAuth/static token guidance, incompatible-store blockers, schema/payload recreation targets, confirmation text, receipt artifact path, plan TTL, inventory checksum, config snapshot id, and enqueue-time auth snapshot id.

- [ ] **Step 4: Run DTO tests**

Run: `cargo test -p axon-api reset_plan --no-fail-fast`

Expected: reset plan and receipt DTOs validate.

---

### Task 2: Reset Dry-Run And Admin Execution

**Files:**
- Modify: `crates/axon-cli/src/commands/reset.rs`
- Modify: `crates/axon-services/src/reset.rs`
- Modify: `crates/axon-jobs/src/workers/reset.rs`
- Test: `crates/axon-services/src/reset_tests.rs`
- Test: `crates/axon-cli/src/commands/reset_tests.rs`

**Interfaces:**
- Consumes: `AuthSnapshot`, `ResetPlan`.
- Produces: `axon reset --dry-run`, `axon reset --yes`, and job-backed execution.

- [ ] **Step 1: Add failing reset tests**

```rust
#[tokio::test]
async fn reset_dry_run_reports_counts_without_mutation() {
    let harness = reset_harness_with_rows_and_vectors().await;
    let plan = harness.reset_dry_run().await.unwrap();
    assert!(plan.estimates.sqlite_rows > 0);
    assert!(plan.estimates.qdrant_points > 0);
    assert!(plan.estimates.artifact_files > 0);
    assert_eq!(harness.sqlite_row_count().await, plan.estimates.sqlite_rows);
}

#[tokio::test]
async fn reset_exec_requires_admin_and_confirmed_plan() {
    let harness = reset_harness_with_rows_and_vectors().await;
    let err = harness.reset_exec_without_admin().await.unwrap_err();
    assert_eq!(err.code.to_string(), "auth.scope_required");
    let err = harness.reset_exec_without_plan_id(admin_snapshot()).await.unwrap_err();
    assert_eq!(err.code.to_string(), "reset.plan_required");
}
```

- [ ] **Step 2: Run reset tests**

Run: `cargo test -p axon-services reset --no-fail-fast`

Expected: dry-run/admin/plan-id behavior fails until implemented.

- [ ] **Step 3: Implement plan-first execution**

`reset --dry-run` reports counts without mutation and may store only explicit plan/receipt-preview artifacts. `reset --yes --plan-id <id>` executes the plan as a reset job with admin snapshot. CLI can create and execute in one command only when the generated plan id is bound to the same normalized body, config snapshot, auth snapshot, selectors, estimates, inventory checksum, and unexpired TTL.

- [ ] **Step 4: Implement chunked execution**

Re-estimate before the first destructive chunk and fail if stores changed. Chunk SQLite deletes by table/key ranges, Qdrant deletes by collection/page selectors, and artifact deletes by directory entries. Each chunk writes receipt progress and can resume from the last completed chunk.

- [ ] **Step 5: Recreate fresh stores and validate target shape**

After destructive reset, initialize the fresh SQLite schema, recreate the Qdrant collection with target payload indexes, invalidate old auth/token cache state or surface re-auth guidance, and write created-shape details into the receipt.

- [ ] **Step 6: Run reset tests**

Run:

```bash
cargo test -p axon-services reset --no-fail-fast
cargo test -p axon-cli reset --no-fail-fast
```

Expected: dry-run, admin enforcement, plan-id binding, chunking, fresh schema/Qdrant shape, auth guidance, and receipts pass.

---

### Task 3: Prune Plan Execution Receipts And Audit Events

**Files:**
- Modify: `crates/axon-services/src/prune.rs`
- Modify: `crates/axon-prune/src/**`
- Modify: `crates/axon-jobs/src/workers/prune.rs`
- Test: `crates/axon-services/src/prune_tests.rs`

**Interfaces:**
- Produces: prune planning, confirmation, execution, interruption, resume, completion audit events.

- [ ] **Step 1: Add failing prune audit test**

```rust
#[tokio::test]
async fn prune_emits_audit_events_for_lifecycle() {
    let harness = prune_harness().await;
    let plan = harness.plan_prune(admin_snapshot()).await.unwrap();
    harness.execute_prune(plan.prune_plan_id.clone(), admin_snapshot()).await.unwrap();
    let audit = harness.audit_events();
    assert!(audit.contains_kind("prune.plan"));
    assert!(audit.contains_kind("prune.confirm"));
    assert!(audit.contains_kind("prune.execute"));
    assert!(audit.contains_kind("prune.complete"));
}
```

- [ ] **Step 2: Run prune tests**

Run: `cargo test -p axon-services prune --no-fail-fast`

Expected: audit and receipt fields fail until implemented.

- [ ] **Step 3: Implement prune receipts**

Prune plans include selectors, estimated SQLite rows, Qdrant points, artifact files, risk flags, confirmation requirements, generation fences, chunk plan, and receipt artifact target. Execution updates receipt after every chunk.

- [ ] **Step 4: Move old purge/dedupe/cleanup behavior behind prune**

Route purge, dedupe, job/cache retention, cleanup debt, vector deletes, artifact deletes, graph orphan cleanup, and memory forgetting cleanup through `axon-prune` plans. Delete ad hoc direct Qdrant/SQLite/filesystem destructive paths once prune coverage exists.

- [ ] **Step 5: Run prune tests**

Run: `cargo test -p axon-services prune --no-fail-fast`

Expected: prune lifecycle, audit events, generation fencing, idempotent cleanup-debt retries, and resumable receipts pass.

---

### Task 4: Startup Doctor And Incompatible Store Blockers

**Files:**
- Modify: `crates/axon-services/src/preflight.rs`
- Modify: `crates/axon-cli/src/commands/doctor.rs`
- Modify: `crates/axon-cli/src/commands/unified_server.rs`
- Modify: `crates/axon-jobs/src/runtime.rs`
- Test: `crates/axon-services/src/preflight_tests.rs`

**Interfaces:**
- Produces: `IncompatibleStoreReport` and worker startup block.

- [ ] **Step 1: Add failing startup blocker test**

```rust
#[tokio::test]
async fn incompatible_non_empty_store_blocks_workers_before_side_effects() {
    let harness = startup_harness_with_legacy_job_rows().await;
    let err = harness.start_unified_workers().await.unwrap_err();
    assert_eq!(err.code.to_string(), "startup.incompatible_store");
    assert_eq!(harness.side_effect_count(), 0);
}
```

- [ ] **Step 2: Run preflight tests**

Run: `cargo test -p axon-services preflight incompatible --no-fail-fast`

Expected: startup does not yet block all incompatible stores.

- [ ] **Step 3: Implement store inventory**

Preflight inventories jobs, legacy job-family tables, ledger/source/code-index/watch/memory tables, graph, vectors, artifacts, config files, OAuth/static token settings, generated schemas, and Qdrant payload/index shape. Vector-store inspection uses bounded count APIs, strict timeouts, and cacheable results.

- [ ] **Step 4: Block unified workers**

Doctor/startup returns an actionable report and does not spawn source/watch/source-job/prune workers until reset succeeds or a developer override is explicitly set. Reset planning may remain available so operators can recover.

- [ ] **Step 5: Add fresh-shape blocker tests**

Assert old job-family tables, old code-index generations, stale watch rows, old Qdrant payload shape, missing target Qdrant indexes, and stale generated schema manifests are all reported with reset/reindex remediation.

- [ ] **Step 6: Run blocker tests**

Run: `cargo test -p axon-services preflight incompatible --no-fail-fast`

Expected: incompatible stores block workers before side effects, reset remains recoverable, and fresh stores pass.

---

### Task 5: Config Preflight And Rewrite Dry-Run

**Files:**
- Modify: `crates/axon-cli/src/commands/preflight.rs`
- Modify: `crates/axon-cli/src/commands/setup.rs`
- Modify: `crates/axon-core/src/config/**`
- Test: `crates/axon-cli/src/commands/preflight_tests.rs`
- Test: `crates/axon-core/src/config/config_rewrite_tests.rs`

**Interfaces:**
- Produces: `axon preflight --config` and `axon setup config rewrite --dry-run`.

- [ ] **Step 1: Add failing config tests**

```rust
#[test]
fn preflight_reports_removed_config_keys_with_replacements() {
    let report = preflight_config_fixture("AXON_MCP_HTTP_TOKEN=secret").unwrap_err();
    assert_eq!(report.code, "config.removed_key");
    assert_eq!(report.replacement.as_deref(), Some("AXON_HTTP_TOKEN"));
}

#[test]
fn setup_rewrite_dry_run_does_not_write_files() {
    let fs = fake_config_fs_with_stale_keys();
    let preview = setup_config_rewrite_dry_run(&fs).unwrap();
    assert!(preview.env_edits.iter().any(|edit| edit.new_key == "AXON_HTTP_TOKEN"));
    assert_eq!(fs.write_count(), 0);
}
```

- [ ] **Step 2: Run config tests**

Run:

```bash
cargo test -p axon-cli preflight --no-fail-fast
cargo test -p axon-core config_rewrite --no-fail-fast
```

Expected: stale key reporting and dry-run behavior fail until implemented.

- [ ] **Step 3: Implement config validation and rewrite preview**

Use the same removed-key registry as generated schemas. Preserve `.env` and `config.toml`; rewrite only with explicit non-dry-run confirmation and report every changed key/path, placement error, redacted secret, and restart/reload requirement.

- [ ] **Step 4: Run config tests**

Run:

```bash
cargo test -p axon-cli preflight --no-fail-fast
cargo test -p axon-core config_rewrite --no-fail-fast
```

Expected: preflight and rewrite dry-run pass.

---

### Task 6: Update `xtask check-repo-structure`

**Files:**
- Modify: `xtask/src/check_repo_structure.rs`
- Test: `xtask/src/check_repo_structure_tests.rs`

**Interfaces:**
- Produces: end-state repo structure validation.

- [ ] **Step 1: Add failing structure test**

```rust
#[test]
fn repo_structure_check_uses_end_state_contract() {
    let report = check_repo_structure_fixture(end_state_workspace()).unwrap();
    assert!(report.required_crates.contains("axon-adapters"));
    assert!(report.required_artifacts.contains("docs/reference/sources/vector-payload.schema.json"));
    assert!(report.removed_crates_absent.contains("axon-code-index"));
}
```

- [ ] **Step 2: Run xtask structure tests**

Run: `cargo test -p xtask check_repo_structure --no-fail-fast`

Expected: existing PR0 skeleton validation fails the end-state expectations.

- [ ] **Step 3: Implement end-state checks**

Validate real target-crate dependencies, required fixtures/generated artifacts, removed-crate absence after cutover, root bootstrap-only state, generated schema/doc source manifests, public surface removal checks, and no non-bootstrap root `src/*` domain modules.

- [ ] **Step 4: Run structure tests**

Run: `cargo test -p xtask check_repo_structure --no-fail-fast`

Expected: repo-structure check matches current/end-state contract.

---

### Task 7: Final Phase 11 Verification

- [ ] **Step 1: Run targeted reset/preflight checks**

Run:

```bash
cargo test -p axon-services reset preflight prune --no-fail-fast
cargo test -p axon-cli reset preflight setup --no-fail-fast
cargo test -p axon-jobs reset prune --no-fail-fast
cargo test -p axon-web reset preflight --no-fail-fast
cargo test -p xtask check_repo_structure --no-fail-fast
```

Expected: targeted Phase 11 behavior passes without live providers.

- [ ] **Step 2: Run schema/repo checks**

Run:

```bash
cargo xtask schemas generate --check
cargo xtask docs generate --check
cargo xtask check
```

Expected: generated artifacts, docs, reset/preflight/prune contracts, and repo structure are current.

- [ ] **Step 3: Run Tier 5 reset/preflight cutover cases**

Run: `cargo test --workspace tier5 --no-fail-fast`

Expected: incompatible store block, reset dry-run, reset yes, fresh SQLite/Qdrant shape, removed config validation, canonical local/web reindex, ask/query retrieval from target payloads, and provider backpressure cases pass or are explicitly tracked as Phase 12 blockers.
