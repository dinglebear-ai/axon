# endpoints

Last Modified: 2026-05-21

Discover API-like endpoints from a target page without storing, embedding, or
queuing work by default.

```bash
axon endpoints https://example.com --json
axon endpoints https://example.com --first-party-only true
axon endpoints https://example.com --include-bundles true --max-scripts 40 --max-scan-bytes 8388608
axon endpoints https://example.com --verify --json
axon endpoints https://example.com --capture-network --json
```

## Layers

Layer 1 is static discovery. Axon fetches the target page through the
SSRF-guarded HTTP client, parses inline scripts and `<script src>` references,
fetches bounded first-party JavaScript bundles, and scans text for API-like
relative paths, absolute HTTP(S) URLs, GraphQL endpoints, and WebSocket URLs.
JavaScript is not executed in this layer.

Layer 2 is optional verification with `--verify`. Axon resolves relative
endpoints against the page origin and probes HTTP endpoints without cookies,
credentials, or CLI `--header` values. Verification uses short timeouts,
conservative concurrency, SSRF validation immediately before every probe, and
safe unauthenticated methods. Probe failures are reported per endpoint and do
not make the command fail.

Layer 3 is optional browser network capture with `--capture-network`. This mode
executes page code and can trigger real network calls. It requires a configured
Chrome endpoint and fails clearly when Chrome capture is unavailable.

## Output

JSON output returns:

- `url`
- `endpoints[]` with `value`, `normalized_url`, `kind`, `first_party`,
  `source`, `source_url`, and optional `verified`
- `hosts`
- `scripts_discovered`
- `bundles_fetched`
- `bundles_scanned`
- `truncated`
- `warnings`
- `elapsed_ms`

Endpoint kinds are `relative_path`, `absolute_url`, `graphql`, and
`websocket`. Sources are `inline_script`, `script_bundle`, `html_attribute`,
and `network_capture`.

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
