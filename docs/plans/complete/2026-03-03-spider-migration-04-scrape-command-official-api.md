# Spider Migration 04: Move Scrape Command Back To Official Spider Scrape API Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Remove manual `subscribe()+crawl_raw()/crawl()` scrape flow and use Spider official scrape API path for single-page scraping.

**Architecture:** Introduce a thin adapter that uses official Spider scrape entrypoints for HTTP/Chrome. Keep SSRF validation, output formatting, and embed behavior in CLI. If current Spider release still has `scrape_raw()` race, patch at engine level or upstream and pin updated Spider version before removing workaround.

**Tech Stack:** Rust, Spider `Website` APIs, existing `core/content` transform functions, Tokio.

---

### Task 1: Characterize current scrape behavior with failing tests

**Files:**
- Create: `crates/cli/commands/scrape_migration_tests.rs`
- Modify: `crates/cli/commands/scrape.rs`
- Test: `crates/cli/commands/scrape_migration_tests.rs`

**Step 1: Write failing tests**

```rust
#[tokio::test]
async fn scrape_payload_returns_markdown_title_description_status() {
    // verify contract fields for successful URL
}

#[tokio::test]
async fn scrape_payload_surfaces_non_2xx_as_error() {
    // fixture returns 404; expect explicit error
}

#[tokio::test]
async fn scrape_respects_custom_headers() {
    // fixture requires X-Test-Header; verify request includes it
}
```

**Step 2: Run tests to confirm fail**

Run: `cargo test scrape_migration_tests -- --nocapture`
Expected: FAIL

**Step 3: Wire tests module**

```rust
#[cfg(test)]
mod scrape_migration_tests;
```

**Step 4: Re-run for semantic failure**

Run: `cargo test scrape_migration_tests -- --nocapture`
Expected: FAIL on assertions.

**Step 5: Commit**

```bash
git add crates/cli/commands/scrape.rs crates/cli/commands/scrape_migration_tests.rs
git commit -m "test: add scrape migration contract coverage"
```

### Task 2: Add official Spider scrape adapter (temporary dual-path)

**Files:**
- Modify: `crates/cli/commands/scrape.rs`
- Modify: `Cargo.toml` (if Spider bump needed)
- Test: `crates/cli/commands/scrape_migration_tests.rs`

**Step 1: Implement adapter function**

```rust
async fn scrape_page_official(cfg: &Config, url: &str) -> Result<Page, Box<dyn Error>> {
    let mut website = build_scrape_website(cfg, url)?;
    // prefer official API (example; exact method based on Spider API surface)
    let page = if matches!(cfg.render_mode, RenderMode::Chrome) {
        website.scrape().await
    } else {
        website.scrape_raw().await
    };
    page.ok_or("spider returned no page for this URL".into())
}
```

**Step 2: Keep manual path behind feature flag for rollback**

```rust
if cfg.experimental_scrape_official {
    scrape_page_official(...).await
} else {
    scrape_page_legacy_subscribe(...).await
}
```

**Step 3: Run tests**

Run: `cargo test scrape_migration_tests -- --nocapture`
Expected: PASS (legacy + official path parity where enabled)

**Step 4: Commit**

```bash
git add crates/cli/commands/scrape.rs Cargo.toml
git commit -m "refactor: add official spider scrape adapter with guarded rollout"
```

### Task 3: Flip default to official path and remove legacy subscribe flow

**Files:**
- Modify: `crates/cli/commands/scrape.rs`
- Modify: `crates/core/config/types.rs` (if temporary flag introduced)
- Modify: `crates/core/config/parse.rs` (if temporary flag introduced)
- Test: `crates/cli/commands/scrape_migration_tests.rs`

**Step 1: Make official path default**

Remove legacy branch from:
- `scrape_payload`
- `scrape_one`

**Step 2: Delete deprecated workaround comments and helper code**

Delete references to:
- biased-select race
- explicit subscribe collector path

**Step 3: Run tests**

Run: `cargo test scrape -- --nocapture`
Expected: PASS

Run: `cargo test scrape_migration_tests -- --nocapture`
Expected: PASS

**Step 4: Commit**

```bash
git add crates/cli/commands/scrape.rs crates/core/config/types.rs crates/core/config/parse.rs
git commit -m "chore: remove legacy scrape subscribe workaround"
```

### Task 4: Remove duplicate title/description extraction paths

**Files:**
- Modify: `crates/cli/commands/scrape.rs`
- Modify: `crates/core/content.rs` (if helper extraction should be centralized)

**Step 1: Centralize output shaping once**

```rust
fn build_scrape_response(url: &str, page: &Page) -> ScrapeResponse {
    // one location for title/description/markdown/status extraction
}
```

**Step 2: Use helper in both `scrape_payload` and CLI print path**

This prevents drift in JSON and stdout output generation.

**Step 3: Run tests**

Run: `cargo test select_output -- --nocapture`
Expected: PASS

**Step 4: Commit**

```bash
git add crates/cli/commands/scrape.rs crates/core/content.rs
git commit -m "refactor: unify scrape response shaping"
```

### Task 5: Final verification and docs

**Files:**
- Modify: `README.md`
- Modify: `crates/cli/CLAUDE.md`

**Step 1: Update docs**

```md
Scrape command now uses official Spider scrape APIs.
Manual subscribe-based scrape workaround removed.
```

**Step 2: Run final gates**

Run: `cargo fmt --check`
Expected: PASS

Run: `cargo clippy --all-targets --all-features -- -D warnings`
Expected: PASS

Run: `cargo test scrape -- --nocapture`
Expected: PASS

**Step 3: Commit**

```bash
git add README.md crates/cli/CLAUDE.md
git commit -m "docs: record official spider scrape migration"
```
