# URL Normalization
Last Modified: 2026-07-15

URL normalization gives web sources stable identity.

## Rules

- Normalize scheme and host casing.
- Remove fragments for fetch identity.
- Preserve meaningful query parameters when adapter policy requires them.
- Avoid collapsing distinct pages into one source item.
- Keep the original requested URL for provenance.

## Use

Normalized URLs are used for source ids, manifest diffing, duplicate detection,
vector payloads, graph edges, and cleanup selectors.
