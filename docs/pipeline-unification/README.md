# Pipeline Unification Contracts
Last Modified: 2026-06-30

This folder is the design contract packet for GitHub issue
[#298](https://github.com/jmagar/axon/issues/298). It supersedes the narrower
command-only framing in issue #280.

These files are implementation contracts for the desired clean break. Many of
them also contain a `Current Implementation Snapshot` section sourced from a
read-only review of the current code. Treat those snapshots as the bridge from
today's Axon to the target contract:

- **Implemented today** describes behavior that exists in the current checkout.
- **Partially implemented** describes behavior with a real foothold but not the
  final shared boundary.
- **Planned by this contract** describes the end-state implementation target.

The snapshots are not compatibility promises. They exist so implementation work
can replace the current surface deliberately without pretending old commands,
routes, DTOs, payload fields, or crate boundaries have already disappeared.

The goal is one source pipeline, not a CLI cleanup:

```text
SourceRequest
  -> SourceResolver
  -> SourceRouter
  -> SourceAcquisition
  -> SourceManifestDiff
  -> SourceGeneration
  -> SourceEnrichment
  -> SourceDocument
  -> SourceParseFacts / GraphCandidate
  -> SourceGraph
  -> DocumentPreparer
  -> PreparedDocument
  -> EmbeddingBatch
  -> EmbeddingProvider
  -> VectorPointBatch
  -> VectorStore
  -> DocumentStatus
  -> GenerationPublisher
  -> CleanupDebt
```

CLI, MCP, REST, jobs, and watch configuration must all map into the same
transport-neutral contract. The shared model lives in `axon-api` and
`axon-services`; transports are thin adapters.

## Directory Map

```text
docs/pipeline-unification/
  README.md
  foundation/
  crates/
  sources/
  runtime/
  surfaces/
  schemas/
  configuration/
  delivery/
```

### Foundation

Core architecture, shared DTOs, crate structure, and boundary ownership.

| File | Purpose |
|---|---|
| [foundation/source-pipeline.md](foundation/source-pipeline.md) | End-to-end shared pipeline and ownership boundaries. |
| [foundation/api-contract.md](foundation/api-contract.md) | Transport-neutral DTOs and service-layer ownership. |
| [foundation/type-and-service-contract.md](foundation/type-and-service-contract.md) | Registry for concrete DTOs, enums, stage results, traits, services, stores, and providers. |
| [foundation/boundary-map.md](foundation/boundary-map.md) | Named responsibility/provider boundaries across ingest, retrieval, and operations. |
| [foundation/crate-structure.md](foundation/crate-structure.md) | Target Rust workspace crate boundaries after source-pipeline unification. |
| [foundation/repo-structure.md](foundation/repo-structure.md) | End-state repository tree and per-crate tree templates. |
| [foundation/shared-utilities-contract.md](foundation/shared-utilities-contract.md) | `axon-core` utility/helper rules and promotion criteria. |

Detailed type/service contracts:

| File | Purpose |
|---|---|
| [foundation/types/dto-contract.md](foundation/types/dto-contract.md) | Concrete request/result/document/batch/state DTO shapes. |
| [foundation/types/enum-contract.md](foundation/types/enum-contract.md) | Closed enum variants and JSON names. |
| [foundation/types/stage-result-contract.md](foundation/types/stage-result-contract.md) | Per-stage result DTOs, counts, degradation, and event rules. |
| [foundation/types/trait-contract.md](foundation/types/trait-contract.md) | Executable domain boundary traits and fake requirements. |
| [foundation/types/service-contract.md](foundation/types/service-contract.md) | `axon-services` orchestration service traits. |
| [foundation/types/store-contract.md](foundation/types/store-contract.md) | Durable store traits, transactions, leases, and reset behavior. |
| [foundation/types/provider-contract.md](foundation/types/provider-contract.md) | External/provider traits, capabilities, reservations, cooling, and fakes. |

### Crates

Per-crate implementation contracts for the target workspace shape. Each crate
directory contains a human implementation contract in `README.md` and an
agent-facing maintenance contract in `CLAUDE.md`.

| File | Purpose |
|---|---|
| [crates/README.md](crates/README.md) | Crate contract index, standard sections, dependency rules, and acceptance criteria. |

### Sources

Source identity, adapters, chunking, metadata, and graph contracts.

| File | Purpose |
|---|---|
| [sources/adapter-scopes.md](sources/adapter-scopes.md) | Adapter-declared scopes and source capabilities. |
| [sources/new-source-contract.md](sources/new-source-contract.md) | Full implementation contract for bringing a new source online. |
| [sources/url-normalization.md](sources/url-normalization.md) | URI, URL, alias, authority, and canonicalization rules. |
| [sources/parsing-contract.md](sources/parsing-contract.md) | `axon-parse` parser registry, parse facts, graph candidates, and parser families. |
| [sources/chunking-contract.md](sources/chunking-contract.md) | `SourceDocument` to `PreparedDocument` routing contract. |
| [sources/metadata-payload.md](sources/metadata-payload.md) | Shared and source-specific VectorStore/ledger payload fields. |
| [sources/source-graph.md](sources/source-graph.md) | SourceGraph node, edge, authority, and evidence contract. |

### Runtime

Execution model, providers, storage, observability, errors, and security.

| File | Purpose |
|---|---|
| [runtime/job-contract.md](runtime/job-contract.md) | One durable job model, attempts, events, heartbeats, scheduling, and retries. |
| [runtime/ledger-contract.md](runtime/ledger-contract.md) | SourceLedger manifests, diffs, generations, leases, and cleanup debt. |
| [runtime/memory-contract.md](runtime/memory-contract.md) | Durable memory lifecycle, recall, decay, review, graph, vector, and context contract. |
| [runtime/provider-contract.md](runtime/provider-contract.md) | Provider capabilities, reservations, cooling, fakes, and throughput boundaries. |
| [runtime/storage-contract.md](runtime/storage-contract.md) | SQLite/Qdrant/artifact/cache ownership, retention, cleanup debt, and restore. |
| [runtime/schema-contract.md](runtime/schema-contract.md) | SQLite table ownership, migration rules, indexes, and integrity requirements. |
| [runtime/observability-contract.md](runtime/observability-contract.md) | Progress, heartbeat, logs, traces, status, and metrics. |
| [runtime/error-handling.md](runtime/error-handling.md) | Error taxonomy, degradation, retry, and safe failure behavior. |
| [runtime/security-contract.md](runtime/security-contract.md) | SSRF, local path, secret, redaction, artifact, and tool-execution policy. |
| [runtime/auth-contract.md](runtime/auth-contract.md) | Caller context, scopes, visibility, and job auth propagation. |
| [runtime/redaction-contract.md](runtime/redaction-contract.md) | Redactor boundary, detectors, metadata visibility, and fail-closed behavior. |
| [runtime/pruning-contract.md](runtime/pruning-contract.md) | Prune plans, cleanup debt execution, dedupe, and destructive cleanup safety. |

### Surfaces

Human CLI, REST/OpenAPI, MCP, app surfaces, presentation, and target help
output.

| File | Purpose |
|---|---|
| [surfaces/command-contract.md](surfaces/command-contract.md) | Human CLI command model. |
| [surfaces/rest-contract.md](surfaces/rest-contract.md) | REST/OpenAPI mapping to the shared source model. |
| [surfaces/tool-contract.md](surfaces/tool-contract.md) | MCP tool schema and response envelope contract. |
| [surfaces/axon-help.md](surfaces/axon-help.md) | Hand-rolled target `axon --help` / `axon help` output. |
| [surfaces/web-contract.md](surfaces/web-contract.md) | Browser web app contract for control panel and setup flows. |
| [surfaces/palette-contract.md](surfaces/palette-contract.md) | Palette Tauri desktop app contract. |
| [surfaces/android-contract.md](surfaces/android-contract.md) | Android app REST/SSE/mobile-session contract. |
| [surfaces/chrome-extension-contract.md](surfaces/chrome-extension-contract.md) | Chrome extension capture and browser-context contract. |
| [surfaces/presentation-contract.md](surfaces/presentation-contract.md) | Shared design token, palette, icon, density, and accessibility contract. |

### Schemas

Generated and machine-readable shape contracts.

| File | Purpose |
|---|---|
| [schemas/README.md](schemas/README.md) | Schema inventory, generation rules, naming, and checks. |
| [schemas/schema-generator-contract.md](schemas/schema-generator-contract.md) | `xtask` schema generator architecture, fixtures, snapshots, and CI contract. |
| [schemas/mcp-tool-schema.md](schemas/mcp-tool-schema.md) | Exact generated MCP tool schema, action/subaction enums, and drift checks. |
| [schemas/openapi-schema.md](schemas/openapi-schema.md) | REST/OpenAPI artifact contract. |
| [schemas/cli-schema.md](schemas/cli-schema.md) | Machine-readable CLI command/flag schema. |
| [schemas/api-dto-schema.md](schemas/api-dto-schema.md) | `axon-api` DTO/envelope/enum schema contract. |
| [schemas/config-schema.md](schemas/config-schema.md) | `.env` and `config.toml` schema generation contract. |
| [schemas/event-schema.md](schemas/event-schema.md) | `axon-observe` event/heartbeat/metric schema contract. |
| [schemas/error-schema.md](schemas/error-schema.md) | `axon-error` error taxonomy schema contract. |
| [schemas/database-schema.md](schemas/database-schema.md) | SQLite table/index/migration schema contract. |
| [schemas/graph-schema.md](schemas/graph-schema.md) | SourceGraph node/edge/evidence schema contract. |
| [schemas/vector-payload-schema.md](schemas/vector-payload-schema.md) | VectorStore payload and payload-index schema contract. |
| [schemas/provider-capability-schema.md](schemas/provider-capability-schema.md) | Provider capability/health/limit schema contract. |

### Configuration

Desired boot/config split.

| File | Purpose |
|---|---|
| [configuration/env-contract.md](configuration/env-contract.md) | Desired `.env` shape: URLs, secrets, runtime/bootstrap, compose only. |
| [configuration/config-contract.md](configuration/config-contract.md) | Desired `config.toml` shape: compact tuning and behavior defaults. |

### Delivery

How the clean break is tested, cut over, and old surfaces are removed.

| File | Purpose |
|---|---|
| [delivery/testing-contract.md](delivery/testing-contract.md) | Unit, fake-boundary, transport parity, job, provider, and live-smoke tests. |
| [delivery/documentation-contract.md](delivery/documentation-contract.md) | Final docs inventory, generated docs, freshness checks, and doc ownership. |
| [delivery/docs-generator-contract.md](delivery/docs-generator-contract.md) | `xtask` docs generator architecture, examples, fixtures, and CI contract. |
| [delivery/current-implementation-sweep.md](delivery/current-implementation-sweep.md) | Current codebase implementation inventory used to ground the clean-break contracts. |
| [delivery/implementation-checklist.md](delivery/implementation-checklist.md) | Pre-plan phase checklist and exit criteria. |
| [delivery/implementation-plan.md](delivery/implementation-plan.md) | Ordered execution plan, guardrails, phase proofs, and the selected one-vertical spike. |
| [delivery/dependency-order-map.md](delivery/dependency-order-map.md) | Dependency gates, do-not-start rules, and parallelizable implementation tracks. |
| [delivery/first-implementation-pr.md](delivery/first-implementation-pr.md) | Exact first PR scope and follow-up PR boundaries. |
| [delivery/issue-pr-draft.md](delivery/issue-pr-draft.md) | GitHub issue and PR body draft for the contract packet. |
| [delivery/contradiction-review.md](delivery/contradiction-review.md) | Cross-contract and current-implementation contradiction sweep. |
| [delivery/cutover-contract.md](delivery/cutover-contract.md) | Empty-database clean-slate cutover and reindex assumptions. |
| [delivery/surface-removal-contract.md](delivery/surface-removal-contract.md) | Removed command/action/route/field/config-key deletion rules. |

## Recommended Reading Order

1. [foundation/source-pipeline.md](foundation/source-pipeline.md)
2. [foundation/api-contract.md](foundation/api-contract.md)
3. [foundation/type-and-service-contract.md](foundation/type-and-service-contract.md)
4. [foundation/crate-structure.md](foundation/crate-structure.md)
5. [crates/README.md](crates/README.md)
6. [foundation/boundary-map.md](foundation/boundary-map.md)
7. [foundation/shared-utilities-contract.md](foundation/shared-utilities-contract.md)
8. [foundation/repo-structure.md](foundation/repo-structure.md)
9. [runtime/ledger-contract.md](runtime/ledger-contract.md)
10. [sources/parsing-contract.md](sources/parsing-contract.md)
11. [runtime/job-contract.md](runtime/job-contract.md)
12. [runtime/memory-contract.md](runtime/memory-contract.md)
13. [runtime/provider-contract.md](runtime/provider-contract.md)
14. [runtime/auth-contract.md](runtime/auth-contract.md),
   [runtime/security-contract.md](runtime/security-contract.md), and
   [runtime/redaction-contract.md](runtime/redaction-contract.md)
15. [runtime/error-handling.md](runtime/error-handling.md) and
   [runtime/observability-contract.md](runtime/observability-contract.md)
16. [sources/adapter-scopes.md](sources/adapter-scopes.md)
17. [sources/new-source-contract.md](sources/new-source-contract.md)
18. [surfaces/command-contract.md](surfaces/command-contract.md),
   [surfaces/rest-contract.md](surfaces/rest-contract.md), and
   [surfaces/tool-contract.md](surfaces/tool-contract.md)
19. [surfaces/web-contract.md](surfaces/web-contract.md),
   [surfaces/palette-contract.md](surfaces/palette-contract.md),
   [surfaces/android-contract.md](surfaces/android-contract.md),
   [surfaces/chrome-extension-contract.md](surfaces/chrome-extension-contract.md),
   and [surfaces/presentation-contract.md](surfaces/presentation-contract.md)
20. [schemas/README.md](schemas/README.md)
21. [configuration/env-contract.md](configuration/env-contract.md) and
   [configuration/config-contract.md](configuration/config-contract.md)
22. [delivery/documentation-contract.md](delivery/documentation-contract.md),
   [delivery/current-implementation-sweep.md](delivery/current-implementation-sweep.md),
   [delivery/implementation-checklist.md](delivery/implementation-checklist.md),
   [delivery/contradiction-review.md](delivery/contradiction-review.md),
   [delivery/testing-contract.md](delivery/testing-contract.md),
   [delivery/cutover-contract.md](delivery/cutover-contract.md), and
   [delivery/surface-removal-contract.md](delivery/surface-removal-contract.md)

## Non-Negotiables

- `axon-error` owns typed error taxonomy; `axon-observe` owns event/span/metric
  plumbing; `axon-api` owns transport-neutral projections and envelopes.
- `axon-services` owns resolution, orchestration, SourceLedger/SourceGraph
  coordination, execution-affinity handling, and shared validation.
- CLI, MCP, and REST must not reimplement routing decisions.
- Every adapter emits `SourceDocument`; adapters do not emit `PreparedDocument`
  directly.
- Every mutable or refreshable source has ledger-owned lifecycle state.
- Every async or detached operation uses the unified job model. Former crawl,
  embed, ingest, extract, watch, research, prune, and reset jobs are job
  kinds/stages, not separate infrastructure concepts.
- Provider throughput is scheduled globally. Bulk source embedding, watch
  refreshes, and maintenance work must not starve interactive ask/query/retrieve.
- LLM completion, embedding, vector storage, ledger persistence, graph storage,
  memory storage, artifacts, credentials/secrets, cache, and
  health checks are separate provider boundaries. See
  [foundation/boundary-map.md](foundation/boundary-map.md) for the promoted
  boundary list and promotion criteria.
- Every async or detached operation returns a pollable status descriptor.
- Removed commands/actions/routes are deleted from normal schemas. There are no
  compatibility aliases and no public tombstone window.
- Existing local indexed data does not need to migrate. Assume empty stores and
  reindex after the refactor lands.
- `.env` is only for URLs, secrets, runtime/bootstrap values, and compose
  interpolation. Non-secret tuning lives in `config.toml`.
- Source-specific optimization is allowed behind adapters and chunk routers, but
  the external pipeline shape stays one path.
- Parsing, ledger, auth, schema, pruning, redaction, and shared utilities each
  have explicit owner contracts; do not hide them inside generic helpers.

## Naming

Use these words consistently:

- **source**: a user-addressable thing Axon can acquire or refresh.
- **adapter**: source-specific acquisition and normalization logic.
- **scope**: adapter-declared acquisition strategy, such as `page`, `site`,
  `repo`, `package`, or `subreddit`.
- **source item**: the smallest ledger-tracked input unit, such as a file, page,
  feed entry, transcript, package version, issue, or PR.
- **generation**: a publishable snapshot of a mutable source.
- **prepared document**: post-chunk, pre-embedding data ready for the vector
  pipeline.
- **authority**: confidence/evidence that a source is official, inferred,
  community-maintained, mirrored, or unknown.
