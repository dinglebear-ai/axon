# Spider Migration 02: Replace CLI Robots Backfill With Engine/Spider Pipeline Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Eliminate custom robots/sitemap backfill fetch logic in `crates/cli` and use the existing engine sitemap backfill path as the sole implementation.

**Architecture:** Move all backfill URL fetch+markdown+manifest writes to `crates/crawl/engine/sitemap.rs::append_sitemap_backfill()`. CLI sync crawl only orchestrates and logs results. Keep CLI output fields stable by mapping engine metrics.

**Tech Stack:** Rust, Spider engine (`crates/crawl/engine/sitemap.rs`), Tokio, serde, existing crawl manifest subsystem.

---

### Task 1: Add failing integration tests for sync crawl backfill contract

**Files:**
- Create: `crates/cli/commands/crawl/sync_backfill_migration_tests.rs`
- Modify: `crates/cli/commands/crawl.rs`
- Test: `crates/cli/commands/crawl/sync_backfill_migration_tests.rs`

**Step 1: Write failing tests**

```rust
#[tokio::test]
async fn sync_crawl_uses_engine_backfill_metrics_not_cli_loop() {
    // run sync crawl against fixture site
    // assert summary includes sitemap_* fields expected from engine output
}

#[tokio::test]
async fn sync_crawl_does_not_append_manifest_via_cli_backfill_codepath() {
    // verify no duplicate manifest rows for sitemap URLs
    // verify changed flag semantics match engine path
}
```

**Step 2: Run test to verify failure**

Run: `cargo test sync_backfill_migration_tests -- --nocapture`
Expected: FAIL

**Step 3: Wire test module**

```rust
// in crawl.rs
#[cfg(test)]
mod sync_backfill_migration_tests;
```

**Step 4: Re-run for semantic failure**

Run: `cargo test sync_backfill_migration_tests -- --nocapture`
Expected: FAIL on assertions only.

**Step 5: Commit**

```bash
git add crates/cli/commands/crawl.rs crates/cli/commands/crawl/sync_backfill_migration_tests.rs
git commit -m "test: add sync crawl backfill migration coverage"
```

### Task 2: Replace CLI backfill call with engine API call

**Files:**
- Modify: `crates/cli/commands/crawl/sync_crawl.rs`
- Modify: `crates/crawl/engine.rs`
- Modify: `crates/crawl/engine/sitemap.rs`
- Test: `crates/cli/commands/crawl/sync_backfill_migration_tests.rs`

**Step 1: Expose backfill API from engine root if needed**

```rust
// crates/crawl/engine.rs
pub use sitemap::append_sitemap_backfill;
```

**Step 2: Replace CLI supplement block**

```rust
// sync_crawl.rs
let backfill_stats = crate::crates::crawl::engine::append_sitemap_backfill(
    cfg,
    start_url,
    &cfg.output_dir,
    &merged_seen,
    &mut final_summary,
).await?;
```

**Step 3: Remove dependency on `append_robots_backfill`**

Delete import and callsite in `sync_crawl.rs`.

**Step 4: Run tests**

Run: `cargo test sync_backfill_migration_tests -- --nocapture`
Expected: PASS

**Step 5: Commit**

```bash
git add crates/cli/commands/crawl/sync_crawl.rs crates/crawl/engine.rs crates/crawl/engine/sitemap.rs
git commit -m "refactor: route sync backfill through engine sitemap pipeline"
```

### Task 3: Remove obsolete CLI backfill implementation

**Files:**
- Delete: `crates/cli/commands/crawl/audit/backfill.rs`
- Modify: `crates/cli/commands/crawl/audit.rs`
- Modify: `crates/cli/commands/crawl/sync_crawl.rs`

**Step 1: Delete module and exports**

```rust
// audit.rs remove:
mod backfill;
pub(super) use backfill::append_robots_backfill;
```

**Step 2: Run compile and targeted tests**

Run: `cargo test sync_backfill_migration_tests -- --nocapture`
Expected: PASS

Run: `cargo test crawl::sync_crawl -- --nocapture`
Expected: PASS

**Step 3: Commit**

```bash
git add crates/cli/commands/crawl/audit.rs crates/cli/commands/crawl/sync_crawl.rs
git rm crates/cli/commands/crawl/audit/backfill.rs
git commit -m "chore: remove cli robots backfill loop"
```

### Task 4: Preserve CLI status and metrics contract

**Files:**
- Modify: `crates/cli/commands/crawl/subcommands.rs`
- Modify: `crates/cli/commands/job_contracts.rs`
- Test: existing status/job tests

**Step 1: Verify metric keys consumed by status views**

Confirm these still populate from engine summary json:
- `sitemap_discovered`
- `sitemap_candidates`
- `sitemap_written`

**Step 2: Add/adjust tests if keys changed**

```rust
#[test]
fn crawl_status_contains_sitemap_metrics_after_engine_backfill() {
    // serialize CrawlJob result_json and assert keys exist
}
```

**Step 3: Run tests**

Run: `cargo test job_contracts -- --nocapture`
Expected: PASS

Run: `cargo test status -- --nocapture`
Expected: PASS

**Step 4: Commit**

```bash
git add crates/cli/commands/crawl/subcommands.rs crates/cli/commands/job_contracts.rs
git commit -m "fix: keep crawl status metrics stable after backfill migration"
```

### Task 5: Documentation and final verification

**Files:**
- Modify: `docs/ARCHITECTURE.md`
- Modify: `crates/cli/CLAUDE.md`

**Step 1: Document single backfill path**

```md
Sync crawl backfill now uses `crawl::engine::append_sitemap_backfill` directly.
No CLI-owned fetch/markdown/manifest backfill loop remains.
```

**Step 2: Run full gates**

Run: `cargo fmt --check`
Expected: PASS

Run: `cargo clippy --all-targets --all-features -- -D warnings`
Expected: PASS

Run: `cargo test crawl -- --nocapture`
Expected: PASS

**Step 3: Commit**

```bash
git add docs/ARCHITECTURE.md crates/cli/CLAUDE.md
git commit -m "docs: record engine-only backfill architecture"
```
