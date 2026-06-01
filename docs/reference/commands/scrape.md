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

# Run through the canonical server; output/artifacts are server-owned
AXON_SERVER_URL=http://127.0.0.1:8001 axon scrape https://example.com --json
```

## Behavior Notes

- Non-2xx responses fail that URL with `scrape failed: HTTP <code>`.
- `--output` with multiple URLs is rejected to prevent overwrite.
- Scrape errors are reported per URL; other URLs continue.
- By default, scrape writes markdown under `<output-dir>/scrape-markdown/runs/<uuid>/` (isolated per run) and embeds once at the end. Each scrape invocation writes into its own run directory so only the current session's files are indexed, not historical outputs. Pass `--skip-embed` to fetch/save without indexing.
- In server mode (`AXON_SERVER_URL`), scrape runs on `axon serve`. The CLI prints the server result and portable artifact handle; it does not write host-local markdown as the source of truth. Use `--local` to force the local behavior above.
