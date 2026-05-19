# Spider Migration 01: Replace CLI Sitemap Discovery With Engine/Spider Paths Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Remove `crates/cli/commands/crawl/audit/sitemap.rs` custom sitemap crawler and route all sitemap URL discovery through the existing Spider-backed crawl engine APIs.

**Architecture:** Keep one sitemap pipeline in the codebase: engine-owned (`crawl::engine::sitemap`) + Spider-native `crawl_sitemap()` behavior. CLI should call engine APIs, not parse robots/sitemap XML itself. Preserve existing JSON contracts by adapting engine output into current audit structs.

**Tech Stack:** Rust, Spider (`spider`, `spider_agent`), existing `crates/crawl/engine/sitemap.rs`, Tokio, serde.

---

### Task 1: Lock current behavior with failing characterization tests

**Files:**
- Create: `crates/cli/commands/crawl/audit/sitemap_migration_tests.rs`
- Modify: `crates/cli/commands/crawl/audit.rs`
- Test: `crates/cli/commands/crawl/audit/sitemap_migration_tests.rs`

**Step 1: Write the failing tests**

```rust
#[tokio::test]
async fn discover_sitemap_urls_includes_robots_declared_entries() {
    // use a local http mock server fixture
    // robots.txt declares custom sitemap
    // assert returned urls contain entries from that sitemap
}

#[tokio::test]
async fn discover_sitemap_urls_applies_exclude_path_prefix() {
    // sitemap contains /docs/en/* and /docs/ja/*
    // cfg.exclude_path_prefix = vec!["/docs/ja".into()]
    // assert /docs/ja/* excluded
}

#[tokio::test]
async fn discover_sitemap_urls_respects_include_subdomains_false() {
    // sitemap includes docs.example.com + blog.example.com
    // include_subdomains=false should keep only host-scope urls
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test sitemap_migration_tests -- --nocapture`
Expected: FAIL because fixtures/helpers and engine-backed adapter do not exist yet.

**Step 3: Minimal wiring to compile tests**

```rust
// in audit.rs
#[cfg(test)]
mod sitemap_migration_tests;
```

**Step 4: Run test to verify fail state is semantic**

Run: `cargo test sitemap_migration_tests -- --nocapture`
Expected: FAIL on assertions, not compile errors.

**Step 5: Commit**

```bash
git add crates/cli/commands/crawl/audit.rs crates/cli/commands/crawl/audit/sitemap_migration_tests.rs
git commit -m "test: add sitemap migration characterization coverage"
```

### Task 2: Introduce engine-backed discovery adapter in CLI audit layer

**Files:**
- Modify: `crates/cli/commands/crawl/audit.rs`
- Modify: `crates/cli/commands/crawl/audit/sitemap.rs`
- Modify: `crates/crawl/engine.rs`
- Modify: `crates/crawl/engine/sitemap.rs`
- Test: `crates/cli/commands/crawl/audit/sitemap_migration_tests.rs`

**Step 1: Add an engine-facing discovery API**

```rust
// crates/crawl/engine.rs
pub struct EngineSitemapDiscovery {
    pub urls: Vec<String>,
    pub discovered_sitemap_documents: usize,
    pub parsed_sitemap_documents: usize,
    pub discovered_urls: usize,
}

pub async fn discover_sitemap_urls(cfg: &Config, start_url: &str)
    -> Result<EngineSitemapDiscovery, Box<dyn Error>>
{
    let urls = sitemap::crawl_sitemap_urls(cfg, start_url).await?;
    Ok(EngineSitemapDiscovery {
        discovered_urls: urls.len(),
        urls,
        discovered_sitemap_documents: 0,
        parsed_sitemap_documents: 0,
    })
}
```

**Step 2: Replace custom queue/parser call site with engine call**

```rust
// crates/cli/commands/crawl/audit/sitemap.rs
let engine = crate::crates::crawl::engine::discover_sitemap_urls(cfg, start_url).await?;
let discovered_urls = engine.urls;
```

**Step 3: Keep existing response schema via adapter mapping**

```rust
let stats = SitemapDiscoveryStats {
    discovered_sitemap_documents: engine.discovered_sitemap_documents,
    parsed_sitemap_documents: engine.parsed_sitemap_documents,
    discovered_urls: engine.discovered_urls,
    ..Default::default()
};
```

**Step 4: Run tests**

Run: `cargo test sitemap_migration_tests -- --nocapture`
Expected: PASS

**Step 5: Commit**

```bash
git add crates/crawl/engine.rs crates/crawl/engine/sitemap.rs crates/cli/commands/crawl/audit/sitemap.rs crates/cli/commands/crawl/audit.rs
git commit -m "refactor: route cli sitemap discovery through crawl engine"
```

### Task 3: Remove dead hand-rolled parsing and fetch/retry branches

**Files:**
- Modify: `crates/cli/commands/crawl/audit/sitemap.rs`
- Modify: `crates/cli/commands/crawl/audit.rs`
- Test: `crates/cli/commands/crawl/audit/sitemap_migration_tests.rs`

**Step 1: Delete obsolete helpers**

Remove functions from `sitemap.rs`:
- `default_sitemap_queue`
- `enqueue_robots_sitemaps`
- `in_host_scope`
- `in_path_scope`
- `canonical_sitemap_loc`

**Step 2: Delete unused HTTP retry helper if no longer needed**

```rust
// remove from audit.rs if no callsites remain
async fn fetch_text_with_retry(...) -> Option<String> { ... }
```

**Step 3: Ensure compile and behavior parity**

Run: `cargo test sitemap_migration_tests -- --nocapture`
Expected: PASS

Run: `cargo test crawl::audit -- --nocapture`
Expected: PASS

**Step 4: Commit**

```bash
git add crates/cli/commands/crawl/audit.rs crates/cli/commands/crawl/audit/sitemap.rs
git commit -m "chore: delete cli hand-rolled sitemap parsing path"
```

### Task 4: Verify full CLI flows that depended on old discovery

**Files:**
- Modify: `crates/cli/commands/map.rs` (only if field mapping needed)
- Modify: `crates/cli/commands/crawl/audit/manifest_audit.rs` (only if stats fields changed)
- Test: existing module tests + migration tests

**Step 1: Run focused verification**

Run: `cargo test map -- --nocapture`
Expected: PASS

Run: `cargo test manifest_audit -- --nocapture`
Expected: PASS

**Step 2: Run crate-wide sanity**

Run: `cargo test -p axon --lib crawl -- --nocapture`
Expected: PASS

**Step 3: Commit (only if compatibility edits were required)**

```bash
git add crates/cli/commands/map.rs crates/cli/commands/crawl/audit/manifest_audit.rs
git commit -m "fix: preserve audit and map contracts after sitemap migration"
```

### Task 5: Document migration and remove stale comments

**Files:**
- Modify: `crates/cli/CLAUDE.md`
- Modify: `docs/ARCHITECTURE.md`
- Modify: `README.md`

**Step 1: Update docs to state single sitemap path**

```md
CLI audit/map sitemap discovery now delegates to crawl engine sitemap APIs.
No direct robots/sitemap XML parsing remains in crates/cli.
```

**Step 2: Run docs+compile gate**

Run: `cargo check`
Expected: PASS

**Step 3: Commit**

```bash
git add crates/cli/CLAUDE.md docs/ARCHITECTURE.md README.md
git commit -m "docs: record engine-owned sitemap discovery architecture"
```

### Final Verification Checklist

Run exactly:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test sitemap_migration_tests -- --nocapture
cargo test map -- --nocapture
cargo test crawl -- --nocapture
```

Expected:
- All commands exit 0
- No references remain to removed hand-rolled sitemap helpers
- `discover_sitemap_urls_with_robots` remains as compatibility wrapper only (or renamed to engine-backed equivalent)
