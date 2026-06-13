# axon scrape
Last Modified: 2026-06-07

<!-- BEGIN GENERATED ACTION SURFACES -->
## Surfaces

| Surface | Entry point |
|---|---|
| CLI | `axon scrape ...` |
| REST | `POST /v1/scrape` (Implemented) |
| MCP | `{ "action": "scrape" }` |
| Service | `services::scrape::{scrape_batch,scrape_batch_with_optional_embed}` |

Parity notes: Supports render mode, format, selectors, headers, collection, and optional embedding.
<!-- END GENERATED ACTION SURFACES -->


Version: 1.0.0
Last Updated: 20:29:46 | 03/03/2026 EST

Scrape one or more URLs and return page content as markdown, HTML, raw HTML, or JSON. Runs inline (no queue), validates URLs before network access, and can embed scraped markdown into Qdrant in a single batch.

## Synopsis

```bash
axon scrape <url>... [FLAGS]
axon scrape --urls "<url1>,<url2>" [FLAGS]
axon scrape --url-glob "https://docs.example.com/{1..10}" [FLAGS]
```

## Arguments

| Argument | Description |
|----------|-------------|
| `<url>...` | One or more URLs to scrape |

## URL Input Rules

- At least one URL is required via positional args, `--urls`, or `--url-glob`.
- URL inputs are normalized and deduplicated before execution.

## Flags

All global flags apply. Key flags:

| Flag | Default | Description |
|------|---------|-------------|
| `--format <fmt>` | `markdown` | Output format: `markdown`, `html`, `rawHtml`, `json`. |
| `--render-mode <mode>` | `auto-switch` | Fetch mode: `http`, `chrome`, `auto-switch` (`auto-switch` behaves like HTTP for scrape). |
| `--skip-embed` | `false` | Fetch/save only; do not batch-embed scraped markdown into Qdrant. |
| `--output <path>` | — | Write output to a file (single URL only). |
| `--output-dir <dir>` | `.cache/axon-rust/output` | Base output directory used by embed flow. |
| `--header "Key: Value"` | — | Repeatable custom HTTP headers for scrape requests. |
| `--json` | `false` | Emit structured JSON per URL on stdout. |

## Related Config

| Config | Default | Description |
|--------|---------|-------------|
| `scrape.batch-timeout-secs` / `AXON_SCRAPE_BATCH_TIMEOUT_SECS` | `120` | End-to-end timeout for one service-level scrape batch. Applies to CLI, REST, and MCP service paths. |

## Examples

```bash
# Single URL (default markdown output)
axon scrape https://example.com

# Multiple URLs from CSV
axon scrape --urls "https://a.dev,https://b.dev"

# URL glob expansion with numeric range
axon scrape --url-glob "https://docs.example.com/v{1..3}/intro"

# HTML output to file
axon scrape https://example.com --format html --output page.html

# JSON output
axon scrape https://example.com --json

# Disable embedding
axon scrape https://example.com --skip-embed

# JSON output from the local in-process CLI path
axon scrape https://example.com --json
```

## Behavior Notes

- Non-2xx responses fail that URL with `scrape failed: HTTP <code>`.
- `--output` with multiple URLs is rejected to prevent overwrite.
- Scrape errors are reported per URL; other URLs continue.
- By default, scrape writes markdown under `<output-dir>/scrape-markdown/runs/<uuid>/` (isolated per run) and embeds once at the end. Each scrape invocation writes into its own run directory so only the current session's files are indexed, not historical outputs. Pass `--skip-embed` to fetch/save without indexing.
- Scrape artifacts are written through the shared service artifact writer, which rejects paths outside the output root and uses a temporary file plus rename to avoid partial final files.
- Generic CLI client-to-server forwarding was removed in 5.0.0. `AXON_SERVER_URL` does not route `axon scrape` through HTTP; call the `/v1/scrape` REST route or MCP HTTP endpoint directly when using `axon serve` as a remote service.
