# brand
Last Modified: 2026-06-01

<!-- BEGIN GENERATED ACTION SURFACES -->
## Surfaces

| Surface | Entry point |
|---|---|
| CLI | `axon brand ...` |
| REST | Not inventoried |
| MCP | Not exposed as a dedicated MCP action. |
| Service | `Not inventoried` |

Parity notes: This action page is missing from docs/reference/api-parity.md.
<!-- END GENERATED ACTION SURFACES -->


Analyze a URL's brand identity — extract colors, fonts, logos, and favicon.

## Synopsis

```bash
axon brand [OPTIONS] [URL]...
```

## Arguments

| Argument | Required | Description |
|----------|----------|-------------|
| `[URL]...` | Yes | One or more URLs to analyze. |

## Flags

| Flag | Default | Description |
|------|---------|-------------|
| `--max-pages <n>` | `0` (uncapped) | Maximum pages to traverse. |
| `--max-depth <n>` | `10` | Maximum traversal depth. |
| `--render-mode <mode>` | `auto-switch` | Page fetch mode: `http`, `chrome`, or `auto-switch`. |
| `--include-subdomains <bool>` | `false` | Include subdomains in the web traversal scope. |
| `--header <HEADER>` | — | Custom HTTP request header (`Key: Value`). Repeatable. |
| `--skip-embed` | `false` | Fetch/analyze without indexing into Qdrant. |
| `--collection <name>` | `axon` | Qdrant collection name. |
| `--wait <bool>` | `false` | Block until async work completes. |
| `--json` | `false` | Machine-readable JSON output. |

## Usage

```bash
# Extract a site's brand identity
axon brand https://stripe.com

# Multiple URLs, JSON output
axon brand https://stripe.com https://vercel.com --json
```

## Behavior

- Fetches the page(s) and extracts brand signals: dominant/accent colors, font families, logo image(s), and favicon.
- Uses the standard render ladder (`auto-switch` falls back to Chrome for JS-heavy pages) so token-driven design systems are captured.
- With `--json`, emits a structured brand payload suitable for downstream theming or design-system import.

## See also

- [`extract`](extract.md) — LLM-powered structured data extraction.
- [`screenshot`](screenshot.md) — capture a full-page screenshot.
