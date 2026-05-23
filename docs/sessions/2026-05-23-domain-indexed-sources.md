# Domain Indexed Sources

Date: 2026-05-23

## Summary

Added exact-domain discovery surfaces for indexed source URLs and indexed-domain checks.

## Changes

- `axon sources --domain <host-or-url>` lists indexed URLs for an exact normalized domain.
- `axon sources --domain <host> --all` uses `AXON_SOURCES_DOMAIN_LIMIT`, capped at 10,000, for bounded bulk listing.
- `axon domains --domain <host-or-url>` reports whether the exact normalized domain has indexed URLs.
- MCP and REST discovery requests accept `domain`; domain-filtered sources use cursor pagination and reject numeric offsets.
- Qdrant domain checks use typed filters on `domain` and `chunk_index = 0`, with a `limit: 1` existence check for indexed-domain lookup.

## Verification

- `cargo test domain --no-fail-fast`
- `cargo fmt --check`
- `git diff --check`
- `cargo check`
