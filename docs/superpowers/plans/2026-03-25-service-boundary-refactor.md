# Service Boundary Refactor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Enforce a strict service boundary so `cli`, `mcp`, and `web` no longer own backend selection, runtime semantics, or direct store access.

**Architecture:** Introduce a shared `ServiceContext`, typed service errors/capabilities, and typed start/status contracts. Move async start semantics and capability policy into `crates/services`, then sweep consumers so they only parse input and render service results.

**Tech Stack:** Rust, Tokio, sqlx, existing Axon services/jobs modules

---

### Task 1: Add `ServiceContext` And Shared Contract Types

**Files:**
- Create: `crates/services/context.rs`
- Create: `crates/services/types/contracts.rs`
- Modify: `crates/services.rs`
- Modify: `crates/services/types.rs`
- Modify: `lib.rs`
- Test: `crates/services/context.rs` or existing `#[cfg(test)]` service tests

- [ ] **Step 1: Write the failing test**

Add tests that assert:
- a `ServiceContext` can be created in full and lite mode
- capability resolution differs by mode

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --locked service_context --lib`
Expected: FAIL because `ServiceContext` and capability contracts do not exist yet

- [ ] **Step 3: Write minimal implementation**

Add:
- `ServiceContext`
- `ServiceCapabilities`
- typed `ServiceError`
- shared start/status contract types

Keep the first pass minimal: enough structure to compile and support later tasks.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --locked service_context --lib`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add lib.rs crates/services.rs crates/services/context.rs crates/services/types.rs crates/services/types/contracts.rs
git commit -m "refactor: add shared service context and contracts"
```

### Task 2: Remove `JobBackend` From Consumer Dispatch

**Files:**
- Modify: `lib.rs`
- Modify: `crates/cli/commands/mod.rs` and command signatures it exports
- Modify: `crates/cli/commands/crawl.rs`
- Modify: `crates/cli/commands/embed.rs`
- Modify: `crates/cli/commands/extract.rs`
- Modify: `crates/cli/commands/ingest.rs`
- Test: targeted command signature/unit tests already in those modules

- [ ] **Step 1: Write the failing test**

Add or update a small compile-oriented test proving command entrypoints accept `&ServiceContext` instead of `Arc<dyn JobBackend>`.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --locked crawl::tests embed::tests extract::tests ingest::tests --lib`
Expected: FAIL due to signature mismatch and missing context plumbing

- [ ] **Step 3: Write minimal implementation**

Change `lib.rs` to:
- build `ServiceContext`
- stop passing `JobBackend` into command handlers

Update command signatures to accept `&ServiceContext`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --locked crawl::tests embed::tests extract::tests ingest::tests --lib`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add lib.rs crates/cli/commands/crawl.rs crates/cli/commands/embed.rs crates/cli/commands/extract.rs crates/cli/commands/ingest.rs
git commit -m "refactor: route consumers through service context"
```

### Task 3: Move Async Start Semantics Into Services

**Files:**
- Modify: `crates/services/crawl.rs`
- Modify: `crates/services/embed.rs`
- Modify: `crates/services/extract.rs`
- Modify: `crates/services/ingest.rs`
- Modify: `crates/services/jobs.rs`
- Modify: `crates/services/types/service.rs`
- Modify: `crates/cli/commands/crawl.rs`
- Modify: `crates/cli/commands/embed.rs`
- Modify: `crates/cli/commands/extract.rs`
- Modify: `crates/cli/commands/ingest.rs`
- Test: add service-layer tests in the service modules or `crates/services/jobs.rs`

- [ ] **Step 1: Write the failing test**

Add service tests that assert:
- `crawl_start` returns a typed start outcome in lite and full mode
- `embed_start`, `extract_start`, `ingest_start` do the same
- CLI no longer calls `wait_for_job()` directly

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --locked crawl_start embed_start extract_start ingest_start --lib`
Expected: FAIL because services still do not own those semantics

- [ ] **Step 3: Write minimal implementation**

For each command family:
- introduce typed start outcome in services
- move lite/full enqueue-or-complete policy into services
- remove direct `backend.enqueue(...)` and `wait_for_job(...)` logic from CLI

Do not broaden scope to unrelated command rendering.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --locked crawl_start embed_start extract_start ingest_start --lib`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/services/crawl.rs crates/services/embed.rs crates/services/extract.rs crates/services/ingest.rs crates/services/jobs.rs crates/services/types/service.rs crates/cli/commands/crawl.rs crates/cli/commands/embed.rs crates/cli/commands/extract.rs crates/cli/commands/ingest.rs
git commit -m "refactor: move async job start semantics into services"
```

### Task 4: Add Typed Status And Progress Snapshots

**Files:**
- Modify: `crates/services/jobs.rs`
- Modify: `crates/services/types/service.rs`
- Modify: `crates/jobs/lite/query.rs`
- Modify: `crates/services/crawl.rs`
- Modify: `crates/services/embed.rs`
- Modify: `crates/services/extract.rs`
- Modify: `crates/services/ingest.rs`
- Modify: `crates/services/refresh.rs`
- Test: service-layer status tests

- [ ] **Step 1: Write the failing test**

Add tests that assert:
- job status returns shared snapshot fields
- progress is represented in typed shape rather than transport-specific parsing

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --locked job_status --lib`
Expected: FAIL because shared status/progress snapshots are incomplete

- [ ] **Step 3: Write minimal implementation**

Map backend/full/lite job rows into:
- common state
- progress fields
- domain-specific details

Keep missing progress fields as `None` instead of inventing values.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --locked job_status --lib`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/services/jobs.rs crates/services/types/service.rs crates/jobs/lite/query.rs crates/services/crawl.rs crates/services/embed.rs crates/services/extract.rs crates/services/ingest.rs crates/services/refresh.rs
git commit -m "refactor: add typed service status and progress snapshots"
```

### Task 5: Centralize Capability Policy In Services

**Files:**
- Modify: `crates/services/export.rs`
- Modify: `crates/services/watch.rs`
- Modify: `crates/services/refresh.rs`
- Modify: `crates/services/graph.rs`
- Modify: `crates/services/context.rs`
- Test: service capability tests

- [ ] **Step 1: Write the failing test**

Add tests that assert:
- `export`, `graph`, `refresh schedule`, and watch scheduler capability decisions come from services
- unsupported mode returns typed `ServiceError`

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --locked capability --lib`
Expected: FAIL because these policy checks are still split across layers

- [ ] **Step 3: Write minimal implementation**

Move policy decisions into services and return typed unsupported errors.

For `watch`, remove direct `make_pool()` calls where possible and route through service-owned backend/repository helpers.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --locked capability --lib`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/services/export.rs crates/services/watch.rs crates/services/refresh.rs crates/services/graph.rs crates/services/context.rs
git commit -m "refactor: centralize service capability policy"
```

### Task 6: Unify `export` Behind Shared Services

**Files:**
- Modify: `crates/services/export.rs`
- Modify: `crates/cli/commands/export.rs`
- Modify: `crates/mcp/server/handlers_system.rs`
- Modify: `crates/services/types/export.rs`
- Test: `crates/cli/commands/export.rs` tests and service export tests

- [ ] **Step 1: Write the failing test**

Add tests that assert:
- CLI `export` no longer rejects lite mode in the command handler
- MCP `handle_export` no longer implements its own lite-mode policy
- both call the same service entrypoint

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --locked run_export handle_export --lib`
Expected: FAIL because both consumers still own policy and direct DB work

- [ ] **Step 3: Write minimal implementation**

Create a backend-aware export service entrypoint and make:
- CLI call it
- MCP call it

Keep the first lite export version sparse-but-valid where data is unavailable.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --locked run_export handle_export --lib`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/services/export.rs crates/cli/commands/export.rs crates/mcp/server/handlers_system.rs crates/services/types/export.rs
git commit -m "refactor: unify export behind shared services"
```

### Task 7: Sweep CLI For Remaining Backend Leaks

**Files:**
- Modify: `crates/cli/commands/refresh.rs`
- Modify: `crates/cli/commands/crawl/subcommands.rs`
- Modify: `crates/cli/commands/embed.rs`
- Modify: `crates/cli/commands/extract.rs`
- Modify: `crates/cli/commands/ingest_common.rs`
- Modify: `crates/cli/commands/watch.rs`
- Test: targeted command tests

- [ ] **Step 1: Write the failing test**

Add tests that assert CLI no longer:
- branches on `cfg.lite_mode` for capability policy
- decides worker ownership semantics

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --locked refresh:: crawl::subcommands:: embed:: extract:: ingest_common:: watch:: --lib`
Expected: FAIL because CLI still owns some of those branches

- [ ] **Step 3: Write minimal implementation**

Replace remaining consumer policy with service calls and typed error/result mapping.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --locked refresh:: crawl::subcommands:: embed:: extract:: ingest_common:: watch:: --lib`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/cli/commands/refresh.rs crates/cli/commands/crawl/subcommands.rs crates/cli/commands/embed.rs crates/cli/commands/extract.rs crates/cli/commands/ingest_common.rs crates/cli/commands/watch.rs
git commit -m "refactor: remove remaining cli backend policy leaks"
```

### Task 8: Sweep MCP And Web Consumers

**Files:**
- Modify: `crates/mcp/server/handlers_system.rs`
- Modify: `crates/mcp/server/handlers_graph.rs`
- Modify: `crates/mcp/server.rs`
- Modify: `crates/web.rs`
- Test: MCP handler tests and any web route/unit tests available

- [ ] **Step 1: Write the failing test**

Add tests that assert MCP/web consume services only for the touched flows.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --locked mcp:: web:: --lib`
Expected: FAIL where direct policy or direct implementation calls remain

- [ ] **Step 3: Write minimal implementation**

Update MCP and web transport code to:
- call services
- map typed service results/errors
- stop owning capability policy

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --locked mcp:: web:: --lib`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/mcp/server/handlers_system.rs crates/mcp/server/handlers_graph.rs crates/mcp/server.rs crates/web.rs
git commit -m "refactor: align mcp and web to strict service boundary"
```

### Task 9: Add Guardrails And Documentation

**Files:**
- Modify: `docs/ACP.md`
- Modify: `CLAUDE.md`
- Modify: any relevant service docs under `crates/services/`
- Create or modify: tests guarding consumer purity if practical

- [ ] **Step 1: Write the failing test**

If practical, add a small guardrail test or lint-like check for forbidden direct calls from consumers.

- [ ] **Step 2: Run test to verify it fails**

Run: targeted guardrail test command
Expected: FAIL before guardrail is implemented

- [ ] **Step 3: Write minimal implementation**

Document:
- strict boundary rule
- `ServiceContext`
- consumer purity expectations

Add the smallest practical automated guardrail.

- [ ] **Step 4: Run test to verify it passes**

Run: guardrail test command plus relevant doc-adjacent tests if any
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add docs/ACP.md CLAUDE.md crates/services
git commit -m "docs: codify strict service boundary rules"
```

### Task 10: Final Verification

**Files:**
- No new files required

- [ ] **Step 1: Run focused library tests**

Run:
```bash
cargo test --locked service_context --lib
cargo test --locked crawl_start --lib
cargo test --locked job_status --lib
cargo test --locked capability --lib
```

Expected: PASS

- [ ] **Step 2: Run targeted command tests**

Run:
```bash
cargo test --locked crawl::tests embed::tests extract::tests ingest::tests refresh:: watch:: --lib
```

Expected: PASS

- [ ] **Step 3: Build the binary**

Run:
```bash
cargo build --locked --bin axon
```

Expected: PASS

- [ ] **Step 4: Run a smoke check**

Run:
```bash
AXON_LITE=1 ./target/debug/axon crawl list
./target/debug/axon export --json
```

Expected:
- commands return via shared services
- no consumer-owned lite/full branching regressions

- [ ] **Step 5: Commit final integration work**

```bash
git add -A
git commit -m "refactor: enforce strict service boundary across consumers"
```
