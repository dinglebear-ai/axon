# Crate Structure
Last Modified: 2026-07-15

Axon is a Cargo workspace with a thin root binary and focused crates under
`crates/`.

## Shape

The root `axon` binary delegates to `axon-cli`. The product crates are layered:
API and error types at the bottom, core utilities and domain crates in the
middle, services as composition, and CLI/MCP/web transports at the edge.

## Current Crate Families

| Family | Purpose |
|---|---|
| `axon-api`, `axon-error`, `axon-authz` | wire contracts, errors, authz |
| `axon-core`, `axon-observe` | config, safety helpers, events |
| `axon-adapters`, `axon-document`, `axon-parse` | acquisition and document preparation |
| `axon-ledger`, `axon-graph` | source identity, generations, graph state |
| `axon-embedding`, `axon-vectors`, `axon-retrieval`, `axon-llm` | embedding, vector storage, retrieval, synthesis |
| `axon-extract`, `axon-memory` | structured extraction and durable memory |
| `axon-jobs` | durable unified job store and workers |
| `axon-services` | orchestration facade |
| `axon-cli`, `axon-mcp`, `axon-web` | transports |

## Removed Crate Rule

Old pre-unification single-purpose crates must not return. Web crawl, ingest,
embed, source-ledger, and code-search behavior belongs in the unified crate
layout above.
