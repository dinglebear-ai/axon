# Source Pipeline

Last Modified: 2026-07-19

All source acquisition, refresh, watch, indexing, graph extraction, embedding,
publishing, and cleanup flow through one pipeline. CLI, MCP, and REST are thin
transport projections over the same `SourceRequest` DTO.

> Contract source:
> [`docs/pipeline-unification/foundation/source-pipeline.md`](../pipeline-unification/foundation/source-pipeline.md).
> Implementation orchestrator:
> [`crates/axon-services/src/source.rs`](../../crates/axon-services/src/source.rs)
> (`index_source` → `index_source_with_auth` → `index_source_inner`).

## The pipeline

```text
SourceRequest
  → resolve and route          (axon-route)
  → acquire                    (axon-adapters, per-family)
  → ledger generation + manifest (axon-ledger)
  → normalize / parse / prepare (axon-document, axon-parse, axon-extract)
  → embed                      (axon-embedding)
  → publish / vector write     (axon-vectors, axon-ledger)
  → graph                      (axon-graph)
  → cleanup debt               (axon-prune)
  → SourceResult
```

One durable `job_id` crosses every stage. Logs, events, ledger rows, graph
updates, artifacts, vector payloads, and document status all share it.

## Implemented stage order

The orchestrator (`crates/axon-services/src/source.rs::index_source_with_auth`)
runs in this order. The notable implementation detail: **graphing runs after
publishing**, not before — the graph is derived from the already-committed
manifest.

```text
requested
  → resolving / routing / authorizing   (routing::resolve_authorized_source_route)
  → family dispatch (dispatch_kind)     one call performing:
        acquiring → diffing → preparing → embedding → upserting → publishing
  → graphing                            (graph::write_baseline_graph, reads committed manifest)
  → cleaning                            (prune::drain_cleanup_debt_full_with_boundaries)
  → complete                            (result_map::to_source_result_with_counts)
```

The family adapter owns the inner acquire→prepare→embed→publish run. The
post-publish graph step writes the source container + document nodes/edges from
`counts.graph_candidates`, and the cleanup step drains debt across vector,
graph, memory, jobs, artifact, and document-cache stores.

## Stage → owner map

| Stage | Owning crate |
|---|---|
| `requested` | transport (axon-cli / axon-mcp / axon-web build the `SourceRequest`) |
| `resolving` / `routing` / `authorizing` | `axon-route` (+ `axon-services::source::routing`) |
| `planning` / `leasing` / `discovering` / `diffing` / `fetching` / `enriching` / `normalizing` | `axon-adapters` (family adapter) + `axon-ledger` (diff/lease) |
| `parsing` | `axon-parse` |
| `preparing` / `batching` | `axon-document` |
| `embedding` | `axon-embedding` |
| `vectorizing` / `upserting` | `axon-vectors` |
| `publishing` | `axon-ledger` (generation commit, inside family dispatch) |
| `graphing` | `axon-graph` via `axon-services::source::graph::write_baseline_graph` |
| `cleaning` | `axon-prune` via `axon-services::source::prune` |
| `complete` | `axon-services::source::result_map` |

## SourceRequest → SourceResult

Both DTOs live in `axon-api::source`. Transports construct a `SourceRequest`;
the orchestrator returns a `SourceResult`.

`SourceRequest` required fields: `source`, `intent` (`acquire`/`refresh`/`watch`/`map`),
`embed`, `refresh` (`if_stale`/`force`/`never`), `watch` (`disabled`/`ensure`/`enabled`),
`execution`, `output`, `limits`, `options`. Optional: `scope`, `collection`,
`adapter`, `authority_hint`, `metadata`, `idempotency_key`.

`SourceResult` required fields: `job_id`, `source_id`, `canonical_uri`,
`source_kind`, `adapter`, `scope`, `status` (`queued`/`running`/`degraded`/`failed`/`complete`),
`ledger`, `graph`, `counts`, `warnings`. Optional: `inline`, `job`, `watch`,
`artifacts`, `errors`.

## Adapter families

The family dispatch (`dispatch_kind`) selects one of:

| Adapter | Source examples |
|---|---|
| `web` | pages, docs sites (`--scope page`/`site`/`docs`) |
| `local` | directories, workspaces, repos on disk |
| `git` | GitHub, GitLab, Gitea/Forgejo, generic git (`--scope repo`) |
| `registry` | crates.io, npm, PyPI, Docker (`--scope package`) |
| `reddit` | subreddits (`--scope subreddit`) |
| `youtube` | transcripts |
| `feed` | RSS/Atom |
| `sessions` | Claude/Codex/Gemini exports |
| `cli_tool` | CLI-tool-defined sources |
| `mcp_tool` | MCP-tool-defined sources |
| `memory` | memory records (not a source adapter, but reuses the document/embedding/graph path) |
| `upload` | uploaded files/archives/WARC/Repomix |

Each adapter emits `SourceDocument` values; adapters never emit
`PreparedDocument` directly (that is the DocumentPreparer's job).

## Transport projections

| Operation | CLI | MCP | REST |
|---|---|---|---|
| source run | `axon <source>` | `action=source` | `POST /v1/sources` |
| map | `axon map <source>` | `action=map` | `POST /v1/map` |
| refresh | `axon <source> --refresh` | `action=source refresh=force` | `POST /v1/sources/{id}/refresh` |
| watch | `axon watch create <source>` | `action=watch subaction=create` | `POST /v1/watches` |
| status | `axon jobs get <job_id>` | `action=jobs subaction=get` | `GET /v1/jobs/{job_id}` |

## Execution modes

| Mode | Behavior |
|---|---|
| foreground (`--wait true`) | enqueue, run workers in-process, block to terminal |
| background | enqueue, return job descriptor |
| watch | create/ensure a recurring freshness lifecycle |
| map | discover only, `embed=false` |
| no-embed | acquire/normalize/graph without writing vectors |

## One pipeline, not many

There is no separate pipeline per source family. Source-specific optimization
happens inside adapters, parsers, chunk profiles, and provider configuration —
not by creating a second path. `axon scrape <url>` is a one-page projection of
this same pipeline (`scope=page`, `embed=true`, `limits.max_pages=1`).

If this pipeline changes, update this file and
[`crates/axon-services/src/CLAUDE.md`](../../crates/axon-services/src/CLAUDE.md)
in the same PR.
