# Endpoint Discovery Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:executing-plans` or `lavra:lavra-work-single` to implement this plan task-by-task. Keep tests in sibling `_tests.rs` files; do not add inline `#[cfg(test)] mod tests`.

**Goal:** Add first-class endpoint discovery to Axon: cheap static endpoint extraction first, optional unauthenticated verification second, and optional Chrome network capture later.

**Architecture:** Reimplement the Webclaw endpoint-discovery pattern in Axon-local code. Keep endpoint extraction as a pure parser under `src/core/content/`, wrap it in a services-first orchestration layer, then expose it through CLI, MCP, action API, and REST without adding durable jobs or embedding by default.

**Tech Stack:** Rust 2024, existing Axon HTTP/SSRF utilities, `regex`, `url`, `serde`, existing services layer, existing single-tool MCP routing, optional existing Chrome/CDP infrastructure for layer 3.

**Source Requirements:**
- Pattern inventory: `WEBCLAW_PATTERNS_TO_BORROW.md`
- Webclaw static discovery reference: `../webclaw/crates/webclaw-core/src/endpoints.rs`
- Webclaw fetch safety reference: `../webclaw/crates/webclaw-fetch/src/client.rs`
- Axon fetch/service reference: `src/services/scrape.rs`, `src/crawl/scrape.rs`, `src/core/http/client.rs`
- Axon routing reference: `src/mcp/schema.rs`, `src/mcp/server.rs`, `src/services/action_api.rs`

---

## Scope

Layer 1 is the first implementation target.

- Fetch the target page through Axon's SSRF-safe fetch/scrape path.
- Parse inline scripts and external `<script src>` references.
- Resolve script URLs against the page URL.
- Fetch a bounded number of first-party JS bundles by default.
- Scan HTML and JS text for API-like relative paths, absolute URLs, GraphQL endpoints, and WebSocket URLs.
- Return endpoint kind, value, normalized URL where available, first-party classification, source, source URL, hosts, bundle counts, warnings, and truncation flags.

Layer 2 is optional verification.

- Add `--verify` and matching MCP/REST flags.
- Resolve relative endpoints against the page origin.
- Validate every probed URL with the same SSRF guard used by fetch/scrape.
- Probe without credentials, cookies, or user-supplied auth headers.
- Use short timeouts, low concurrency, redirect caps, rate limits, and safe methods.
- Report method, status, content type, final URL, redirect count, reachability, and error class.

Layer 3 is later opt-in Chrome capture.

- Add `--capture-network` only after layer 1/2 contracts are stable.
- Load the page with existing Chrome/CDP infrastructure.
- Capture observed request URLs and merge them with static discoveries.
- Report `source=network_capture` and keep it opt-in because it can execute page code and hit real services.

## Non-Goals

- Do not execute JavaScript in layer 1.
- Do not infer auth requirements or attach credentials.
- Do not prove API semantics.
- Do not follow sourcemaps in the initial feature.
- Do not discover dynamically concatenated strings like `"/api/" + resourceId` until Chrome capture or a later AST pass.
- Do not store, embed, or enqueue endpoint reports by default.

---

## Public Contract

### CLI

Add:

```bash
axon endpoints <url> --json
axon endpoints <url> --first-party-only true
axon endpoints <url> --include-bundles true --max-scripts 40 --max-scan-bytes 8388608
axon endpoints <url> --verify
axon endpoints <url> --capture-network
```

Layer 1 should work without Chrome. `--capture-network` should fail clearly when Chrome is unavailable.

### MCP

Add a new routed request variant under the existing single `axon` tool:

```json
{
  "action": "endpoints",
  "url": "https://example.com",
  "include_bundles": true,
  "first_party_only": false,
  "unique_only": true,
  "max_scripts": 40,
  "max_scan_bytes": 8388608,
  "verify": false,
  "capture_network": false
}
```

Use `axon:read` scope for transient endpoint discovery and verification. Escalate to `axon:write` only if a later change stores reports or creates jobs.

### REST / Action API

Expose the same request/response through the current server routing layer while `/v1/actions` still exists. If the REST cutover lands first, map endpoint discovery to the canonical direct REST route instead of adding new action-envelope debt.

---

## Data Model

Add service result structs in `src/services/types/service.rs`:

- `EndpointKind`: `relative_path`, `absolute_url`, `graphql`, `websocket`
- `EndpointSourceKind`: `inline_script`, `script_bundle`, `html_attribute`, `network_capture`
- `DiscoveredEndpoint`
  - `value`
  - `normalized_url`
  - `kind`
  - `first_party`
  - `source`
  - `source_url`
  - `verified`
- `EndpointVerification`
  - `attempted_url`
  - `method`
  - `status`
  - `content_type`
  - `final_url`
  - `redirect_count`
  - `reachable`
  - `error`
- `EndpointReport`
  - `url`
  - `endpoints`
  - `hosts`
  - `scripts_discovered`
  - `bundles_fetched`
  - `bundles_scanned`
  - `truncated`
  - `warnings`
  - `elapsed_ms`

Keep field names snake_case for CLI JSON, REST, and MCP parity.

---

## Task 1: Pure Static Extractor

**Files:**
- Create: `src/core/content/endpoints.rs`
- Create: `src/core/content/endpoints_tests.rs`
- Modify: `src/core/content.rs`

- [ ] Implement a pure extractor that accepts HTML text, base URL, and pre-fetched bundle text.
- [ ] Extract script sources with caps: default `max_scripts=40`, default `max_scan_bytes=8 MiB`.
- [ ] Scan bounded text only. Truncate oversized HTML/bundles and surface `truncated=true`.
- [ ] Detect relative API-ish paths, absolute `http(s)` URLs, GraphQL endpoints, and `ws(s)` URLs.
- [ ] Classify first-party using the base URL host.
- [ ] Deduplicate by normalized endpoint value while preserving first source.
- [ ] Test caps, deduplication, host classification, script URL resolution, GraphQL, WebSocket, and malformed HTML.

## Task 2: Service Orchestration And CLI

**Files:**
- Create: `src/services/endpoints.rs`
- Create: `src/services/endpoints_tests.rs`
- Create: `src/cli/commands/endpoints.rs`
- Modify: `src/services.rs`
- Modify: `src/services/types/service.rs`
- Modify: `src/cli/commands.rs`
- Modify: `src/core/config/cli.rs`
- Modify: `src/core/config/types/enums.rs`
- Modify: `src/core/config/parse/build_config/command_dispatch.rs`
- Modify: `tests/cli_help_contract.rs`
- Add fixture if needed under `tests/fixtures/cli-help/`

- [ ] Add `EndpointOptions` with explicit caps and booleans for bundles, first-party filtering, verification, and network capture.
- [ ] Fetch the page through the existing SSRF-safe path used by services.
- [ ] Fetch external script bundles with strict timeout, redirect, content-type, script-count, and decompressed byte caps.
- [ ] Prefer first-party bundles by default; allow third-party scanning only through an explicit option if added.
- [ ] Render human output for interactive CLI and stable JSON for `--json`.
- [ ] Keep logs/progress on stderr and machine-readable data on stdout.
- [ ] Test service behavior using fake/local responses where possible, plus CLI parse/help contracts.

## Task 3: MCP, Action API, And REST Wiring

**Files:**
- Modify: `src/mcp/schema.rs`
- Modify: `src/mcp/server.rs`
- Modify: `src/services/action_api.rs`
- Modify: `src/services/action_api/commands.rs`
- Modify: `src/services/action_api/commands/dispatchers.rs`
- Modify: `src/web/server/handlers/exploration.rs` or REST handler selected by current server routing
- Modify: `src/web/server/openapi.rs`
- Modify: `src/web/server/routing.rs`
- Modify: `tests/mcp_contract_parity.rs`
- Modify: `src/services/action_api_tests.rs`

- [ ] Add `AxonRequest::Endpoints` with schema fields matching CLI/service options.
- [ ] Update tool descriptions and request schema docs so endpoint discovery is discoverable from MCP.
- [ ] Map endpoint discovery to `axon:read` in MCP and action API scope checks.
- [ ] Return the same typed `EndpointReport` shape across MCP, action API, and REST.
- [ ] Add contract parity tests for schema, scope, dispatch, and JSON output.

## Task 4: Layer 2 Verification

**Files:**
- Modify: `src/services/endpoints.rs`
- Modify: `src/services/types/service.rs`
- Modify/Create: `src/services/endpoints_tests.rs`
- Modify: `src/core/http/client.rs` only if shared safe probe support is needed

- [ ] Implement `--verify` as an opt-in pass after static discovery.
- [ ] Resolve relative endpoints against the target page origin.
- [ ] Run SSRF validation immediately before every outbound probe.
- [ ] Probe with no credentials, no cookies, no custom auth headers.
- [ ] Use `HEAD` first, fall back to `OPTIONS` or a bounded `GET` only when safe and configured.
- [ ] Cap concurrency and total probes; default should be conservative.
- [ ] Report status, content type, final URL, redirect count, reachability, and error class without treating failures as command failure.
- [ ] Test SSRF rejection, timeout classification, redirect limits, 405 fallback, and partial verification results.

## Task 5: Layer 3 Browser Network Capture

**Files:**
- Create or modify Chrome/CDP capture module under the existing `src/core/http/cdp.rs` or crawl Chrome area
- Modify: `src/services/endpoints.rs`
- Modify: `src/services/types/service.rs`
- Modify/Create: service tests with a fake capture provider

- [ ] Add an internal network-event capture abstraction so the endpoint service is not tied directly to Chrome.
- [ ] Capture request URLs, resource type where available, method, and initiator/source when available.
- [ ] Merge observed endpoints with static endpoints using deterministic dedupe rules.
- [ ] Keep this off by default and require `--capture-network`.
- [ ] Enforce page-load timeout, network-idle timeout, max request count, and SSRF/host filtering.
- [ ] Document that this mode executes page code and can trigger real network calls.

## Task 6: Endpoint Discovery Documentation

**Files:**
- Create: `docs/commands/endpoints.md`
- Modify: `docs/commands/README.md`
- Modify: `docs/MCP-TOOL-SCHEMA.md`
- Modify: `docs/MCP.md`

- [ ] Document layer behavior, defaults, caps, and non-goals.
- [ ] Include examples for static discovery, verification, and Chrome capture.
- [ ] Add safety notes: no JS execution in layer 1, no credentials in verification, Chrome capture is opt-in.
- [ ] Document endpoint-discovery-specific resource controls: script count cap, scan byte cap, endpoint output cap, bundle fetch byte cap, verification probe cap, and Chrome request-event cap.

---

## Validation

Targeted checks during implementation:

```bash
cargo test -q endpoints
cargo test -q mcp_contract_parity
cargo test -q action_api
cargo test -q cli_help_contract
cargo check --bin axon
```

Manual smoke checks:

```bash
axon endpoints https://example.com --json
axon endpoints https://example.com --verify --json
axon endpoints https://example.com --capture-network --json
```

Security checks:

- Verify SSRF rejection for loopback, link-local, private IP, DNS rebinding-style hostnames, and disallowed schemes.
- Verify large decompressed bundle bodies abort before buffering unbounded text.
- Verify verification probes do not forward CLI `--header` auth values or stored credentials.
