# axon domains
Last Modified: 2026-03-03

Version: 1.0.0
Last Updated: 20:30:18 | 03/03/2026 EST

List indexed domains for the active Qdrant collection.

By default this runs in fast facet mode (`domain -> vector count`). Optional detailed mode performs a full scroll to add unique URL counts per domain.

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

`domains` reads Qdrant metadata and does not require Postgres, Redis, or AMQP.

## Flags

All global flags apply. Key flags:

| Flag | Default | Description |
|------|---------|-------------|
| `--collection <name>` | `cortex` | Qdrant collection to inspect. |
| `--json` | `false` | JSON output format. |

## Examples

```bash
# Fast domain facet output
axon domains

# JSON output
axon domains --json

# Different collection
axon domains --collection docs-local
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
