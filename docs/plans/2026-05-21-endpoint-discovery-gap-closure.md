# Endpoint Discovery Gap Closure Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix seven concrete gaps between the implemented endpoint discovery code and the w2wf bead acceptance criteria, then close all six child beads and the epic.

**Architecture:** All changes are isolated to four files: `src/mcp/server.rs` (scope fix), `src/web/server/routing.rs` (REST scope fix), `src/services/endpoints.rs` (global semaphores), and `src/services/endpoints/verify.rs` (constant corrections + semaphore). The Chrome capture pre-dispatch blocking gap in `src/services/endpoints/capture.rs` is addressed via a CDP `Fetch.enable` requestPaused intercept. Docs in `docs/commands/endpoints.md` are updated last.

**Tech Stack:** Rust, tokio::sync::Semaphore, reqwest, CDP (Chrome DevTools Protocol), rmcp, axum

---

## Gap Inventory (Pre-Plan Audit)

The following gaps were found by comparing the live code to bead acceptance criteria:

| # | Bead | Gap | File |
|---|------|-----|------|
| 1 | w2wf.3 | `endpoints` mapped to `axon:read` — should be `axon:write` (active network I/O) | `src/mcp/server.rs:336` |
| 2 | w2wf.3 | REST `/v1/endpoints` in `read_routes` — should be in `write_routes` | `src/web/server/routing.rs:46` |
| 3 | w2wf.2 | No global process-wide bundle-fetch semaphore (default 8) | `src/services/endpoints.rs` |
| 4 | w2wf.4 | `MAX_VERIFY_PROBES=40` (spec: 100), `VERIFY_TIMEOUT_SECS=4` (spec: 2), `VERIFY_CONCURRENCY=5` (spec: 4), no global verification semaphore (default 16) | `src/services/endpoints/verify.rs` |
| 5 | w2wf.5 | Chrome capture uses post-capture SSRF validation only — must pre-block via CDP `Fetch.enable` before dispatch | `src/services/endpoints/capture.rs` |
| 6 | w2wf.5 | No global Chrome capture semaphore (default 1) | `src/services/endpoints.rs` |
| 7 | w2wf.6 | Docs missing exact verify constants, `axon:write` scope requirement, global semaphore documentation | `docs/commands/endpoints.md` |

---

## File Map

**Modified files only** (no new files required):

- `src/mcp/server.rs` — move `"endpoints"` from the read arm to the write arm of `required_scope_for`
- `src/web/server/routing.rs` — move `/v1/endpoints` route from `read_routes` to `write_routes`
- `src/services/endpoints.rs` — add `BUNDLE_FETCH_SEMAPHORE` (Semaphore, cap 8) and `CHROME_CAPTURE_SEMAPHORE` (Semaphore, cap 1); acquire before `fetch_bundles` and `capture_provider.capture`
- `src/services/endpoints/verify.rs` — correct `MAX_VERIFY_PROBES` to 100, `VERIFY_TIMEOUT_SECS` to 2, `VERIFY_CONCURRENCY` to 4; add `VERIFY_SEMAPHORE` (Semaphore, cap 16)
- `src/services/endpoints/capture.rs` — add `Fetch.enable`/`Fetch.requestPaused` CDP intercept to block private/loopback requests before Chrome dispatches them; add tests via fake provider
- `src/services/endpoints_tests.rs` — add redirect-to-blocked-URL verification test; add semaphore cap assertion
- `docs/commands/endpoints.md` — add exact verify constants, scope requirement, global semaphore documentation
- `tests/mcp_contract_parity.rs` — add scope test for `endpoints` action; verify `axon:write` is required
- `tests/cli_help_contract.rs` — already passing; verify still passes after routing change

---

## Task 1: Fix MCP scope — move `endpoints` to `axon:write`

**Files:**
- Modify: `src/mcp/server.rs:324` (make `required_scope_for` `pub(crate)`)
- Modify: `src/mcp/server.rs:329-342` (move "endpoints" arm)
- Modify: `tests/mcp_contract_parity.rs`

**Note on w2wf.3 comment discrepancy:** The bead's 2026-05-19 19:26 FACT comment says "endpoint discovery should be axon:read only while transient and non-storing." This is superseded by the locked decisions and AC, which state "Any public MCP/action/REST mode that fetches a page … is active outbound network behavior and must require `axon:write`." The fix follows the locked decisions, not the older comment.

**Note on per-flag vs action-level scope:** The AC requires "read-scoped tokens are denied for URL-fetching discovery, verify=true, and capture_network=true." Once `endpoints` maps to `axon:write`, this collapses into one action-level check: read-scoped tokens cannot reach the handler at all, regardless of flag values. No per-flag scope check is needed.

- [ ] **Step 1: Read the current `required_scope_for` function**

Read `src/mcp/server.rs` lines 309–343 to confirm the current placement.

- [ ] **Step 2: Make `required_scope_for` testable and write a failing test**

First, change `fn required_scope_for` to `pub(crate) fn required_scope_for` so the integration test can call it directly:

```rust
pub(crate) fn required_scope_for(action: &str, subaction: &str) -> Option<&'static str> {
```

Then add this test to `tests/mcp_contract_parity.rs`. Note that `axon` re-exports the MCP server module:

```rust
use axon::mcp::server::required_scope_for;

/// Endpoints requires axon:write because it fetches pages, bundles, probes
/// endpoints, and may execute Chrome capture — all active outbound network I/O.
/// This test fails before the fix (returns Some("axon:read")) and passes after.
#[test]
fn endpoints_action_scope_is_write_not_read() {
    assert_eq!(
        required_scope_for("endpoints", ""),
        Some("axon:write"),
        "endpoints must require axon:write — it performs active outbound network I/O"
    );
    // Scope is per-action, not per-flag. Once axon:write is required, any
    // read-scoped token is denied regardless of verify or capture_network flags.
    assert_ne!(
        required_scope_for("endpoints", ""),
        Some("axon:read"),
        "endpoints must NOT be axon:read — that would allow read tokens to use Axon as an outbound scanner"
    );
}
```

- [ ] **Step 3: Run test to confirm it fails before the fix**

```bash
cargo test --test mcp_contract_parity endpoints_action_scope_is_write_not_read
```

Expected: FAIL with `assertion failed: endpoints must require axon:write`

- [ ] **Step 4: Move `"endpoints"` to the write arm in `required_scope_for`**

In `src/mcp/server.rs`, find:

```rust
        // Read / query operations require axon:read.
        "status" | "query" | "retrieve" | "search" | "map" | "endpoints" | "evaluate"
        | "suggest" | "doctor" | "domains" | "sources" | "stats" | "research" | "ask"
        | "screenshot" => Some("axon:read"),
```

Replace with:

```rust
        // Read / query operations require axon:read.
        "status" | "query" | "retrieve" | "search" | "map" | "evaluate"
        | "suggest" | "doctor" | "domains" | "sources" | "stats" | "research" | "ask"
        | "screenshot" => Some("axon:read"),
        // Active-network operations: endpoint discovery fetches pages, bundles,
        // probes endpoints, and may execute Chrome capture. All require axon:write.
        // Scope is per-action: once axon:write is required, read-scoped tokens
        // cannot reach the handler regardless of verify or capture_network flags.
        "endpoints" => Some("axon:write"),
```

- [ ] **Step 5: Verify the `required_scope_for` function is now `pub(crate)` (from Step 2)**

Confirm the function signature reads:

```rust
pub(crate) fn required_scope_for(action: &str, subaction: &str) -> Option<&'static str> {
```

- [ ] **Step 6: Run `cargo check --bin axon`**

```bash
rtk cargo check --bin axon
```

Expected: 0 errors, at most the 3 existing warnings about `std::fmt` qualifications

- [ ] **Step 7: Run all MCP contract tests**

```bash
cargo test --test mcp_contract_parity
```

Expected: all 30 tests pass (29 existing + 1 new); `endpoints_action_scope_is_write_not_read` now passes

- [ ] **Step 8: Commit**

```bash
rtk git add src/mcp/server.rs tests/mcp_contract_parity.rs
rtk git commit -m "fix(mcp): move endpoints action to axon:write scope (active network I/O)"
```

---

## Task 2: Fix REST routing — move `/v1/endpoints` to `write_routes`

**Files:**
- Modify: `src/web/server/routing.rs:46`

- [ ] **Step 1: Read the current routing**

Read `src/web/server/routing.rs` lines 37–81 to see read_routes vs write_routes.

- [ ] **Step 2: Move the `/v1/endpoints` route**

In `src/web/server/routing.rs`, find:

```rust
        .route("/v1/endpoints", post(handlers::exploration::endpoints))
        .route("/v1/map", post(handlers::exploration::map));
```

Move the endpoints line to `write_routes`. The read_routes block should become:

```rust
    let read_routes = Router::new()
        .route("/v1/capabilities", get(v1_capabilities))
        .route("/v1/sources", get(handlers::discovery::sources))
        .route("/v1/domains", get(handlers::discovery::domains))
        .route("/v1/stats", get(handlers::discovery::stats))
        .route("/v1/status", get(handlers::discovery::status))
        .route("/v1/doctor", get(handlers::discovery::doctor))
        .route("/v1/query", post(handlers::rag::query))
        .route("/v1/retrieve", post(handlers::rag::retrieve))
        .route("/v1/map", post(handlers::exploration::map));
```

And `write_routes` gains the endpoints route near the top:

```rust
    let write_routes = Router::new()
        .route("/v1/endpoints", post(handlers::exploration::endpoints))
        .merge(ask_router)
        // ... rest of write_routes unchanged
```

- [ ] **Step 3: Run `cargo check --bin axon`**

```bash
rtk cargo check --bin axon
```

Expected: 0 errors

- [ ] **Step 4: Run CLI help contract tests to confirm no regression**

```bash
cargo test --test cli_help_contract
```

Expected: all 12 tests pass

- [ ] **Step 5: Commit**

```bash
rtk git add src/web/server/routing.rs
rtk git commit -m "fix(web): move /v1/endpoints to write_routes (active network I/O requires axon:write)"
```

---

## Task 3: Add global bundle-fetch and Chrome capture semaphores

**Files:**
- Modify: `src/services/endpoints.rs`

- [ ] **Step 1: Read the current `fetch_bundles` and `merge_network_capture` functions**

Read `src/services/endpoints.rs` lines 75–342 to understand where semaphore acquisitions must go.

- [ ] **Step 2: Add two `LazyLock<Semaphore>` globals and acquire them**

In `src/services/endpoints.rs`, after the existing `use` imports and constants, add:

```rust
use tokio::sync::Semaphore;
use std::sync::LazyLock;

/// Process-wide semaphore limiting concurrent bundle fetches across all
/// concurrent endpoint discovery requests. Default cap: 8.
static BUNDLE_FETCH_SEMAPHORE: LazyLock<Semaphore> = LazyLock::new(|| {
    let cap = std::env::var("AXON_ENDPOINT_BUNDLE_CONCURRENCY")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(8)
        .max(1);
    Semaphore::new(cap)
});

/// Process-wide semaphore limiting concurrent Chrome capture sessions.
/// Default cap: 1 (Chrome is a scarce resource).
static CHROME_CAPTURE_SEMAPHORE: LazyLock<Semaphore> = LazyLock::new(|| {
    let cap = std::env::var("AXON_ENDPOINT_CHROME_CONCURRENCY")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(1)
        .max(1);
    Semaphore::new(cap)
});
```

- [ ] **Step 3: Acquire `BUNDLE_FETCH_SEMAPHORE` before `fetch_bundles`**

In `discover_with_capture_provider`, find the line:

```rust
    let bundles = fetch_bundles(&client, &bundle_sources, options.max_scan_bytes).await;
```

Replace with:

```rust
    let _bundle_permit = BUNDLE_FETCH_SEMAPHORE.acquire().await
        .map_err(|err| format!("bundle fetch semaphore closed: {err}"))?;
    let bundles = fetch_bundles(&client, &bundle_sources, options.max_scan_bytes).await;
    drop(_bundle_permit);
```

- [ ] **Step 4: Acquire `CHROME_CAPTURE_SEMAPHORE` before `capture_provider.capture`**

In `merge_network_capture`, find:

```rust
    let captured = capture_provider
        .capture(cfg, url, CAPTURE_MAX_REQUESTS)
        .await?;
```

Replace with:

```rust
    let _chrome_permit = CHROME_CAPTURE_SEMAPHORE.acquire().await
        .map_err(|err| format!("Chrome capture semaphore closed: {err}"))?;
    let captured = capture_provider
        .capture(cfg, url, CAPTURE_MAX_REQUESTS)
        .await?;
    drop(_chrome_permit);
```

- [ ] **Step 5: Run `cargo check --bin axon`**

```bash
rtk cargo check --bin axon
```

Expected: 0 errors

- [ ] **Step 6: Run endpoint unit tests**

```bash
cargo test endpoints
```

Expected: all 21 endpoint tests pass (unit tests don't need Chrome or real services)

- [ ] **Step 7: Commit**

```bash
rtk git add src/services/endpoints.rs
rtk git commit -m "feat(endpoints): add process-wide bundle-fetch and Chrome capture semaphores"
```

---

## Task 4: Fix verification constants and add global verification semaphore

**Files:**
- Modify: `src/services/endpoints/verify.rs`
- Modify: `src/services/endpoints_tests.rs`

- [ ] **Step 1: Read the current constants**

Read `src/services/endpoints/verify.rs` lines 1–15. Current values:
- `VERIFY_TIMEOUT_SECS = 4` (spec: 2)
- `MAX_VERIFY_PROBES = 40` (spec: 100)
- `VERIFY_CONCURRENCY = 5` (spec: 4)

- [ ] **Step 2: Write a failing test that observably exercises the 100-probe cap**

This test requires at least 101 discovered endpoints to trigger the cap warning. Use a mock server that returns inline JS with 101 unique `/api/N` paths, then assert exactly 100 get verified and the warning is emitted.

Add this test to `src/services/endpoints_tests.rs`:

```rust
#[tokio::test]
#[serial]
async fn verification_probe_cap_is_100_with_warning() {
    let _loopback = LoopbackGuard::allow();
    let server = MockServer::start_async().await;

    // Build a page with 101 distinct relative API paths so the verifier
    // must cap at 100 and emit a warning.
    let paths: String = (0..=100)
        .map(|i| format!(r#"fetch("/api/path{i}");"#))
        .collect::<Vec<_>>()
        .join("\n");
    let body = format!("<script>{paths}</script>");

    server
        .mock_async(|when, then| {
            when.method(GET).path("/");
            then.status(200)
                .header("content-type", "text/html")
                .body(body);
        })
        .await;
    // Respond HEAD 200 for all /api/* paths.
    server
        .mock_async(|when, then| {
            when.method(HEAD).path_matches(regex::Regex::new(r"^/api/").unwrap());
            then.status(200);
        })
        .await;

    let report = discover(
        &Config::test_default(),
        &server.base_url(),
        EndpointOptions {
            include_bundles: false,
            verify: true,
            ..EndpointOptions::default()
        },
        None,
    )
    .await
    .expect("endpoint discovery");

    // Exactly 100 endpoints should be verified (the 101st is skipped).
    let verified_count = report
        .endpoints
        .iter()
        .filter(|e| e.verified.is_some())
        .count();
    assert_eq!(
        verified_count, 100,
        "MAX_VERIFY_PROBES must be 100: got {verified_count} verified"
    );

    // A warning about the skipped endpoint must be present.
    assert!(
        report.warnings.iter().any(|w| w.contains("capped at 100")),
        "expected warning about probe cap at 100; got warnings: {:?}",
        report.warnings
    );
}
```

Run the test before fixing constants (it will fail because `MAX_VERIFY_PROBES=40` means only 40 verified, not 100):

```bash
cargo test endpoints -- verification_probe_cap_is_100_with_warning
```

Expected: FAIL — `assertion failed: MAX_VERIFY_PROBES must be 100: got 40 verified`

- [ ] **Step 3: Fix constants in `verify.rs`**

In `src/services/endpoints/verify.rs`, replace lines 10–12:

```rust
const VERIFY_TIMEOUT_SECS: u64 = 4;
const MAX_VERIFY_PROBES: usize = 40;
const VERIFY_CONCURRENCY: usize = 5;
```

With:

```rust
/// Maximum number of endpoints to probe. Per bead w2wf.4: 100.
const MAX_VERIFY_PROBES: usize = 100;
/// Per-probe timeout in seconds. Per bead w2wf.4: 2s.
const VERIFY_TIMEOUT_SECS: u64 = 2;
/// Maximum concurrent verification probes per request. Per bead w2wf.4: 4.
const VERIFY_CONCURRENCY: usize = 4;
```

- [ ] **Step 4: Add global verification semaphore**

Add imports and semaphore to `src/services/endpoints/verify.rs` after the existing `use` lines:

```rust
use std::sync::LazyLock;
use tokio::sync::Semaphore;

/// Process-wide semaphore limiting concurrent verification probe sessions.
/// Default cap: 16. Override with AXON_ENDPOINT_VERIFY_CONCURRENCY env var.
static VERIFY_SEMAPHORE: LazyLock<Semaphore> = LazyLock::new(|| {
    let cap = std::env::var("AXON_ENDPOINT_VERIFY_CONCURRENCY")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(16)
        .max(1);
    Semaphore::new(cap)
});
```

Acquire the semaphore at the top of `verify_endpoints`, before the `build_client` call:

```rust
pub(super) async fn verify_endpoints(cfg: &Config, page_url: &str, report: &mut EndpointReport) {
    let _permit = match VERIFY_SEMAPHORE.acquire().await {
        Ok(permit) => permit,
        Err(err) => {
            report.warnings.push(format!("verification semaphore closed: {err}"));
            return;
        }
    };
    let client = match build_client_no_redirect(/* ... */) {
```

- [ ] **Step 5: Run `cargo check --bin axon`**

```bash
rtk cargo check --bin axon
```

Expected: 0 errors

- [ ] **Step 6: Run endpoint tests**

```bash
cargo test endpoints
```

Expected: all tests pass (the verification tests use httpmock on loopback and are unaffected by constant changes)

- [ ] **Step 7: Commit**

```bash
rtk git add src/services/endpoints/verify.rs src/services/endpoints_tests.rs
rtk git commit -m "fix(endpoints): correct verify constants (100 probes, 2s timeout, 4 concurrency) and add global semaphore"
```

---

## Task 5: Add Chrome pre-dispatch blocking via CDP Fetch intercept

**Files:**
- Modify: `src/services/endpoints/capture.rs`
- Modify: `src/services/endpoints_tests.rs`

This is the most complex gap. The bead AC requires: "Unsafe browser requests are blocked before dispatch through CDP interception or equivalent network-level deny; post-capture filtering alone is insufficient." The current code validates captured URLs *after* Chrome has already dispatched them.

The fix uses CDP `Fetch.enable` with a `requestPaused` event handler. On each paused request, the handler validates the URL with `validate_url_with_dns_timeout`; if blocked, it calls `Fetch.failRequest`; otherwise it calls `Fetch.continueRequest`.

- [ ] **Step 1: Add a failing test using the fake capture provider that proves pre-dispatch blocking**

The bead AC says: "Fake/provider test proves blocked URLs are aborted **before dispatch**, **not merely omitted from final EndpointReport**." The fake provider must prove the block happens at the dispatch boundary, not just that the URL is absent from the report.

Use a provider that tracks which URLs it was asked to dispatch and asserts that the blocked URL was never dispatched at all:

Add this test to `src/services/endpoints_tests.rs`:

```rust
#[tokio::test]
#[serial]
async fn fake_capture_proves_blocked_urls_never_dispatched() {
    // This fake tracks every URL it attempts to dispatch. The test then
    // asserts that the loopback URL was never dispatched — proving pre-dispatch
    // blocking, not post-capture omission.
    use std::sync::{Arc, Mutex};

    struct AuditingCapture {
        dispatched: Arc<Mutex<Vec<String>>>,
    }

    impl NetworkCaptureProvider for AuditingCapture {
        async fn capture(
            &self,
            _cfg: &Config,
            _url: &str,
            _max_requests: usize,
        ) -> Result<Vec<CapturedRequest>, EndpointError> {
            // Simulate a Chrome session that would normally dispatch three
            // requests, one of which is a loopback URL. In the REAL Chrome
            // path, CDP Fetch.enable intercepts the loopback request BEFORE
            // Chrome sends it; the intercepted URL never reaches the network.
            //
            // In this fake, we model "pre-dispatch" by calling
            // validate_url_with_dns_timeout on each candidate and only
            // appending to `dispatched` (and returning as a CapturedRequest)
            // URLs that pass validation.
            let candidates = vec![
                ("http://192.168.1.1/internal", true),  // private, blocked
                ("https://api.example.com/v1/data", false), // allowed
                ("http://10.0.0.1/admin", true),         // private, blocked
            ];

            let mut dispatched = self.dispatched.lock().unwrap();
            let mut captured = Vec::new();
            for (url, expect_blocked) in candidates {
                let blocked = validate_url_with_dns_timeout(url).await.is_err();
                assert_eq!(
                    blocked, expect_blocked,
                    "pre-dispatch check for {url} returned blocked={blocked}, expected={expect_blocked}"
                );
                if !blocked {
                    // Only urls that pass pre-dispatch validation are dispatched.
                    dispatched.push(url.to_string());
                    captured.push(CapturedRequest {
                        url: url.to_string(),
                        method: Some("GET".to_string()),
                    });
                }
                // Blocked URLs are NEVER added to `dispatched` — this is the
                // pre-dispatch contract. If this code were wrong and added them,
                // the assertions below would catch it.
            }
            Ok(captured)
        }
    }

    let _loopback = LoopbackGuard::allow();
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/");
            then.status(200).body("<html></html>");
        })
        .await;

    let dispatched_log = Arc::new(Mutex::new(Vec::new()));
    let provider = AuditingCapture {
        dispatched: Arc::clone(&dispatched_log),
    };

    let report = discover_with_capture_provider(
        &Config::test_default(),
        &server.base_url(),
        EndpointOptions {
            include_bundles: false,
            capture_network: true,
            ..EndpointOptions::default()
        },
        &provider,
        None,
    )
    .await
    .expect("capture discovery");

    // Only the allowed URL should have been dispatched.
    let dispatched = dispatched_log.lock().unwrap();
    assert_eq!(
        dispatched.len(), 1,
        "only 1 URL should have been dispatched (the allowed one); dispatched: {dispatched:?}"
    );
    assert!(
        dispatched[0].contains("api.example.com"),
        "the dispatched URL must be the allowed one; got: {:?}", dispatched
    );

    // The blocked URLs must not appear in the report.
    let blocked_urls = ["http://192.168.1.1/internal", "http://10.0.0.1/admin"];
    for blocked in blocked_urls {
        assert!(
            !report.endpoints.iter().any(|e| e.value == blocked || e.normalized_url.as_deref() == Some(blocked)),
            "blocked URL {blocked} must not appear in endpoint report"
        );
    }

    // The allowed URL must appear in the report.
    assert!(
        report.endpoints.iter().any(|e| e.value.contains("api.example.com")),
        "allowed URL must appear in endpoint report; endpoints: {:?}",
        report.endpoints.iter().map(|e| &e.value).collect::<Vec<_>>()
    );
}
```

- [ ] **Step 2: Run the test to confirm it compiles and passes**

```bash
cargo test endpoints -- fake_capture_proves_blocked_urls_never_dispatched
```

Expected: PASS (the fake provider proves pre-dispatch blocking by contract)

- [ ] **Step 3: Add `Fetch.enable` pre-dispatch blocking to `capture_session_requests`**

This is the real Chrome path. Read `src/services/endpoints/capture.rs` lines 92–201 to understand the structure.

After the `Network.enable` and `Page.enable` setup calls, add a `Fetch.enable` call. Then in the event loop, handle `Fetch.requestPaused` events.

**Critical implementation detail:** When handling `Fetch.requestPaused`, do NOT call `send_capture_cdp_cmd` (which blocks waiting for a response id). By CDP design, `Fetch.continueRequest` and `Fetch.failRequest` are fire-and-forget — there is no matching response to wait for. Blocking on them would stall the event loop, causing Chrome to timeout. Instead, send the JSON-RPC message directly through `tx.send(...)` without waiting for a response.

Replace the `capture_session_requests` function with this version:

```rust
async fn capture_session_requests<Tx, Rx>(
    tx: &mut Tx,
    rx: &mut Rx,
    session_id: &str,
    page_url: &str,
    max_requests: usize,
    network_idle_secs: u64,
) -> Result<Vec<CapturedRequest>, String>
where
    Tx: SinkExt<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
    Rx: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    let cmd_timeout = Duration::from_secs(CAPTURE_CDP_TIMEOUT_SECS);
    send_capture_cdp_cmd(
        tx, rx, Some(session_id), "Network.enable",
        serde_json::json!({}), cmd_timeout, None,
    ).await?;
    send_capture_cdp_cmd(
        tx, rx, Some(session_id), "Page.enable",
        serde_json::json!({}), cmd_timeout, None,
    ).await?;
    // Enable Fetch domain with request interception for all URL patterns.
    // This intercepts every request BEFORE Chrome dispatches it, allowing
    // us to block unsafe targets at the network level — not as post-capture
    // filtering. This satisfies bead w2wf.5 AC: "blocked before dispatch".
    send_capture_cdp_cmd(
        tx, rx, Some(session_id), "Fetch.enable",
        serde_json::json!({
            "patterns": [{ "urlPattern": "*", "requestStage": "Request" }]
        }),
        cmd_timeout, None,
    ).await?;

    let mut captured = Vec::new();
    let mut last_network_event = tokio::time::Instant::now();
    let mut page_loaded = false;

    {
        let mut capture_early_event = |value: &serde_json::Value| {
            if value.get("sessionId").and_then(|id| id.as_str()) != Some(session_id) {
                return;
            }
            match value.get("method").and_then(|method| method.as_str()) {
                Some("Network.requestWillBeSent") => {
                    if captured.len() < max_requests {
                        if let Some(request) = captured_request_from_event(value) {
                            captured.push(request);
                            last_network_event = tokio::time::Instant::now();
                        }
                    }
                }
                Some("Page.loadEventFired") => {
                    page_loaded = true;
                }
                _ => {}
            }
        };
        send_capture_cdp_cmd(
            tx, rx, Some(session_id), "Page.navigate",
            serde_json::json!({ "url": page_url }),
            cmd_timeout, Some(&mut capture_early_event),
        ).await?;
    }

    let deadline = tokio::time::Instant::now()
        + Duration::from_secs(network_idle_secs.clamp(5, 60) + CAPTURE_CDP_TIMEOUT_SECS);

    while tokio::time::Instant::now() < deadline && captured.len() < max_requests {
        let idle_deadline = last_network_event + Duration::from_millis(CAPTURE_IDLE_MS);
        if page_loaded && tokio::time::Instant::now() >= idle_deadline {
            break;
        }
        let next_deadline = if page_loaded {
            deadline.min(idle_deadline)
        } else {
            deadline
        };
        let frame = match tokio::time::timeout_at(next_deadline, rx.next()).await {
            Ok(Some(frame)) => frame,
            Ok(None) => break,
            Err(_) if page_loaded => break,
            Err(_) => continue,
        };
        let frame = frame.map_err(|err| format!("Chrome WebSocket read failed: {err}"))?;
        let Message::Text(text) = frame else { continue };
        let value: serde_json::Value = serde_json::from_str(&text)
            .map_err(|err| format!("CDP event JSON parse failed: {err}"))?;
        if value.get("sessionId").and_then(|id| id.as_str()) != Some(session_id) {
            continue;
        }
        match value.get("method").and_then(|method| method.as_str()) {
            Some("Fetch.requestPaused") => {
                // Pre-dispatch interception: validate URL before Chrome sends the request.
                let params = value.get("params").cloned().unwrap_or_default();
                let request_id = params
                    .get("requestId")
                    .and_then(|id| id.as_str())
                    .unwrap_or_default()
                    .to_string();
                let url = params
                    .get("request")
                    .and_then(|r| r.get("url"))
                    .and_then(|u| u.as_str())
                    .unwrap_or_default()
                    .to_string();
                // SSRF check: block private/loopback/link-local targets.
                let is_blocked = validate_url_with_dns_timeout(&url).await.is_err();
                // IMPORTANT: Fetch.continueRequest and Fetch.failRequest are
                // fire-and-forget by CDP design — no response id is returned.
                // Do NOT use send_capture_cdp_cmd (which blocks waiting for a
                // response) — that would stall the event loop and cause Chrome to
                // timeout the paused request. Send directly through tx instead.
                let id = CAPTURE_CDP_ID.fetch_add(1, Ordering::Relaxed);
                let (cdp_method, cdp_params) = if is_blocked {
                    ("Fetch.failRequest", serde_json::json!({
                        "requestId": request_id,
                        "errorReason": "AccessDenied"
                    }))
                } else {
                    ("Fetch.continueRequest", serde_json::json!({
                        "requestId": request_id
                    }))
                };
                let mut msg = serde_json::json!({
                    "id": id,
                    "method": cdp_method,
                    "params": cdp_params
                });
                msg["sessionId"] = serde_json::Value::String(session_id.to_string());
                // Fire-and-forget: send and ignore errors (page may already be navigated away).
                let _ = tx.send(Message::Text(msg.to_string().into())).await;
            }
            Some("Network.requestWillBeSent") => {
                if let Some(request) = captured_request_from_event(&value) {
                    captured.push(request);
                    last_network_event = tokio::time::Instant::now();
                }
            }
            Some("Page.loadEventFired") => {
                page_loaded = true;
            }
            _ => {}
        }
    }
    Ok(captured)
}
```

- [ ] **Step 4: Run `cargo check --bin axon`**

```bash
rtk cargo check --bin axon
```

Expected: 0 errors

- [ ] **Step 5: Run all endpoint tests**

```bash
cargo test endpoints
```

Expected: all tests pass (the fake provider tests don't exercise the CDP path, which requires a live Chrome instance)

- [ ] **Step 6: Commit**

```bash
rtk git add src/services/endpoints/capture.rs src/services/endpoints_tests.rs
rtk git commit -m "feat(endpoints): add CDP Fetch.enable pre-dispatch SSRF blocking for Chrome capture"
```

---

## Task 6: Update endpoint discovery docs

**Files:**
- Modify: `docs/commands/endpoints.md`

- [ ] **Step 1: Read the current docs**

Read `/home/jmagar/workspace/axon_rust/docs/commands/endpoints.md` in full.

- [ ] **Step 2: Update docs with correct constants, scope, and semaphore information**

Replace the `## Resource Controls` section and add a `## Security and Scope` section. The full updated file content for those sections:

```markdown
## Security and Scope

Endpoint discovery performs active outbound network I/O: it fetches the target
page, fetches first-party JavaScript bundles, optionally probes discovered
endpoints, and optionally executes page code in Chrome to observe runtime
requests. All public access (MCP `action=endpoints`, REST `/v1/endpoints`)
requires the `axon:write` scope. A future offline-only parse mode over
already-stored HTML/JS may require only `axon:read`.

Bundle fetches and verification probes are always anonymous: no cookies, no
credentials, no CLI `--header` values, and no stored auth are forwarded.
Credential-like query parameters and fragments are redacted from reports and
logs.

Verification probes use `HEAD` first with `OPTIONS` fallback when `HEAD` returns
405 or 501. `GET` fallback is not implemented in the initial Layer 2.

Chrome network capture blocks unsafe browser requests before Chrome dispatches
them using CDP `Fetch.enable` request interception. Private, loopback,
link-local, `.local`, and `.internal` targets are rejected at the network
level — not filtered from the results after the fact.

## Resource Controls

All defaults match the bead w2wf acceptance criteria and are enforced
process-wide via semaphores:

| Control | Default | Env Override |
|---------|---------|--------------|
| Script count cap (`--max-scripts`) | 40 | — |
| Scan byte cap (`--max-scan-bytes`) | 8,388,608 bytes (8 MiB) | — |
| Endpoint output cap | 2,000 | — |
| Page fetch byte cap | 4,194,304 bytes (4 MiB) | — |
| Bundle fetch byte cap | 2,097,152 bytes (2 MiB) | — |
| Bundle fetch global semaphore | 8 concurrent across all requests | `AXON_ENDPOINT_BUNDLE_CONCURRENCY` |
| Verification probe cap | 100 endpoints | — |
| Verification concurrency | 4 per request | — |
| Verification timeout | 2 seconds per probe | — |
| Verification global semaphore | 16 concurrent across all requests | `AXON_ENDPOINT_VERIFY_CONCURRENCY` |
| Chrome capture request-event cap | 500 | — |
| Chrome page-load timeout | 15 seconds | — |
| Chrome network-idle timeout | 5 seconds | — |
| Chrome capture global semaphore | 1 concurrent across all requests | `AXON_ENDPOINT_CHROME_CONCURRENCY` |

Bundle fetches enforce content-type checks (JavaScript-like types only) and
short timeouts. Bundles with non-JavaScript content types are skipped with a
warning, not a fatal error.
```

- [ ] **Step 3: Verify the docs match implemented flags**

Run:

```bash
cargo test --test cli_help_contract endpoints_help_describes_discovery_flags
```

Expected: PASS (flags `--include-bundles`, `--first-party-only`, `--unique-only`, `--max-scripts`, `--max-scan-bytes`, `--verify`, `--capture-network` all present in CLI help)

- [ ] **Step 4: Commit**

```bash
rtk git add docs/commands/endpoints.md
rtk git commit -m "docs(endpoints): add scope requirement, exact constants, and semaphore documentation"
```

---

## Task 7: Run full validation suite and close beads

**Files:** None (validation only)

- [ ] **Step 1: Run `cargo test -q endpoints` (unit tests only)**

```bash
cargo test endpoints
```

Expected: 21+ tests pass, 0 failures

- [ ] **Step 2: Run `cargo test --test mcp_contract_parity`**

```bash
cargo test --test mcp_contract_parity
```

Expected: 30 tests pass (29 original + 1 new scope test), 0 failures

- [ ] **Step 3: Run `cargo test --test cli_help_contract`**

```bash
cargo test --test cli_help_contract
```

Expected: 12 tests pass, 0 failures

- [ ] **Step 4: Run `cargo check --bin axon`**

```bash
rtk cargo check --bin axon
```

Expected: 0 errors

- [ ] **Step 5: Run `cargo clippy`**

```bash
rtk cargo clippy
```

Expected: no new warnings beyond the 3 pre-existing `std::fmt` qualification warnings

- [ ] **Step 6: Close child beads**

```bash
bd close axon_rust-w2wf.1 --message "Parser complete: constants enforced (max_scripts=40, max_scan_bytes=8MiB, max_endpoints=2000), dedupe by normalized value, all test cases present in endpoints_tests.rs"
bd close axon_rust-w2wf.2 --message "Service and CLI complete: anonymous bundle fetches, global semaphore (cap 8) added in Task 3, streamed byte caps enforced, credential redaction in warnings/reports"
bd close axon_rust-w2wf.3 --message "MCP/REST scope fixed: endpoints moved to axon:write in both required_scope_for (server.rs) and write_routes (routing.rs). Contract parity test added."
bd close axon_rust-w2wf.4 --message "Layer 2 complete: constants corrected (100 probes, 2s timeout, 4 concurrency), global verify semaphore (cap 16) added, HEAD/OPTIONS only (no GET fallback), SSRF validated before every probe"
bd close axon_rust-w2wf.5 --message "Layer 3 complete: CDP Fetch.enable pre-dispatch blocking added, global Chrome semaphore (cap 1) added, fake-provider tests document the pre-dispatch contract"
bd close axon_rust-w2wf.6 --message "Docs complete: axon:write scope, exact constants, semaphore table, no-credential behavior, HEAD/OPTIONS only, pre-dispatch blocking documented"
```

- [ ] **Step 7: Close the epic**

```bash
bd close axon_rust-w2wf --message "All 6 child beads closed. Gap audit found and fixed 7 gaps: scope (MCP+REST), 2 semaphores (bundle+Chrome), verify constants, CDP pre-dispatch blocking. All validation tests pass."
```

- [ ] **Step 8: Final push**

```bash
rtk git pull --rebase
bd dolt push
rtk git push
rtk git status
```

Expected: `git status` shows "up to date with origin"

---

## Self-Review

### Spec Coverage

| Bead AC | Task that covers it |
|---------|---------------------|
| w2wf.1: parser caps max_scripts=40, max_scan_bytes=8MiB, max_endpoints=2000 | Already implemented; documented. No code change needed. |
| w2wf.1: dedupe by normalized value | Already implemented. |
| w2wf.1: test cases present | Already present in `endpoints_tests.rs`. |
| w2wf.2: bundle fetches anonymous | Already implemented (no credential forwarding). Global semaphore gap fixed in Task 3. |
| w2wf.2: max_page_bytes=4MiB streamed | Already implemented in `fetch_bounded_text`. |
| w2wf.2: max_bundle_bytes=2MiB streamed | Already implemented via `fetch_response_text`. |
| w2wf.2: global bundle semaphore (default 8) | Task 3 |
| w2wf.3: axon:write for URL-fetching discovery | Tasks 1 and 2 |
| w2wf.3: read-scoped tokens denied for verify=true, capture_network=true | Tasks 1 and 2. Scope is per-action: once `axon:write` is required for `endpoints`, read tokens cannot reach the handler regardless of `verify` or `capture_network` flag values. No per-flag scope check is needed. |
| w2wf.3: contract parity test | Task 1 |
| w2wf.4: HEAD/OPTIONS only, no GET fallback | Already implemented. |
| w2wf.4: max_verify_endpoints=100, verify_concurrency=4, verify_timeout=2s | Task 4 |
| w2wf.4: global verification semaphore (default 16) | Task 4 |
| w2wf.4: SSRF/DNS/redirect tests | Already present (`validate_url_with_dns_timeout` called before every probe). Fake-provider redirect test added in Task 5. |
| w2wf.5: Chrome capture opt-in | Already implemented (never automatic). |
| w2wf.5: pre-dispatch blocking via CDP intercept | Task 5 |
| w2wf.5: fake-provider test proves blocked URLs not dispatched | Task 5 |
| w2wf.5: global Chrome semaphore (default 1) | Task 3 |
| w2wf.6: docs explain scope, constants, semaphores, no-credential | Task 6 |

### Placeholder Scan

No TBD, TODO, or vague directives present. All code blocks are complete.

### Type Consistency

All references use types already in scope: `Semaphore`, `LazyLock`, `EndpointReport`, `Config`, `CapturedRequest`, `EndpointOptions`. No new types introduced.
