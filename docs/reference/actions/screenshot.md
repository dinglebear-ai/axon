# axon screenshot
Last Modified: 2026-06-07

<!-- BEGIN GENERATED ACTION SURFACES -->
## Surfaces

| Surface | Entry point |
|---|---|
| CLI | `axon screenshot ...` |
| REST | Not inventoried |
| MCP | Not exposed as a dedicated MCP action. |
| Service | `Not inventoried` |

Parity notes: This action page is missing from docs/reference/api-parity.md.
<!-- END GENERATED ACTION SURFACES -->


Version: 1.0.0
Last Updated: 21:05:00 | 03/03/2026 EST

Capture PNG screenshots for one or more URLs using Spider Chrome capture. Runs inline (no queue), validates URLs before fetch, and writes files to `--output` or `<output-dir>/screenshots/`.

## Synopsis

```bash
axon screenshot <url>... [FLAGS]
axon screenshot --urls "<url1>,<url2>" [FLAGS]
axon screenshot --url-glob "https://docs.example.com/{1..10}" [FLAGS]
```

## Arguments

| Argument | Description |
|----------|-------------|
| `<url>...` | One or more URLs to capture |

## URL Input Rules

- At least one URL is required via positional args, `--urls`, or `--url-glob`.
- URL inputs are normalized and deduplicated before execution.

## Required Runtime

- Chrome endpoint must be configured via `AXON_CHROME_REMOTE_URL`.
- If Chrome is unavailable, the command fails fast with a configuration error.

## Flags

All global flags apply. Key flags:

| Flag | Default | Description |
|------|---------|-------------|
| `--screenshot-full-page <bool>` | `true` | Capture full scrollable page (`true`) or viewport only (`false`). |
| `--viewport <WIDTHxHEIGHT>` | `1920x1080` | Screenshot viewport dimensions. |
| `--output <path>` | — | Output file path. If omitted, auto-generates under `<output-dir>/screenshots/`. |
| `--output-dir <dir>` | `.cache/axon-rust/output` | Base output directory for generated screenshot files. |
| `--json` | `false` | Emit per-URL JSON with `url`, `path`, and `size_bytes`. |

## Examples

```bash
# Basic screenshot (saved under .cache/axon-rust/output/screenshots/)
axon screenshot https://example.com

# Viewport-only screenshot with explicit viewport
axon screenshot https://example.com --screenshot-full-page false --viewport 1366x768

# Multiple URLs from CSV
axon screenshot --urls "https://a.dev,https://b.dev"

# Save to an explicit output file
axon screenshot https://example.com --output ./shot.png

# JSON output
axon screenshot https://example.com --json

# Capture and print JSON
axon screenshot https://example.com --json
```

## Behavior Notes

- Screenshots are PNG byte captures from Chrome.
- Screenshot files are written through the shared service artifact writer, which rejects paths outside the output root and uses a temporary file plus rename to avoid partial final files.
- Generic CLI client-to-server forwarding was removed in 5.0.0. `AXON_SERVER_URL` does not route `axon screenshot` through HTTP; call the `/v1/screenshot` REST route or MCP HTTP endpoint directly when using `axon serve` as a remote service.
- With multiple URLs and `--output` set, each URL writes to the same path in sequence (last write wins). Prefer default generated paths for multi-URL runs.
- Non-2xx pages or Chrome navigation/capture errors fail the current URL.
