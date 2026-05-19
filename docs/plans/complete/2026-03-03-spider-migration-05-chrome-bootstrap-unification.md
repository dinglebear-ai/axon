# Spider Migration 05: Remove CLI Chrome Bootstrap Duplication And Use Engine/Spider Resolution Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Delete duplicate Chrome/CDP bootstrap probe code in `crates/cli/commands/crawl/runtime.rs` and rely on shared engine/Spider resolution paths.

**Architecture:** Keep CDP resolution in one place (`crates/crawl/engine/runtime.rs::resolve_cdp_ws_url`). CLI sync crawl should call that shared path and stop implementing separate reqwest probe/backoff/host rewrite logic.

**Tech Stack:** Rust, Spider runtime configuration, Tokio, reqwest removal from CLI runtime.

---

### Task 1: Add tests covering bootstrap behavior through shared runtime

**Files:**
- Create: `crates/cli/commands/crawl/runtime_migration_tests.rs`
- Modify: `crates/cli/commands/crawl/runtime.rs`
- Test: `crates/cli/commands/crawl/runtime_migration_tests.rs`

**Step 1: Write failing tests**

```rust
#[tokio::test]
async fn bootstrap_returns_resolved_ws_url_from_engine_path() {
    // mock remote /json/version endpoint
    // assert resolved_ws_url is Some(ws://...)
}

#[tokio::test]
async fn bootstrap_returns_warning_when_resolution_fails() {
    // unreachable endpoint -> warnings populated
}

#[test]
fn resolve_initial_mode_autoswitch_starts_http() {
    // existing behavior guard
}
```

**Step 2: Run tests**

Run: `cargo test runtime_migration_tests -- --nocapture`
Expected: FAIL

**Step 3: Wire tests module**

```rust
#[cfg(test)]
mod runtime_migration_tests;
```

**Step 4: Re-run for semantic fail**

Run: `cargo test runtime_migration_tests -- --nocapture`
Expected: FAIL on assertions only.

**Step 5: Commit**

```bash
git add crates/cli/commands/crawl/runtime.rs crates/cli/commands/crawl/runtime_migration_tests.rs
git commit -m "test: add crawl runtime migration coverage"
```

### Task 2: Replace local probe logic with shared engine resolver

**Files:**
- Modify: `crates/cli/commands/crawl/runtime.rs`
- Modify: `crates/crawl/engine/runtime.rs` (if additional public helper required)
- Modify: `crates/crawl/engine.rs`

**Step 1: Export resolver from crawl engine if not already public**

```rust
// crates/crawl/engine.rs
pub(crate) use runtime::resolve_cdp_ws_url;
```

**Step 2: Refactor bootstrap function**

```rust
let resolved = crate::crates::crawl::engine::resolve_cdp_ws_url(remote_url).await;
if let Some(ws_url) = resolved {
    outcome.remote_ready = true;
    outcome.resolved_ws_url = Some(ws_url);
    return outcome;
}
```

**Step 3: Remove local `probe_cdp_connection` calls from flow**

Keep warning behavior and retry semantics only if still required by product behavior.

**Step 4: Run tests**

Run: `cargo test runtime_migration_tests -- --nocapture`
Expected: PASS

**Step 5: Commit**

```bash
git add crates/cli/commands/crawl/runtime.rs crates/crawl/engine/runtime.rs crates/crawl/engine.rs
git commit -m "refactor: unify crawl chrome bootstrap on engine resolver"
```

### Task 3: Delete duplicate host rewrite and manual sleep backoff logic

**Files:**
- Modify: `crates/cli/commands/crawl/runtime.rs`

**Step 1: Remove obsolete code**

Delete:
- `probe_cdp_connection`
- direct `reqwest::Client::builder()` path in CLI runtime bootstrap
- custom `is_docker_service_host` host rewrite branch in CLI runtime module

**Step 2: Ensure imports are clean**

Remove now-unused imports:
- `reqwest`
- `Url`
- `Duration` where not needed

**Step 3: Run compile + tests**

Run: `cargo check`
Expected: PASS

Run: `cargo test crawl::runtime -- --nocapture`
Expected: PASS

**Step 4: Commit**

```bash
git add crates/cli/commands/crawl/runtime.rs
git commit -m "chore: remove duplicate cli cdp probe implementation"
```

### Task 4: Validate sync crawl integration behavior

**Files:**
- Modify: `crates/cli/commands/crawl/sync_crawl.rs` (if output text needs updates)

**Step 1: Add/adjust integration test**

```rust
#[tokio::test]
async fn run_crawl_phase_uses_resolved_ws_url_when_available() {
    // ensure ws_cfg_holder is populated and consumed
}
```

**Step 2: Run tests**

Run: `cargo test run_crawl_phase -- --nocapture`
Expected: PASS

**Step 3: Commit**

```bash
git add crates/cli/commands/crawl/sync_crawl.rs
git commit -m "test: verify sync crawl consumes unified bootstrap resolution"
```

### Task 5: Documentation and final verification

**Files:**
- Modify: `crates/cli/CLAUDE.md`
- Modify: `docs/ARCHITECTURE.md`

**Step 1: Update docs**

```md
CLI crawl runtime no longer owns a CDP probe implementation.
CDP resolution is delegated to crawl engine runtime resolver.
```

**Step 2: Final verification**

Run: `cargo fmt --check`
Expected: PASS

Run: `cargo clippy --all-targets --all-features -- -D warnings`
Expected: PASS

Run: `cargo test crawl -- --nocapture`
Expected: PASS

**Step 3: Commit**

```bash
git add crates/cli/CLAUDE.md docs/ARCHITECTURE.md
git commit -m "docs: record crawl runtime bootstrap unification"
```
