# Backend Internal Unification Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove backend-specific branching and `jobs::*` orchestration from service entrypoints so lite/full differences live behind service-native runtimes and stores.

**Architecture:** Replace the temporary `JobBackend` bridge in `ServiceContext` with service-owned runtime/store abstractions. Unify `jobs`, `watch`, `refresh schedule`, and `graph` behind backend-aware service internals so `crates/services` owns one orchestration path while `crates/jobs` becomes implementation detail. Keep the refactor incremental, TDD-first, and compatibility-safe at the service contract boundary.

**Tech Stack:** Rust, Tokio, sqlx, existing Axon `services` and `jobs` modules, SQLite lite store, Postgres full store

---

## File Structure

### Signature Rule

Any service entrypoint that needs resolved backend/runtime/store dependencies must take `&ServiceContext`, not just `&Config`.

Pure helpers may continue to take `&Config` when they only need stateless configuration-derived behavior, for example:
- URL normalization and manifest-path resolution
- formatting-free validation helpers
- pure capability-free transformations

Runtime-backed entrypoints must be migrated together with their callers. Do not hide runtime resolution behind fresh ad hoc constructors inside service modules.

### Existing files to refactor

- `lib.rs`
  - Builds runtime dependencies and currently constructs `ServiceContext` with the temporary `job_backend` bridge.
- `crates/services/context.rs`
  - Current capability and dependency container. Needs to stop exposing `JobBackend`.
- `crates/services/jobs.rs`
  - Current service façade for list/status/cancel/cleanup/clear/recover/worker. Still branches on `cfg.lite_mode` and calls `jobs::*` directly.
- `crates/services/crawl.rs`
  - Current start semantics for crawl. Still depends on the temporary backend bridge for lite-mode in-process completion.
- `crates/services/embed.rs`
  - Same problem as crawl: lite/full split and direct backend dependency.
- `crates/services/extract.rs`
  - Same problem as crawl/embed.
- `crates/services/ingest.rs`
  - Same problem as crawl/embed/extract.
- `crates/services/watch.rs`
  - Consumer boundary is fixed, but internals still directly branch between `watch` and `watch_lite`.
- `crates/services/refresh.rs`
  - Core refresh service; should stay focused on refresh jobs/URL resolution.
- `crates/services/refresh_schedule.rs`
  - New schedule orchestration layer; still directly uses `jobs::refresh` storage functions and full-mode assumptions.
- `crates/services/graph.rs`
  - Service entrypoint still opens pools and calls graph job functions directly.

### New files to add

- `crates/services/runtime.rs`
  - Service-native runtime traits and resolved backend container for generic job operations.
- `crates/services/runtime/full.rs`
  - Full-mode implementation of the service-native runtime.
- `crates/services/runtime/lite.rs`
  - Lite-mode implementation of the service-native runtime.
- `crates/services/watch_store.rs`
  - Shared watch-store trait plus resolver.
- `crates/services/watch_store/full.rs`
  - Full-mode watch persistence implementation.
- `crates/services/watch_store/lite.rs`
  - Lite-mode watch persistence implementation.
- `crates/services/refresh_schedule_store.rs`
  - Schedule-store trait plus resolver. Can start with full-only implementation if lite remains unsupported.
- `crates/services/graph_runtime.rs`
  - Graph runtime abstraction and resolved implementation boundary.

### Tests to add or expand

- `crates/services/context.rs`
  - Extend service-context tests to prove `JobBackend` is gone and service-native runtime exists.
- `crates/services/jobs.rs`
  - Add runtime-backed tests for list/status/cancel/cleanup/clear/recover/worker.
- `crates/services/crawl.rs`
  - Update start tests to use service-native runtime rather than backend trait doubles.
- `crates/services/embed.rs`
  - Same as crawl.
- `crates/services/extract.rs`
  - Same as crawl.
- `crates/services/ingest.rs`
  - Same as crawl.
- `crates/services/watch.rs`
  - Add store-backed tests for list/create/run/finish without direct lite/full branching in the service entrypoint.
- `crates/services/refresh_schedule.rs`
  - Add store/runtime tests proving due-sweep and scheduler logic depend on store/runtime traits, not raw job helpers.
- `crates/services/graph.rs`
  - Add tests proving capability policy and runtime dispatch are service-owned.

---

### Task 1: Replace `JobBackend` With A Service-Native Runtime

**Files:**
- Create: `crates/services/runtime.rs`
- Create: `crates/services/runtime/full.rs`
- Create: `crates/services/runtime/lite.rs`
- Modify: `crates/services.rs`
- Modify: `crates/services/context.rs`
- Modify: `lib.rs`
- Test: `crates/services/context.rs`

- [ ] **Step 1: Write the failing test**

Add tests in `crates/services/context.rs` that assert:
- `ServiceContext` exposes a service-native runtime dependency instead of `job_backend`
- full mode resolves a full runtime
- lite mode resolves a lite runtime

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --locked service_context --lib`
Expected: FAIL because `ServiceContext` still exposes `job_backend`

- [ ] **Step 3: Write minimal implementation**

Add:
- `ServiceJobRuntime` trait in `crates/services/runtime.rs`
- `FullServiceRuntime` in `crates/services/runtime/full.rs`
- `LiteServiceRuntime` in `crates/services/runtime/lite.rs`

Change `ServiceContext` to hold:
- `jobs: Arc<dyn ServiceJobRuntime>`
- existing capabilities

Update `lib.rs` to resolve the runtime and stop calling `with_job_backend(...)`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --locked service_context --lib`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add lib.rs crates/services.rs crates/services/context.rs crates/services/runtime.rs crates/services/runtime/full.rs crates/services/runtime/lite.rs
git commit -m "refactor: replace job backend with service runtime"
```

### Task 2: Refactor `services/jobs.rs` To Use The Runtime

**Files:**
- Modify: `crates/services/jobs.rs`
- Modify: `crates/services/types/service.rs`
- Modify: `crates/services/context.rs`
- Modify: `crates/cli/commands/common_jobs.rs`
- Modify: `crates/cli/commands/crawl/subcommands.rs`
- Modify: `crates/cli/commands/embed.rs`
- Modify: `crates/cli/commands/extract.rs`
- Modify: `crates/cli/commands/ingest_common.rs`
- Modify: `crates/cli/commands/refresh.rs`
- Modify: `crates/mcp/server/*` callers of service job operations, if any
- Test: `crates/services/jobs.rs`

- [ ] **Step 1: Write the failing test**

Add tests in `crates/services/jobs.rs` that assert:
- `list_jobs`
- `job_status`
- `cancel_job`
- `cleanup_jobs`
- `clear_jobs`
- `recover_jobs`
- `run_worker`

all operate through a runtime double rather than branching on `cfg.lite_mode`.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --locked services::jobs --lib`
Expected: FAIL because `services/jobs.rs` still branches on mode and calls `jobs::*` directly

- [ ] **Step 3: Write minimal implementation**

Refactor `crates/services/jobs.rs` so:
- service entrypoints call the resolved runtime from `ServiceContext`
- job-kind dispatch is owned by the runtime implementation
- `cfg.lite_mode` disappears from service orchestration paths in this file

Update all callers of runtime-backed job services to pass `&ServiceContext` rather than only `&Config`.

Keep service-facing `ServiceJob` unchanged unless tests require a translator cleanup.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --locked services::jobs --lib`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/services/jobs.rs crates/services/types/service.rs crates/services/context.rs crates/cli/commands/common_jobs.rs crates/cli/commands/crawl/subcommands.rs crates/cli/commands/embed.rs crates/cli/commands/extract.rs crates/cli/commands/ingest_common.rs crates/cli/commands/refresh.rs
git commit -m "refactor: route service jobs through unified runtime"
```

### Task 3: Rewire Async Start Services To The Unified Runtime

**Files:**
- Modify: `crates/services/crawl.rs`
- Modify: `crates/services/embed.rs`
- Modify: `crates/services/extract.rs`
- Modify: `crates/services/ingest.rs`
- Modify: `crates/services/types/contracts.rs`
- Modify: corresponding CLI/MCP/web callers if signatures change
- Test: `crates/services/crawl.rs`
- Test: `crates/services/embed.rs`
- Test: `crates/services/extract.rs`
- Test: `crates/services/ingest.rs`

- [ ] **Step 1: Write the failing test**

Add or update tests proving:
- crawl/embed/extract/ingest start paths use the service runtime
- no test doubles implement `jobs::backend::JobBackend`
- lite completion/enqueue behavior still matches the existing contract

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --locked crawl_start embed_start extract_start ingest_start --lib`
Expected: FAIL because these service modules still depend on the backend bridge

- [ ] **Step 3: Write minimal implementation**

For each service module:
- swap `ServiceContext::require_job_backend()` usage to the new runtime
- move any remaining direct enqueue/status polling helpers behind runtime methods
- keep `JobStartOutcome` and command-facing behavior unchanged

If a runtime-backed start function currently accepts `&Config`, migrate it to `&ServiceContext` and update all transport callers in the same task.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --locked crawl_start embed_start extract_start ingest_start --lib`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/services/crawl.rs crates/services/embed.rs crates/services/extract.rs crates/services/ingest.rs crates/services/types/contracts.rs
git commit -m "refactor: move start services onto unified runtime"
```

### Task 4: Introduce A Unified Watch Store

**Files:**
- Create: `crates/services/watch_store.rs`
- Create: `crates/services/watch_store/full.rs`
- Create: `crates/services/watch_store/lite.rs`
- Modify: `crates/services.rs`
- Modify: `crates/services/context.rs`
- Modify: `crates/services/watch.rs`
- Modify: `crates/cli/commands/watch.rs`
- Modify: `crates/cli/commands/refresh/schedule.rs`
- Modify: any MCP/web callers of watch services
- Test: `crates/services/watch.rs`

- [ ] **Step 1: Write the failing test**

Add tests for `crates/services/watch.rs` asserting:
- `list_watch_defs`
- `create_watch_def`
- `list_watch_runs`
- `create_watch_run`
- `get_watch_def`
- `finish_watch_run`
- `run_watch_now`

all go through a resolved watch store/runtime double, not direct lite/full branches.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --locked services::watch --lib`
Expected: FAIL because `services/watch.rs` still branches between `watch` and `watch_lite`

- [ ] **Step 3: Write minimal implementation**

Create `WatchStore` with full/lite implementations and update `ServiceContext` to resolve it.

Refactor `services/watch.rs` so:
- no `cfg.lite_mode` checks remain in entrypoints
- no direct `make_pool()` calls remain
- `run_watch_now` uses refresh/other services only through service-owned dependencies

If watch service entrypoints become context-backed, update CLI/MCP/web callers in this task rather than deferring them.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --locked services::watch --lib`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/services.rs crates/services/context.rs crates/services/watch.rs crates/services/watch_store.rs crates/services/watch_store/full.rs crates/services/watch_store/lite.rs crates/cli/commands/watch.rs crates/cli/commands/refresh/schedule.rs
git commit -m "refactor: unify watch service storage"
```

### Task 5: Introduce A Refresh Schedule Store

**Files:**
- Create: `crates/services/refresh_schedule_store.rs`
- Modify: `crates/services.rs`
- Modify: `crates/services/context.rs`
- Modify: `crates/services/refresh_schedule.rs`
- Modify: `crates/services/refresh.rs`
- Modify: `crates/cli/commands/refresh.rs`
- Modify: `crates/cli/commands/refresh/schedule.rs`
- Modify: any MCP/web callers of refresh schedule services
- Test: `crates/services/refresh_schedule.rs`

- [ ] **Step 1: Write the failing test**

Add tests asserting:
- `refresh_schedule_list/create/delete/enable/disable`
- `refresh_schedule_run_due`
- `refresh_schedule_worker`

all use a schedule store/runtime abstraction rather than direct `jobs::refresh` storage helpers.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --locked refresh_schedule --lib`
Expected: FAIL because `refresh_schedule.rs` still imports `jobs::refresh` persistence functions directly

- [ ] **Step 3: Write minimal implementation**

Add a `RefreshScheduleStore` abstraction with:
- full implementation backed by the existing Postgres schedule tables
- explicit unsupported resolution for lite mode if lite scheduler remains unsupported

Refactor `refresh_schedule.rs` to depend on that store plus the unified job runtime.

If schedule service entrypoints become context-backed, update all callers in the same task.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --locked refresh_schedule --lib`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/services.rs crates/services/context.rs crates/services/refresh.rs crates/services/refresh_schedule.rs crates/services/refresh_schedule_store.rs crates/cli/commands/refresh.rs crates/cli/commands/refresh/schedule.rs
git commit -m "refactor: unify refresh schedule service storage"
```

### Task 6: Introduce A Graph Runtime Boundary

**Files:**
- Create: `crates/services/graph_runtime.rs`
- Modify: `crates/services.rs`
- Modify: `crates/services/context.rs`
- Modify: `crates/services/graph.rs`
- Modify: `crates/cli/commands/graph.rs`
- Modify: `crates/mcp/server/handlers_graph.rs`
- Modify: any web graph callers
- Test: `crates/services/graph.rs`

- [ ] **Step 1: Write the failing test**

Add tests asserting:
- `graph_build`
- `graph_status`
- `graph_explore`
- `graph_stats`
- `graph_worker`

depend on a graph runtime abstraction instead of directly opening pools and calling `jobs::graph`.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --locked graph_ --lib`
Expected: FAIL because `services/graph.rs` still directly uses `make_pool()` and graph job helpers

- [ ] **Step 3: Write minimal implementation**

Create `GraphRuntime` with:
- full implementation for current graph behavior
- unsupported implementation or capability resolution for lite mode

Move pool/schema/job orchestration behind the runtime.

If graph service entrypoints become context-backed, update all consumers in this task.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --locked graph_ --lib`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/services.rs crates/services/context.rs crates/services/graph.rs crates/services/graph_runtime.rs crates/cli/commands/graph.rs crates/mcp/server/handlers_graph.rs
git commit -m "refactor: isolate graph behind service runtime"
```

### Task 7: Rebuild Capabilities From Resolved Runtimes And Stores

**Files:**
- Modify: `crates/services/context.rs`
- Modify: `crates/services/export.rs`
- Modify: `crates/services/watch.rs`
- Modify: `crates/services/refresh_schedule.rs`
- Modify: `crates/services/graph.rs`
- Test: `crates/services/context.rs`

- [ ] **Step 1: Write the failing test**

Add tests asserting capability flags come from resolved runtime/store support, not directly from `cfg.lite_mode`.

Examples:
- graph unsupported because graph runtime is unsupported
- refresh scheduler unsupported because schedule store is unsupported
- watch scheduler supported only when watch store/runtime supports it

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --locked capability --lib`
Expected: FAIL because capability construction still derives from mode checks

- [ ] **Step 3: Write minimal implementation**

Refactor `ServiceCapabilities::from_config` into runtime-aware resolution, likely during `ServiceContext::new`.

Do not over-expand feature detection; keep it grounded in the actual resolved dependencies.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --locked capability --lib`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/services/context.rs crates/services/export.rs crates/services/watch.rs crates/services/refresh_schedule.rs crates/services/graph.rs
git commit -m "refactor: derive service capabilities from resolved runtimes"
```

### Task 8: Remove Direct `jobs::*` Imports From Service Entry Points

**Files:**
- Modify: `crates/services/jobs.rs`
- Modify: `crates/services/crawl.rs`
- Modify: `crates/services/embed.rs`
- Modify: `crates/services/extract.rs`
- Modify: `crates/services/ingest.rs`
- Modify: `crates/services/watch.rs`
- Modify: `crates/services/refresh_schedule.rs`
- Modify: `crates/services/graph.rs`
- Test: migration guards in `crates/cli/commands/services_migration_tests.rs` or a new `crates/services/runtime_migration_tests.rs`

- [ ] **Step 1: Write the failing test**

Add a source-guard test that fails if service entrypoint modules import raw `crate::crates::jobs::*` implementation modules directly, except in runtime/store implementation files.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --locked migration --lib`
Expected: FAIL because service entrypoint modules still import `jobs::*` directly

- [ ] **Step 3: Write minimal implementation**

Update service entrypoint modules so:
- only runtime/store implementation files talk directly to `jobs::*`
- top-level service files depend on service-native abstractions only

Keep legitimate re-exports or test-only imports narrow.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --locked migration --lib`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/services/jobs.rs crates/services/crawl.rs crates/services/embed.rs crates/services/extract.rs crates/services/ingest.rs crates/services/watch.rs crates/services/refresh_schedule.rs crates/services/graph.rs
git commit -m "refactor: remove direct jobs imports from service entrypoints"
```

### Task 9: Full Verification Sweep

**Files:**
- Modify: any touched files needed for cleanup
- Test: focused service tests plus compile and migration guards

- [ ] **Step 1: Run the focused verification suite**

Run:

```bash
cargo check --locked
cargo test --locked commands_accept_service_context --lib
cargo test --locked run_watch_lists_in_lite_mode --lib
cargo test --locked refresh_schedule_run_due_uses_service_refresh_start --lib
cargo test --locked service_context --lib
cargo test --locked services::jobs --lib
cargo test --locked services::watch --lib
cargo test --locked refresh_schedule --lib
cargo test --locked graph_ --lib
cargo test --locked migration --lib
```

Expected: PASS across all targeted verification

- [ ] **Step 2: Run a source-boundary grep for final confirmation**

Run:

```bash
rg -n "make_pool\\(|cfg\\.lite_mode|jobs::" \
  crates/services \
  crates/cli \
  crates/mcp/server \
  crates/web
```

Expected:
- service entrypoint files show no raw backend-orchestration leaks
- remaining hits are either runtime/store implementation files or tests

- [ ] **Step 3: Update docs if interfaces changed**

If runtime/capability/service-contract docs changed:
- update `docs/superpowers/specs/2026-03-25-service-boundary-design.md`
- update any relevant `crates/services/CLAUDE.md` notes if needed

- [ ] **Step 4: Commit**

```bash
git add crates/services lib.rs docs/superpowers/specs/2026-03-25-service-boundary-design.md
git commit -m "refactor: finish backend internal service unification"
```

---

## Notes For The Implementer

- Do not mix this plan with lite `export` work. That is a separate follow-up after runtime/store unification is complete.
- Preserve the current consumer-facing service contracts where possible. The goal is to move ownership inward, not to churn the transports again.
- Keep `crates/services/refresh.rs` focused on refresh jobs and URL resolution; put schedule-specific storage/runtime concerns in `crates/services/refresh_schedule.rs` and `crates/services/refresh_schedule_store.rs`.
- When a feature remains unsupported in lite mode, encode that by resolving an unsupported runtime/store, not by scattering `cfg.lite_mode` checks through service entrypoints.
