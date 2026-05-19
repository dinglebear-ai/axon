# Spider Migration 06: Remove Manual Map Sitemap Merge And Use Engine-Owned URL Set Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Remove manual sitemap append/sort/dedup logic from `crates/cli/commands/map.rs` and use one engine-owned URL discovery set.

**Architecture:** `map` should consume a single engine response that already includes crawl-discovered + sitemap-discovered URLs with deterministic dedupe. CLI map should format output only.

**Tech Stack:** Rust, crawl engine map/sitemap APIs, serde_json output contract.

---

### Task 1: Create failing tests for map output contract and dedupe semantics

**Files:**
- Create: `crates/cli/commands/map_migration_tests.rs`
- Modify: `crates/cli/commands/map.rs`
- Test: `crates/cli/commands/map_migration_tests.rs`

**Step 1: Write failing tests**

```rust
#[tokio::test]
async fn map_payload_returns_unique_urls_without_cli_side_dedup() {
    // fixture where sitemap duplicates crawler links
    // assert urls are unique and stable order
}

#[tokio::test]
async fn map_payload_reports_sitemap_url_count_consistently() {
    // verify mapped_urls/sitemap_urls/pages_seen fields
}

#[tokio::test]
async fn map_autoswitch_only_falls_back_when_no_pages_seen() {
    // preserve existing AutoSwitch policy
}
```

**Step 2: Run tests**

Run: `cargo test map_migration_tests -- --nocapture`
Expected: FAIL

**Step 3: Wire test module**

```rust
#[cfg(test)]
mod map_migration_tests;
```

**Step 4: Re-run for semantic fail**

Run: `cargo test map_migration_tests -- --nocapture`
Expected: FAIL on assertions.

**Step 5: Commit**

```bash
git add crates/cli/commands/map.rs crates/cli/commands/map_migration_tests.rs
git commit -m "test: add map migration coverage"
```

### Task 2: Add engine API that returns final unified map URL set

**Files:**
- Modify: `crates/crawl/engine.rs`
- Modify: `crates/crawl/engine/sitemap.rs`
- Modify: `crates/cli/commands/map.rs`
- Test: `crates/cli/commands/map_migration_tests.rs`

**Step 1: Create engine helper**

```rust
pub struct MapResult {
    pub summary: CrawlSummary,
    pub urls: Vec<String>,
    pub sitemap_urls: usize,
}

pub async fn map_with_sitemap(cfg: &Config, start_url: &str) -> Result<MapResult, Box<dyn Error>> {
    // run crawl_and_collect_map
    // optionally run crawl_sitemap_urls / append_sitemap_backfill-style discovery
    // dedupe once in engine
    // return stable sorted urls + sitemap_urls count
}
```

**Step 2: Replace CLI merge code with engine call**

```rust
let map_result = crate::crates::crawl::engine::map_with_sitemap(cfg, start_url).await?;
```

**Step 3: Remove CLI-level append/sort/dedup block**

Delete sections in `map_payload` and `run_map` that:
- call `discover_sitemap_urls_with_robots`
- `final_urls.append(...)`
- `final_urls.sort(); final_urls.dedup();`

**Step 4: Run tests**

Run: `cargo test map_migration_tests -- --nocapture`
Expected: PASS

**Step 5: Commit**

```bash
git add crates/crawl/engine.rs crates/crawl/engine/sitemap.rs crates/cli/commands/map.rs
git commit -m "refactor: move map url merge and dedupe into crawl engine"
```

### Task 3: Remove deprecated CLI dependency on audit sitemap helper

**Files:**
- Modify: `crates/cli/commands/map.rs`
- Modify: `crates/cli/commands/crawl.rs` (if export no longer needed)
- Modify: `crates/cli/commands/crawl/audit.rs` (if symbol visibility changes)

**Step 1: Drop import**

```rust
// remove from map.rs
use crate::crates::cli::commands::crawl::discover_sitemap_urls_with_robots;
```

**Step 2: Clean up re-exports**

If no other callers remain, remove:
- `pub(crate) use audit::discover_sitemap_urls_with_robots;` from `crawl.rs`

**Step 3: Run tests**

Run: `cargo test map -- --nocapture`
Expected: PASS

Run: `cargo check`
Expected: PASS

**Step 4: Commit**

```bash
git add crates/cli/commands/map.rs crates/cli/commands/crawl.rs crates/cli/commands/crawl/audit.rs
git commit -m "chore: remove map dependency on cli audit sitemap helper"
```

### Task 4: Validate JSON/output compatibility

**Files:**
- Modify: `crates/cli/commands/map.rs` (only if field mapping changes)
- Modify: `crates/cli/commands/mcp.rs` (if map payload schema is reused there)

**Step 1: Add explicit contract test**

```rust
#[test]
fn map_payload_json_has_expected_fields() {
    // assert url, mapped_urls, sitemap_urls, pages_seen, thin_pages, elapsed_ms, urls
}
```

**Step 2: Run tests**

Run: `cargo test map_payload_json_has_expected_fields -- --nocapture`
Expected: PASS

**Step 3: Commit**

```bash
git add crates/cli/commands/map.rs crates/cli/commands/mcp.rs
git commit -m "test: lock map payload schema after engine unification"
```

### Task 5: Documentation and final verification

**Files:**
- Modify: `docs/ARCHITECTURE.md`
- Modify: `README.md`

**Step 1: Document engine-owned map URL set**

```md
`map` now consumes a unified URL set from crawl engine.
CLI no longer merges/dedupes sitemap URLs itself.
```

**Step 2: Final verification**

Run: `cargo fmt --check`
Expected: PASS

Run: `cargo clippy --all-targets --all-features -- -D warnings`
Expected: PASS

Run: `cargo test map -- --nocapture`
Expected: PASS

**Step 3: Commit**

```bash
git add docs/ARCHITECTURE.md README.md
git commit -m "docs: record map command engine unification"
```
