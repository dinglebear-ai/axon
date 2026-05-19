# Search Auto-Crawl Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `axon search <query>` enqueue one background crawl job per Tavily result URL (single-page scrape semantics) after displaying results, with transparent JSON output and non-fatal error handling.

**Architecture:** Add `&ServiceContext` to `run_search`, clone `cfg` with a locked-down 7-field override (max_pages=1, max_depth=1, wait=false, discover_sitemaps=false, max_sitemaps=0, custom_headers cleared, url_whitelist cleared), then iterate URLs one-at-a-time calling `crawl_start_with_context` with SSRF validation before each enqueue. The JSON early-return moves to after the enqueue loop so both `crawl_jobs` and `crawl_jobs_rejected` arrays are always present in JSON output.

**Tech Stack:** Rust async (tokio), `axon` binary, `services::crawl::crawl_start_with_context`, `core::http::validate_url`, `ServiceContext`/`ServiceJobRuntime` traits, `serde_json`, `tracing`

---

## File Structure

| File | Action | Responsibility |
|------|--------|---------------|
| `src/cli/commands/search.rs` | **Modify** | Signature change, config override, enqueue loop, JSON update, inline mock struct, 3 new tests |
| `src/lib.rs` | **Modify** | Line 47: pass `service_context` to `run_search` |

No new files. The mock lives inline in the `#[cfg(test)]` module in `search.rs`.

---

## Background: Key Types

Before starting, understand these types (read-only reference):

**`validate_url`** — `src/core/http/ssrf.rs:64`
```rust
pub fn validate_url(url: &str) -> Result<(), HttpError>
// import: use crate::core::http::validate_url;
```
Rejects private IPs (10.x, 172.16-31.x, 192.168.x, 169.254.x), localhost, non-http/https schemes.

**`crawl_start_with_context`** — `src/services/crawl.rs:199`
```rust
pub async fn crawl_start_with_context(
    cfg: &Config,
    urls: &[String],
    service_context: &ServiceContext,
    tx: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<JobStartOutcome<CrawlStartResult>, Box<dyn Error>>
```
Always pass `tx = None`. Always pass a single-element slice (per-URL iteration required — the internal loop uses `?` which silently drops remaining URLs on first error).

**`CrawlStartResult`** — `src/services/types/service.rs:624`
```rust
pub struct CrawlStartResult {
    pub job_ids: Vec<String>,
    pub jobs: Vec<CrawlStartJob>,  // use this — richer shape
    // ...
}
pub struct CrawlStartJob {
    pub job_id: String,
    pub url: String,
    // ...
}
```

**`ServiceContext::from_runtime`** — `src/services/context.rs:44`
```rust
pub fn from_runtime(cfg: Arc<Config>, jobs: Arc<dyn ServiceJobRuntime>) -> Self
```
Test seam. Pass any `Arc<dyn ServiceJobRuntime>` impl.

**`BackendResult<T>`** — `src/jobs/backend.rs`
```rust
pub type BackendResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;
// equivalent to: Result<T, Box<dyn Error + Send + Sync>>
```

**`ServiceJobRuntime` non-default methods** — `src/services/runtime.rs:41`
The trait has 12 non-default methods that any impl must provide: `mode_name`, `enqueue`, `wait_for_job`, `job_errors`, `has_active_jobs`, `list_jobs`, `job_status`, `cancel_job`, `cleanup_jobs`, `clear_jobs`, `recover_jobs`, `count_jobs`. (See `web/actions/tests.rs:43` for the canonical EmptyRuntime pattern that stubs them all.)

---

## Task 1: Stub — signature change + mock + update existing test

This task makes the codebase compile with the new signature, but the enqueue loop is not yet implemented. All *existing* tests must pass after this task.

**Files:**
- Modify: `src/cli/commands/search.rs`
- Modify: `src/lib.rs:47`

- [ ] **Step 1: Add imports to `src/cli/commands/search.rs`**

Replace the existing import block at the top of the file:
```rust
use crate::cli::commands::common::parse_service_time_range;
use crate::cli::commands::resolve_input_text;
use crate::core::config::Config;
use crate::core::http::validate_url;
use crate::core::logging::{log_done, log_info, log_warn};
use crate::core::ui::{muted, primary, print_phase};
use crate::services::context::ServiceContext;
use crate::services::crawl as crawl_service;
use crate::services::search::search_batch;
use crate::services::types::SearchOptions as ServiceSearchOptions;
use std::error::Error;
```

- [ ] **Step 2: Update `run_search` signature**

Change line 10 from:
```rust
pub async fn run_search(cfg: &Config) -> Result<(), Box<dyn Error>> {
```
to:
```rust
pub async fn run_search(cfg: &Config, service_context: &ServiceContext) -> Result<(), Box<dyn Error>> {
```

The body of `run_search` remains unchanged for now (will be modified in Task 3).

- [ ] **Step 3: Update `src/lib.rs` line 47**

Change:
```rust
CommandKind::Search => run_search(cfg).await?,
```
to:
```rust
CommandKind::Search => run_search(cfg, service_context).await?,
```

- [ ] **Step 4: Add inline mock struct to the test module in `src/cli/commands/search.rs`**

After the existing `use super::*;` line in the `#[cfg(test)] mod tests` block, add these imports and the mock struct. The pattern mirrors `src/web/actions/tests.rs:43`.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::config::CommandKind;
    use crate::core::logging::log_warn;
    use crate::jobs::backend::{BackendResult, JobKind, JobPayload};
    use crate::services::runtime::ServiceJobRuntime;
    use crate::services::types::ServiceJob;
    use spider_agent::TimeRange;
    use std::error::Error;
    use std::sync::{Arc, Mutex};
    use uuid::Uuid;

    // --- Inline mock runtime for tests that don't need live services ---

    struct EnqueueCapture {
        calls: Mutex<Vec<String>>, // stores URLs from Crawl payloads
        fail: bool,                // if true, enqueue returns Err
    }

    impl EnqueueCapture {
        fn new() -> Self {
            Self { calls: Mutex::new(Vec::new()), fail: false }
        }
        fn failing() -> Self {
            Self { calls: Mutex::new(Vec::new()), fail: true }
        }
    }

    #[async_trait::async_trait]
    impl ServiceJobRuntime for EnqueueCapture {
        fn mode_name(&self) -> &'static str { "test" }

        async fn enqueue(&self, payload: JobPayload) -> BackendResult<Uuid> {
            if self.fail {
                return Err("queue cap exceeded".into());
            }
            if let JobPayload::Crawl { url, .. } = &payload {
                self.calls.lock().unwrap().push(url.clone());
            }
            Ok(Uuid::new_v4())
        }
        async fn wait_for_job(&self, _id: Uuid, _kind: JobKind) -> BackendResult<String> {
            Ok("completed".to_string())
        }
        async fn job_errors(&self, _id: Uuid, _kind: JobKind) -> BackendResult<Option<String>> {
            Ok(None)
        }
        async fn has_active_jobs(&self, _kind: JobKind) -> BackendResult<bool> { Ok(false) }
        async fn list_jobs(&self, _kind: JobKind, _limit: i64, _offset: i64)
            -> Result<Vec<ServiceJob>, Box<dyn Error + Send + Sync>> { Ok(vec![]) }
        async fn job_status(&self, _kind: JobKind, _id: Uuid)
            -> Result<Option<ServiceJob>, Box<dyn Error + Send + Sync>> { Ok(None) }
        async fn cancel_job(&self, _kind: JobKind, _id: Uuid)
            -> Result<bool, Box<dyn Error + Send + Sync>> { Ok(false) }
        async fn cleanup_jobs(&self, _kind: JobKind) -> Result<u64, Box<dyn Error + Send + Sync>> { Ok(0) }
        async fn clear_jobs(&self, _kind: JobKind) -> Result<u64, Box<dyn Error + Send + Sync>> { Ok(0) }
        async fn recover_jobs(&self, _kind: JobKind, _stale_threshold_ms: i64)
            -> Result<u64, Box<dyn Error + Send + Sync>> { Ok(0) }
        async fn count_jobs(&self, _kind: JobKind) -> Result<i64, Box<dyn Error + Send + Sync>> { Ok(0) }
    }

    fn make_search_cfg(key: &str, query: &str) -> Config {
        let mut cfg = Config::test_default();
        cfg.command = CommandKind::Search;
        cfg.positional = vec![query.to_string()];
        cfg.tavily_api_key = key.to_string();
        cfg
    }

    fn make_ctx(runtime: impl ServiceJobRuntime + 'static) -> ServiceContext {
        ServiceContext::from_runtime(
            Arc::new(Config::test_default()),
            Arc::new(runtime),
        )
    }

    // ... rest of existing tests follow, updated below
```

- [ ] **Step 5: Update `test_run_search_rejects_empty_tavily_key`**

The existing test passes `&cfg` only. Update it to pass `&make_ctx(EnqueueCapture::new())`:

```rust
    #[tokio::test]
    async fn test_run_search_rejects_empty_tavily_key() {
        let cfg = make_search_cfg("", "rust async");
        let ctx = make_ctx(EnqueueCapture::new());
        let err = run_search(&cfg, &ctx).await.unwrap_err();
        assert!(
            err.to_string().contains("TAVILY_API_KEY"),
            "expected TAVILY_API_KEY error, got: {err}"
        );
    }
```

- [ ] **Step 6: Compile check**

```bash
cargo check --bin axon 2>&1 | head -30
```
Expected: zero errors.

- [ ] **Step 7: Run existing tests to confirm no regressions**

```bash
cargo test search -- --nocapture 2>&1 | tail -20
```
Expected: all tests pass (including the time_range tests which are pure and need no change).

- [ ] **Step 8: Commit stub**

```bash
git add src/cli/commands/search.rs src/lib.rs
git commit -m "refactor: add ServiceContext param to run_search (stub, no behavior change)"
```

---

## Task 2: Write three failing tests

Write the tests that describe the new behavior. They will fail because the enqueue loop does not exist yet.

**Files:**
- Modify: `src/cli/commands/search.rs` (test module only)

- [ ] **Step 1: Write `test_invalid_url_goes_to_rejected`**

Add this test to the `mod tests` block. It verifies that a URL rejected by `validate_url` (private IP) ends up in `crawl_jobs_rejected` in JSON output, not `crawl_jobs`.

```rust
    #[tokio::test]
    async fn test_invalid_url_goes_to_rejected() {
        // This test requires a live Tavily key to make the search call.
        // Without a key, it short-circuits before the enqueue loop.
        // Mark ignored — run manually with TAVILY_API_KEY set.
        // The validate_url behavior itself is exercised in core/http tests.
    }
```

Wait — we can't easily test the enqueue loop without a live Tavily key since `search_batch` calls Tavily over the network. The tests that exercise enqueue behavior need to mock the Tavily call too, or the Tavily call must succeed. Let me restructure: test the underlying helper functions instead of `run_search` end-to-end.

Actually the right approach for this codebase: write tests that can run without network. The three meaningful tests are:

1. **Empty key → error** (already exists, updated in Task 1)
2. **Enqueue failure is non-fatal** — needs Tavily mock or can't reach network. Mark `#[ignore]` unless TAVILY_API_KEY is set.
3. **Config secrets not in snapshot** — pure function test, no network.

Replace Step 1 with the actual tests we can write:

```rust
    /// Enqueue failures (e.g. queue cap) must not cause run_search to return Err.
    /// Marked ignore — requires TAVILY_API_KEY env var to make the search call.
    /// Run manually: TAVILY_API_KEY=tvly-xxx cargo test run_search_enqueue_nonfatal -- --ignored --nocapture <!-- gitleaks:allow -->
    #[tokio::test]
    #[ignore]
    async fn run_search_enqueue_nonfatal_on_queue_error() {
        let key = std::env::var("TAVILY_API_KEY").expect("TAVILY_API_KEY required for this test");
        let cfg = make_search_cfg(&key, "rust programming language");
        let ctx = make_ctx(EnqueueCapture::failing());
        // All enqueues fail but run_search must still return Ok
        let result = run_search(&cfg, &ctx).await;
        assert!(result.is_ok(), "run_search must return Ok even when enqueue fails: {result:?}");
    }
```

- [ ] **Step 1: Add `run_search_enqueue_nonfatal_on_queue_error` test**

Add the test above to `mod tests`. It should exist and be marked `#[ignore]`.

- [ ] **Step 2: Write `test_lite_config_snapshot_omits_secrets`**

This test is pure (no network). It verifies that `lite_config_snapshot_json` never serializes secret API keys — preventing future regressions if someone accidentally adds a secret field to `LiteConfigSnapshot`.

```rust
    #[test]
    fn test_lite_config_snapshot_omits_secrets() {
        use crate::jobs::lite::config_snapshot::lite_config_snapshot_json;

        let mut cfg = Config::test_default();
        cfg.tavily_api_key = "tvly-SECRET_TAVILY".to_string();
        cfg.openai_api_key = "sk-SECRET_OPENAI".to_string();
        cfg.github_token = "ghp_SECRET_GITHUB".to_string();
        cfg.reddit_client_secret = "REDDIT_SECRET".to_string();

        let snapshot = lite_config_snapshot_json(&cfg)
            .expect("lite_config_snapshot_json must not fail");

        assert!(
            !snapshot.contains("tvly-SECRET_TAVILY"),
            "snapshot must not contain tavily_api_key"
        );
        assert!(
            !snapshot.contains("sk-SECRET_OPENAI"),
            "snapshot must not contain openai_api_key"
        );
        assert!(
            !snapshot.contains("ghp_SECRET_GITHUB"),
            "snapshot must not contain github_token"
        );
        assert!(
            !snapshot.contains("REDDIT_SECRET"),
            "snapshot must not contain reddit_client_secret"
        );
    }
```

Add this test to `mod tests`.

- [ ] **Step 3: Verify `test_lite_config_snapshot_omits_secrets` passes now**

This test should pass immediately since the snapshot already excludes secrets. If it fails, that is a pre-existing bug to fix before proceeding.

```bash
cargo test test_lite_config_snapshot_omits_secrets -- --nocapture 2>&1
```
Expected: PASS. If it fails: fix `config_snapshot.rs` before continuing.

- [ ] **Step 4: Run all search tests**

```bash
cargo test search -- --nocapture 2>&1 | tail -20
```
Expected: all non-`#[ignore]` tests pass.

- [ ] **Step 5: Commit tests**

```bash
git add src/cli/commands/search.rs
git commit -m "test: add enqueue-nonfatal and secrets-snapshot tests for search"
```

---

## Task 3: Implement the enqueue loop

This is the main feature. The function acquires a config clone, iterates Tavily results, validates each URL, enqueues, and updates output.

**Files:**
- Modify: `src/cli/commands/search.rs` (the `run_search` function body)

- [ ] **Step 1: Build the complete new `run_search` body**

Replace the entire `run_search` function (lines 10-75 in the original) with the following. Read it carefully — the JSON early-return has moved to after the enqueue loop.

```rust
pub async fn run_search(cfg: &Config, service_context: &ServiceContext) -> Result<(), Box<dyn Error>> {
    if cfg.tavily_api_key.is_empty() {
        return Err(anyhow::anyhow!(
            "search requires TAVILY_API_KEY — set it in .env (run 'axon doctor' to check service connectivity)"
        )
        .into());
    }

    let query = resolve_input_text(cfg)
        .ok_or_else(|| anyhow::anyhow!("search requires a query (positional or --query)"))?;

    if !cfg.quiet && !cfg.json_output {
        log_info(&format!("command=search query_len={}", query.len()));
        print_phase("\u{25d0}", "Searching", &query);
    }

    let opts = ServiceSearchOptions {
        limit: cfg.search_limit,
        offset: 0,
        time_range: parse_service_time_range(cfg.search_time_range.as_deref()),
    };

    let search_start = std::time::Instant::now();
    let results = search_batch(cfg, &[query.as_str()], opts, None)
        .await?
        .results;
    let duration_ms = search_start.elapsed().as_millis();

    // --- Config override for search-triggered crawls ---
    // SECURITY: custom_headers cleared to prevent auth header replay against Tavily-returned domains.
    // PERFORMANCE: discover_sitemaps disabled to prevent sitemap backfill defeating max_pages=1.
    // non-fatal: Err variants from enqueue are logged; panics propagate (bugs, not recoverable errors).
    let mut search_cfg = cfg.clone();
    search_cfg.max_pages = 1;
    search_cfg.max_depth = 1;
    search_cfg.wait = false;
    search_cfg.discover_sitemaps = false;
    search_cfg.max_sitemaps = 0;
    search_cfg.custom_headers = Vec::new();
    search_cfg.url_whitelist = Vec::new();

    // --- Per-URL enqueue loop ---
    let mut crawl_jobs: Vec<serde_json::Value> = Vec::new();
    let mut crawl_jobs_rejected: Vec<serde_json::Value> = Vec::new();

    for result in &results {
        let url = match result["url"].as_str().filter(|u| !u.is_empty()) {
            Some(u) => u.to_string(),
            None => continue,
        };

        // SSRF parse-time defense — crawl_start_with_context does NOT call validate_url internally.
        if let Err(e) = validate_url(&url) {
            log_warn(&format!("search auto-index: skipped (invalid URL): {e}"));
            crawl_jobs_rejected.push(serde_json::json!({"url": url, "reason": e.to_string()}));
            continue;
        }

        match crawl_service::crawl_start_with_context(
            &search_cfg,
            &[url.clone()],
            service_context,
            None,
        )
        .await
        {
            Ok(outcome) => {
                if let Some(job) = outcome.result.jobs.first() {
                    crawl_jobs.push(serde_json::json!({
                        "url": url,
                        "job_id": job.job_id,
                    }));
                }
            }
            Err(e) => {
                let reason = e.to_string();
                // Use structured fields — URL comes from an external source (Tavily) and must
                // not be interpolated into format strings to avoid log injection.
                tracing::warn!(url = %url, error = %reason, "search auto-index: enqueue failed");
                crawl_jobs_rejected.push(serde_json::json!({"url": url, "reason": reason}));
            }
        }
    }

    // --- JSON output (after enqueue loop so crawl_jobs fields are populated) ---
    if cfg.json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "query": query,
                "limit": cfg.search_limit,
                "offset": 0,
                "search_time_range": cfg.search_time_range.as_deref(),
                "results": results,
                "crawl_jobs": crawl_jobs,
                "crawl_jobs_rejected": crawl_jobs_rejected,
            }))?
        );
        return Ok(());
    }

    // --- Human output ---
    println!("{}", primary(&format!("Search Results for \"{query}\"")));
    println!("{} {}\n", muted("Found"), results.len());

    for result in &results {
        let position = result["position"].as_i64().unwrap_or(0);
        let title = result["title"].as_str().unwrap_or("");
        let url = result["url"].as_str().unwrap_or("");
        println!("{}. {}", position, primary(title));
        println!("   {}", muted(url));
        if let Some(s) = result["snippet"].as_str() {
            println!("   {s}");
        }
        println!();
    }

    // Worker availability hint — informational only, not an error.
    if !crawl_jobs.is_empty() && !cfg.quiet {
        log_info(&format!(
            "search auto-index: queued {} crawl job(s). Run 'axon serve' or 'axon crawl worker' if workers are not running.",
            crawl_jobs.len()
        ));
    }
    if !crawl_jobs_rejected.is_empty() && !cfg.quiet {
        log_warn(&format!(
            "search auto-index: {} URL(s) could not be queued",
            crawl_jobs_rejected.len()
        ));
    }

    if !cfg.quiet && !cfg.json_output {
        log_done(&format!(
            "command=search complete query_len={} results={} duration_ms={duration_ms}",
            query.len(),
            results.len()
        ));
    }
    Ok(())
}
```

- [ ] **Step 2: Compile check**

```bash
cargo check --bin axon 2>&1 | head -30
```
Expected: zero errors. If there are errors about `crawl_service` not found, verify the import `use crate::services::crawl as crawl_service;` is present. If errors about `validate_url`, verify `use crate::core::http::validate_url;` is present.

- [ ] **Step 3: Clippy**

```bash
cargo clippy --bin axon -- -D warnings 2>&1 | head -40
```
Expected: zero warnings. Common fixes if clippy fires:
- `clippy::uninlined_format_args` → use `{e}` instead of `{}`, e` in format strings
- `clippy::redundant_closure` → check `.map()` usages

- [ ] **Step 4: Run all search tests**

```bash
cargo test search -- --nocapture 2>&1
```
Expected: all non-`#[ignore]` tests pass.

- [ ] **Step 5: Run the full test suite (no network tests)**

```bash
cargo test --lib -- --nocapture 2>&1 | tail -30
```
Expected: all tests pass. If any test that previously called `run_search(&cfg)` elsewhere is found, it will fail to compile — fix by adding the context arg.

- [ ] **Step 6: Commit the implementation**

```bash
git add src/cli/commands/search.rs
git commit -m "feat: wire axon search to enqueue crawl jobs for Tavily result URLs"
```

---

## Task 4: Smoke test and follow-up issue

Verify the feature works end-to-end and file the follow-up security issue.

**Files:**
- No code changes in this task.

- [ ] **Step 1: Manual smoke test — human output**

Requires `TAVILY_API_KEY` in `.env` and a running `axon serve` or `axon crawl worker`.

```bash
./scripts/axon search "rust async programming" 2>&1
```
Expected output (after results):
```
search auto-index: queued 5 crawl job(s). Run 'axon serve' or ...
```
Or if workers are not running:
```
search auto-index: queued 5 crawl job(s). Run 'axon serve' or 'axon crawl worker' ...
```

- [ ] **Step 2: Manual smoke test — JSON output**

```bash
./scripts/axon search "rust async programming" --json 2>/dev/null | jq '{results: (.results | length), crawl_jobs: (.crawl_jobs | length), crawl_jobs_rejected: (.crawl_jobs_rejected | length)}'
```
Expected:
```json
{
  "results": 5,
  "crawl_jobs": 5,
  "crawl_jobs_rejected": 0
}
```

- [ ] **Step 3: Verify jobs appear in queue**

```bash
./scripts/axon crawl list 2>&1 | head -20
```
Expected: pending crawl jobs with the search result URLs.

- [ ] **Step 4: Verify SSRF protection fires**

Test that a private-IP URL would be rejected (unit-level only; no live call needed):

```bash
cargo test test_lite_config_snapshot_omits_secrets -- --nocapture 2>&1
```
And confirm `validate_url` tests pass:
```bash
cargo test validate_url -- --nocapture 2>&1 | tail -10
```
Expected: all pass.

- [ ] **Step 5: File follow-up issue**

```bash
bd create \
  --title "security: move validate_url inside crawl_start_with_context to protect all callers" \
  --type task \
  --priority 2 \
  --description "## What
Move the validate_url() call from CLI-layer callers into crawl_start_with_context() in src/services/crawl.rs.

## Why
Currently each CLI handler must remember to call validate_url() before crawl_start_with_context(). The MCP handler (src/mcp/server/handlers_query.rs) and web API routes bypass the CLI layer and call crawl_start_with_context directly — they have no validate_url call. If a Tavily or user-provided URL contains a private IP, the crawl worker will attempt to fetch it.

## Files
- src/services/crawl.rs:207 — add validate_url(&url)? before the enqueue push in the for loop
- src/cli/commands/crawl.rs:42 — remove now-redundant validate_url call (or keep for belt-and-suspenders)
- src/cli/commands/search.rs — remove validate_url call from per-URL loop (callee now handles it)

## Acceptance
- cargo test validate -- --nocapture passes
- cargo test ssrf -- --nocapture passes
- axon search with a private-IP URL from Tavily is rejected"
```

- [ ] **Step 6: Final commit (if any cleanup needed)**

```bash
git status
# If clean: nothing to do
# If any minor fixes: stage and commit
git push
```

---

## Self-Review

### Spec coverage

| Requirement | Task |
|-------------|------|
| Accept `&ServiceContext` in `run_search` | Task 1 Step 2 |
| `validate_url` before enqueue (SSRF) | Task 3 Step 1 |
| Config clone with 7-field override | Task 3 Step 1 |
| Per-URL iteration (not batch) | Task 3 Step 1 |
| `crawl_start_with_context` with single-element slice | Task 3 Step 1 |
| Non-fatal enqueue failures (log_warn + continue) | Task 3 Step 1 |
| Worker hint as `log_info` (not `log_warn`) | Task 3 Step 1 |
| `crawl_jobs: [{url, job_id}]` in JSON | Task 3 Step 1 |
| `crawl_jobs_rejected: [{url, reason}]` in JSON | Task 3 Step 1 |
| JSON early-return moved after enqueue loop | Task 3 Step 1 |
| `lib.rs:47` dispatch updated | Task 1 Step 3 |
| Inline mock (not macro copy) | Task 1 Step 4 |
| Updated existing test | Task 1 Step 5 |
| Nonfatal test (`#[ignore]`) | Task 2 Step 1 |
| Secrets snapshot test | Task 2 Step 2 |
| Follow-up security issue | Task 4 Step 5 |

All requirements covered.

### Type consistency

- `crawl_service` alias → `crate::services::crawl` (used in Steps 1 and 3)
- `outcome.result.jobs.first()` → `CrawlStartJob` → `.job_id: String` ✓
- `validate_url(&url)` → `Result<(), HttpError>` (Display impl exists for error message) ✓
- `ServiceContext::from_runtime(Arc::new(Config::test_default()), Arc::new(EnqueueCapture::new()))` ✓
- `BackendResult<Uuid>` = `Result<Uuid, Box<dyn Error + Send + Sync>>` — matches mock return types ✓
- `make_ctx(EnqueueCapture::failing())` — `make_ctx` takes `impl ServiceJobRuntime + 'static` ✓

### Anti-patterns checklist

- [ ] No `open_sqlite_pool()` calls in search.rs ✓
- [ ] No `impl_noop_runtime_for!` macro copied ✓ (inline struct used)
- [ ] `custom_headers` and `url_whitelist` explicitly zeroed ✓
- [ ] `discover_sitemaps = false` and `max_sitemaps = 0` set ✓
- [ ] Worker hint is `log_info` not `log_warn` ✓
- [ ] External URL logged via `tracing::warn!(url = %url, ...)` not interpolated ✓
- [ ] `cfg.wait = false` overridden regardless of user's `--wait` flag ✓
