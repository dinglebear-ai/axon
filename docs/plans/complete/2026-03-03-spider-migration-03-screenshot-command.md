# Spider Migration 03: Replace Custom Screenshot CDP Client With Spider Screenshot API Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Remove `crates/cli/commands/screenshot/cdp.rs` custom raw-CDP client and implement screenshot capture through Spider official screenshot support.

**Architecture:** Build screenshot via `Website` + Spider chrome features (`with_screenshot(ScreenShotConfig)` and crawl) rather than manually issuing `Target.createTarget` / `Page.captureScreenshot`. Keep command UX and output contracts (`--json`, saved path) unchanged.

**Tech Stack:** Rust, Spider chrome screenshot feature, Tokio, existing CLI config and output formatting.

---

### Task 1: Add failing behavior tests for screenshot command contract

**Files:**
- Create: `crates/cli/commands/screenshot/screenshot_migration_tests.rs`
- Modify: `crates/cli/commands/screenshot/mod.rs`
- Test: `crates/cli/commands/screenshot/screenshot_migration_tests.rs`

**Step 1: Write failing tests**

```rust
#[tokio::test]
async fn screenshot_writes_png_and_reports_size() {
    // run screenshot_one against local fixture page
    // assert file exists and starts with PNG magic bytes
}

#[tokio::test]
async fn screenshot_json_contract_is_stable() {
    // assert JSON has url/path/size_bytes keys
}

#[tokio::test]
async fn screenshot_requires_chrome_remote_url() {
    // cfg without chrome_remote_url => explicit error
}
```

**Step 2: Run tests and confirm fail**

Run: `cargo test screenshot_migration_tests -- --nocapture`
Expected: FAIL

**Step 3: Wire tests into module**

```rust
// mod.rs
#[cfg(test)]
mod screenshot_migration_tests;
```

**Step 4: Re-run for semantic fail**

Run: `cargo test screenshot_migration_tests -- --nocapture`
Expected: FAIL on behavior assertions.

**Step 5: Commit**

```bash
git add crates/cli/commands/screenshot/mod.rs crates/cli/commands/screenshot/screenshot_migration_tests.rs
git commit -m "test: add screenshot migration behavior coverage"
```

### Task 2: Implement Spider-based screenshot capture helper

**Files:**
- Create: `crates/cli/commands/screenshot/spider_capture.rs`
- Modify: `crates/cli/commands/screenshot/mod.rs`
- Modify: `crates/crawl/engine/runtime.rs` (if shared screenshot config helper is needed)
- Test: `crates/cli/commands/screenshot/screenshot_migration_tests.rs`

**Step 1: Add helper that uses `Website` and Spider screenshot config**

```rust
pub async fn spider_screenshot(
    cfg: &Config,
    url: &str,
) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut website = Website::new(url);
    website.with_chrome_connection(cfg.chrome_remote_url.clone());
    website.with_screenshot(Some(ScreenShotConfig::new(
        cfg.viewport_width,
        cfg.viewport_height,
        cfg.screenshot_full_page,
        false,
        true,
        Some(ScreenshotParams::png()),
    )));
    website.crawl().await;
    let page = website.get_pages().into_iter().next().ok_or("no page")?;
    page.screenshot_bytes.ok_or("missing screenshot bytes".into())
}
```

**Step 2: Replace `cdp_screenshot` call in command path**

```rust
let bytes = spider_capture::spider_screenshot(cfg, &normalized).await?;
```

**Step 3: Keep output path + JSON response behavior unchanged**

No changes to:
- `url_to_screenshot_filename`
- `format_screenshot_json`
- file write logic in `screenshot_one`

**Step 4: Run tests**

Run: `cargo test screenshot_migration_tests -- --nocapture`
Expected: PASS

**Step 5: Commit**

```bash
git add crates/cli/commands/screenshot/mod.rs crates/cli/commands/screenshot/spider_capture.rs crates/crawl/engine/runtime.rs
git commit -m "refactor: migrate screenshot command to spider screenshot api"
```

### Task 3: Remove obsolete custom CDP client implementation

**Files:**
- Delete: `crates/cli/commands/screenshot/cdp.rs`
- Modify: `crates/cli/commands/screenshot/mod.rs`

**Step 1: Remove exports/imports referencing old CDP file**

```rust
// remove
mod cdp;
pub(crate) use cdp::{cdp_screenshot, resolve_browser_ws_url};
```

**Step 2: Remove dead fallback/hostname rewrite logic from CLI screenshot path**

Delete custom `/json/version` fallback handling that is now covered by Spider runtime path.

**Step 3: Run tests**

Run: `cargo test screenshot -- --nocapture`
Expected: PASS

Run: `cargo check`
Expected: PASS

**Step 4: Commit**

```bash
git add crates/cli/commands/screenshot/mod.rs
git rm crates/cli/commands/screenshot/cdp.rs
git commit -m "chore: delete hand-rolled screenshot cdp client"
```

### Task 4: Add regression test for full-page mode parity

**Files:**
- Modify: `crates/cli/commands/screenshot/screenshot_migration_tests.rs`

**Step 1: Add full-page assertion test**

```rust
#[tokio::test]
async fn screenshot_full_page_flag_is_honored() {
    // compare dimensions or metadata between full_page=true/false captures
    // assert full_page output is taller for long fixture page
}
```

**Step 2: Run test**

Run: `cargo test screenshot_full_page_flag_is_honored -- --nocapture`
Expected: PASS

**Step 3: Commit**

```bash
git add crates/cli/commands/screenshot/screenshot_migration_tests.rs
git commit -m "test: verify full-page screenshot behavior after migration"
```

### Task 5: Documentation and verification

**Files:**
- Modify: `docs/HEADLESS_OPTIONS.md`
- Modify: `crates/cli/CLAUDE.md`

**Step 1: Update docs**

```md
Screenshot command now uses Spider screenshot support; custom raw CDP command path removed.
```

**Step 2: Final verification**

Run: `cargo fmt --check`
Expected: PASS

Run: `cargo clippy --all-targets --all-features -- -D warnings`
Expected: PASS

Run: `cargo test screenshot -- --nocapture`
Expected: PASS

**Step 3: Commit**

```bash
git add docs/HEADLESS_OPTIONS.md crates/cli/CLAUDE.md
git commit -m "docs: record screenshot migration to spider api"
```
