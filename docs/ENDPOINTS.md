# Endpoints — API Endpoint Discovery, Verification & Protocol Probing

Last Modified: 2026-05-29

Canonical reference for the `axon endpoints` subsystem: how Axon discovers API
endpoints from a web page, optionally verifies they are reachable, captures the
live network surface via Chrome, and fingerprints discovered HTTP endpoints for
JSON-RPC 2.0 / OpenRPC / MCP protocol support.

> **Scope.** `endpoints` is a **single-page** operation, not a crawl. It fetches
> one target URL, scans its inline HTML + first-party JavaScript bundles, and
> (optionally) drives Chrome to observe runtime requests. It does not follow
> links, respect a crawl depth, or apply the crawler's locale/path-prefix
> exclusions. For multi-page coverage, crawl first, then run `endpoints` per page.

---

## 1. What it does

Given a URL, `endpoints` produces an `EndpointReport` listing every API-looking
URL referenced by the page, classified by kind and source, with optional
reachability and protocol metadata.

Five capabilities, layered (each strictly opt-in beyond static discovery):

| Stage | Default | Flag | What it adds |
|-------|---------|------|--------------|
| **Static discovery** | always | — | Regex scan of HTML + first-party JS bundles for endpoint URLs |
| **First-party bundles** | on | `--include-bundles` | Fetch & scan same-origin `<script src>` bundles |
| **Verification** | off | `--verify` | Unauthenticated `HEAD`/`OPTIONS` probes for reachability |
| **Network capture** | off | `--capture-network` | Chrome CDP observation of runtime requests (executes page JS) |
| **RPC probing** | off | `--probe-rpc` | Fingerprint HTTP endpoints for JSON-RPC 2.0 / OpenRPC / MCP |

---

## 2. Quick start

```bash
# Static discovery (HTML + first-party bundles)
axon endpoints https://example.com

# First-party only, JSON output
axon endpoints https://example.com --first-party-only true --json

# Add reachability probes
axon endpoints https://example.com --verify

# Add Chrome network capture (requires a running Chrome/CDP instance)
axon endpoints https://example.com --capture-network

# Add JSON-RPC / MCP protocol fingerprinting
axon endpoints https://example.com --probe-rpc

# Everything
axon endpoints https://example.com --verify --capture-network --probe-rpc --json
```

---

## 3. Pipeline

`services::endpoints::discover()` (`src/services/endpoints.rs`) orchestrates the
stages in this fixed order:

1. **Normalize + SSRF-validate** the target URL (`normalize_url` → `validate_url_with_dns`, 2 s DNS timeout).
2. **Fetch target HTML** with a bounded streaming reader (`max_scan_bytes` cap; 8 s timeout). `error_for_status` required.
3. **Discover script sources** — regex `<script src>` extraction; resolve relative URLs against the base; keep only `http(s)`; tag each first-party vs third-party; cap at `max_scripts`.
4. **Fetch first-party bundles** (when `--include-bundles`) — concurrent, content-type-gated (must look JavaScript-like), capped at `MAX_BUNDLE_BYTES` (2 MiB) each, bounded by the bundle semaphore.
5. **Extract endpoints** (`core::content::extract_endpoints`) — regex scan of the HTML and each bundle; classify, normalize, dedupe, drop noise.
6. **Network capture** (when `--capture-network`) — Chrome CDP session observes runtime requests, merges new endpoints (`network_capture` source).
7. **First-party filter** (when `--first-party-only`) — retain only first-party endpoints; recompute the host list.
8. **Verify** (when `--verify`) — `HEAD` (fallback `OPTIONS` on 405/501) per endpoint; attach `EndpointVerification`.
9. **Probe RPC** (when `--probe-rpc`) — protocol ladder per HTTP endpoint; attach `RpcProbeResult`.
10. Stamp `elapsed_ms`; return the `EndpointReport`.

Output split: JSON data → stdout (`--json` or pretty by default in human mode);
progress/logs → stderr. Keep this split intact for server/MCP callers.

---

## 4. Module map

```
src/core/content/endpoints.rs          # extract_endpoints(): regexes, push/normalize/dedupe, host accounting
src/core/content/endpoints/
├── classify.rs                        # looks_like_endpoint, classify_value, is_noise_value, is_valid_absolute_host
├── scan.rs                            # scan_text(): relative / graphql / websocket / absolute sub-scans
└── script_sources.rs                 # discover_script_sources(): <script src> + first-party tagging
src/services/endpoints.rs              # discover() orchestration; bundle fetch; capture merge; first-party filter
src/services/endpoints/
├── capture.rs                         # capture_requests_with_chrome(): CDP Network.* observation
├── verify.rs                          # verify_endpoints(): HEAD/OPTIONS reachability probes
├── probe.rs                           # probe_rpc_endpoints(): JSON-RPC / OpenRPC / MCP ladder
└── probe_tests.rs                     # probe sidecar tests (httpmock)
src/services/types/endpoints.rs        # EndpointReport, DiscoveredEndpoint, EndpointKind, EndpointSourceKind,
                                       #   EndpointVerification, RpcProbeResult, RpcProtocol, RpcTransport, EndpointOptions
src/cli/commands/endpoints.rs          # run_endpoints(): CLI handler + human/JSON rendering
src/mcp/server/handlers_query.rs       # handle_endpoints(): MCP action
src/mcp/schema/requests.rs             # EndpointsRequest (MCP wire shape)
src/web/server/handlers/exploration.rs # POST /v1/endpoints handler
src/web/server/openapi.rs              # OpenAPI schema registration
```

The pure-extraction layer (`core/content/endpoints*`) has **no** I/O — it scans
strings. All network work (bundle fetch, capture, verify, probe) lives in the
services layer.

---

## 5. Data model (`src/services/types/endpoints.rs`)

### `EndpointReport`
| field | type | meaning |
|-------|------|---------|
| `url` | string | normalized target URL |
| `endpoints` | `DiscoveredEndpoint[]` | discovered endpoints |
| `hosts` | string[] | distinct hosts across normalized endpoints |
| `scripts_discovered` | usize | `<script src>` count found |
| `bundles_fetched` | usize | bundles successfully downloaded |
| `bundles_scanned` | usize | bundles actually scanned |
| `truncated` | bool | a byte/script/endpoint cap was hit |
| `warnings` | string[] | non-fatal issues (caps, skipped bundles, client failures) |
| `elapsed_ms` | u64 | wall-clock duration |

### `DiscoveredEndpoint`
| field | type | meaning |
|-------|------|---------|
| `value` | string | raw discovered value (relative or absolute) |
| `normalized_url` | string? | absolute URL resolved against the base origin |
| `kind` | `EndpointKind` | `relative_path` / `absolute_url` / `graphql` / `websocket` |
| `first_party` | bool | same registrable domain as the page |
| `source` | `EndpointSourceKind` | `inline_script` / `script_bundle` / `html_attribute` / `network_capture` |
| `source_url` | string? | where it was found (page or bundle URL) |
| `verified` | `EndpointVerification?` | present only when `--verify` |
| `rpc_probe` | `RpcProbeResult?` | present only when `--probe-rpc` and a protocol matched |

### `EndpointVerification`
`attempted_url`, `method` (`HEAD`/`OPTIONS`), `status` (u16?), `content_type`,
`final_url`, `redirect_count` (always 0 — redirects are not followed),
`reachable` (`status < 500`), `error` (`"<class>: <detail>"`, e.g.
`ssrf_rejected`, `probe_error`).

### `RpcProbeResult`
`protocol` (`RpcProtocol`: `jsonrpc2` / `openrpc` / `mcp`), `transport`
(`RpcTransport`: `http` / `sse`), `server_name`, `server_version`, `methods[]`,
`tools[]`. Fields are populated per detected protocol (`server_name`/`tools` are
MCP-only; `methods` is JSON-RPC/OpenRPC-only). The wire strings match the
historical free-form values; the type is a flat struct and does not statically
forbid contradictory combinations.

---

## 6. Static discovery (`core/content`)

### Sources scanned
- **Inline HTML / scripts** (`inline_script`)
- **HTML attributes** — `href` / `src` / `action` (`html_attribute`)
- **First-party JS bundles** (`script_bundle`) — only when `--include-bundles`

### Patterns
| regex | matches | kind |
|-------|---------|------|
| `REL_PATH_RE` | quoted paths starting `/` and containing `api`/`graphql`/`gql`/`rest`/`gateway`/`internal`/`rpc`/`json` or a `/vN/` segment | `relative_path` (or `graphql`) |
| `ABS_URL_RE` | `http(s)://host[:port]/path` | `absolute_url` (or `graphql`) |
| `WS_URL_RE` | `ws(s)://host[:port]/path` | `websocket` |
| `GRAPHQL_WORD_RE` | quoted strings containing `graphql` or `/gql` | `graphql` / `websocket` |
| `ATTR_URL_RE` | `href`/`src`/`action` attribute values | classified by `classify_value` |

### Noise filtering (`is_noise_value`)
Dropped before insertion: values `< 4` chars; bare `/api`, `/api/`, `/rest`,
`/rest/`; spec/namespace hosts (`schema.org`, `json-schema.org`, `w3.org`,
`example.com/org/net`); and static assets by extension (`.js .css .png .jpg
.jpeg .gif .svg .ico .woff .woff2 .ttf .eot .otf .webp .avif .mp4 .webm .mp3
.pdf .map`). Absolute URLs additionally pass `is_valid_absolute_host` — rejecting
minifier garbage like single-label hosts (`http://n/x`) and sub-2-char or
non-alphabetic TLDs.

### Normalization & dedupe
- Relative paths join against the base **origin** (`/foo` → `https://host/foo`); other relatives join against the full base URL. WebSocket and absolute URLs parse as-is.
- Dedupe key is the normalized URL when `--unique-only` (default); otherwise `normalized|source|source_url` (keeps the same endpoint from multiple sources).

### First-party determination
- Relative paths (`/…`, not `//…`) are always first-party.
- Absolute endpoints compare **registrable domain** against the page host, so `www.example.co.uk` and `api.example.co.uk` are both first-party. A multi-label TLD table (`.co.uk`, `.com.au`, `.gov.uk`, …) keeps the comparison correct.

### Caps (defaults; see §9)
`max_scripts` (40, clamped 1–200), `max_scan_bytes` (8 MiB, clamped 1 KiB–64 MiB),
`DEFAULT_MAX_ENDPOINTS` (2000). Hitting any cap sets `truncated = true`. Bundle
downloads are additionally capped at `MAX_BUNDLE_BYTES` (2 MiB) each.

---

## 7. Network capture (`endpoints/capture.rs`)

`--capture-network` drives a headless Chrome via the Chrome DevTools Protocol to
observe the page's **runtime** request surface — endpoints assembled by JS that
never appear as literals in the source.

- Requires a reachable Chrome management/CDP endpoint (`AXON_CHROME_REMOTE_URL` / `chrome.remote-url`); without it, the stage errors.
- Creates a fresh `about:blank` target, attaches a flat session, enables `Network`, and intercepts **every** request **before dispatch** via `Fetch.enable` with `requestStage: Request`. Each paused request is SSRF-checked and either continued or failed at the network level — a private/loopback/link-local/`.local`/`.internal` target is blocked before Chrome ever connects, not filtered out afterward (bead `w2wf.5`).
- Records request URLs after `Page.loadEventFired` until the network stays quiet for `CAPTURE_IDLE_MS` = 750 ms, bounded by `network_idle_secs` (clamped 5–60 s) plus the CDP command timeout `CAPTURE_CDP_TIMEOUT_SECS` = 5 s; up to `CAPTURE_MAX_REQUESTS` = 500 requests.
- Surviving URLs are SSRF-revalidated in the merge layer (`CAPTURE_VALIDATION_CONCURRENCY` = 32), classified (`websocket`/`graphql`/`absolute_url`), de-duplicated against existing endpoints, and merged with `source = network_capture`. Respects `--first-party-only` and `DEFAULT_MAX_ENDPOINTS`.
- **Executes page JavaScript** — heavier and less safe than static discovery; opt-in only.

---

## 8. Verification (`endpoints/verify.rs`)

`--verify` issues unauthenticated reachability probes:

- Client built with **no redirect following** (`build_client_no_redirect`); `redirect_count` is therefore always 0 and `final_url` is the requested URL.
- Method ladder: `HEAD`; if the server returns 405/501, retry with `OPTIONS`.
- `reachable = status < 500`. WebSocket endpoints are skipped (not HTTP-probeable).
- Each probe is SSRF-validated first; failures surface as `error: "ssrf_rejected: …"` or `"probe_error: …"` rather than throwing.
- Caps: `MAX_VERIFY_PROBES` = 100 endpoints (excess noted in `warnings`), `VERIFY_TIMEOUT_SECS` = 2 s/probe, `VERIFY_CONCURRENCY` = 4 in-session.

---

## 9. RPC probing (`endpoints/probe.rs`)

`--probe-rpc` fingerprints each eligible `http(s)` endpoint by a **first-match
ladder** (`probe_one`):

1. **MCP** — POST `initialize`; on a `result.serverInfo`, capture the `Mcp-Session-Id` header, send `notifications/initialized`, then POST `tools/list` (replaying the session id). Yields `protocol: mcp`, `server_name/version`, `tools`.
2. **OpenRPC** — POST `rpc.discover`; requires an `openrpc` field; collects `methods[].name`.
3. **`system.listMethods`** — POST; collects the returned method-name array → `protocol: jsonrpc2`.
4. **JSON-RPC fingerprint** — POST a bogus method; accept if the error code is `-32601` (method not found) → `protocol: jsonrpc2`.
5. **SSE transport** — GET with `Accept: text/event-stream`; a `text/event-stream` response → `protocol: mcp, transport: sse`.

Transport handling: POSTs send `Accept: application/json, text/event-stream`.
JSON responses are parsed directly; `text/event-stream` responses are parsed
**incrementally** (first complete `data:` event, blank-line delimited, scanned on
the raw byte buffer and drained per block so a keepalive/empty preamble on a
kept-open stream cannot stall the probe). Response bodies are byte-bounded at
`MAX_PROBE_BODY_BYTES` (256 KiB).

Caps & safety:
- `MAX_PROBE_ENDPOINTS` = 20 (excess noted in `warnings`).
- Timeout clamped to a hard `PROBE_TIMEOUT_SECS` = 3 s ceiling (a lower configured `request_timeout_ms` can shorten it, never lengthen).
- `AXON_ENDPOINT_PROBE_CONCURRENCY` (default 4) is a **process-wide** cap acquired per-endpoint inside the fan-out, so it bounds total in-flight probes across concurrent discovery sessions.
- Every endpoint is SSRF-validated before probing.
- MCP `protocolVersion` advertised: `2025-06-18` (servers echo their own `serverInfo` regardless).

> **Surface gap (current):** `--probe-rpc` is **CLI-only**. The MCP `EndpointsRequest`
> and the `POST /v1/endpoints` body do not expose a probe flag, and there is no
> env/TOML toggle, so MCP/web callers always run with `probe_rpc = false`. Wire a
> request field through `EndpointsRequest` + the web handler to enable it there.

---

## 10. Configuration

### CLI flags (`axon endpoints <URL> …`)
| flag | type | default | field |
|------|------|---------|-------|
| `--include-bundles <bool>` | bool | `true` | `endpoints_include_bundles` |
| `--first-party-only <bool>` | bool | `false` | `endpoints_first_party_only` |
| `--unique-only <bool>` | bool | `true` | `endpoints_unique_only` |
| `--max-scripts <n>` | usize | `40` | `endpoints_max_scripts` |
| `--max-scan-bytes <n>` | usize | `8388608` | `endpoints_max_scan_bytes` |
| `--verify` | flag | `false` | `endpoints_verify` |
| `--capture-network` | flag | `false` | `endpoints_capture_network` |
| `--probe-rpc` | flag | `false` | `endpoints_probe_rpc` |
| `--json` (global) | flag | `false` | `json_output` |

`--include-bundles`, `--first-party-only`, `--unique-only` take an explicit
`true`/`false` (clap `ArgAction::Set`); `--verify`, `--capture-network`,
`--probe-rpc` are bare switches (`SetTrue`).

### Environment variables (concurrency caps; all process-wide)
| var | default | controls |
|-----|---------|----------|
| `AXON_ENDPOINT_BUNDLE_CONCURRENCY` | 8 | concurrent first-party bundle fetches |
| `AXON_ENDPOINT_CHROME_CONCURRENCY` | 1 | concurrent Chrome capture sessions |
| `AXON_ENDPOINT_VERIFY_CONCURRENCY` | 16 | concurrent verification *sessions* (× `VERIFY_CONCURRENCY`=4 probes each) |
| `AXON_ENDPOINT_PROBE_CONCURRENCY` | 4 | total in-flight RPC probes across sessions |

Each is read once at first use (`LazyLock` semaphore), parsed as `usize`, floored at 1.

### Internal constants
`BUNDLE_TIMEOUT_SECS=8`, `MAX_BUNDLE_BYTES=2 MiB`, `CAPTURE_MAX_REQUESTS=500`,
`CAPTURE_VALIDATION_CONCURRENCY=32`, `CAPTURE_IDLE_MS=750`, `CAPTURE_CDP_TIMEOUT_SECS=5`,
`MAX_VERIFY_PROBES=100`, `VERIFY_TIMEOUT_SECS=2`, `VERIFY_CONCURRENCY=4`,
`MAX_PROBE_ENDPOINTS=20`, `PROBE_TIMEOUT_SECS=3`, `MAX_PROBE_BODY_BYTES=256 KiB`,
`DEFAULT_MAX_SCRIPTS=40`, `DEFAULT_MAX_SCAN_BYTES=8 MiB`, `DEFAULT_MAX_ENDPOINTS=2000`.

---

## 11. Surfaces

### CLI — `axon endpoints`
`run_endpoints` (`src/cli/commands/endpoints.rs`) builds `EndpointOptions` from
the config, calls `discover()`, and renders either pretty human output (one
bullet per endpoint with `kind`, `source`, optional `status=…`, optional
`rpc=<protocol>`) or `--json` (the full `EndpointReport`).

### MCP — `action: "endpoints"`
`handle_endpoints` (`src/mcp/server/handlers_query.rs`). Request
(`EndpointsRequest`, `src/mcp/schema/requests.rs`):

```jsonc
{ "url": "https://example.com",
  "include_bundles": true, "first_party_only": false, "unique_only": true,
  "max_scripts": 40, "max_scan_bytes": 8388608,
  "verify": false, "capture_network": false,
  "response_mode": "inline" }
```

`url` is required and MCP-URL-validated. Unset fields fall back to
`options_from_config`. Result is the `EndpointReport` JSON via the standard
response-mode envelope. Authz: classified as an active/network action
(`src/mcp/server/authz.rs`). **No `probe_rpc` field** — see §9 gap.

### HTTP — `POST /v1/endpoints`
`handlers::exploration::endpoints` (`src/web/server/routing.rs`,
`handlers/exploration.rs`). Scope: `axon:write` (active network op). Mirrors the
MCP option set; returns the `EndpointReport` as JSON. Schemas registered in
`src/web/server/openapi.rs` (`EndpointReport`, `DiscoveredEndpoint`,
`EndpointVerification`, `RpcProbeResult`, `RpcProtocol`, `RpcTransport`,
`EndpointKind`, `EndpointSourceKind`).

---

## 12. Security

- **SSRF guard everywhere.** The target URL, every bundle URL, every captured URL, every verify probe, and every RPC probe pass `validate_url_with_dns` (loopback/link-local/RFC-1918/IPv6-ULA/`localhost`/`.internal`/`.local` blocked) with a 2 s DNS timeout. Connect-time DNS-rebinding is additionally closed by the reqwest `SsrfBlockingResolver`. See `docs/SECURITY.md` and `src/core/http/ssrf.rs`.
- **Static discovery is read-only.** It fetches the page and same-origin bundles and never executes JS.
- **`--capture-network` executes page JavaScript** in Chrome — opt-in, gated behind its own flag and a 1-wide Chrome semaphore. Capture intercepts requests **pre-dispatch** via CDP `Fetch.enable` and rejects private/loopback/link-local/`.local`/`.internal` targets at the network level before Chrome connects, so a page cannot use JS to reach an internal host even transiently.
- **`--verify` / `--probe-rpc` send live requests** to discovered (possibly third-party) hosts: `HEAD`/`OPTIONS` for verify, `POST`/`GET` for probe. Both are unauthenticated, capped, and short-timeout. Probing is more active than verifying (it POSTs JSON-RPC payloads); enable deliberately.
- Verification uses a **non-redirecting** client so a probe cannot be bounced to an internal target post-validation.

---

## 13. Testing

```bash
cargo test endpoints              # all endpoint tests
cargo test --lib endpoints::probe # RPC probe ladder (httpmock; 15 tests)
cargo test content::endpoints     # extraction / classify / noise tests
```

Probe tests (`src/services/endpoints/probe_tests.rs`) use `httpmock` + a
loopback-allow guard and cover the full ladder (MCP + session replay, MCP-over-SSE,
OpenRPC, `system.listMethods`, `-32601` fingerprint, SSE transport), the timeout
clamp, the 256 KiB body cap, content-type fall-through, ladder precedence, the
`notifications/initialized` handshake, and a non-RPC negative case. Service-level
discovery/verify/capture tests (`src/services/endpoints_tests.rs`) use `httpmock`
mock servers and a fake capture provider.

---

## 14. Gotchas

- **`endpoints` ≠ crawl.** Single page only; no link following, no locale/path-prefix exclusions, no depth.
- **`--probe-rpc` and `--capture-network` are CLI-only / heavier.** Probe has no MCP/web/env toggle (§9); capture needs Chrome and runs page JS.
- **`redirect_count` is always 0** in verification by design (non-redirecting client).
- **Third-party reach.** `--verify`/`--probe-rpc` contact non-first-party hosts unless you pass `--first-party-only true`. Filtering happens *before* verify/probe, so combine the flags to keep probes on-origin.
- **Caps fail soft.** Exceeding script/byte/endpoint/probe caps sets `truncated`/adds a `warning`; it never errors. Inspect `report.warnings` when results look short.
- **Bundle content-type gate.** Non-JavaScript-like bundles are skipped with a warning, even if same-origin.

---

## 15. Lineage

The discovery/extraction core was ported from the "webclaw" tooling (beads epic
`axon_rust-jej7`); the discovery plan is `docs/plans/2026-05-19-endpoint-discovery.md`
with a follow-on gap-closure pass against the `w2wf` acceptance criteria (verify
caps/timeouts). Notable releases: `v4.12.4` (CDP `Page.navigate` deadlock +
capture/quality fixes), `v4.13.0` (`--probe-rpc` introduced), `v4.13.1`
(probe-rpc hardening: per-endpoint concurrency, 3 s timeout clamp, MCP
session-id/initialized, SSE parsing, bounded reads, typed protocol/transport
enums; see `CHANGELOG.md`).

---

## Related docs
- `docs/SECURITY.md` — SSRF model and the DNS-rebinding resolver
- `docs/MCP.md` / `docs/MCP-TOOL-SCHEMA.md` — MCP action routing and wire schema
- `docs/CONFIG.md` — full environment-variable reference
- `src/services/CLAUDE.md`, `src/core/CLAUDE.md` — services / core layer conventions
