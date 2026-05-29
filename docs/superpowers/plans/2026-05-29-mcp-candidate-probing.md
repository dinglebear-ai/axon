# MCP Candidate Probing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** When `--probe-rpc` is set, axon synthesizes well-known MCP candidate URLs (`/mcp`, `/api/mcp` on the seed host, and optionally `mcp.<apex>` under a new `--probe-rpc-subdomains` flag), probes them with the existing MCP/JSON-RPC handshake, and reports confirmed servers — across CLI, MCP, and the web `/v1/endpoints` API.

**Architecture:** All synthesis/probe logic lives in the services layer (`src/services/endpoints/`), so CLI/MCP/web share identical behavior. A new `candidates.rs` module holds apex derivation + candidate enumeration + the synthesize-and-probe driver; `probe.rs` gains a strict (positive-signal-only) probe entry. The initial page fetch becomes non-fatal under `--probe-rpc` so bare MCP endpoints (which serve no HTML) still get probed.

**Tech Stack:** Rust, tokio, reqwest, `psl` crate (new — Public Suffix List, compiled in), `httpmock` + `serial_test` (existing test deps).

**Spec:** `docs/superpowers/specs/2026-05-29-mcp-candidate-probing-design.md`

**Key reference facts (verified against the codebase):**
- `EndpointOptions`, `EndpointReport`, `EndpointSourceKind`, `RpcProbeResult`, `RpcProtocol`, `RpcTransport` live in `src/services/types/endpoints.rs`; re-exported via `pub use endpoints::*` in `src/services/types.rs` (glob — new public types surface automatically).
- The only `EndpointReport { … }` struct literal is `src/core/content/endpoints.rs:172` (`new_endpoint_report`).
- `src/services/endpoints.rs:44` declares `mod probe;`. Submodules use `use super::{EndpointError, validate_url_with_dns_timeout};`.
- `probe_rpc_endpoints(cfg, &mut report)` is defined in `probe.rs` and called at the end of `discover_with_capture_provider` in `endpoints.rs`. It probes only existing `report.endpoints` via a `PROBE_SEMAPHORE` + `buffer_unordered` (NO `tokio::spawn`, so the `#[cfg(test)]` loopback bypass thread-local propagates).
- `probe_rpc` is **CLI-only today**: neither the MCP `EndpointsRequest` (`src/mcp/schema/requests.rs:285`) nor the web `EndpointsRequest` (`src/web/server/handlers/exploration.rs:23`) exposes it. This plan adds **both** `probe_rpc` and `probe_rpc_subdomains` to those structs for true parity.
- New-flag config mirror sites (follow the existing `endpoints_probe_rpc` chain exactly): `src/core/config/cli.rs:357`, `src/core/config/parse/build_config/command_dispatch.rs` (field/default/assign), `src/core/config/parse/build_config/config_literal.rs`, `src/core/config/types/config.rs:123`, `src/core/config/types/config_impls.rs` (default + Debug).
- Test convention: sidecar `_tests.rs` with `#[cfg(test)] #[path="…"] mod tests;`. Loopback SSRF bypass in tests via `LoopbackGuard::allow()` + `#[serial]` (see `src/services/endpoints/probe_tests.rs`).

---

## Task 1: New flag plumbed end-to-end (no-op) + new output types

**Files:**
- Modify: `Cargo.toml` (add `psl`)
- Modify: `src/services/types/endpoints.rs` (EndpointOptions field, EndpointSourceKind variant, new Mcp* types, EndpointReport field)
- Modify: `src/core/config/cli.rs`, `src/core/config/parse/build_config/command_dispatch.rs`, `src/core/config/parse/build_config/config_literal.rs`, `src/core/config/types/config.rs`, `src/core/config/types/config_impls.rs`
- Modify: `src/services/endpoints.rs` (`options_from_config`)
- Modify: `src/cli/commands/endpoints.rs` (options literal + print + no-op warn)
- Modify: `src/core/content/endpoints.rs:172` (add field to the one report literal)
- Test: `src/services/types/endpoints_tests.rs` (new sidecar)

- [ ] **Step 1: Add the `psl` dependency**

In `Cargo.toml`, under `[dependencies]`, add:

```toml
psl = "2"
```

- [ ] **Step 2: Write the failing test for the new output types' wire format**

Create `src/services/types/endpoints_tests.rs`:

```rust
use super::*;

#[test]
fn synthesized_mcp_source_kind_wire_string() {
    assert_eq!(EndpointSourceKind::SynthesizedMcp.as_str(), "synthesized_mcp");
    let json = serde_json::to_string(&EndpointSourceKind::SynthesizedMcp).unwrap();
    assert_eq!(json, "\"synthesized_mcp\"");
}

#[test]
fn mcp_candidate_attempt_roundtrips() {
    let attempt = McpCandidateAttempt {
        url: "https://mcp.foo.com/mcp".to_string(),
        host_kind: McpHostKind::ApexSubdomain,
        path: "/mcp".to_string(),
        outcome: McpProbeOutcome::Confirmed,
        rpc_probe: None,
    };
    let json = serde_json::to_value(&attempt).unwrap();
    assert_eq!(json["host_kind"], "apex_subdomain");
    assert_eq!(json["outcome"], "confirmed");
    // rpc_probe is None → omitted
    assert!(json.get("rpc_probe").is_none());
}

#[test]
fn empty_mcp_candidates_omitted_from_report() {
    let report = EndpointReport {
        url: "https://x.test".to_string(),
        endpoints: Vec::new(),
        hosts: Vec::new(),
        scripts_discovered: 0,
        bundles_fetched: 0,
        bundles_scanned: 0,
        truncated: false,
        warnings: Vec::new(),
        elapsed_ms: 0,
        mcp_candidates: Vec::new(),
    };
    let json = serde_json::to_value(&report).unwrap();
    assert!(json.get("mcp_candidates").is_none());
}
```

At the bottom of `src/services/types/endpoints.rs` add:

```rust
#[cfg(test)]
#[path = "endpoints_tests.rs"]
mod tests;
```

- [ ] **Step 3: Run the test to verify it fails to compile**

Run: `cargo test -p axon --lib endpoints_tests 2>&1 | tail -20`
Expected: FAIL — `McpCandidateAttempt`, `McpHostKind`, `McpProbeOutcome`, `EndpointSourceKind::SynthesizedMcp`, and `EndpointReport.mcp_candidates` do not exist yet.

- [ ] **Step 4: Add the new types and fields in `src/services/types/endpoints.rs`**

Add the `SynthesizedMcp` variant to `EndpointSourceKind` (after `NetworkCapture`) and its `as_str` arm:

```rust
pub enum EndpointSourceKind {
    InlineScript,
    ScriptBundle,
    HtmlAttribute,
    NetworkCapture,
    /// Not discovered in the page — synthesized from the target URL and
    /// confirmed by an RPC probe (well-known MCP path or `mcp.<apex>` host).
    SynthesizedMcp,
}
```

```rust
            Self::NetworkCapture => "network_capture",
            Self::SynthesizedMcp => "synthesized_mcp",
```

Add `probe_rpc_subdomains` to `EndpointOptions` (after `probe_rpc`) and to its `Default`:

```rust
    pub probe_rpc: bool,
    /// Additionally synthesize + probe `mcp.<registrable-apex>` candidates.
    /// No-op unless `probe_rpc` is also set.
    pub probe_rpc_subdomains: bool,
```

```rust
            probe_rpc: false,
            probe_rpc_subdomains: false,
```

Add the new types (place after `RpcProbeResult`, before `EndpointKind`):

```rust
/// Which host a synthesized MCP candidate targets.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum McpHostKind {
    /// Same host as the target URL (e.g. `foo.com/mcp`).
    SameHost,
    /// `mcp.<registrable-apex>` subdomain (e.g. `mcp.foo.com/mcp`).
    ApexSubdomain,
}

/// Outcome of probing one synthesized MCP candidate.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum McpProbeOutcome {
    /// Returned a positive JSON-RPC/MCP signal.
    Confirmed,
    /// Reachable validation passed but no positive RPC signal (404, non-RPC
    /// JSON, transport error, or timeout all fold here).
    Unconfirmed,
    /// Rejected by the SSRF guard before any request was made.
    Blocked,
}

/// One synthesized MCP candidate and its probe outcome.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct McpCandidateAttempt {
    pub url: String,
    pub host_kind: McpHostKind,
    pub path: String,
    pub outcome: McpProbeOutcome,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rpc_probe: Option<RpcProbeResult>,
}
```

Add the field to `EndpointReport` (after `elapsed_ms`):

```rust
    pub elapsed_ms: u64,
    /// Synthesized MCP candidate probe attempts (omitted when empty).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mcp_candidates: Vec<McpCandidateAttempt>,
```

- [ ] **Step 5: Fix the one `EndpointReport` struct literal**

In `src/core/content/endpoints.rs`, in `new_endpoint_report` (line ~172), add the field to the literal:

```rust
        warnings: Vec::new(),
        elapsed_ms: 0,
        mcp_candidates: Vec::new(),
    }
```

- [ ] **Step 6: Add the `--probe-rpc-subdomains` CLI arg**

In `src/core/config/cli.rs`, immediately after the `probe_rpc` field (line ~357):

```rust
    /// Also probe `mcp.<apex>` subdomain candidates for MCP/JSON-RPC. No-op without --probe-rpc.
    #[arg(long = "probe-rpc-subdomains", action = ArgAction::SetTrue)]
    pub(super) probe_rpc_subdomains: bool,
```

- [ ] **Step 7: Thread the field through the config build chain**

In `src/core/config/parse/build_config/command_dispatch.rs`: add the field beside `endpoints_probe_rpc` in the struct (line ~64), in its default init (line ~112), and in the assignment (line ~168):

```rust
    pub endpoints_probe_rpc: bool,
    pub endpoints_probe_rpc_subdomains: bool,
```
```rust
            endpoints_probe_rpc: false,
            endpoints_probe_rpc_subdomains: false,
```
```rust
            out.endpoints_probe_rpc = args.probe_rpc;
            out.endpoints_probe_rpc_subdomains = args.probe_rpc_subdomains;
```

In `src/core/config/parse/build_config/config_literal.rs` (after line ~95):

```rust
    cfg.endpoints_probe_rpc = inputs.dispatched.endpoints_probe_rpc;
    cfg.endpoints_probe_rpc_subdomains = inputs.dispatched.endpoints_probe_rpc_subdomains;
```

In `src/core/config/types/config.rs` (after line ~123):

```rust
    pub endpoints_probe_rpc: bool,
    pub endpoints_probe_rpc_subdomains: bool,
```

In `src/core/config/types/config_impls.rs`: default (after line ~49) and Debug field (after line ~303):

```rust
            endpoints_probe_rpc: false,
            endpoints_probe_rpc_subdomains: false,
```
```rust
            .field("endpoints_probe_rpc", &self.endpoints_probe_rpc)
            .field("endpoints_probe_rpc_subdomains", &self.endpoints_probe_rpc_subdomains)
```

- [ ] **Step 8: Set the option from config in both builders**

In `src/services/endpoints.rs` `options_from_config` (after `probe_rpc:` line ~231):

```rust
        probe_rpc: cfg.endpoints_probe_rpc,
        probe_rpc_subdomains: cfg.endpoints_probe_rpc_subdomains,
```

In `src/cli/commands/endpoints.rs` options literal (after `probe_rpc:` line ~23):

```rust
        probe_rpc: cfg.endpoints_probe_rpc,
        probe_rpc_subdomains: cfg.endpoints_probe_rpc_subdomains,
```

Add a no-op warning + print line in `run_endpoints`. After building `options` (line ~24), before the `if !cfg.json_output` block:

```rust
    if options.probe_rpc_subdomains && !options.probe_rpc {
        crate::core::logging::log_warn(
            "--probe-rpc-subdomains has no effect without --probe-rpc",
        );
    }
```

And in the `if !cfg.json_output` options print block, after the `probeRpc` line (line ~33):

```rust
        print_option("probeRpc", &options.probe_rpc.to_string());
        print_option("probeRpcSubdomains", &options.probe_rpc_subdomains.to_string());
```

> Note: confirm `log_warn` is exported from `crate::core::logging` (it is used elsewhere per CLAUDE.md). If the import path differs, match the existing `log_done`/`log_info` import in this file.

- [ ] **Step 9: Run the type test + full check**

Run: `cargo test -p axon --lib endpoints_tests 2>&1 | tail -20`
Expected: PASS (3 tests).
Run: `rtk cargo check 2>&1 | tail -20`
Expected: clean compile (flag is parsed and plumbed but not yet consumed).

- [ ] **Step 10: Commit**

```bash
git add -A
git commit -m "feat(endpoints): add --probe-rpc-subdomains flag + MCP candidate output types"
```

---

## Task 2: Apex derivation + candidate enumeration (pure logic)

**Files:**
- Create: `src/services/endpoints/candidates.rs`
- Create: `src/services/endpoints/candidates_tests.rs`
- Modify: `src/services/endpoints.rs` (add `mod candidates;`)

- [ ] **Step 1: Write the failing unit tests**

Create `src/services/endpoints/candidates_tests.rs`:

```rust
use super::*;

#[test]
fn apex_simple_com() {
    assert_eq!(registrable_apex("foo.com").as_deref(), Some("foo.com"));
    assert_eq!(registrable_apex("docs.foo.com").as_deref(), Some("foo.com"));
}

#[test]
fn apex_multi_part_tld() {
    assert_eq!(registrable_apex("docs.foo.co.uk").as_deref(), Some("foo.co.uk"));
    assert_eq!(registrable_apex("a.b.foo.com.au").as_deref(), Some("foo.com.au"));
}

#[test]
fn apex_rejects_ip_and_unknown() {
    assert_eq!(registrable_apex("127.0.0.1"), None);
    assert_eq!(registrable_apex("[::1]"), None);
    assert_eq!(registrable_apex("localhost"), None);
}

#[test]
fn candidates_same_host_only() {
    let c = mcp_candidate_urls("https://docs.foo.com/bar", false);
    let urls: Vec<&str> = c.iter().map(|x| x.url.as_str()).collect();
    assert_eq!(
        urls,
        vec!["https://docs.foo.com/mcp", "https://docs.foo.com/api/mcp"]
    );
    assert!(c.iter().all(|x| x.host_kind == McpHostKind::SameHost));
}

#[test]
fn candidates_with_subdomain() {
    let c = mcp_candidate_urls("https://docs.foo.com/bar", true);
    let urls: Vec<&str> = c.iter().map(|x| x.url.as_str()).collect();
    assert_eq!(
        urls,
        vec![
            "https://docs.foo.com/mcp",
            "https://docs.foo.com/api/mcp",
            "https://mcp.foo.com/mcp",
            "https://mcp.foo.com/api/mcp",
        ]
    );
}

#[test]
fn candidates_collapse_when_host_is_mcp() {
    // Target host already mcp.* → subdomain set duplicates same-host, so skipped.
    let c = mcp_candidate_urls("https://mcp.foo.com/x", true);
    let urls: Vec<&str> = c.iter().map(|x| x.url.as_str()).collect();
    assert_eq!(
        urls,
        vec!["https://mcp.foo.com/mcp", "https://mcp.foo.com/api/mcp"]
    );
}

#[test]
fn candidates_skip_subdomain_for_ip() {
    let c = mcp_candidate_urls("http://127.0.0.1:9000/x", true);
    // same-host candidates still produced (will be SSRF-blocked later), no subdomain
    assert!(c.iter().all(|x| x.host_kind == McpHostKind::SameHost));
    assert_eq!(c.len(), 2);
}
```

- [ ] **Step 2: Create `candidates.rs` with the pure logic**

Create `src/services/endpoints/candidates.rs`:

```rust
use crate::services::types::McpHostKind;
use url::Url;

/// Well-known MCP paths probed on each candidate host.
const MCP_PATHS: [&str; 2] = ["/mcp", "/api/mcp"];

/// One synthesized MCP candidate URL.
pub(super) struct Candidate {
    pub host_kind: McpHostKind,
    pub path: &'static str,
    pub url: String,
}

/// Registrable apex (eTLD+1) for `host` via the Public Suffix List.
/// Returns `None` for raw IPs, single-label hosts, and unknown suffixes.
pub(super) fn registrable_apex(host: &str) -> Option<String> {
    let bare = host.trim_start_matches('[').trim_end_matches(']');
    if bare.parse::<std::net::IpAddr>().is_ok() {
        return None;
    }
    psl::domain_str(host).map(|d| d.to_string())
}

/// Host[:port] authority of a parsed URL, lowercased host.
fn authority(url: &Url) -> Option<String> {
    let host = url.host_str()?.to_ascii_lowercase();
    Some(match url.port() {
        Some(p) => format!("{host}:{p}"),
        None => host,
    })
}

/// Synthesize MCP candidate URLs from `target`.
///
/// Same-host candidates always use the target's scheme + authority. Subdomain
/// candidates (`mcp.<apex>`, https) are added only when `include_subdomain` and
/// an apex resolves, and are skipped when the seed host is already `mcp.*`
/// (they would duplicate the same-host set).
pub(super) fn mcp_candidate_urls(target: &str, include_subdomain: bool) -> Vec<Candidate> {
    let mut out = Vec::new();
    let Ok(url) = Url::parse(target) else {
        return out;
    };
    let scheme = url.scheme();
    let Some(auth) = authority(&url) else {
        return out;
    };
    for path in MCP_PATHS {
        out.push(Candidate {
            host_kind: McpHostKind::SameHost,
            path,
            url: format!("{scheme}://{auth}{path}"),
        });
    }

    if include_subdomain {
        if let Some(host) = url.host_str().map(|h| h.to_ascii_lowercase()) {
            if !host.starts_with("mcp.") {
                if let Some(apex) = registrable_apex(&host) {
                    for path in MCP_PATHS {
                        out.push(Candidate {
                            host_kind: McpHostKind::ApexSubdomain,
                            path,
                            url: format!("https://mcp.{apex}{path}"),
                        });
                    }
                }
            }
        }
    }
    out
}

#[cfg(test)]
#[path = "candidates_tests.rs"]
mod tests;
```

- [ ] **Step 3: Declare the module**

In `src/services/endpoints.rs`, after `mod probe;` (line ~44):

```rust
mod candidates;
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p axon --lib candidates 2>&1 | tail -25`
Expected: PASS (7 tests). If `psl::domain_str` is the wrong API name, fix `registrable_apex` only — the tests pin the required behavior.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(endpoints): synthesize MCP candidate URLs with PSL apex derivation"
```

---

## Task 3: Strict (positive-signal-only) probe entry

**Files:**
- Modify: `src/services/endpoints/probe.rs` (add `probe_candidate`)
- Modify: `src/services/endpoints/probe_tests.rs` (add tests)

- [ ] **Step 1: Write the failing tests**

Append to `src/services/endpoints/probe_tests.rs`:

```rust
#[tokio::test]
#[serial]
async fn probe_candidate_confirms_mcp() {
    let _loopback = LoopbackGuard::allow();
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(POST).path("/mcp").body_includes("initialize");
            then.status(200)
                .header("content-type", "application/json")
                .json_body(serde_json::json!({
                    "jsonrpc": "2.0", "id": 1,
                    "result": { "serverInfo": { "name": "demo", "version": "1" }, "capabilities": {} }
                }));
        })
        .await;
    let result = probe_candidate(&probe_client(), &server.url("/mcp")).await;
    assert!(matches!(result, Some(r) if r.protocol == Some(RpcProtocol::Mcp)));
}

#[tokio::test]
#[serial]
async fn probe_candidate_rejects_bare_sse_stream() {
    // A GET-only text/event-stream endpoint must NOT confirm via the strict path
    // (the weak SSE content-type fallback is intentionally excluded).
    let _loopback = LoopbackGuard::allow();
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/sse");
            then.status(200).header("content-type", "text/event-stream").body("data: hi\n\n");
        })
        .await;
    // POSTs return 404 (no mock) → no positive signal.
    let result = probe_candidate(&probe_client(), &server.url("/sse")).await;
    assert!(result.is_none());
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p axon --lib probe_candidate 2>&1 | tail -15`
Expected: FAIL — `probe_candidate` not found.

- [ ] **Step 3: Implement `probe_candidate` in `probe.rs`**

Add after `probe_one` (around line 121). It acquires the shared semaphore, then runs only the positive-signal POST probes — no `probe_sse_transport`:

```rust
/// Strict probe for synthesized candidates: positive-signal POST probes only,
/// no bare-SSE content-type fallback (too false-positive-prone for guesses).
/// Streamable-HTTP MCP is still detected because `probe_mcp`'s `initialize`
/// response (incl. `text/event-stream` bodies) is parsed inside `send_jsonrpc`.
///
/// Callers MUST validate the URL through the SSRF guard first; this acquires the
/// process-wide probe semaphore and issues requests unconditionally.
pub(super) async fn probe_candidate(client: &reqwest::Client, url: &str) -> Option<RpcProbeResult> {
    let _permit = PROBE_SEMAPHORE.acquire().await.ok()?;
    if let Some(r) = probe_mcp(client, url).await {
        return Some(r);
    }
    if let Some(r) = probe_openrpc(client, url).await {
        return Some(r);
    }
    if let Some(r) = probe_list_methods(client, url).await {
        return Some(r);
    }
    probe_jsonrpc_fingerprint(client, url).await
}
```

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p axon --lib probe_candidate 2>&1 | tail -15`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(endpoints): add strict positive-signal probe entry for synthesized candidates"
```

---

## Task 4: Synthesize-and-probe driver + wire into the probe step

**Files:**
- Modify: `src/services/endpoints/candidates.rs` (add `synthesize_and_probe_mcp`)
- Modify: `src/services/endpoints/probe.rs` (signature change + call synthesis)
- Modify: `src/services/endpoints.rs` (`recompute_hosts` visibility + updated call site)
- Modify: `src/services/endpoints/candidates_tests.rs` (integration tests)

- [ ] **Step 1: Write the failing integration tests**

Append to `src/services/endpoints/candidates_tests.rs`:

```rust
use crate::core::config::Config;
use crate::core::http::{get_allow_loopback, set_allow_loopback};
use crate::services::types::{EndpointReport, EndpointSourceKind, McpProbeOutcome};
use httpmock::prelude::*;
use serial_test::serial;

struct LoopbackGuard {
    previous: bool,
}
impl LoopbackGuard {
    fn allow() -> Self {
        let previous = get_allow_loopback();
        set_allow_loopback(true);
        Self { previous }
    }
}
impl Drop for LoopbackGuard {
    fn drop(&mut self) {
        set_allow_loopback(self.previous);
    }
}

fn empty_report(url: &str) -> EndpointReport {
    EndpointReport {
        url: url.to_string(),
        endpoints: Vec::new(),
        hosts: Vec::new(),
        scripts_discovered: 0,
        bundles_fetched: 0,
        bundles_scanned: 0,
        truncated: false,
        warnings: Vec::new(),
        elapsed_ms: 0,
        mcp_candidates: Vec::new(),
    }
}

#[tokio::test]
#[serial]
async fn synthesized_same_host_mcp_confirms() {
    let _loopback = LoopbackGuard::allow();
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(POST).path("/mcp").body_includes("initialize");
            then.status(200).header("content-type", "application/json").json_body(serde_json::json!({
                "jsonrpc": "2.0", "id": 1,
                "result": { "serverInfo": { "name": "demo", "version": "1" }, "capabilities": {} }
            }));
        })
        .await;
    let client = crate::core::http::build_client(3, Some(crate::core::http::axon_ua())).unwrap();
    let mut report = empty_report(&server.url("/x"));

    synthesize_and_probe_mcp(&client, &server.url("/x"), false, &mut report).await;

    // /mcp confirmed, /api/mcp unconfirmed (404).
    assert_eq!(report.mcp_candidates.len(), 2);
    let confirmed: Vec<_> = report
        .mcp_candidates
        .iter()
        .filter(|a| a.outcome == McpProbeOutcome::Confirmed)
        .collect();
    assert_eq!(confirmed.len(), 1);
    assert!(confirmed[0].url.ends_with("/mcp"));
    // Confirmed candidate added to endpoints as synthesized_mcp.
    assert!(report
        .endpoints
        .iter()
        .any(|e| e.source == EndpointSourceKind::SynthesizedMcp && e.first_party));
}

#[tokio::test]
#[serial]
async fn synthesized_candidate_blocked_when_loopback_disallowed() {
    // No LoopbackGuard → SSRF guard blocks 127.0.0.1.
    set_allow_loopback(false);
    let client = crate::core::http::build_client(3, Some(crate::core::http::axon_ua())).unwrap();
    let mut report = empty_report("http://127.0.0.1:9/x");

    synthesize_and_probe_mcp(&client, "http://127.0.0.1:9/x", false, &mut report).await;

    assert!(!report.mcp_candidates.is_empty());
    assert!(report
        .mcp_candidates
        .iter()
        .all(|a| a.outcome == McpProbeOutcome::Blocked));
    assert!(report.endpoints.is_empty());
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p axon --lib synthesized 2>&1 | tail -20`
Expected: FAIL — `synthesize_and_probe_mcp` not found.

- [ ] **Step 3: Implement `synthesize_and_probe_mcp` in `candidates.rs`**

Add these imports at the top of `candidates.rs`:

```rust
use super::{probe, validate_url_with_dns_timeout};
use crate::services::types::{
    DiscoveredEndpoint, EndpointKind, EndpointReport, EndpointSourceKind, McpCandidateAttempt,
    McpHostKind, McpProbeOutcome,
};
use futures_util::{StreamExt, stream};
```

(Keep the existing `use crate::services::types::McpHostKind;` line merged into the block above; remove the duplicate.)

Add the driver:

```rust
/// Concurrency for synthesized-candidate probing (small fixed set; mirrors the
/// discovered-endpoint probe concurrency).
const SYNTH_PROBE_CONCURRENCY: usize = 4;

/// Synthesize MCP candidates from `target`, SSRF-validate, probe each with the
/// strict probe, append confirmed ones to `report.endpoints`, and record every
/// attempt in `report.mcp_candidates`.
///
/// Uses `buffer_unordered` (NOT `tokio::spawn`) so the `#[cfg(test)]` loopback
/// bypass thread-local propagates correctly.
pub(super) async fn synthesize_and_probe_mcp(
    client: &reqwest::Client,
    target: &str,
    include_subdomain: bool,
    report: &mut EndpointReport,
) {
    let candidates = mcp_candidate_urls(target, include_subdomain);
    // Dedup against already-discovered endpoints — those are probed by the
    // normal path; never double-probe.
    let candidates: Vec<Candidate> = candidates
        .into_iter()
        .filter(|c| {
            !report.endpoints.iter().any(|e| {
                e.normalized_url.as_deref() == Some(c.url.as_str()) || e.value == c.url
            })
        })
        .collect();

    let attempts: Vec<(Candidate, McpProbeOutcome, Option<crate::services::types::RpcProbeResult>)> =
        stream::iter(candidates)
            .map(|c| {
                let client = client.clone();
                async move {
                    if validate_url_with_dns_timeout(&c.url).await.is_err() {
                        return (c, McpProbeOutcome::Blocked, None);
                    }
                    match probe::probe_candidate(&client, &c.url).await {
                        Some(rpc) => (c, McpProbeOutcome::Confirmed, Some(rpc)),
                        None => (c, McpProbeOutcome::Unconfirmed, None),
                    }
                }
            })
            .buffer_unordered(SYNTH_PROBE_CONCURRENCY)
            .collect()
            .await;

    for (c, outcome, rpc) in attempts {
        if outcome == McpProbeOutcome::Confirmed {
            if let Some(rpc) = rpc.clone() {
                report.endpoints.push(DiscoveredEndpoint {
                    value: c.url.clone(),
                    normalized_url: Some(c.url.clone()),
                    kind: EndpointKind::AbsoluteUrl,
                    // Same host or mcp.<apex-of-target> — same registrable org by
                    // construction.
                    first_party: true,
                    source: EndpointSourceKind::SynthesizedMcp,
                    source_url: Some(target.to_string()),
                    verified: None,
                    rpc_probe: Some(rpc),
                });
            }
        }
        report.mcp_candidates.push(McpCandidateAttempt {
            url: c.url,
            host_kind: c.host_kind,
            path: c.path.to_string(),
            outcome,
            rpc_probe: rpc,
        });
    }
}
```

- [ ] **Step 4: Make `recompute_hosts` reachable + change `probe_rpc_endpoints` signature**

In `src/services/endpoints.rs`, change `fn recompute_hosts` to `pub(super) fn recompute_hosts`.

In `src/services/endpoints/probe.rs`, change the signature and add the synthesis call. Add `EndpointSourceKind` to the `use crate::services::types::{…}` import. New signature + tail:

```rust
pub(super) async fn probe_rpc_endpoints(
    cfg: &Config,
    target_url: &str,
    include_subdomain: bool,
    report: &mut EndpointReport,
) {
    let client = match build_client(probe_timeout_secs(cfg), Some(axon_ua())) {
        Ok(c) => c,
        Err(err) => {
            report
                .warnings
                .push(format!("rpc probe client unavailable: {err}"));
            return;
        }
    };

    // ... existing discovered-endpoint probing loop unchanged ...

    // Synthesize + probe well-known MCP candidates from the target URL itself.
    super::candidates::synthesize_and_probe_mcp(&client, target_url, include_subdomain, report)
        .await;
    if report
        .endpoints
        .iter()
        .any(|e| e.source == crate::services::types::EndpointSourceKind::SynthesizedMcp)
    {
        super::recompute_hosts(report);
    }
}
```

- [ ] **Step 5: Update the call site in `endpoints.rs`**

In `discover_with_capture_provider`, update the probe call (currently `probe_rpc_endpoints(cfg, &mut report).await;`):

```rust
    if options.probe_rpc {
        emit_endpoint_log(&tx, "endpoint discovery probing RPC protocols").await;
        probe_rpc_endpoints(cfg, &normalized, options.probe_rpc_subdomains, &mut report).await;
    }
```

- [ ] **Step 6: Run to verify pass**

Run: `cargo test -p axon --lib synthesized 2>&1 | tail -25`
Expected: PASS (2 tests).
Run: `rtk cargo check 2>&1 | tail -15`
Expected: clean.

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "feat(endpoints): probe synthesized MCP candidates during --probe-rpc"
```

---

## Task 5: Non-fatal initial fetch under --probe-rpc

**Files:**
- Modify: `src/services/endpoints.rs` (`discover_with_capture_provider`)
- Modify: `src/services/endpoints/candidates_tests.rs` OR a new endpoints sidecar — see Step 1

- [ ] **Step 1: Write the failing test**

This needs the full `discover` path. Add to the existing endpoints test module. First find it: `grep -n 'mod tests' src/services/endpoints.rs` → sidecar at line ~501. Add to that sidecar file (the path it points to):

```rust
#[tokio::test]
#[serial_test::serial]
async fn probe_rpc_recovers_from_non_html_fetch() {
    use crate::core::http::{get_allow_loopback, set_allow_loopback};
    struct G(bool);
    impl Drop for G { fn drop(&mut self) { set_allow_loopback(self.0); } }
    let _g = G(get_allow_loopback());
    set_allow_loopback(true);

    let server = httpmock::MockServer::start_async().await;
    // Seed URL returns 401 (no HTML) ...
    server.mock_async(|when, then| {
        when.method(httpmock::Method::GET).path("/seed");
        then.status(401);
    }).await;
    // ... but /mcp on the same host is a real MCP server.
    server.mock_async(|when, then| {
        when.method(httpmock::Method::POST).path("/mcp").body_includes("initialize");
        then.status(200).header("content-type", "application/json").json_body(serde_json::json!({
            "jsonrpc": "2.0", "id": 1,
            "result": { "serverInfo": { "name": "demo", "version": "1" }, "capabilities": {} }
        }));
    }).await;

    let mut cfg = crate::core::config::Config::default();
    cfg.endpoints_probe_rpc = true;
    let opts = crate::services::types::EndpointOptions {
        probe_rpc: true,
        capture_network: false,
        ..crate::services::types::EndpointOptions::default()
    };
    let report = super::discover(&cfg, &server.url("/seed"), opts, None).await.unwrap();

    // Did NOT error; recorded a fetch warning; still found the synthesized MCP.
    assert!(report.warnings.iter().any(|w| w.contains("initial fetch failed")));
    assert!(report.endpoints.iter().any(|e|
        e.source == crate::services::types::EndpointSourceKind::SynthesizedMcp));
}
```

> If `Config::default()` is not available, mirror the `make_test_config()` helper used elsewhere in this sidecar. Check the top of the sidecar file for an existing config constructor and reuse it.

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p axon --lib probe_rpc_recovers 2>&1 | tail -20`
Expected: FAIL — `discover` returns `Err` on the 401 (the `?` propagates).

- [ ] **Step 3: Make the initial fetch non-fatal under probe_rpc**

In `discover_with_capture_provider`, replace the fetch line:

```rust
    let (html, html_truncated) =
        fetch_bounded_text(&client, &normalized, options.max_scan_bytes, true).await?;
```

with:

```rust
    let (html, html_truncated, fetch_error) =
        match fetch_bounded_text(&client, &normalized, options.max_scan_bytes, true).await {
            Ok((h, t)) => (h, t, None),
            // Under --probe-rpc a failed/non-HTML fetch is recoverable: we still
            // synthesize + probe MCP candidates against the seed host.
            Err(e) if options.probe_rpc => (String::new(), false, Some(e.to_string())),
            Err(e) => return Err(e),
        };
    let fetch_failed = fetch_error.is_some();
```

After `let mut report = extract_endpoints(…);` (and the existing `report.truncated |= …` lines), record the warning:

```rust
    if let Some(err) = fetch_error {
        report.warnings.push(format!(
            "initial fetch failed: {err}; probing synthesized MCP candidates against seed host only"
        ));
    }
```

Guard the network-capture block so it's skipped when the page never loaded:

```rust
    if options.capture_network && !fetch_failed {
        emit_endpoint_log(&tx, "endpoint discovery starting network capture").await;
        // ... unchanged body ...
    }
```

(The bundle-fetch / `first_party_only` / `verify` steps are no-ops on an empty `report` — leave them unchanged. `probe_rpc_endpoints` then runs and synthesizes.)

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p axon --lib probe_rpc_recovers 2>&1 | tail -20`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(endpoints): make initial fetch non-fatal under --probe-rpc for bare endpoints"
```

---

## Task 6: MCP parity — expose probe_rpc + probe_rpc_subdomains

**Files:**
- Modify: `src/mcp/schema/requests.rs` (`EndpointsRequest`)
- Modify: `src/mcp/server/handlers_query.rs` (`handle_endpoints` overrides)

- [ ] **Step 1: Add request fields**

In `src/mcp/schema/requests.rs`, in `EndpointsRequest` (after `capture_network`, before `response_mode`):

```rust
    pub capture_network: Option<bool>,
    pub probe_rpc: Option<bool>,
    pub probe_rpc_subdomains: Option<bool>,
    pub response_mode: Option<ResponseMode>,
```

- [ ] **Step 2: Apply overrides in the handler**

In `src/mcp/server/handlers_query.rs` `handle_endpoints`, after the `capture_network` override block (line ~169):

```rust
        if let Some(value) = req.capture_network {
            options.capture_network = value;
        }
        if let Some(value) = req.probe_rpc {
            options.probe_rpc = value;
        }
        if let Some(value) = req.probe_rpc_subdomains {
            options.probe_rpc_subdomains = value;
        }
```

- [ ] **Step 3: Verify compile + schema generation**

Run: `rtk cargo check 2>&1 | tail -15`
Expected: clean. `schemars::JsonSchema` derive on `EndpointsRequest` picks up the new optional fields automatically.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat(mcp): expose probe_rpc + probe_rpc_subdomains on endpoints action"
```

---

## Task 7: Web API parity — expose probe_rpc + probe_rpc_subdomains

**Files:**
- Modify: `src/web/server/handlers/exploration.rs` (`EndpointsRequest` + `endpoints` handler)

- [ ] **Step 1: Add request fields**

In `src/web/server/handlers/exploration.rs`, in the web `EndpointsRequest` struct (after `capture_network`, line ~32):

```rust
    capture_network: Option<bool>,
    probe_rpc: Option<bool>,
    probe_rpc_subdomains: Option<bool>,
```

- [ ] **Step 2: Apply overrides in the `endpoints` handler**

After the `capture_network` override (line ~151):

```rust
    if let Some(value) = req.capture_network {
        options.capture_network = value;
    }
    if let Some(value) = req.probe_rpc {
        options.probe_rpc = value;
    }
    if let Some(value) = req.probe_rpc_subdomains {
        options.probe_rpc_subdomains = value;
    }
```

- [ ] **Step 3: Verify compile (OpenAPI picks up fields via ToSchema/Deserialize)**

Run: `rtk cargo check 2>&1 | tail -15`
Expected: clean. The `#[derive(Deserialize, utoipa::ToSchema)]` on the web `EndpointsRequest` exposes the new fields in the OpenAPI doc.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat(web): expose probe_rpc + probe_rpc_subdomains on /v1/endpoints"
```

---

## Task 8: Docs, version bump, full verification

**Files:**
- Modify: `Cargo.toml` (version), `.claude-plugin/plugin.json` (only if it carries a version — per memory it must NOT; skip if absent), `README.md`, `CHANGELOG.md`
- Modify: `CLAUDE.md` (endpoints gotcha note — optional)

- [ ] **Step 1: Bump version (minor — this is a `feat`)**

Read the current `version = "X.Y.Z"` in `Cargo.toml [package]`, bump minor (`X.Y+1.0`). Update the same version string in `README.md` (badge/header). Do NOT add a version to `.claude-plugin/plugin.json` (see memory `feedback_plugin_json_no_version`).

- [ ] **Step 2: Add CHANGELOG entry**

Under the top of `CHANGELOG.md`, add an entry for the bumped version:

```markdown
## [X.Y+1.0] — 2026-05-29

### Added
- `--probe-rpc-subdomains` flag and MCP candidate synthesis: with `--probe-rpc`, axon now probes well-known MCP paths (`/mcp`, `/api/mcp`) on the target host, and (with `--probe-rpc-subdomains`) on the derived `mcp.<registrable-apex>` host. Confirmed servers appear in `endpoints` as `synthesized_mcp`; every attempt is recorded in a new `mcp_candidates` report field. The initial page fetch is now non-fatal under `--probe-rpc`, so bare MCP endpoints (which serve no HTML) can be probed directly. Exposed across CLI, MCP (`endpoints` action), and the web `/v1/endpoints` API — `probe_rpc` is now settable over MCP/HTTP too.
```

- [ ] **Step 3: Format + lint**

Run: `cargo fmt`
Run: `rtk cargo clippy 2>&1 | tail -25`
Expected: no warnings. Fix any clippy findings inline.

- [ ] **Step 4: Full verification gate**

Run: `just verify`
Expected: fmt-check + clippy + check + test all pass.

- [ ] **Step 5: Monolith check on changed files**

Run: `./scripts/check-monolith.sh 2>/dev/null || just precommit`
Confirm `probe.rs` and `candidates.rs` are each ≤ 500 lines and no function exceeds 120 lines. (`probe.rs` was 437; `probe_candidate` adds ~12. `candidates.rs` is new and small.) If `probe.rs` exceeds 500, split the synthesis call helper out — but it lives in `candidates.rs` already, so this should not trigger.

- [ ] **Step 6: Live smoke test (optional, requires network)**

```bash
cargo build --release --bin axon
./target/release/axon endpoints "https://deepwiki.com" --probe-rpc --probe-rpc-subdomains --json 2>/dev/null | python3 -c "import sys,json; r=json.load(sys.stdin); print('mcp_candidates:', json.dumps(r.get('mcp_candidates', []), indent=2))"
```
Expected: `mcp_candidates` lists same-host + `mcp.deepwiki.com` attempts; `mcp.deepwiki.com/mcp` should show `outcome: confirmed` with an `rpc_probe` naming DeepWiki and its tools.

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "chore(release): vX.Y+1.0 — MCP candidate probing"
```

---

## Self-Review (completed during planning)

- **Spec coverage:** trigger model (Task 1+6+7), tight candidate set (Task 2), psl apex (Task 2), strict probe / no SSE (Task 3), confirmed→endpoints + mcp_candidates (Task 1+4), separate budget / semaphore reuse (Task 3+4), SSRF blocked outcome (Task 4), failed-fetch recovery (Task 5), CLI+MCP+web parity (Task 1/6/7), schema propagation (Task 1 types + 6/7 requests), no-op warn (Task 1), dedup on normalized_url (Task 4), first_party for apex (Task 4), loopback-bypass-in-test (Task 4 uses buffer_unordered, no spawn). All covered.
- **Deviation from spec, recorded:** the spec listed 5 `outcome` values (`confirmed/not_rpc/http_error/blocked/timeout`); the existing probe machinery collapses all non-confirmed, non-blocked results to `None`, so this plan uses **3** outcomes — `confirmed | unconfirmed | blocked` — rather than inventing error-surfacing plumbing the codebase doesn't have. `unconfirmed` is the umbrella for 404 / non-RPC / transport error / timeout.
- **Added beyond spec (required for the approved goal):** `probe_rpc` itself is exposed over MCP + web (it was CLI-only), since `probe_rpc_subdomains` would otherwise be unreachable on those surfaces.
- **Type consistency:** `registrable_apex`, `mcp_candidate_urls`, `Candidate`, `probe_candidate`, `synthesize_and_probe_mcp`, `McpHostKind`, `McpProbeOutcome`, `McpCandidateAttempt`, `EndpointSourceKind::SynthesizedMcp` used consistently across tasks.
