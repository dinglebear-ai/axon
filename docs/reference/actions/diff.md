# diff
Last Modified: 2026-06-01

<!-- BEGIN GENERATED ACTION SURFACES -->
## Surfaces

| Surface | Entry point |
|---|---|
| CLI | `axon diff ...` |
| REST | Not inventoried |
| MCP | Not exposed as a dedicated MCP action. |
| Service | `Not inventoried` |

Parity notes: This action page is missing from docs/reference/api-parity.md.
<!-- END GENERATED ACTION SURFACES -->


Compare two URLs and report what changed between them — content, metadata, and links.

## Synopsis

```bash
axon diff [OPTIONS] <URL_A> <URL_B>
```

## Arguments

| Argument | Required | Description |
|----------|----------|-------------|
| `<URL_A>` | Yes | First URL (baseline). |
| `<URL_B>` | Yes | Second URL (comparison). |

## Flags

| Flag | Default | Description |
|------|---------|-------------|
| `--max-pages <n>` | `0` (uncapped) | Maximum pages to traverse per URL. |
| `--max-depth <n>` | `10` | Maximum traversal depth. |
| `--render-mode <mode>` | `auto-switch` | Page fetch mode: `http`, `chrome`, or `auto-switch`. |
| `--include-subdomains <bool>` | `false` | Include subdomains in the web traversal scope. |
| `--header <HEADER>` | — | Custom HTTP request header (`Key: Value`). Repeatable. |
| `--skip-embed` | `false` | Fetch/compare without indexing into Qdrant. |
| `--collection <name>` | `axon` | Qdrant collection name. |
| `--wait <bool>` | `false` | Block until async work completes. |
| `--json` | `false` | Machine-readable JSON output. |

## Usage

```bash
# Compare two versions of a page
axon diff https://example.com/v1 https://example.com/v2

# Compare without indexing, as JSON
axon diff https://a.example.com https://b.example.com --skip-embed --json
```

## Behavior

- Fetches both URLs and computes a structured comparison across content, metadata, and link sets.
- Reports added, removed, and changed regions rather than a raw line diff.
- Respects the standard web traversal/render flags so both sides are fetched the same way.

## See also

- [`scrape`](scrape.md) — fetch a single URL to markdown.
- [`brand`](brand.md) — extract brand identity from a URL.
