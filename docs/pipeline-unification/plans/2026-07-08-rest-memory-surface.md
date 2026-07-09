# REST Memory Surface Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close Phase 3B Task 9's remaining gap — split the single opaque `POST /v1/memory` passthrough into per-verb `/v1/memories` REST routes matching the contract, and add `import`/`export` to CLI, MCP, and REST (currently missing from all three transports).

**Architecture:** `MemoryService` already implements the full lifecycle (remember/search/context/show/link/supersede/reinforce/contradict/pin/archive/forget/review/compact/import/export) per Phase 3B Tasks 5-7 — this is purely a transport-surface change, no new service logic.

**Tech Stack:** Rust 2024, Axum, `axon-web`, `axon-cli`, `axon-mcp`.

## Global Constraints

- Split out of `2026-07-08-finish-job-cutover-and-security-completion.md` per engineering review: independent of the job cutover.
- Import/export routes must have explicit size limits, and must go through the same auth-scope check every other `/v1/*` route uses — a security review found the current single passthrough handler shows no auth extraction, unlike `crates/axon-web/src/server/handlers/sources.rs`'s `AuthContext`-gated pattern, and the new import/export routes must not repeat that gap.
- Do not delete the old `POST /v1/memory` passthrough route in the same change that adds the new per-verb routes without an explicit deprecation window — an external client (e.g. the desktop palette app) may already depend on the old shape. Mark it deprecated (still functional, logs a deprecation warning) for at least one release before removal, and file a follow-up for the actual removal.
- Do not edit `CLAUDE.md`, `AGENTS.md`, or `GEMINI.md`.
- Commit after each task's verification passes.

---

## Source-Of-Truth Contracts

- `docs/pipeline-unification/plans/2026-07-04-phase-3b-security-error-memory-completion.md` (Task 9)
- `docs/pipeline-unification/surfaces/rest-contract.md`

## Current-State Anchors

- Existing single passthrough route: `crates/axon-web/src/server/routing.rs` (confirmed at the `POST /v1/memory` registration), handler in `crates/axon-web/src/**/memory*.rs` — read the handler in full first to see its current request-shape handling and confirm it genuinely lacks auth extraction (a prior review pass flagged this; verify before assuming).
- Auth-gated pattern to copy: `crates/axon-web/src/server/handlers/sources.rs::caller_context_from_auth(&AuthContext) -> CallerContext`.
- `MemoryService`'s complete lifecycle methods (already implemented, Phase 3B Tasks 5-7): read `crates/axon-services/src/memory.rs` for the real method signatures for `remember`/`search`/`context`/`show`/`link`/`supersede`/`reinforce`/`contradict`/`pin`/`archive`/`forget`/`review`/`compact`/`import`/`export`.
- CLI memory commands: `crates/axon-cli/src/commands/memory.rs` (13 of ~15 subactions already wired per prior audit — missing `import`/`export`).
- MCP memory action: `crates/axon-mcp/src/server/handlers_memory.rs` (same gap).

## File Structure

- Modify: `crates/axon-web/src/server/routing.rs`
- Modify/split: `crates/axon-web/src/**/memory*.rs` into per-verb handlers
- Modify: `crates/axon-cli/src/commands/memory.rs`
- Modify: `crates/axon-mcp/src/server/handlers_memory.rs`
- Test: `crates/axon-web/src/memory_tests.rs`, `crates/axon-cli/src/commands/memory_tests.rs`, `crates/axon-mcp/src/memory_tests.rs`

---

## Task 1: Read The Current Handler And Confirm The Auth Gap

**Files:**
- Read only: `crates/axon-web/src/**/memory*.rs`, `crates/axon-web/src/server/handlers/sources.rs`, `crates/axon-services/src/memory.rs`

**Interfaces:**
- Consumes: nothing — research step.
- Produces: confirmed real request/response shape of the current passthrough handler, confirmed presence/absence of auth extraction, confirmed real `MemoryService` method signatures.

- [ ] **Step 1: Read the current handler in full**

Read the existing `POST /v1/memory` handler completely. Note exactly what request shape it accepts today and how it dispatches to `MemoryService`. Confirm whether it extracts `AuthContext`/builds a `CallerContext` at all — if it already does, the "add auth gating" step below becomes "verify parity" instead of "add from scratch."

- [ ] **Step 2: Read `MemoryService`'s real method signatures**

Read `crates/axon-services/src/memory.rs` in full for the exact signatures of every lifecycle method, especially `import`/`export` (their request/response DTOs — `MemoryImportRequest`/`MemoryImportResult`/`MemoryExportRequest`/`MemoryExportResult` per Phase 3B Task 7).

## Task 2: Add Per-Verb REST Routes With Auth Gating

**Files:**
- Modify: `crates/axon-web/src/server/routing.rs`
- Modify/split: `crates/axon-web/src/**/memory*.rs`
- Test: `crates/axon-web/src/memory_tests.rs`

**Interfaces:**
- Consumes: Task 1's confirmed shapes.
- Produces: `POST /v1/memories`, `GET /v1/memories/search`, `GET /v1/memories/{memory_id}`, `POST /v1/memories/{memory_id}/link`, `POST /v1/memories/{memory_id}/supersede`, `POST /v1/memories/{memory_id}/reinforce`, `POST /v1/memories/{memory_id}/contradict`, `POST /v1/memories/{memory_id}/pin`, `POST /v1/memories/{memory_id}/archive`, `DELETE /v1/memories/{memory_id}` (forget), `POST /v1/memories/review`, `POST /v1/memories/{memory_id}/compact`, plus the old `POST /v1/memory` kept functional but marked deprecated.

- [ ] **Step 1: Write failing route-existence test**

```rust
#[tokio::test]
async fn rest_exposes_per_verb_memory_routes() {
    let app = test_app().await;
    for (method, path) in [
        ("POST", "/v1/memories"),
        ("GET", "/v1/memories/search"),
        ("GET", "/v1/memories/{memory_id}"),
        ("POST", "/v1/memories/{memory_id}/link"),
        ("POST", "/v1/memories/{memory_id}/supersede"),
        ("POST", "/v1/memories/{memory_id}/reinforce"),
        ("POST", "/v1/memories/{memory_id}/contradict"),
        ("POST", "/v1/memories/{memory_id}/pin"),
        ("POST", "/v1/memories/{memory_id}/archive"),
        ("DELETE", "/v1/memories/{memory_id}"),
        ("POST", "/v1/memories/review"),
        ("POST", "/v1/memories/{memory_id}/compact"),
    ] {
        assert!(route_exists(&app, method, path), "missing route {method} {path}");
    }
}

#[tokio::test]
async fn old_passthrough_route_still_works_but_logs_deprecation() {
    let app = test_app().await;
    assert!(route_exists(&app, "POST", "/v1/memory"));
    // assert deprecation warning is logged/header set — match whatever
    // deprecation-signaling pattern this repo already uses elsewhere, if any;
    // otherwise add a `Deprecation` response header per RFC 8594.
}
```

Reuse whatever `test_app()`/`route_exists()` helpers `crates/axon-web/src/jobs_tests.rs` (from the Task 3A durable-job-cutover work) already established — do not invent a second pattern.

- [ ] **Step 2: Run test and confirm failure**

Run: `cargo test -p axon-web rest_exposes_per_verb_memory_routes old_passthrough_route_still_works_but_logs_deprecation --no-fail-fast`

Expected: both FAIL.

- [ ] **Step 3: Implement per-verb handlers with auth gating**

Split the single passthrough into per-verb Axum handlers, each: (1) extracts `AuthContext` and builds a `CallerContext`/`AuthSnapshot` via `caller_context_from_auth` exactly like `sources.rs` does, (2) parses its specific request DTO, (3) calls the matching `MemoryService` method — no duplicated lifecycle logic in the handler. Keep the old `POST /v1/memory` route registered, routing to the same underlying logic as the new `POST /v1/memories`, with a `Deprecation` response header (or this repo's existing deprecation-signaling convention if one exists — check first).

- [ ] **Step 4: Run tests**

```bash
cargo test -p axon-web memory --no-fail-fast
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/axon-web/src
git commit -m "feat(web): split memory REST surface into per-verb routes, deprecate passthrough"
```

## Task 3: Add Import/Export To CLI, MCP, And REST

**Files:**
- Modify: `crates/axon-cli/src/commands/memory.rs`
- Modify: `crates/axon-mcp/src/server/handlers_memory.rs`
- Modify: `crates/axon-web/src/**/memory*.rs`
- Test: `crates/axon-cli/src/commands/memory_tests.rs`, `crates/axon-mcp/src/memory_tests.rs`, `crates/axon-web/src/memory_tests.rs`

**Interfaces:**
- Consumes: `MemoryService::import`/`export` (Task 1's confirmed signatures).
- Produces: `axon memory import <path>` / `axon memory export [--output <path>]` CLI subcommands; `import`/`export` MCP subactions; `POST /v1/memories/import`, `POST /v1/memories/export` REST routes — all with explicit size limits.

- [ ] **Step 1: Write failing tests for all three transports**

```rust
// crates/axon-cli/src/commands/memory_tests.rs
#[test]
fn cli_memory_has_import_and_export_subcommands() {
    let subcommands = memory_subcommand_names();
    assert!(subcommands.contains(&"import"));
    assert!(subcommands.contains(&"export"));
}

// crates/axon-mcp/src/memory_tests.rs
#[test]
fn mcp_memory_registry_contains_import_and_export() {
    let actions = memory_subactions();
    assert!(actions.contains(&"import"));
    assert!(actions.contains(&"export"));
}

// crates/axon-web/src/memory_tests.rs
#[tokio::test]
async fn rest_exposes_import_and_export_routes_with_size_limit() {
    let app = test_app().await;
    assert!(route_exists(&app, "POST", "/v1/memories/import"));
    assert!(route_exists(&app, "POST", "/v1/memories/export"));
    let oversized_body = vec![0u8; MAX_MEMORY_IMPORT_BYTES + 1];
    let response = post_memory_import(&app, oversized_body).await;
    assert_eq!(response.status(), axum::http::StatusCode::PAYLOAD_TOO_LARGE);
}
```

- [ ] **Step 2: Run tests and confirm failure**

```bash
cargo test -p axon-cli cli_memory_has_import_and_export_subcommands --no-fail-fast
cargo test -p axon-mcp mcp_memory_registry_contains_import_and_export --no-fail-fast
cargo test -p axon-web rest_exposes_import_and_export_routes_with_size_limit --no-fail-fast
```

Expected: all FAIL.

- [ ] **Step 3: Implement CLI import/export**

Add `axon memory import <path>` reading a local file and calling `MemoryService::import`, and `axon memory export [--output <path>]` calling `MemoryService::export` and writing to the given path or stdout.

- [ ] **Step 4: Implement MCP import/export**

Add `import`/`export` to the MCP `memory` action's subaction list, following the existing subaction dispatch pattern in `handlers_memory.rs`.

- [ ] **Step 5: Implement REST import/export with a size limit**

Add `POST /v1/memories/import` and `POST /v1/memories/export`, with a `MAX_MEMORY_IMPORT_BYTES`/`MAX_MEMORY_EXPORT_BYTES` constant (pick a conservative default, e.g. 10 MiB, matching whatever size-limit convention other large-body routes in this codebase already use — check `crates/axon-web/src/server` for an existing body-size-limit pattern before inventing a new one) enforced via Axum's body-size-limit extractor or middleware, returning `413 Payload Too Large` on violation. Gate both routes with the same `AuthContext` extraction as Task 2's other new routes.

- [ ] **Step 6: Run tests**

```bash
cargo test -p axon-cli memory --no-fail-fast
cargo test -p axon-mcp memory --no-fail-fast
cargo test -p axon-web memory --no-fail-fast
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/axon-cli/src crates/axon-mcp/src crates/axon-web/src
git commit -m "feat: add memory import/export to CLI, MCP, and REST with size limits"
```

## Task 4: File The Old-Route Removal Follow-Up And Verify

- [ ] **Step 1: File the deprecation-removal follow-up**

```bash
bd create --title="Remove deprecated POST /v1/memory passthrough route" --description="Task 2 of docs/pipeline-unification/plans/2026-07-08-rest-memory-surface.md kept the old single-route memory passthrough functional (marked deprecated) alongside the new per-verb /v1/memories routes, to avoid breaking existing external clients (e.g. desktop palette) without a migration window. Remove the old route once all known clients have migrated to the per-verb routes — check the palette app and any other REST memory consumers first." --type=task --priority=3
```

- [ ] **Step 2: Full crate gate**

```bash
cargo test -p axon-web memory --no-fail-fast
cargo test -p axon-cli memory --no-fail-fast
cargo test -p axon-mcp memory --no-fail-fast
cargo clippy -p axon-web -p axon-cli -p axon-mcp --all-targets
```

Expected: PASS.

- [ ] **Step 3: Update the source plan doc**

Mark Phase 3B Task 9 done in `2026-07-04-phase-3b-security-error-memory-completion.md`, noting the old-route deprecation-not-removal decision and the filed follow-up.

- [ ] **Step 4: Commit**

```bash
git add docs/pipeline-unification/plans
git commit -m "docs(pipeline): close out phase 3b task 9 REST memory surface"
```

## Self-Review

- Spec coverage: Phase 3B Task 9 → Tasks 1-3 here.
- Engineering review findings applied: auth gating required on every new route (matching `sources.rs`'s established pattern, not the current handler's apparent gap); explicit size limits on import/export (missing entirely from the originating plan); old route deprecated with a migration window instead of deleted outright in the same change (avoiding an undocumented breaking change to existing external clients).
- Placeholder scan: Task 1 is an explicit read-first step to confirm the auth-gap claim before building around it, and to pin the real `MemoryService` signatures before writing handler code against them.
