# axon screenshot
Last Modified: 2026-03-03

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
- `--start-url` is global but not used by `screenshot` input parsing.
- URL inputs are normalized and deduplicated before execution.

## Required Runtime

- Chrome endpoint must be configured via `AXON_CHROME_REMOTE_URL` or `--chrome-remote-url`.
- If Chrome is unavailable, the command fails fast with a configuration error.

## Flags

All global flags apply. Key flags:

| Flag | Default | Description |
|------|---------|-------------|
| `--screenshot-full-page <bool>` | `true` | Capture full scrollable page (`true`) or viewport only (`false`). |
| `--viewport <WIDTHxHEIGHT>` | `1920x1080` | Screenshot viewport dimensions. |
| `--chrome-remote-url <url>` | env/default | Chrome remote endpoint for capture. |
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

# Capture through the canonical server
AXON_SERVER_URL=http://127.0.0.1:8001 axon screenshot https://example.com --json
```

## Behavior Notes

- Screenshots are PNG byte captures from Chrome.
- In server mode (`AXON_SERVER_URL` / `--server-url`), screenshot capture runs on `axon serve` and returns a server-owned artifact handle/root-relative path. The host CLI does not write the PNG locally unless you force `--local`.
- With multiple URLs and `--output` set, each URL writes to the same path in sequence (last write wins). Prefer default generated paths for multi-URL runs.
- Non-2xx pages or Chrome navigation/capture errors fail the current URL.
