# axon sources
Last Modified: 2026-06-01

<!-- BEGIN GENERATED ACTION SURFACES -->
## Surfaces

| Surface | Entry point |
|---|---|
| CLI | `axon sources ...` |
| REST | Not inventoried |
| MCP | Not exposed as a dedicated MCP action. |
| Service | `Not inventoried` |

Parity notes: This action page is missing from docs/reference/api-parity.md.
<!-- END GENERATED ACTION SURFACES -->


List indexed source URLs in the active Qdrant collection with chunk counts.
Pass `--domain <host>` to list indexed URLs for one exact indexed domain.

## Synopsis

```bash
axon sources [FLAGS]
```

## Arguments

None.

## Required Environment Variables

| Variable | Description |
|----------|-------------|
| `QDRANT_URL` | Qdrant base URL. |

`sources` reads Qdrant metadata.

## Flags

All global flags apply. Key flags:

| Flag | Default | Description |
|------|---------|-------------|
| `--collection <name>` | `axon` | Qdrant collection to inspect. Also settable via `AXON_COLLECTION`. |
| `--json` | `false` | Emits a JSON object with `count`, `limit`, `offset`, and `urls` for unfiltered output. Domain-filtered JSON uses `cursor` / `next_cursor` instead of `offset`. |
| `--domain <host-or-url>` | — | Filter indexed source URLs by exact domain/host. URL input is accepted and normalized to its host. |
| `--all` | `false` | With `--domain`, export up to `AXON_SOURCES_DOMAIN_LIMIT` matching URLs instead of the normal `--limit` page. |

## Examples

```bash
# Human-readable list
axon sources

# JSON output
axon sources --json

# Different collection
axon sources --collection docs-local

# URLs indexed for one exact domain
axon sources --domain docs.rs

# JSON page for one exact domain
axon sources --domain https://docs.rs/std --limit 50 --json

# Explicit full-domain export, capped by AXON_SOURCES_DOMAIN_LIMIT
axon sources --domain docs.rs --all --json
```

## Tuning Environment Variable

| Variable | Default | Description |
|----------|---------|-------------|
| `AXON_SOURCES_FACET_LIMIT` | `100000` | Max URL facets fetched from Qdrant (clamped 1..1,000,000). |
| `AXON_SOURCES_DOMAIN_LIMIT` | `10000` | Max URLs fetched for `sources --domain --all` (clamped 1..10,000). |

## Notes

- `sources` uses Qdrant facet aggregation on `url`.
- If facet results hit the configured limit, human output prints a truncation hint.
- `--domain` uses exact `payload.domain` matching. `example.com` does not include `docs.example.com`.
- `--domain` output is bounded by default via `--limit`; use `--all` only for explicit export.
- REST/MCP domain-filtered sources use `next_cursor` pagination. CLI `--domain` starts at the first page; `--all` raises the one-page cap for explicit exports.
- Domain-filtered REST/MCP calls reject numeric `offset`; pass the returned `next_cursor` as `cursor` instead.
