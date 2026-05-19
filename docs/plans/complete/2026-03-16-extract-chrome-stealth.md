# Extract Chrome Stealth Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire Chrome rendering mode + stealth patches into the extract command's single-URL path so `axon extract --render-mode chrome <url>` bypasses bot protection (Amazon, Cloudflare-protected sites) via headless Chrome with fingerprint/stealth patching.

**Architecture:** Add 10 Chrome/rendering fields to `ExtractWebConfig`. In `run_extract_with_engine`, branch the single-URL path (`limit ≤ 1`) on `render_mode`: Chrome uses a spider `Website` with `limit=1` + stealth + fingerprint via `website.crawl()` (same as the scrape command does); Http keeps the existing reqwest path (already working). The multi-page path (`limit > 1`) also gets `user_agent` wired for consistency.

**Tech Stack:** Rust, spider.rs (`chrome`, `chrome_stealth`, `control` features), reqwest, tokio

---

## Chunk 1: Extend ExtractWebConfig and Wire from Config

### Task 1: Add Chrome Fields to `ExtractWebConfig`

**Files:**
- Modify: `crates/core/content/engine.rs` — extend `ExtractWebConfig` struct (lines 40–49)
- Modify: `crates/cli/commands/extract.rs` — wire new fields from `Config` into `ExtractWebConfig` (lines 200–208)

**Background for the implementor:**

`ExtractWebConfig` currently has 7 fields. We're adding 10 more covering render mode, Chrome CDP connection, stealth options, and request tuning. `RenderMode` already derives `Clone + Copy` (confirmed in `crates/core/config/types/enums.rs:77–79`) so no enum changes needed.

The extract command's `run_extract_sync` function builds `ExtractWebConfig` at line 200 of `extract.rs`. All new fields come directly from `cfg: &Config` which is passed into `run_extract_sync`.

- [ ] **Step 1: Add fields to `ExtractWebConfig` in `crates/core/content/engine.rs`**

Locate the struct at line 40. Add after `custom_headers`:

```rust
pub struct ExtractWebConfig {
    pub start_url: String,
    pub prompt: String,
    pub limit: u32,
    pub openai_base_url: String,
    pub openai_api_key: String,
    pub openai_model: String,
    /// Custom HTTP headers in `"Key: Value"` format, passed through to spider.
    pub custom_headers: Vec<String>,
    // ── Rendering / Chrome ──────────────────────────────────────────────────
    pub render_mode: crate::crates::core::config::RenderMode,
    /// CDP management URL (e.g. `http://axon-chrome:6000`). `None` = no Chrome.
    pub chrome_remote_url: Option<String>,
    pub chrome_stealth: bool,
    pub chrome_anti_bot: bool,
    pub chrome_intercept: bool,
    pub bypass_csp: bool,
    pub accept_invalid_certs: bool,
    pub request_timeout_ms: Option<u64>,
    pub fetch_retries: usize,
    /// User-Agent string (from `AXON_CHROME_USER_AGENT`).
    pub user_agent: Option<String>,
}
```

- [ ] **Step 2: Wire new fields in `crates/cli/commands/extract.rs`**

Locate the `ExtractWebConfig { ... }` literal at line 200. Add the new fields:

```rust
let wcfg = ExtractWebConfig {
    start_url: url.clone(),
    prompt: prompt.to_string(),
    limit: max_pages,
    openai_base_url: openai_base_url_top.clone(),
    openai_api_key: openai_api_key_top.clone(),
    openai_model: openai_model_top.clone(),
    custom_headers: custom_headers.clone(),
    render_mode: cfg.render_mode,
    chrome_remote_url: cfg.chrome_remote_url.clone(),
    chrome_stealth: cfg.chrome_stealth,
    chrome_anti_bot: cfg.chrome_anti_bot,
    chrome_intercept: cfg.chrome_intercept,
    bypass_csp: cfg.bypass_csp,
    accept_invalid_certs: cfg.accept_invalid_certs,
    request_timeout_ms: cfg.request_timeout_ms,
    fetch_retries: cfg.fetch_retries,
    user_agent: cfg.chrome_user_agent.clone(),
};
```

- [ ] **Step 3: Verify compilation**

```bash
cargo check -q --locked
```

Expected: no errors. If struct literal errors appear in other files (e.g. `crates/jobs/`), check for any `ExtractWebConfig { ... }` literals there too — add the new fields with sensible defaults:
- `render_mode: RenderMode::Http`
- `chrome_remote_url: None`
- `chrome_stealth: false`, `chrome_anti_bot: false`, `chrome_intercept: false`, `bypass_csp: false`, `accept_invalid_certs: false`
- `request_timeout_ms: None`, `fetch_retries: 2`
- `user_agent: None`

- [ ] **Step 4: Commit**

```bash
git add crates/core/content/engine.rs crates/cli/commands/extract.rs
git commit -m "feat(extract): add Chrome/rendering fields to ExtractWebConfig"
```

---

## Chunk 2: Chrome Single-URL Path in engine.rs

### Task 2: Add Chrome Website Builder and Branch Single-URL Path

**Files:**
- Modify: `crates/core/content/engine.rs` — add imports, add `build_chrome_extract_website` helper, branch `run_extract_with_engine`

**Background for the implementor:**

Current state of the single-URL path (`run_single_url_extract`) uses `http_client()` (plain reqwest). The fix for the URL normalization bug is in place — this keeps that fix and adds a Chrome alternative for the same `limit ≤ 1` branch.

For Chrome mode, we use spider's `Website` with `limit=1` — exactly how `crates/crawl/scrape.rs` handles single-page scraping. Spider fetches the exact seed URL as the first page even in Chrome mode (this is confirmed: the scrape command works correctly on deep URLs like `/wiki/Rust`).

The Chrome path:
1. Build `Website::new(url)` with `limit=1` + `depth=0`, stealth, fingerprint, intercept, CDP connection
2. Subscribe to the broadcast channel, then use `tokio::join!` + `oneshot` + `tokio::select!` (spider's canonical `fetch_page_chrome` pattern — not `tokio::spawn`)
3. Call `website.crawl().await` in the joined crawl future; the sub future collects pages via biased select
4. Feed collected pages through `collect_page_results` for extraction

If Chrome config is missing (`chrome_remote_url` is `None`) or render mode is not Chrome, fall back to the existing reqwest path — never error.

**Key imports to add** (spider chrome modules, already in Cargo.toml feature set):
```rust
use spider::features::chrome_common::RequestInterceptConfiguration;
```

- [ ] **Step 1: Write a unit test for the Chrome-unavailable fallback**

Add at the bottom of `crates/core/content/engine.rs` (inside a `#[cfg(test)] mod tests { ... }` block):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::crates::core::config::RenderMode;

    /// When Chrome mode is requested but no chrome_remote_url is configured,
    /// the extract engine must fall back to the HTTP path gracefully rather
    /// than panicking or returning an error about a missing CDP connection.
    #[tokio::test]
    async fn extract_chrome_mode_without_remote_url_falls_back_to_http() {
        // We can't make a real network request in unit tests, but we CAN verify
        // that the Chrome path does NOT panic when chrome_remote_url is None.
        // The function will attempt an HTTP fetch of an invalid URL and return
        // an error — that's fine. What matters is no panic + no "CDP not configured" error.
        let engine = std::sync::Arc::new(
            crate::crates::core::content::deterministic::DeterministicExtractionEngine::new(vec![]),
        );
        let wcfg = ExtractWebConfig {
            start_url: "https://example.invalid".to_string(),
            prompt: "test".to_string(),
            limit: 1,
            openai_base_url: String::new(),
            openai_api_key: String::new(),
            openai_model: String::new(),
            custom_headers: vec![],
            render_mode: RenderMode::Chrome,
            chrome_remote_url: None, // ← no Chrome configured
            chrome_stealth: true,
            chrome_anti_bot: true,
            chrome_intercept: true,
            bypass_csp: false,
            accept_invalid_certs: false,
            request_timeout_ms: Some(1000),
            fetch_retries: 0,
            user_agent: None,
        };
        // Should not panic. The URL is intentionally invalid so we get a network
        // error, which is expected. We only care it falls back to HTTP, not Chrome.
        let result = run_extract_with_engine(wcfg, engine).await;
        match result {
            Ok(_) => {} // unlikely with invalid URL, but fine
            Err(e) => {
                let msg = e.to_string();
                // Must NOT be a Chrome/CDP error
                assert!(
                    !msg.contains("CDP") && !msg.contains("chrome_remote_url"),
                    "Expected HTTP fallback error, got Chrome error: {msg}"
                );
            }
        }
    }
}
```

- [ ] **Step 2: Run test — verify it fails for the right reason**

```bash
cargo test extract_chrome_mode_without_remote_url_falls_back_to_http -- --nocapture 2>&1 | tail -20
```

Expected: **FAIL** — either compile error (Chrome imports not yet added) or the function doesn't branch yet. Record the failure message.

- [ ] **Step 3: Add Chrome import to `crates/core/content/engine.rs`**

After the existing imports at the top of the file, add:

```rust
use spider::features::chrome_common::RequestInterceptConfiguration;
```

- [ ] **Step 4: Add `build_chrome_extract_website` helper**

Add this function after `parse_custom_headers` (before `ExtractWebConfig`):

```rust
/// Build a spider `Website` configured for single-page Chrome extraction.
///
/// Applies Chrome stealth, fingerprint patching, network intercept, and CDP
/// connection. Returns `None` when `chrome_remote_url` is absent — callers
/// must fall back to the HTTP path in that case.
fn build_chrome_extract_website(
    url: &str,
    wcfg: &ExtractWebConfig,
) -> Option<spider::website::Website> {
    let chrome_url = wcfg.chrome_remote_url.as_deref()?;

    let ssrf_patterns: Vec<spider::compact_str::CompactString> = ssrf_blacklist_patterns()
        .iter()
        .copied()
        .map(Into::into)
        .collect();

    let mut website = spider::website::Website::new(url);
    website
        .with_limit(1)
        // with_depth(0) prevents spider from discovering/queuing outbound links on the
        // seed page even when limit=1. Without it, spider still runs link-find callbacks
        // for every href on the page — wasted work for a single-page fetch.
        .with_depth(0)
        .with_blacklist_url(Some(ssrf_patterns))
        .with_stealth(wcfg.chrome_stealth || wcfg.chrome_anti_bot)
        .with_fingerprint(true)
        .with_dismiss_dialogs(true)
        .with_chrome_intercept(RequestInterceptConfiguration::new(wcfg.chrome_intercept))
        .with_chrome_connection(Some(chrome_url.to_string()));

    if wcfg.bypass_csp {
        website.with_csp_bypass(true);
    }
    if wcfg.accept_invalid_certs {
        website.with_danger_accept_invalid_certs(true);
    }
    if let Some(ua) = wcfg.user_agent.as_deref() {
        website.with_user_agent(Some(ua));
    }
    if let Some(timeout_ms) = wcfg.request_timeout_ms {
        website.with_request_timeout(Some(std::time::Duration::from_millis(timeout_ms)));
    }
    let retries = wcfg.fetch_retries.min(u8::MAX as usize) as u8;
    website.with_retry(retries);

    website.configuration.disable_log = true;

    Some(website)
}
```

**Note on file size:** After this addition, `engine.rs` will be ~420 lines — within the 500-line monolith limit.

- [ ] **Step 5: Add the Chrome async single-URL extraction helper**

Add this function after `build_chrome_extract_website`.

**Critical pattern note:** Use `tokio::join!` + `oneshot` + `tokio::select!` — the same pattern spider's own test helpers use for `fetch_page_chrome`. This avoids `tokio::spawn` (which would require the futures to be `Send`) and exits the collect loop promptly via the done signal rather than waiting for the broadcast channel to fully drain.

```rust
/// Fetch a single URL via headless Chrome and extract structured data from it.
///
/// Uses spider's Chrome path (`website.crawl()`) with stealth and fingerprint
/// patching. Falls back to the HTTP path when Chrome is not configured.
async fn run_single_url_extract_chrome(
    url: &str,
    engine: Arc<DeterministicExtractionEngine>,
    cfg: &ExtractWebConfig,
    fallback_cfg: FallbackConfig,
) -> Result<ExtractRun, Box<dyn Error>> {
    let Some(mut website) = build_chrome_extract_website(url, cfg) else {
        // No Chrome configured — delegate to the HTTP path.
        return run_single_url_extract(
            url,
            crate::crates::core::http::http_client()?.clone(),
            engine,
            fallback_cfg,
        )
        .await;
    };

    let rx = website.subscribe(16).ok_or("subscribe failed")?;

    // Spider's canonical single-page Chrome fetch pattern:
    // tokio::join! + oneshot avoids tokio::spawn (no Send bound required).
    // The biased select! checks done_rx first — exits the collect loop
    // immediately when crawl signals done, even if the channel hasn't closed.
    let (done_tx, mut done_rx) = tokio::sync::oneshot::channel::<()>();

    let crawl = async move {
        website.crawl().await;
        website.unsubscribe();
        let _ = done_tx.send(());
    };

    // Collect pages into a Vec; return it so tokio::join! can hand it back.
    let sub = async move {
        let mut pages: Vec<spider::page::Page> = Vec::new();
        loop {
            tokio::select! {
                biased;
                _ = &mut done_rx => break,
                result = rx.recv() => {
                    match result {
                        Ok(page) => pages.push(page),
                        Err(_) => break,
                    }
                }
            }
        }
        pages
    };

    let (_, pages) = tokio::join!(crawl, sub);

    // Feed the collected pages through collect_page_results' per-page pipeline.
    // For a single-URL Chrome extract we expect exactly 1 page.
    let http = crate::crates::core::http::http_client()?.clone();
    // Build a one-shot broadcast channel to replay collected pages into
    // collect_page_results (which expects a broadcast::Receiver).
    let (replay_tx, replay_rx) = spider::tokio::sync::broadcast::channel(pages.len().max(1));
    for page in pages {
        let _ = replay_tx.send(page);
    }
    drop(replay_tx); // signal EOF immediately

    let PageCollectResult {
        results,
        pages_visited,
        pages_with_data,
        metrics,
        parser_hits,
    } = collect_page_results(replay_rx, http, Arc::clone(&engine), fallback_cfg).await;

    Ok(ExtractRun {
        start_url: url.to_string(),
        pages_visited,
        pages_with_data,
        results,
        metrics,
        parser_hits,
    })
}
```

**Implementation note:** The replay-channel pattern above keeps the existing `collect_page_results` function unchanged. If `collect_page_results` is already private to this module, a simpler alternative is to inline the per-page processing loop directly after `tokio::join!`. Either approach is correct — prefer whichever avoids adding a new function signature to `collect_page_results`.

- [ ] **Step 6: Branch `run_extract_with_engine` on render mode**

Find the single-URL early return (around line 316–324):

```rust
    // Single-page: bypass spider to fetch the exact URL.
    if wcfg.limit <= 1 {
        return run_single_url_extract(
            &wcfg.start_url,
            http_client()?.clone(),
            engine,
            fallback_cfg,
        )
        .await;
    }
```

Replace with:

```rust
    // Single-page: bypass spider to fetch the exact URL. Spider normalises deep
    // paths to the domain root (Website::new strips the path component), so
    // requests for /wiki/Rust or /recipe/12345 land on the site homepage instead.
    // For Chrome mode, we use spider with limit=1 to get stealth + fingerprint
    // patches. For HTTP mode, plain reqwest fetches the exact URL directly.
    if wcfg.limit <= 1 {
        return match wcfg.render_mode {
            crate::crates::core::config::RenderMode::Chrome => {
                run_single_url_extract_chrome(
                    &wcfg.start_url,
                    engine,
                    &wcfg,
                    fallback_cfg,
                )
                .await
            }
            _ => {
                run_single_url_extract(
                    &wcfg.start_url,
                    http_client()?.clone(),
                    engine,
                    fallback_cfg,
                )
                .await
            }
        };
    }
```

- [ ] **Step 7: Wire `user_agent` in the multi-page path**

In the multi-page `Website` builder block (around line 331–343), add user-agent alongside the existing custom-headers block:

```rust
    // Wire user-agent so HTTP extract crawls use the same UA as scrape/crawl.
    if let Some(ua) = wcfg.user_agent.as_deref() {
        website.with_user_agent(Some(ua));
    }
```

Place this right before or after the existing `if !wcfg.custom_headers.is_empty() { ... }` block.

- [ ] **Step 8: Run the test — verify it passes**

```bash
cargo test extract_chrome_mode_without_remote_url_falls_back_to_http -- --nocapture 2>&1 | tail -20
```

Expected: **PASS** — Chrome mode with `chrome_remote_url: None` delegates to HTTP path, which returns a network error (not a CDP error).

- [ ] **Step 9: Run the full content test suite**

```bash
cargo test content -- --nocapture 2>&1 | tail -20
```

Expected: all tests pass.

- [ ] **Step 10: Run cargo check and clippy**

```bash
cargo check -q --locked && cargo clippy --locked -- -D warnings 2>&1 | grep -E "^error|^warning" | head -20
```

Expected: no errors, no warnings.

- [ ] **Step 11: Manual smoke test — HTTP mode (regression)**

```bash
./scripts/axon extract "https://en.wikipedia.org/wiki/Rust_(programming_language)" \
    --query "programming language facts" --wait true 2>&1
```

Expected: `pages_visited: 1`, `deterministic_pages: 1`, `parser_hits` includes `json-ld` and `open-graph`.

- [ ] **Step 12: Manual smoke test — Chrome mode on a bot-protected site**

```bash
./scripts/axon extract --render-mode chrome \
    "https://www.amazon.com/dp/B09G9FPHY6" \
    --query "product name price specs and rating" \
    --wait true 2>&1
```

Expected: `pages_visited: 1`, `pages_with_data: 1`, results include JSON-LD or OG product data (not just HTML table rows from a CAPTCHA page).

If Chrome is not reachable, the function should fall back to HTTP gracefully and log a warning — not crash.

- [ ] **Step 13: Commit**

```bash
git add crates/core/content/engine.rs crates/cli/commands/extract.rs
git commit -m "feat(extract): add Chrome stealth path to single-URL extraction

When --render-mode chrome, run_extract_with_engine now uses spider's
Chrome path (website.crawl()) with stealth + fingerprint patching for
single-URL extractions. Falls back to HTTP when chrome_remote_url is
not configured. Multi-page path gains user_agent wiring for consistency."
```

---

## Reference: Config Fields Used

| `Config` field | `ExtractWebConfig` field | Purpose |
|----------------|--------------------------|---------|
| `cfg.render_mode` | `render_mode` | Http / Chrome / AutoSwitch dispatch |
| `cfg.chrome_remote_url` | `chrome_remote_url` | CDP management URL (e.g. `http://axon-chrome:6000`) |
| `cfg.chrome_stealth` | `chrome_stealth` | Patches `navigator.webdriver` + other signals |
| `cfg.chrome_anti_bot` | `chrome_anti_bot` | Additional anti-bot evasion (combined with stealth) |
| `cfg.chrome_intercept` | `chrome_intercept` | Network intercept (blocks ads/trackers during crawl) |
| `cfg.bypass_csp` | `bypass_csp` | Disables Content Security Policy enforcement |
| `cfg.accept_invalid_certs` | `accept_invalid_certs` | Accept self-signed TLS certs |
| `cfg.request_timeout_ms` | `request_timeout_ms` | Per-request timeout |
| `cfg.fetch_retries` | `fetch_retries` | Retry count on transient failures |
| `cfg.chrome_user_agent` | `user_agent` | Browser UA string (`AXON_CHROME_USER_AGENT`) |

## Known Limitations

- **AutoSwitch mode** is not wired for extract — the extract command is prompt-driven (not thin-page-ratio driven), so auto-switch doesn't apply. `RenderMode::AutoSwitch` in the new branch falls through to `_` → HTTP path. This is correct behavior.
- **Multi-page Chrome** is out of scope: the multi-page path uses `crawl_raw()` (HTTP only) and is unchanged. Chrome multi-page would require a larger refactor of the `collect_page_results` path.
- **CDP URL pre-resolution** is not done (unlike the crawl engine which pre-resolves via `/json/version`). Spider handles WebSocket discovery internally from the management URL. This adds ~1 extra round-trip but removes a cross-crate dependency.
