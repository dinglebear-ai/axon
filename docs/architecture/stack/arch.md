---
title: "Architecture Stack"
created: 2026-04-04
updated: 2026-08-02
---

# Architecture Stack

Axon is a single Rust binary with three supported transport modes and one shared
runtime:

```text
                         axon
                           |
        +------------------+------------------+
        |                  |                  |
   CLI commands       MCP transport       axon serve
   axon <cmd>         stdio / HTTP         REST + MCP + web
        |                  |                  |
        +------------------+------------------+
                           |
                     axon-services
                           |
        +------------------+------------------+
        |                  |                  |
  unified sources     durable jobs       retrieval / memory
        |                  |                  |
        +------------------+------------------+
                           |
                 SQLite + Qdrant + providers
```

## Transport layer

| Surface | Owner | Contract |
|---|---|---|
| CLI | `axon-cli` | generated command registry |
| MCP | `axon-mcp` | one `axon` tool with action/subaction routing |
| REST, OpenAPI, bundled panel | `axon-web` | direct `/v1` routes and generated OpenAPI |

Transport crates translate wire or user input into `axon-api` DTOs and call
`axon-services`. They do not duplicate routing, authorization, job, source,
retrieval, or storage logic.

## Composition and runtime

`axon-services` composes domain boundaries and owns the end-to-end service
context. `axon-jobs` provides the single durable lifecycle, scheduler,
provider reservations, watch scheduling, workers, heartbeats, and recovery.
`axon-observe` owns progress, events, traces, and metrics.

Workers run inside `axon serve` or `axon jobs worker`. There is no external
queue broker or family-specific worker process.

## Domain layer

- `axon-route`: source identity, canonicalization, adapter and scope routing
- `axon-adapters`: web, local, git, registry, feed, Reddit, YouTube, session,
  tool, and upload acquisition
- `axon-ledger`: source manifests, diffs, generations, leases, publication,
  and cleanup debt
- `axon-document`, `axon-parse`, `axon-extract`: parsing, preparation,
  chunking, and structured extraction
- `axon-embedding`: embedding-provider boundary
- `axon-vectors`: vector-store boundary and Qdrant implementation
- `axon-retrieval`: query, retrieve, ranking, context, and citations
- `axon-graph`: source graph and evidence
- `axon-memory`: durable memory lifecycle
- `axon-llm`: synthesis-provider boundary
- `axon-prune`: cleanup planning and execution

## Shared contracts

- `axon-api`: transport-neutral DTOs, enums, envelopes, and schemas
- `axon-authz`: caller context, scopes, visibility, and policy decisions
- `axon-core`: configuration, paths, HTTP safety, redaction, and shared
  primitives
- `axon-error`: typed error taxonomy

## Storage and providers

| Component | Purpose |
|---|---|
| SQLite | durable jobs, source ledger, graph, memory, events, snapshots |
| Qdrant | dense and sparse vectors plus searchable payloads |
| TEI / OpenAI-compatible endpoint | embeddings |
| Chrome/CDP | rendered web acquisition and screenshots |
| LLM provider | optional answer, research, extraction, and summarization synthesis |

All stateful boundaries have production and fake implementations where tests
need deterministic behavior.

## Configuration stack

```text
CLI flags
   -> ~/.axon/config.toml        non-secret behavior and tuning
   -> .env                       endpoints, credentials, deployment wiring
   -> compiled defaults
   -> validated runtime Config
```

The effective configuration is resolved by `axon-core` and shared by all
transports and workers.

## Deployment stack

The supported application deployments are:

1. Incus system container running the native Axon binary under systemd
2. bare-metal systemd running the native Axon binary

Supporting services such as Qdrant, TEI, and Chrome may run in containers.
`docker-compose.yaml` is the development stack; `docker-compose.prod.yaml`
is the canonical infrastructure image and port reference.

## Enforcement

The repository continuously checks the architecture:

- `cargo xtask check-layering`
- `cargo xtask check-crate-contracts`
- `cargo xtask check-fetch-divergence`
- `cargo xtask check-repo-structure`
- generated CLI, MCP, REST, schema, and public-API drift checks

See [Axon Architecture](../overview.md), [Crate Structure](../crate-structure.md),
and [Source Pipeline](../source-pipeline.md).
