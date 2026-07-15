# CLI Overview
Last Modified: 2026-07-15

The Axon CLI is a transport over the same service contracts used by MCP and
REST.

## Rules

- `axon <source>` is the default acquisition and indexing surface.
- `axon scrape <url>` is retained as a one-page source projection.
- Removed commands do not dispatch and do not act as compatibility aliases.
- `--json` output should use transport-neutral DTO envelopes.

## Generated References

Command tables, JSON command metadata, and help snapshots are generated under
`docs/reference/cli/`.

## Ownership

Parsing and terminal rendering belong to `axon-cli` and `axon-core` config
modules. Pipeline behavior belongs behind `axon-services`.
