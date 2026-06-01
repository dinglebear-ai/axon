# axon domains
Last Modified: 2026-06-01

List indexed domains for the active Qdrant collection.

By default this runs in fast facet mode (`domain -> vector count`). Optional detailed mode performs a full scroll to add unique URL counts per domain.
Pass `--domain <host>` to do an exact indexed/not-indexed check for one domain.

## Synopsis

```bash
axon domains [FLAGS]
```

## Arguments

None.

## Required Environment Variables

| Variable | Description |
|----------|-------------|
| `QDRANT_URL` | Qdrant base URL. |

`domains` reads Qdrant metadata.

## Flags

All global flags apply. Key flags:

| Flag | Default | Description |
|------|---------|-------------|
| `--collection <name>` | `axon` | Qdrant collection to inspect. Also settable via `AXON_COLLECTION`. |
| `--json` | `false` | JSON output format. |
| `--domain <host-or-url>` | — | Check whether one exact indexed domain/host has at least one stored URL. URL input is accepted and normalized to its host. |

## Examples

```bash
# Fast domain facet output
axon domains

# JSON output
axon domains --json

# Different collection
axon domains --collection docs-local

# Check whether one exact domain is indexed
axon domains --domain docs.rs

# Machine-readable exact domain check
axon domains --domain https://docs.rs/std --json
```

## Domain Modes

| Mode | How to enable | Output |
|------|---------------|--------|
| Fast (default) | `AXON_DOMAINS_DETAILED` unset/false | `domain -> vectors` |
| Detailed | `AXON_DOMAINS_DETAILED=1` | `domain -> urls + vectors` |

## Tuning Environment Variable

| Variable | Default | Description |
|----------|---------|-------------|
| `AXON_DOMAINS_FACET_LIMIT` | `100000` | Max facet size for fast mode (clamped 1..1,000,000). |

## Notes

- If fast facet lookup fails, the command automatically falls back to detailed full-scroll mode.
- Fast-mode output includes a tip for enabling detailed mode.
- `--domain` uses a bounded Qdrant scroll with `limit=1`; it does not depend on the top-N domain facet cap.
- `--domain` matches exact `payload.domain`. `example.com` does not include `docs.example.com`.
