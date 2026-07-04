# Old Crate Removal Final Issue Sync Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Finish Phase 12 by removing old crates/modules only after replacement paths pass, running final cutover verification, updating issue #298, and filing deferrable hardening follow-ups.

**Architecture:** Treat crate removal as the last cleanup gate, not an implementation shortcut. Prove canonical replacements, generated removal checks, reset/preflight, Tier 5 cutover cases, and issue checklist state before marking #298 complete.

**Tech Stack:** Rust workspace, Cargo metadata, `cargo xtask check`, GitHub CLI, issue #298 checklist, generated schemas, Tier 5 fake/local cutover tests.

## Engineering Review Corrections

The Lavra engineering review found that old-crate deletion must be blocked by replacement-path evidence, not by import absence alone. This revision requires Tier 5 replacement/cutover cases before deletion, dispatch and call-path tests for removed operations, smaller implementation PRs, and explicit issue-scope honesty: full all-source fixture completeness cannot be deferred if issue #298 is marked complete.

## Global Constraints

- Source of truth: all contracts under `docs/pipeline-unification`, especially `delivery/cutover-contract.md`, `delivery/testing-contract.md`, `delivery/surface-removal-contract.md`, `delivery/dependency-order-map.md`, and `crates/README.md`.
- Remove `axon-vector`, `axon-code-index`, `axon-crawl`, `axon-ingest`, and `axon-extract` only after replacement paths and tests pass.
- Prove no compatibility facades, no transport imports of domain internals, no old code paths reachable from canonical surfaces, and root `axon` is bootstrap only.
- Task-level verification is narrower than the final gate; full workspace tests and live smoke belong at final cutover.
- Before marking #298 complete, run Tier 5 cutover cases from `delivery/testing-contract.md`.
- Run Tier 5 replacement-path/cutover cases before the final old-crate deletion commit, or at minimum before merging that deletion. Crate deletion depends on replacement path proof.
- Import absence is not enough. Add dispatch/call-path tests proving removed CLI/MCP/REST operations cannot reach old modules or compatibility facades.
- Do not combine crate deletion, Tier 5 harness creation, issue audit, and follow-up issue filing in one implementation PR unless explicitly requested.
- Full all-source fixture completeness cannot be deferred if #298 is being marked complete. If it remains incomplete, #298 must remain open or the issue scope must be explicitly narrowed.
- Filing follow-up GitHub issues should not block code cutover unless Jacob explicitly asks for issue hygiene in the same PR.

---

## File Structure

- Modify: `Cargo.toml`
- Modify: crate dependency manifests that still import old crates.
- Delete only after proof: `crates/axon-vector`, `crates/axon-code-index`, `crates/axon-crawl`, `crates/axon-ingest`, `crates/axon-extract`
- Modify: `src/main.rs`, `src/lib.rs` only if bootstrap-only proof exposes old behavior.
- Modify: `xtask/src/check_repo_structure.rs`
- Modify: generated artifacts under `docs/reference/**`
- Update: GitHub issue `#298`
- Create follow-up GitHub issues for deferrable hardening.

---

### Task 1: Replacement Path Gate Before Old Crate Removal

**Files:**
- Test: `xtask/src/check_repo_structure_tests.rs`
- Modify: `xtask/src/check_repo_structure.rs`

**Interfaces:**
- Produces: old-crate removal readiness report.

- [ ] **Step 1: Add failing readiness test**

```rust
#[test]
fn old_crate_removal_is_blocked_until_replacements_pass() {
    let report = old_crate_removal_readiness(workspace_with_old_imports()).unwrap();
    assert!(!report.ready);
    assert!(report.blockers.iter().any(|b| b.contains("transport imports old domain crate")));
}
```

- [ ] **Step 2: Run readiness test**

Run: `cargo test -p xtask old_crate_removal --no-fail-fast`

Expected: blockers are reported for any remaining old crate usage.

- [ ] **Step 3: Implement readiness checks**

Check:

```text
canonical source paths pass targeted tests
generated removal checks pass
reset/preflight blockers pass
no public surface imports old crate handlers
no compatibility facade crates remain
no transport imports domain internals
root axon contains bootstrap only
Cargo metadata has no old crate package after deletion
```

- [ ] **Step 4: Run readiness checks**

Run: `cargo test -p xtask old_crate_removal --no-fail-fast`

Expected: readiness fails until all blockers are resolved.

---

### Task 2: Remove Old Crate Imports And Facades

**Files:**
- Modify: all `Cargo.toml` files with old crate dependencies.
- Modify: service/CLI/MCP/web modules importing old crates.
- Test: `xtask/src/check_repo_structure_tests.rs`

**Interfaces:**
- Produces: workspace with no reachable old crate code paths.

- [ ] **Step 1: Identify old imports**

Run:

```bash
cargo metadata --no-deps --format-version 1 > target/old-crate-metadata.json
```

Expected: metadata file lists current workspace crates before removal.

- [ ] **Step 2: Add import absence test**

```rust
#[test]
fn transports_do_not_import_old_domain_crates() {
    let report = scan_workspace_imports().unwrap();
    for old in ["axon_code_index", "axon_crawl", "axon_ingest", "axon_extract", "axon_vector"] {
        assert!(!report.transport_imports.iter().any(|import| import.crate_name == old));
    }
}
```

- [ ] **Step 3: Remove old imports**

Replace old imports with target crates from the pipeline-unification contracts: source adapters, ledger, vectors, retrieval, graph, jobs, memory, prune, and services. Delete compatibility facade modules instead of forwarding to canonical paths.

- [ ] **Step 4: Run import checks**

Run: `cargo test -p xtask transports_do_not_import_old_domain_crates --no-fail-fast`

Expected: no transport import reaches old domain internals.

---

### Task 3: Delete Old Crates From Workspace

**Files:**
- Modify: `Cargo.toml`
- Delete: `crates/axon-vector`
- Delete: `crates/axon-code-index`
- Delete: `crates/axon-crawl`
- Delete: `crates/axon-ingest`
- Delete: `crates/axon-extract`
- Modify generated references through schema/docs generator.

**Interfaces:**
- Produces: workspace without old crates.

- [ ] **Step 1: Run pre-delete targeted checks**

Run:

```bash
cargo test -p axon-services source --no-fail-fast
cargo test -p axon-vectors generation prune --no-fail-fast
cargo xtask schemas generate --check
```

Expected: replacement source/vector/schema paths pass before deletion.

- [ ] **Step 2: Remove crates from workspace**

Delete old crate directories and remove package entries/dependencies from workspace manifests. Do not keep empty crates or compatibility packages with old names.

- [ ] **Step 3: Run metadata absence check**

Run:

```bash
cargo metadata --no-deps --format-version 1 > target/post-removal-metadata.json
```

Expected: metadata contains none of `axon-vector`, `axon-code-index`, `axon-crawl`, `axon-ingest`, or `axon-extract`.

- [ ] **Step 4: Run repo-structure check**

Run: `cargo xtask check-repo-structure`

Expected: removed-crate absence passes.

---

### Task 4: Phase 6 Through Phase 12 Issue Checklist Audit

**Files:**
- No code files.
- External: GitHub issue `#298`.

**Interfaces:**
- Produces: exact checked/unchecked status update on issue #298.

- [ ] **Step 1: Fetch issue body and comments**

Run:

```bash
gh issue view 298 --json number,title,body,comments,state,url > target/issue-298-final-audit.json
```

Expected: local JSON contains the live checklist and comments.

- [ ] **Step 2: Audit checklist items**

For every Phase 6, 7, 8, 9, 10, 11, and 12 unchecked item, record:

```text
issue checklist text
implemented evidence path/test
status: checked or remains unchecked
reason if unchecked
follow-up issue if deferrable
```

- [ ] **Step 3: Update issue body**

Use `gh issue edit 298 --body-file target/issue-298-updated-body.md` after updating checklist boxes. Only check items backed by code/docs/tests in the current branch.

- [ ] **Step 4: Post audit summary comment**

Run:

```bash
gh issue comment 298 --body-file target/issue-298-final-audit-comment.md
```

Expected: issue comment lists exact checked/unchecked changes and final blockers if any.

---

### Task 5: File Deferrable Hardening Follow-Ups

**Files:**
- External: GitHub issues.

**Interfaces:**
- Produces: follow-up issues for non-blocking hardening.

- [ ] **Step 1: Create follow-up issue drafts**

Draft issues for:

```text
full all-source fixture completeness
presentation/token parity
Android/Chrome client parity
full logs/traces status parity
memory graph/vector enhancements not required for current memory contract
```

- [ ] **Step 2: File follow-ups**

Run one `gh issue create` command per follow-up, each with title, body, labels, and link back to #298.

Expected: every deferrable hardening item has a durable issue URL.

- [ ] **Step 3: Comment follow-up list on #298**

Append the follow-up issue URLs to the #298 final audit comment or a separate comment if the final audit is already posted.

---

### Task 6: Tier 5 Cutover Cases

**Files:**
- Test: `crates/axon-services/src/tier5_cutover_tests.rs`
- Test: `crates/axon-cli/src/tier5_cutover_tests.rs`
- Test: `crates/axon-web/src/tier5_cutover_tests.rs`
- Test: `crates/axon-mcp/src/tier5_cutover_tests.rs`

**Interfaces:**
- Consumes: completed Phases 6-11.
- Produces: final cutover case coverage from `delivery/testing-contract.md`.

- [ ] **Step 1: Add Tier 5 test harness**

```rust
#[tokio::test]
async fn tier5_incompatible_store_block_and_reset_flow() {
    let harness = tier5_harness_with_incompatible_store().await;
    assert_eq!(harness.start_workers().await.unwrap_err().code.to_string(), "startup.incompatible_store");
    let plan = harness.reset_dry_run().await.unwrap();
    assert!(plan.estimates.sqlite_rows > 0);
    let receipt = harness.reset_yes(plan.reset_plan_id).await.unwrap();
    assert!(receipt.created.qdrant_collections.contains(&"axon".to_string()));
}
```

- [ ] **Step 2: Add remaining Tier 5 cases**

Cover removed config validation, removed CLI/MCP/REST absence, old job/code-index/Qdrant payload absence, canonical local source reindex, canonical web source reindex, ask/query retrieval from target payloads, provider backpressure during fresh reindex, and interrupted partial generation not searchable after restart.

- [ ] **Step 3: Run Tier 5 tests**

Run: `cargo test --workspace tier5 --no-fail-fast`

Expected: all Tier 5 fake/local cutover tests pass.

---

### Task 7: Final Verification Gate

- [ ] **Step 1: Run final format check**

Run: `cargo fmt --all -- --check`

Expected: no formatting changes needed.

- [ ] **Step 2: Run repository check**

Run: `cargo xtask check`

Expected: schemas, generated artifacts, repo structure, removal checks, and contract checks pass.

- [ ] **Step 3: Run full workspace tests**

Run: `cargo test --workspace --no-fail-fast`

Expected: all workspace tests pass.

- [ ] **Step 4: Verify old crate absence**

Run:

```bash
cargo metadata --no-deps --format-version 1 | jq -r '.packages[].name' | sort > target/workspace-packages.txt
```

Expected: output does not include `axon-vector`, `axon-code-index`, `axon-crawl`, `axon-ingest`, or `axon-extract`.

- [ ] **Step 5: Mark #298 complete only after evidence exists**

Update #298 only if Tasks 1-7 pass and the checklist audit has no required unchecked items. If required items remain, leave #298 open and list blockers with exact files/tests.
