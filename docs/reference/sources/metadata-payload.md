# Metadata Payload
Last Modified: 2026-07-15

Metadata payloads make vector and graph records traceable back to sources.

## Required Identity

- source id
- generation id
- document id
- canonical URI or local path identity
- adapter/source kind
- content kind
- timestamps and freshness state when available

## Rule

Metadata must be structured and queryable. Avoid prose-only payload fields for
values that clients need to filter, group, prune, or cite.
