# axon scrape
Last Modified: 2026-07-14

<!-- BEGIN GENERATED ACTION SURFACES -->
## Surfaces

| Surface | Entry point |
|---|---|
| CLI | `axon scrape ...` |
| REST | Not inventoried |
| MCP | Not exposed as a dedicated MCP action. |
| Service | `Not inventoried` |

Parity notes: This action page is missing from docs/reference/api-parity.md.
<!-- END GENERATED ACTION SURFACES -->


Version: 1.0.0
Last Updated: 20:29:46 | 03/03/2026 EST

Fetch, render, normalize, embed, and return or save exactly one web page. `axon scrape` is a CLI convenience projection over the unified source pipeline:

```text
SourceRequest { source: url, scope: page, embed: true }
```

It does not crawl links. Use `axon <url> --scope site` or `--scope docs` for multi-page acquisition.

## Synopsis

```bash
axon scrape <url> [FLAGS]
```

## Arguments

| Argument | Description |
|----------|-------------|
| `<url>` | One URL to fetch/render/normalize as exactly one page |

## URL Input Rules

- Exactly one URL is required.
- The URL is resolved through the same source router and web adapter used by other unified source requests.

## Flags

All global flags apply. Key flags:

| Flag | Default | Description |
|------|---------|-------------|
| `--no-embed` | `false` | Fetch/render/normalize and return or save clean content without publishing vectors. |
| `--inline` | `false` | Return the cleaned page body inline when it fits the output policy. |
| `--json` | `false` | Emit the unified source result as structured JSON. |

## Related Config

| Config | Default | Description |
|--------|---------|-------------|
| Web source config | varies | Render mode, page limits, headers, artifact output, and embed behavior resolve through the unified source/web adapter configuration. |

## Examples

```bash
# Fetch one page and embed it
axon scrape https://example.com

# JSON output
axon scrape https://example.com --json

# Return clean content inline when policy allows it
axon scrape https://example.com --inline

# Fetch/normalize without publishing vectors
axon scrape https://example.com --no-embed
```

## Behavior Notes

- `scrape` is page-scoped: it fetches/renders/normalizes exactly the requested page and does not follow links.
- Embedding is on by default because single pages can be valuable source material. Use `--no-embed` only for content-output-only workflows.
- Under the hood, scrape writes through the same ledger, artifact, prepare, embed, and publish stages as other web source requests.
- Scrape is retained as a CLI convenience command only. REST and MCP callers use `POST /v1/sources` or MCP `action=source` with `scope=page`.
- No legacy scrape engine, legacy `/v1/scrape` route, or dedicated MCP `scrape` action is part of the end-state contract.
