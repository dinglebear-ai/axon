# Boundary Map
Last Modified: 2026-07-15

This page records the clean-break ownership boundaries for Axon after the
source-pipeline unification.

## Rule

Transports translate requests and render responses. They do not own source
acquisition, parsing, embedding, vector publishing, job persistence, auth
policy, or cleanup semantics.

## Primary Boundaries

| Boundary | Owner |
|---|---|
| CLI argument parsing and terminal rendering | `axon-cli` |
| MCP request dispatch | `axon-mcp` |
| REST/web routing | `axon-web` |
| Transport-neutral DTOs | `axon-api` |
| Runtime config, HTTP safety, content utilities | `axon-core` |
| Source acquisition adapters | `axon-adapters` |
| Ledger, graph, documents, parsing, embedding, vectors | domain crates |
| Composition and orchestration | `axon-services` |
| Durable jobs | `axon-jobs` |

## Review Checklist

- A transport never imports a domain crate internal `ops` module.
- New source behavior starts with `SourceRequest`.
- Destructive behavior goes through reset or prune services.
- Observability is emitted through shared pipeline phases and events.
