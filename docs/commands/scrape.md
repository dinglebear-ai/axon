# axon scrape
Last Modified: 2026-03-03

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
- `--start-url` is global but not used by `scrape`.
- URL inputs are normalized and deduplicated before execution.

## Flags

All global flags apply. Key flags:

| Flag | Default | Description |
|------|---------|-------------|
| `--format <fmt>` | `markdown` | Output format: `markdown`, `html`, `rawHtml`, `json`. |
| `--render-mode <mode>` | `auto-switch` | Fetch mode: `http`, `chrome`, `auto-switch` (`auto-switch` behaves like HTTP for scrape). |
| `--embed <bool>` | `true` | Batch-embed scraped markdown into Qdrant after all URLs finish. |
| `--output <path>` | — | Write output to a file (single URL only). |
| `--output-dir <dir>` | `.cache/axon-rust/output` | Base output directory used by embed flow. |
| `--header "Key: Value"` | — | Repeatable custom HTTP headers for scrape requests. |
| `--request-timeout-ms <ms>` | profile default | Per-request timeout. |
| `--fetch-retries <n>` | profile default | Fetch retry count. |
| `--json` | `false` | Emit structured JSON per URL on stdout. |

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
axon scrape https://example.com --embed false
```

## Behavior Notes

- Non-2xx responses fail that URL with `scrape failed: HTTP <code>`.
- `--output` with multiple URLs is rejected to prevent overwrite.
- Scrape errors are reported per URL; other URLs continue.
- When `--embed true`, markdown is written under `<output-dir>/scrape-markdown/runs/<uuid>/` (isolated per run) and embedded once at the end. Each scrape invocation writes into its own run directory so only the current session's files are indexed, not historical outputs.
