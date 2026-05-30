# Auto-synthesized MCP endpoint probing

**Date:** 2026-05-29
**Status:** Design approved, pending spec review
**Scope:** `src/services/endpoints/` (probe + new candidates module), `src/services/types/endpoints.rs`, `src/core/config/cli.rs`, MCP schema, web `/v1/actions`

## Goal

When `--probe-rpc` runs, axon should probe not only the endpoints *discovered in
the page* but also a small set of **well-known MCP candidate URLs** derived from
the target URL itself. This catches MCP servers that a site's frontend never
references in its HTML/JS — the case that made `axon endpoints https://deepwiki.com`
find 44 endpoints but miss `mcp.deepwiki.com`, and the case behind the original
ask ("point axon at a known MCP endpoint and have it confirmed").

Today there is **no** candidate synthesis: `probe_rpc_endpoints()` probes only the
absolute-HTTP endpoints already present in `report.endpoints`. This feature adds
synthesis as a final step of that function.

## Decisions (locked)

| # | Decision | Choice |
|---|----------|--------|
| 1 | Same-host trigger | Always-on whenever `--probe-rpc` is set. No new flag. |
| 2 | Subdomain (`mcp.<apex>`) trigger | Gated behind a **new** flag `--probe-rpc-subdomains` (default false). Explicitly requested by the user, so it does not violate the no-unrequested-flags rule. |
| 3 | Candidate paths | **Tight:** `/mcp` and `/api/mcp` only. SSE paths (`/sse`, `/mcp/sse`) dropped — the transport is deprecated and its only confirmation signal is weak (see Decision 6). |
| 4 | Apex derivation | Add the **`psl`** crate (Public Suffix List, compiled in, no runtime fetch). |
| 5 | Output | Confirmed candidates appended to `report.endpoints`; every attempt recorded in a new `report.mcp_candidates` field. |
| 6 | Probe strictness | Synthesized candidates use a **strict probe** = positive-signal POST probes only, skipping `probe_sse_transport` (content-type-only check, false-positive-prone on guesses). |
| 7 | Failed/non-HTML initial fetch | **Non-fatal under `--probe-rpc`:** synthesis still runs against the seed host even when the target's first GET returns non-2xx or non-HTML. This is what makes `axon endpoints https://mcp.deepwiki.com/mcp --probe-rpc` work. |

## Candidate generation

For target `https://docs.foo.co.uk/bar` with `--probe-rpc --probe-rpc-subdomains`:

| Host | Scheme | Gate | Paths |
|------|--------|------|-------|
| `docs.foo.co.uk` (seed host) | target scheme | always (under `--probe-rpc`) | `/mcp`, `/api/mcp` |
| `mcp.foo.co.uk` (registrable apex) | `https` | `--probe-rpc-subdomains` | `/mcp`, `/api/mcp` |

= up to **4 candidates** (2 same-host + 2 subdomain).

Rules:
- **Apex** via `registrable_apex(host)` (new helper). If it can't resolve a
  registrable domain (raw IP, unknown public suffix), the subdomain set is
  skipped — no naive last-2-labels fallback.
- If the seed host already starts with `mcp.`, the subdomain set collapses into
  the same-host set (its URLs are identical), so it is skipped.
- **Dedup on `normalized_url`** (not raw `value`, so `…/mcp` and `…/mcp/` don't
  double-probe): any synthesized URL whose normalized form matches an
  already-discovered endpoint is dropped — the discovered one is probed by the
  normal path. Synthesized candidates are also deduped against each other.

## Probing & safety

- Each candidate passes through the existing `validate_url_with_dns_timeout()`
  (SSRF guard) **before** any request. A candidate resolving to
  loopback/private/link-local is recorded `outcome: "blocked"` and never fetched.
- **Strict probe** (`probe_one_strict`): runs the existing positive-signal POST
  probes in order — `probe_mcp` (MCP `initialize` → `tools/list`),
  `probe_openrpc` (`rpc.discover`), `probe_list_methods` (`system.listMethods`),
  `probe_jsonrpc_fingerprint` (`-32601`). It **does not** call
  `probe_sse_transport`. No new network handshakes are introduced; this is a
  gated reuse of the existing probe functions.
- **Streamable-HTTP MCP is still detected.** Servers like DeepWiki reply to the
  `initialize` POST with a `text/event-stream` body; that is parsed inside
  `send_jsonrpc`/`read_first_sse_json`, independent of `probe_sse_transport`.
  Verified live (DeepWiki returned `event: message\ndata:{…serverInfo…}`).
  Dropping the SSE fallback loses nothing for real MCP servers.
- **Separate budget:** synthesized candidates (≤4) do not count against
  `MAX_PROBE_ENDPOINTS` (20, which caps discovered-endpoint probing). They reuse
  the same `AXON_ENDPOINT_PROBE_CONCURRENCY` semaphore.

## Failed-fetch handling (Decision 7)

`discover_with_capture_provider` currently does
`fetch_bounded_text(&client, &normalized, …).await?` — a fatal `?`. Change: when
`options.probe_rpc` is set, a fetch error (non-2xx, transport, non-HTML body) is
**recovered**, not propagated:

- The error text is pushed to `report.warnings` (e.g. `initial fetch failed: <…>; probing synthesized MCP candidates against seed host only`).
- Page-discovered endpoints are empty (nothing was fetched), bundle scanning is
  skipped, network capture is skipped.
- `probe_rpc_endpoints` still runs and synthesizes same-host candidates from the
  seed host (and subdomain candidates if `--probe-rpc-subdomains`).

When `probe_rpc` is **not** set, the fetch error stays fatal (current behavior —
no reason to swallow it).

## Output (schema change)

- **Confirmed** candidates (returned a real `rpc_probe` via the strict probe) are
  appended to `report.endpoints` with:
  - new `EndpointSourceKind::SynthesizedMcp` → serializes as `"synthesized_mcp"`
  - `first_party: true` for `mcp.<apex>` confirms (same registrable org) and for
    seed-host confirms; computed from registrable-apex equality, not bare host
    equality.
  - `rpc_probe` populated, `source_url` = the seed/target URL.
- **New field** on `EndpointReport`:
  ```rust
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub mcp_candidates: Vec<McpCandidateAttempt>,
  ```
  where:
  ```rust
  pub struct McpCandidateAttempt {
      pub url: String,
      pub host_kind: McpHostKind,   // "same_host" | "apex_subdomain"
      pub path: String,             // "/mcp" | "/api/mcp"
      pub outcome: McpProbeOutcome, // "confirmed" | "unconfirmed" | "blocked"
      // The existing probe machinery collapses every non-confirmed, non-blocked
      // result (404 / non-RPC JSON / transport error / timeout) to `None`. Rather
      // than invent error-surfacing plumbing the codebase lacks, use 3 outcomes:
      // blocked (SSRF-rejected pre-request), confirmed (positive RPC signal),
      // unconfirmed (everything else).
      #[serde(default, skip_serializing_if = "Option::is_none")]
      pub rpc_probe: Option<RpcProbeResult>,
  }
  ```
  All new types derive `utoipa::ToSchema`.
- **Schema propagation:** the new `EndpointSourceKind` variant and the new report
  field/types must surface in the MCP tool schema (`src/mcp/schema.rs`) and the
  web OpenAPI surface.

## Wiring

- **CLI flag:** add `--probe-rpc-subdomains` (bool, `ArgAction::SetTrue`) in
  `src/core/config/cli.rs` beside `--probe-rpc`; plumb into `EndpointOptions`.
  - Help text states it is a **no-op without `--probe-rpc`**.
  - If `probe_rpc_subdomains` is set while `probe_rpc` is false, emit a
    `log_warn` ("--probe-rpc-subdomains has no effect without --probe-rpc") so the
    user isn't silently given no probing.
- **MCP action schema:** add the same boolean input to the `endpoints` action.
- **Web `/v1/actions`:** thread the new input flag through the actions handler —
  otherwise the subdomain toggle is unreachable over HTTP. (Parity: input flag →
  CLI + MCP action + web actions; output field → MCP schema + web OpenAPI.)
- **Integration point:** inside `probe_rpc_endpoints(cfg, &mut report)` in
  `probe.rs`, after the discovered-endpoint loop, call
  `synthesize_and_probe_mcp(cfg, target_url, opts, &mut report)`. Thread the
  normalized target URL and the subdomain flag into `probe_rpc_endpoints` (new
  params).
- **New module:** `src/services/endpoints/candidates.rs` holds `registrable_apex`,
  candidate enumeration, and `synthesize_and_probe_mcp`, keeping `probe.rs` under
  the 500-line monolith cap (currently 437).

## Testing

- `candidates.rs` unit tests:
  - `registrable_apex`: `.com`, `.co.uk`, `.com.au`, raw IPv4/IPv6, already-`mcp.`
    host, single-label host.
  - candidate enumeration counts: same-host-only (2) vs +subdomain (4); collapse
    when seed host is `mcp.*`; skip subdomain when apex unresolvable.
  - dedup vs a discovered endpoint at the same normalized URL.
- probe integration test (mock JSON-RPC server, existing wiremock-style pattern in
  `probe_tests.rs`): a synthesized `/mcp` confirms → lands in both
  `report.endpoints` (as `synthesized_mcp`) and `report.mcp_candidates`
  (`confirmed`). **The `#[cfg(test)]` loopback bypass must be propagated into the
  spawned probe stream** (ssrf.rs warns the thread-local flag must be set across
  spawned tasks) or the test fails "blocked host" before exercising anything.
- SSRF test: a synthesized candidate resolving to a private host → `blocked`,
  never requested.
- Negative test: a candidate that returns generic `text/event-stream` on GET is
  **not** confirmed (strict probe skips the SSE fallback) → `not_rpc`.
- Failed-fetch test: target returns 401 with `--probe-rpc` → discovery does not
  error; `report.warnings` records the fetch failure; same-host candidates are
  still attempted.

## Out of scope

- No standalone `axon probe <url>` command. Bare-endpoint probing is achieved via
  the failed-fetch recovery (Decision 7) on the existing `endpoints` command.
- No `/sse`, `/rpc`, or `api.<apex>` candidates.
- No authenticated probing (no header threading into the prober) — auth-gated MCP
  servers like GitHub Copilot's still return `blocked`/`http_error`.

## Dependencies

- New crate: `psl` (Cargo.toml + Cargo.lock).
- Version bump on the implementation branch per repo policy (`feat` → minor).
