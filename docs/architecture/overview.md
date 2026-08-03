---
title: "Axon Architecture"
created: 2026-02-25
updated: 2026-08-02
---

# Axon Architecture

Axon is one Rust product with three transport surfaces over one services layer:

- CLI: `axon <command>`
- MCP: `axon mcp` or MCP-over-HTTP at `/mcp`
- Web and REST: `axon serve` on `127.0.0.1:8001` by default

The transports share the same DTOs, authorization rules, durable jobs, source
pipeline, stores, and provider boundaries. They do not own separate business
logic or source-family runtimes.

## System context

`axon serve` hosts the HTTP API, MCP-over-HTTP endpoint, bundled web panel,
and in-process durable-job workers. The CLI can call services directly for
foreground work or enqueue detached jobs into the same SQLite job store.

External runtime dependencies are intentionally small:

- SQLite for jobs, source ledger, graph, memory, events, and configuration
  snapshots
- Qdrant for vector storage and retrieval
- TEI or an OpenAI-compatible embedding endpoint
- Chrome/CDP for rendered web acquisition when requested
- configured LLM providers through `axon-llm`

There is no Postgres, Redis, RabbitMQ, AMQP broker, external worker service, or
separate per-source database.

## Workspace layers

`src/main.rs` and `src/lib.rs` form a thin root binary shim. Product logic
lives in 23 workspace crates:

```text
transport:     axon-cli        axon-mcp        axon-web
                         \       |       /
composition:                  axon-services
                                   |
runtime:             axon-jobs     axon-observe
                                   |
domain:       axon-adapters  axon-route  axon-ledger  axon-graph
              axon-document  axon-parse  axon-extract
              axon-embedding axon-vectors axon-retrieval axon-llm
              axon-memory    axon-prune
                                   |
shared:          axon-api  axon-authz  axon-core  axon-error
```

Dependency direction and the exact exception ledger are enforced by
`cargo xtask check-layering`. See
[Crate Structure](crate-structure.md), [Crate Ownership](crate-ownership.md),
and [Dependency Layering](dependency-layering.md).

## Unified source pipeline

Every source family enters through `SourceRequest` and returns
`SourceResult`:

```text
SourceRequest
  -> resolve and route                 axon-route
  -> authorize                         axon-authz + axon-services
  -> acquire and normalize             axon-adapters
  -> diff and create generation        axon-ledger
  -> parse and prepare                 axon-parse + axon-document + axon-extract
  -> embed                             axon-embedding
  -> vectorize and upsert              axon-vectors
  -> publish committed generation      axon-ledger
  -> graph committed documents         axon-graph
  -> drain cleanup debt                axon-prune
  -> SourceResult
```

Web, local files, git repositories, package registries, Reddit, YouTube, feeds,
sessions, CLI tools, MCP tools, memory records, and uploads use adapters within
this pipeline. Adapter-specific optimizations do not create alternate publish,
job, or vector paths.

One durable `job_id` crosses the full run. Logs, events, attempts, stages,
heartbeats, artifacts, ledger rows, graph evidence, vector points, and terminal
status all retain that identity.

See [Source Pipeline](source-pipeline.md) for stage ordering and ownership.

## Durable jobs

`axon-jobs` owns one SQLite-backed lifecycle for source, extract, watch, map,
research, ask, query, retrieve, memory, graph, prune, provider-probe, and reset
operations.

Workers run in-process under `axon serve` or in `axon jobs worker`. They
claim jobs atomically, renew heartbeats and provider reservations, honor
cancellation at safe boundaries, and recover stale work through the watchdog.
Retries append a new attempt under the same job identifier.

Canonical statuses are `queued`, `pending`, `running`, `waiting`,
`blocked`, `canceling`, `completed`, `completed_degraded`, `failed`,
`canceled`, `expired`, and `skipped`.

See [Runtime Jobs](../reference/runtime/jobs.md).

## Retrieval and answer synthesis

Committed vector generations are queried through `axon-retrieval` and
`axon-vectors`:

```text
query / retrieve / ask
  -> request planning and filters
  -> dense and optional sparse vector search
  -> committed-generation filtering
  -> ranking and fusion
  -> context and citation assembly
  -> optional LLM synthesis
```

`query` and `retrieve` expose retrieval results. `ask` adds synthesis and
citations through `axon-llm`. Failed or staged generations are not exposed by
default.

## Persistence ownership

| Data | Owner | Store |
|---|---|---|
| durable jobs, attempts, stages, reservations | `axon-jobs` | SQLite |
| sources, manifests, generations, leases, cleanup debt | `axon-ledger` | SQLite |
| graph nodes, edges, evidence | `axon-graph` | SQLite |
| durable memory lifecycle | `axon-memory` | SQLite + Qdrant |
| events and observability records | `axon-observe` | SQLite / structured output |
| vectors and searchable payloads | `axon-vectors` | Qdrant |
| artifacts, document cache, acquisition output | `axon-services` boundaries | filesystem / SQLite metadata |

Migrations are owned by their crates. The generated database inventory is
[docs/reference/runtime/database-schema.md](../reference/runtime/database-schema.md).

## Configuration

Configuration has two intentional sources:

- `.env` for endpoints, credentials, bootstrap, and deployment wiring
- `~/.axon/config.toml` for non-secret behavior and tuning

CLI flags override both for the current invocation. Runtime configuration is
resolved through `axon-core`; transports do not implement separate config
parsers. See [Configuration](../guides/configuration.md).

## Security boundaries

- `axon-authz` owns caller context, scopes, visibility, and execution-affinity
  decisions.
- `axon-core` owns shared redaction and HTTP/SSRF primitives.
- `axon-adapters` enforces acquisition-specific URL, local-path, credential,
  and tool-execution constraints.
- Jobs persist immutable authorization snapshots and workers re-enforce them.
- Destructive operations use plan/execute or explicit confirmation contracts.
- REST and MCP do not infer local trust from network location alone.

See [Runtime Auth](../reference/runtime/auth.md),
[Runtime Security](../reference/runtime/security.md), and
[Redaction](../reference/runtime/redaction.md).

## Deployment

Supported production deployments run the native Axon binary:

- Incus system container, preferred
- bare-metal systemd

Both run `axon serve`; Qdrant, TEI, and Chrome may run as supporting
containers. Docker Compose remains useful for development and infrastructure
reference, but the Axon application binary is not defined as a production
Docker deployment contract.

See [Deployment](../operations/deployment.md).

## Authoritative references

- [Source Pipeline](source-pipeline.md)
- [Crate Structure](crate-structure.md)
- [Boundary Map](boundary-map.md)
- [Runtime Jobs](../reference/runtime/jobs.md)
- [CLI Registry](../reference/cli/commands.md)
- [MCP Tool Contract](../reference/mcp/tool-contract.md)
- [REST Routes](../reference/rest/routes.md)
- [Generated API DTOs](../reference/api/dto.md)
- [Generated Database Schema](../reference/runtime/database-schema.md)
