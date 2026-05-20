# endpoints
Last Modified: 2026-05-19

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

## Resource Controls

- `--max-scripts` defaults to `40`.
- `--max-scan-bytes` defaults to `8388608` bytes.
- Endpoint output is capped internally to keep responses bounded.
- Bundle fetches enforce content type checks, short timeouts, and decompressed
  body caps.
- Verification probes are capped and run with low concurrency.
- Chrome capture is opt-in and has its own request-event cap.
