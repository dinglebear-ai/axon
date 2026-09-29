---
title: "Architecture Overview -- Axon"
created: 2026-04-04
updated: 2026-09-27
---

# Architecture overview

Axon is one Rust application with CLI, MCP, and HTTP/web projections over
shared typed services. The unified source pipeline is implemented, not a
future migration. This page is the short orientation; see the current
[architecture overview](../overview.md), [crate map](../crate-structure.md),
and [ownership rules](../crate-ownership.md) for the detailed boundaries.

## Entry points

| Surface | Entry point | Ownership |
|---|---|---|
| CLI | axon commands and bare source targets | axon-cli |
| MCP | axon mcp, or the /mcp endpoint hosted by axon serve | axon-mcp |
| REST and web panel | axon serve, /v1 routes, and panel routes | axon-web |

The thin root binary composes these crates. Transport handlers validate and
map requests; they do not own a second implementation of source acquisition,
provider retries, document preparation, or storage operations. Shared wire
DTOs belong in axon-api; single-domain behavior stays with the domain, while
axon-services composes cross-domain work and the runtime.

## One source pipeline

~~~text
SourceRequest
  -> resolve and route (axon-route)
  -> acquire (axon-adapters)
  -> source identity, generation, manifest (axon-ledger)
  -> normalize, parse, prepare (axon-document / axon-parse / axon-extract)
  -> embed (axon-embedding)
  -> publish and retrieve (axon-vectors / axon-retrieval)
  -> graph and cleanup debt (axon-graph / axon-prune)
~~~

The source job retains one durable job ID across its stages. Focused scrape,
crawl, embed, and ingest projections reuse this pipeline; they do not create
independent queues or hand a source job off to a separate embedding job.
Adapters acquire source content, while preparation, providers, and stores
retain their separate ownership. See [source pipeline](../source-pipeline.md)
and [adding a source](../../development/adding-source.md).

## Durable work and persistent state

Axon stores jobs in SQLite and runs workers in the same Tokio runtime as the
server. The unified lifecycle owns attempts, stages, events, heartbeats,
artifacts, cancellation, recovery, and provider reservations. Watches enqueue
ordinary source jobs and record their runs rather than becoming a second
execution engine.

Use axon jobs commands to inspect and control work. A detached submission
requires a process with active workers; returning a job ID is not proof that
the work completed. The exact state machine and command shapes live in the
[job lifecycle reference](../../reference/job-lifecycle.md) and
[generated CLI registry](../../reference/cli/commands.md).

The ledger owns source generations and document state. Graph, memory,
observability, and provider caches remain separate domain stores. Consult the
[generated database schema](../../reference/runtime/database-schema.json)
for tables, migration owners, and constraints; do not maintain duplicate table
counts or a second schema here.

## Retrieval and synthesis

Queries operate over committed indexed state through the retrieval domain.
Embedding, vector storage, ranking, context assembly, and synthesis use their
configured providers and typed policies. Source adapters do not write directly
to Qdrant or shell out to an LLM from a transport handler. See the
[RAG guide](../../guides/ask-rag.md) and
[configuration reference](../../guides/configuration.md) for current options.

## MCP and HTTP boundaries

The primary axon MCP tool dispatches action/subaction requests. The catalog
also includes axon_status_dashboard; task handling and resources are part of
the transport. Inspect crates/axon-mcp/src/server.rs and the matching runtime
catalog instead of assuming the primary action schema lists every tool.

Non-loopback HTTP requires OAuth or the configured static bearer token.
Panel setup/configuration routes have their own password/session boundary;
an API bearer token is not a substitute for a panel unlock. See
[MCP documentation](../../reference/mcp/overview.md) and
[security](../../operations/security.md).

## Deployment

Supported production deployments run the native Axon binary under systemd,
either in the documented Incus system container or on bare-metal Linux.
Qdrant, TEI, and Chrome/CDP are external providers, commonly hosted in
containers or on other machines. Compose remains useful for reference and
local provider infrastructure; running Axon itself in Docker is not the
supported production lifecycle. macOS is a development host.

See [deployment](../../operations/deployment.md) for the operational path.
The [pipeline-unification packet](../../pipeline-unification/README.md)
preserves the implemented design and dated delivery history; its old
timelines do not supersede the live implementation.
