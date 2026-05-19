# Services Layer Completion Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Complete the services-layer migration by removing two orphaned `run_*_native()` dead-code exports, then optionally creating a thin `watch` service module so the CLI watch command is consistent with every other command.

**Architecture:** The migration is functionally complete — all 21 web sync modes, all MCP handlers, and all CLI commands already route through `crates::services::*`. The only remaining work is dead code cleanup (`run_evaluate_native` / `run_suggest_native` still exported but never called) and bringing the `watch` CLI command into alignment with the service-layer pattern.

**Tech Stack:** Rust, Tokio, `crates::services::*`, `crates::jobs::watch`

---

## Current State (Verified)

| Surface | Status |
|---------|--------|
| CLI commands (all 19) | ✅ All use `crates::services::*` |
| MCP handlers (all 50+) | ✅ All use `crates::services::*` |
| Web sync modes (all 21) | ✅ All wired in `dispatch.rs` via `call_*()` in `service_calls.rs` |
| `run_evaluate_native` export | ⚠️ Dead — exported in `crates/vector/ops.rs` but never called |
| `run_suggest_native` export | ⚠️ Dead — exported in `crates/vector/ops.rs` but never called |
| `watch` CLI command | ⚠️ Calls `crates::jobs::watch::*` directly (no service module) |

---

## Chunk 1: Remove Dead Code

### Task 1: Delete Orphaned `run_*_native()` Exports

`run_evaluate_native` and `run_suggest_native` were the old subprocess entry points. Both CLI commands and web sync modes now use `query_svc::evaluate()` / `query_svc::suggest()` via the services layer. The native functions exist in `crates/vector/ops/commands/{evaluate,suggest}.rs` and are re-exported via `crates/vector/ops/commands.rs` → `crates/vector/ops.rs`. They need to be removed from all three locations.

**Files:**
- Modify: `crates/vector/ops/commands/evaluate.rs` (remove function)
- Modify: `crates/vector/ops/commands/suggest.rs` (remove function)
- Modify: `crates/vector/ops/commands.rs` (remove `pub use` lines)
- Modify: `crates/vector/ops.rs` (remove `pub use` line)

- [ ] **Step 1: Confirm nothing calls these functions**

```bash
grep -rn "run_evaluate_native\|run_suggest_native" \
  crates/ lib.rs main.rs \
  --include="*.rs" \
  | grep -v "commands/evaluate.rs\|commands/suggest.rs\|migration_test"
```

Expected: zero lines output. If anything shows up, stop and investigate before proceeding.

- [ ] **Step 2: Confirm what IS still needed from evaluate.rs and suggest.rs**

```bash
grep -rn "evaluate_payload\|discover_crawl_suggestions" crates/ --include="*.rs"
```

Expected output includes:
- `crates/services/query.rs` uses `evaluate_payload` and `discover_crawl_suggestions`
- Do NOT remove these — only remove the `run_*_native` functions.

- [ ] **Step 3: Remove `run_evaluate_native` from evaluate.rs**

In `crates/vector/ops/commands/evaluate.rs`, find and delete the entire `pub async fn run_evaluate_native(...)` function body. Keep `pub async fn evaluate_payload(...)` intact.

- [ ] **Step 4: Remove `run_suggest_native` from suggest.rs**

In `crates/vector/ops/commands/suggest.rs`, find and delete the entire `pub async fn run_suggest_native(...)` function body. Keep `pub async fn discover_crawl_suggestions(...)` intact.

- [ ] **Step 5: Remove pub use in commands.rs**

In `crates/vector/ops/commands.rs`, remove these two lines:

```rust
pub use evaluate::run_evaluate_native;
pub use suggest::run_suggest_native;
```

Keep:
```rust
pub use evaluate::evaluate_payload;
pub use suggest::discover_crawl_suggestions;
```

- [ ] **Step 6: Remove pub use in ops.rs**

In `crates/vector/ops.rs`, find line 10 (or wherever it is):

```rust
pub use commands::{run_evaluate_native, run_suggest_native};
```

Either remove the entire line (if these were the only two exports from `commands`) or trim it to only the remaining exports. Check what else is exported from `commands`:

```bash
grep "pub use commands" crates/vector/ops.rs
grep "pub fn\|pub use" crates/vector/ops/commands.rs
```

Update accordingly to keep `evaluate_payload` and `discover_crawl_suggestions` accessible.

- [ ] **Step 7: cargo check**

```bash
cargo check 2>&1 | grep "^error"
```

Expected: no errors. If migration tests reference the removed functions, delete those assertions too.

- [ ] **Step 8: Run tests**

```bash
cargo test --lib 2>&1 | tail -10
```

Expected: all tests pass.

- [ ] **Step 9: Run full quality gate**

```bash
just verify
```

Expected: fmt-check, clippy, check, and tests all green.

- [ ] **Step 10: Commit**

```bash
git add crates/vector/ops/commands/evaluate.rs \
        crates/vector/ops/commands/suggest.rs \
        crates/vector/ops/commands.rs \
        crates/vector/ops.rs
git commit -m "chore: remove orphaned run_evaluate_native and run_suggest_native dead code"
```

---

## Chunk 2: Watch Service Module

This chunk is lower priority than Chunk 1. The watch CLI command works correctly today — this is a consistency improvement to bring `watch` into alignment with every other CLI command.

### Context

The `watch` CLI command (`crates/cli/commands/watch.rs`) calls `crates::jobs::watch::*` directly. The relevant job functions (from `crates/jobs/watch.rs`) are:

```rust
pub async fn create_watch_def(cfg: &Config, input: &WatchDefCreate) -> Result<WatchDef, Box<dyn Error>>
pub async fn list_watch_defs(cfg: &Config, limit: i64) -> Result<Vec<WatchDef>, Box<dyn Error>>
pub async fn list_watch_runs(cfg: &Config, watch_id: Uuid, limit: i64) -> Result<Vec<WatchRun>, Box<dyn Error>>
```

The `run-now` subcommand already calls `refresh_service::refresh_start()` — it's already on services for dispatch.

### Task 2: Create `crates/services/watch.rs`

**Files:**
- Create: `crates/services/watch.rs`
- Modify: `crates/services.rs` (or wherever `pub mod` declarations live for the services crate)
- Modify: `crates/cli/commands/watch.rs`

- [ ] **Step 1: Find where services modules are declared**

```bash
grep -n "pub mod" crates/services.rs 2>/dev/null \
  || grep -n "pub mod" crates/services/mod.rs 2>/dev/null \
  || find crates/services -name "*.rs" -maxdepth 1 | head -20
```

Note the exact file and location where `pub mod query;`, `pub mod system;`, etc. are declared.

- [ ] **Step 2: Read watch.rs CLI to confirm all call sites**

```bash
cat crates/cli/commands/watch.rs
```

Identify every `watch_jobs::*` or `watch::*` import and call site (should be 3: `list_watch_defs`, `create_watch_def`, `list_watch_runs`).

- [ ] **Step 3: Write the failing test**

In a new file `crates/services/watch.rs`, write test stubs first:

```rust
#[cfg(test)]
mod tests {
    // These tests will fail until implementations are added below.
    // Compile-time check that all expected public functions exist with correct signatures.
    use super::*;
    use crate::crates::core::config::Config;

    #[allow(dead_code)]
    fn _assert_signatures(_cfg: &Config) {
        // If these lines compile, the function signatures are correct.
        // This is a type-level check, not a runtime check.
        async fn _f1(cfg: &Config) {
            let _: Result<Vec<crate::crates::jobs::watch::WatchDef>, _> =
                list_watch_defs(cfg, 10).await;
        }
        async fn _f2(cfg: &Config, input: &crate::crates::jobs::watch::WatchDefCreate) {
            let _: Result<crate::crates::jobs::watch::WatchDef, _> =
                create_watch_def(cfg, input).await;
        }
        async fn _f3(cfg: &Config, id: uuid::Uuid) {
            let _: Result<Vec<crate::crates::jobs::watch::WatchRun>, _> =
                list_watch_runs(cfg, id, 10).await;
        }
    }
}
```

- [ ] **Step 4: Run to confirm it fails**

```bash
cargo check 2>&1 | grep "error\[E"
```

Expected: errors like `cannot find function 'list_watch_defs' in module 'super'` (functions not yet defined).

- [ ] **Step 5: Implement `crates/services/watch.rs`**

```rust
use std::error::Error;
use uuid::Uuid;

use crate::crates::core::config::Config;
use crate::crates::jobs::watch::{
    self as watch_jobs, WatchDef, WatchDefCreate, WatchRun,
};

pub async fn list_watch_defs(
    cfg: &Config,
    limit: i64,
) -> Result<Vec<WatchDef>, Box<dyn Error>> {
    watch_jobs::list_watch_defs(cfg, limit).await
}

pub async fn create_watch_def(
    cfg: &Config,
    input: &WatchDefCreate,
) -> Result<WatchDef, Box<dyn Error>> {
    watch_jobs::create_watch_def(cfg, input).await
}

pub async fn list_watch_runs(
    cfg: &Config,
    watch_id: Uuid,
    limit: i64,
) -> Result<Vec<WatchRun>, Box<dyn Error>> {
    watch_jobs::list_watch_runs(cfg, watch_id, limit).await
}
```

**Note:** Adjust the `use` path if `crate::crates::jobs::watch` is not the correct module path. Check with:
```bash
grep -n "pub mod watch" crates/jobs.rs 2>/dev/null || grep -rn "mod watch" crates/jobs/ --include="*.rs"
```

- [ ] **Step 6: Add `pub mod watch;` to the services declaration file**

In the file found in Step 1, add:
```rust
pub mod watch;
```

alongside the other `pub mod` declarations (e.g., after `pub mod refresh;`).

- [ ] **Step 7: cargo check — confirm it compiles**

```bash
cargo check 2>&1 | grep "^error"
```

Expected: no errors.

- [ ] **Step 8: Update `crates/cli/commands/watch.rs` to use services**

Replace:
```rust
use crate::crates::jobs::watch as watch_jobs;
// (or whatever the current import is)
```

With:
```rust
use crate::crates::services::watch as watch_svc;
```

Then update the three call sites:

| Before | After |
|--------|-------|
| `watch_jobs::list_watch_defs(cfg, 200).await` | `watch_svc::list_watch_defs(cfg, 200).await` |
| `watch_jobs::create_watch_def(cfg, &WatchDefCreate { ... }).await` | `watch_svc::create_watch_def(cfg, &WatchDefCreate { ... }).await` |
| `watch_jobs::list_watch_runs(cfg, watch_id, limit).await` | `watch_svc::list_watch_runs(cfg, watch_id, limit).await` |

Leave the `refresh_service::refresh_start()` call unchanged — it already uses services.

- [ ] **Step 9: cargo check + tests**

```bash
cargo check 2>&1 | grep "^error"
cargo test --lib 2>&1 | tail -10
```

- [ ] **Step 10: Run full quality gate**

```bash
just verify
```

Expected: all green.

- [ ] **Step 11: Commit**

```bash
git add crates/services/watch.rs \
        crates/services.rs \
        crates/cli/commands/watch.rs
git commit -m "feat: add watch service module, migrate watch CLI command through services layer"
```

---

## Final Verification

- [ ] **Confirm no remaining `run_*_native` pub exports**

```bash
grep -rn "pub.*run_.*native\|pub use.*native" crates/ --include="*.rs" \
  | grep -v "migration_test\|#\[cfg(test)\]"
```

Expected: zero results.

- [ ] **Confirm watch CLI uses services**

```bash
grep -n "jobs::watch\|watch_jobs" crates/cli/commands/watch.rs
```

Expected: zero results (all calls now go through `watch_svc`).

- [ ] **Final quality gate**

```bash
just verify
```

Expected: all green.

---

## Architecture After Completion

Every user-facing operation flows through `crates::services::*`:

```
CLI (crates/cli/commands/*.rs)          → crates::services::*
Web WS (crates/web/execute/sync_mode/)  → crates::services::*
MCP (crates/mcp/server/handlers_*.rs)  → crates::services::*
                  ↓
crates::jobs::* / crates::vector::ops::* / crates::crawl::*
```

No surface calls `run_*_native()`, raw `qdrant_*()` helpers, or job functions directly from dispatch code.
