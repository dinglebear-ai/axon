# Source Pipeline Contract
Last Modified: 2026-07-14

## Contract

This is the clean-break contract. The web acquisition slice is now implemented
through SourceRequest; other source families still have follow-up work called
out below.

All source acquisition, refresh, watch, indexing, graph extraction, embedding,
publishing, and cleanup flow through one pipeline.

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

No source adapter may bypass this path for normal indexing. Source-specific
optimization happens inside adapters, parsers, chunk profiles, and provider
configuration, not by creating a second pipeline.

## Current Implementation Snapshot

Implemented today:

- The shared post-acquisition boundary exists in `axon-vector`: callers either
  build `SourceDocument` explicitly or call helpers that create a
  `SourceDocument` internally, `prepare_source_document` creates `PreparedDoc`,
  and `embed_prepared_docs` writes through the TEI/Qdrant embedding pipeline.
- The standalone `axon-code-index` crate named in earlier drafts of this doc
  does not exist in the workspace. Code-search's real SQLite ledger/generation
  path lives inside `axon-vector` (see `crates/axon-vector/src/ops/qdrant/filter.rs`,
  `.../tei/qdrant_store/payload_indexes.rs`, and `axon-route`'s
  `local-code://` id helpers) alongside `axon-vectors` (the target crate). It
  tracks local projects, files, hashes, sizes, mtimes, pending state,
  committed generation, leases, and cleanup debt.
- `SourceRequest`/`SourceResult` and the envelope DTOs in `axon-api` are
  implemented and exercised, not target-only: `SourceRequest` alone has 40+
  call sites across `axon-services` and `axon-cli` (see
  `crates/axon-api/src/source_tests.rs`, `crates/axon-api/src/mcp_schema.rs`).
  The target pipeline's *types* exist and are unit-tested; what remains
  unwired is routing every source family's command/service/job path through
  them uniformly (see "Partially implemented" below).
- Code-search local vectors use `local-code://<project_key>/g/<generation>/<path>`
  item URLs and payload fields such as `source_type=local_code`,
  `local_project_key`, `local_generation`, and `code_file_path`.
- Code-search reindexing performs manifest diff, creates the next generation,
  marks files pending, embeds manifest batches for the refresh, marks files
  indexed, commits the generation, and then runs generation-fenced cleanup debt.
  Today that refresh path can re-prepare and re-embed all tracked files in the
  manifest after any diff, even though the diff is still used to decide whether
  a refresh is needed and to clean removed files.

Partially implemented:

- Local code indexing shares `SourceDocument -> PreparedDoc -> embed_prepared_docs`,
  but its ledger/generation/cleanup model is not the general source pipeline yet.
- Web page/site/docs acquisition now enters through `SourceRequest` and Source
  jobs. `axon scrape <url>` is retained as a one-page projection with
  `scope=page`, `embed=true`, `limits.max_pages=1`, clean content output, and
  no crawl fanout. Bare web sources with `--scope site|docs`, search/research
  auto-index, refresh, and URL watch enqueue Source jobs rather than legacy
  Crawl jobs or child Embed jobs.
- `axon crawl <url>` is reserved at CLI routing time with replacement guidance
  to use `axon <url> --scope site|docs`. The old MCP `crawl` action and REST
  `/v1/crawl` route are removed from public surfaces. Legacy `JobKind::Crawl`
  rows are migration-only and are dead-lettered instead of recovered/requeued.
- Refresh still facets Qdrant payloads by `source_type`/`seed_url`; web origins
  re-enter through Source jobs, while non-web legacy snapshot handling remains
  follow-up until all source families share the universal SourceLedger.
- URL watch and code-search watch are separate systems today. URL watch is a
  SQLite URL change detector that dispatches web refreshes as Source jobs;
  `code-search-watch` is reserved/removed from the public command surface and
  local-code watch behavior remains follow-up.
- `graphing` currently runs **after** `publishing`, not before it as the Stage
  Registry order below implies. `axon-services::source::index_source_with_auth`
  dispatches acquire+prepare+embed+publish through the family bridge first,
  then calls `graph::write_baseline_graph` (which reads the already-published
  manifest to build the source container + document nodes/edges) and only
  after that runs `prune::drain_cleanup_debt`. So the real order is
  `... -> upserting -> publishing -> graphing -> cleaning -> complete`, with
  graph writes derived from committed state rather than gating it. See
  `crates/axon-services/src/source.rs::index_source_with_auth` and
  `crates/axon-services/src/source/graph.rs::write_baseline_graph`.

Planned by this contract:

- `SourceRequest`, `SourceResolver`, `SourceRouter`, `SourceLedger`,
  `SourceGraph`, `DocumentStatus`, `EmbeddingProvider`, `VectorStore`, and
  cleanup debt are the single path for every source family.
- `axon <source>`, REST `/v1/sources`, and MCP `action=source` are projections
  over the same request shape.
- `axon scrape <url>` is a CLI convenience projection over the same request
  shape for exactly one web page; it embeds by default and must not crawl.
- Site/docs crawl behavior is the source surface: `axon <url> --scope site` or
  `axon <url> --scope docs`, REST `/v1/sources` with the same scope, or MCP
  `action=source`.
- Graph extraction, source ledger rows, document status rows, progress events,
  and vector payloads all share the same `job_id`, source identifiers, and
  generation identifiers.

## Design Rules

- `SourceRequest` is the entry shape for acquisition/indexing.
- `axon <source>`, REST `/v1/sources`, and MCP `action=source` are transport
  projections over the same request.
- Adapters emit `SourceDocument`, never `PreparedDocument`.
- `DocumentPreparer` emits `PreparedDocument`, never vector points.
- `EmbeddingProvider` emits embeddings, not VectorStore payload policy.
- `VectorStore` writes point batches; it is not the source ledger.
- `SourceLedger` owns mutable source state, manifests, diffs, generations,
  leases, publish state, and cleanup debt.
- `SourceGraph` owns nodes, edges, evidence, merge/conflict rules, and graph
  queries.
- `DocumentStatus` owns per-document lifecycle state.
- Every job has one `job_id` that crosses logs, events, ledger rows, graph
  updates, artifacts, vector payloads, and status.

## Stage Registry

| Stage | Owner | Input | Required Output | May Degrade | May Mutate |
|---|---|---|---|---:|---:|
| `requested` | transport | CLI/MCP/REST input | `SourceRequest` | no | no |
| `resolving` | `SourceResolver` | `SourceRequest` | `ResolvedSource` | yes | cache only |
| `routing` | `SourceRouter` | `ResolvedSource` | selected adapter/scope/providers | no | no |
| `authorizing` | auth/security | route plan | access/execution decision | no | no |
| `planning` | pipeline | route plan | `SourcePlan` | yes | job row |
| `leasing` | `LedgerStore` | source/job | lease | no | yes |
| `discovering` | adapter | source plan | manifest item candidates | yes | artifacts |
| `diffing` | `LedgerStore` | manifest | added/modified/removed/unchanged sets | yes | ledger |
| `fetching` | adapter/providers | changed items | fetched/acquired items | yes | artifacts |
| `enriching` | enrichment pipeline | fetched/acquired items + source metadata | `SourceEnrichment[]` | yes | status/artifacts |
| `normalizing` | adapter | fetched items | `SourceDocument[]` | yes | document cache |
| `parsing` | parser/preparer | source docs | `SourceParseFacts[]`, `GraphCandidate[]` | yes | no |
| `graphing` | `GraphStore` | graph candidates | graph nodes/edges/evidence | yes | graph |
| `preparing` | `DocumentPreparer` | source docs | `PreparedDocument[]` | yes | status |
| `batching` | embedding pipeline | prepared docs | `EmbeddingBatch[]` | yes | no |
| `embedding` | `EmbeddingProvider` | embedding batches | embedding vectors | yes | provider metrics |
| `vectorizing` | vector payload builder | vectors + chunks | `VectorPointBatch[]` | no | no |
| `upserting` | `VectorStore` | point batches | write result | yes | vectors |
| `publishing` | `GenerationPublisher` | complete generation | committed generation | no | ledger/status |
| `cleaning` | `Prune`/ledger | cleanup debt | cleanup result | yes | vectors/artifacts/ledger |
| `complete` | pipeline | terminal result | `SourceResult` | no | status/events |

## SourceRequest

Required fields:

| Field | Type | Meaning |
|---|---|---|
| `source` | string | User/source URI, path, shorthand, package id, repo id, or source id. |
| `intent` | enum | `acquire`, `refresh`, `watch`, `map`. |
| `embed` | bool | Whether to store vectors. Defaults true except map/no-embed. |
| `refresh` | enum | `if_stale`, `force`, `never`. |
| `watch` | enum | `disabled`, `ensure`, `enabled`. |
| `execution` | object | foreground/background/wait policy. |
| `output` | object | response mode, artifact/path preferences. |
| `limits` | object | source/page/file/chunk/provider limits. |
| `options` | object | adapter-specific options validated by capability schema. |

Optional fields:

| Field | Type | Meaning |
|---|---|---|
| `scope` | string | Adapter-declared acquisition strategy. |
| `collection` | string | Vector collection override. |
| `adapter` | string | Forced adapter when supported. |
| `authority_hint` | object | Official/community/pinned source hints. |
| `metadata` | object | User/system metadata additions. |
| `idempotency_key` | string | Deduplicate job creation. |

## SourceResult

Required fields:

| Field | Type | Meaning |
|---|---|---|
| `job_id` | string | Durable job id. |
| `source_id` | string | Stable source id. |
| `canonical_uri` | string | Canonical source URI. |
| `source_kind` | string | Source kind. |
| `adapter` | string | Adapter name/version. |
| `scope` | string | Scope executed. |
| `status` | enum | `queued`, `running`, `degraded`, `failed`, `complete`. |
| `ledger` | object | Generation, item, cleanup state. |
| `graph` | object | Graph update summary. |
| `counts` | object | Item/document/chunk/vector counts. |
| `warnings` | array | Non-fatal warnings. |

Optional fields:

| Field | Type | Meaning |
|---|---|---|
| `inline` | object | Small inline result. |
| `job` | object | Pollable job descriptor. |
| `watch` | object | Watch descriptor. |
| `artifacts` | array | Artifact refs. |
| `errors` | array | Structured errors. |

## Stage Inputs and Outputs

### Resolving

Inputs:

- raw source string
- requested scope/adapter
- authority hints
- local path context
- known source aliases
- SourceGraph authority registry

Outputs:

- canonical URI
- source kind
- candidate adapters/scopes
- selected/default scope
- authority and confidence
- normalized source display label
- warnings for ambiguous resolution

### Routing

The router validates the resolved source against adapter capability.

Route plan includes:

- adapter name/version
- scope
- provider requirements
- credential requirements
- execution affinity
- safety class
- default chunking hints
- watch/refresh support
- expected item kind
- option schema validation result

### Discovering and Diffing

Mutable/refreshable sources must build a manifest before expensive work when
possible.

Manifest item fields:

- `source_item_key`
- canonical item URI/path
- item kind
- size/hash/mtime/version when known
- parent/source relationship
- fetch plan
- graph hints

Diff result:

| Set | Meaning |
|---|---|
| `added` | new item absent from previous manifest |
| `modified` | item key present but content/version changed |
| `removed` | previous item no longer present |
| `unchanged` | item can reuse committed state |
| `skipped` | ignored by policy/limit |
| `failed` | item could not be discovered/diffed |

### Generation Lifecycle

Mutable sources write into a new generation.

Rules:

- Search uses only committed generation by default.
- New generation is committed only after required items are prepared, embedded,
  and status rows are publish-safe.
- Failed generations do not become visible for normal retrieval.
- Cleanup debt is created for superseded generations and removed items.
- Cleanup may run after publish but must be idempotent.
- Immutable sources may use source version/commit instead of numeric generation,
  but ledger accounting still applies.

### Enrichment

`SourceEnrichment` is the optional stage between source generation/acquisition
and normalized documents. It may call `LlmProvider`, but it is not named after
the provider because enrichment can also come from registry APIs, static
analyzers, structured metadata, authority maps, or source-specific adapters.

Enrichment may produce:

- extracted fields
- classification labels
- summaries
- parser hints
- chunk hints
- source metadata
- graph candidates
- warnings/degraded status
- artifacts for large enrichment output

Rules:

- Enrichment is optional and capability-driven.
- Required enrichment failures keep the generation uncommitted.
- Optional enrichment failures produce warnings/degraded status.
- Enrichment must not write vector points.
- Enrichment must not persist graph data directly; it emits `GraphCandidate`.
- Enrichment output is redacted before entering public metadata or artifacts.

### Normalizing

Adapters normalize acquired items into `SourceDocument`.

Normalization owns:

- content text/binary refs
- canonical item URI
- source item key
- content kind
- shared metadata
- safe source-specific metadata
- artifact refs for raw/large data
- chunk/parser hints

### Parsing and Preparing

Parsing and chunking use `chunking-contract.md`.

Outputs:

- `SourceParseFacts`
- `GraphCandidate`
- `PreparedDocument`
- warnings/errors
- preparation metrics

### Embedding and Vectorizing

Embedding pipeline owns:

- batching
- throughput controls
- provider retries/cooling
- embedding model metadata
- vector payload construction
- VectorStore writes

Rules:

- If `embed=false`, documents may still be normalized, parsed, graphed, and
  stored in ledger/cache as configured, but no vector points are written.
- Vector payloads follow `metadata-payload.md`.
- Failed embedding before publish keeps generation uncommitted unless the source
  policy explicitly allows degraded partial publish.

### Publishing and Cleanup

Publishing commits the generation atomically from the user's perspective.

Required publish checks:

- source generation exists
- required documents have terminal status
- vector writes are complete or degraded by policy
- graph writes are complete or degraded by policy
- cleanup debt is recorded
- status/events show final state

Cleanup owns:

- old vector points
- stale document cache entries
- stale artifacts when retention allows
- stale graph evidence when no longer valid
- old generations beyond retention

## Source Categories

| Category | Examples | Mutable | Watchable | Generation Required |
|---|---|---:|---:|---:|
| web docs/site | docs sites, pages | yes | yes | yes |
| local files/repos | directories, workspaces, repos | yes | yes | yes |
| hosted git | GitHub/GitLab/Gitea repos | branch yes, commit no | yes for branch | branch yes |
| packages/registries | crates/npm/pypi/docker/etc. | version no, package yes | yes | yes for package/latest |
| feeds/social/media | RSS, Reddit, YouTube | yes | yes | yes |
| sessions | Claude/Codex/Gemini exports | append-only/mutable | yes | yes |
| CLI/MCP tools | tool schemas/calls | depends on call | yes when configured | yes |
| uploads | files/archives/WARC/Repomix | immutable after upload | no | optional |
| memory | memory records | yes | lifecycle-owned | memory-owned |

Memory is not a source adapter, but may reuse document, embedding, graph, and
status paths.

## Execution Modes

| Mode | Behavior |
|---|---|
| foreground | run now and render progress |
| background | enqueue and return job descriptor |
| wait | block until terminal state |
| watch | create/ensure recurring freshness lifecycle |
| map | discover only; `embed=false` |
| no-embed | acquire/normalize/graph without vector storage |

Transport projections:

- CLI: `--wait`, `--watch`, `--no-embed`, `--refresh`
- REST: request body `execution`, `watch`, `embed`, `refresh`
- MCP: action fields `wait`, `watch`, `embed`, `refresh`

## Provider Boundaries

| Boundary | Owns |
|---|---|
| `SearchProvider` | external search results |
| `FetchProvider` | HTTP/file/package fetches |
| `RenderProvider` | browser rendering/screenshots/automation |
| `LlmProvider` | extraction, synthesis, judging, enrichment |
| `EmbeddingProvider` | embedding batches |
| `VectorStore` | vector point writes/search/deletes |
| `LedgerStore` | source state, manifests, generations, leases |
| `GraphStore` | graph nodes/edges/evidence |
| `MemoryStore` | durable memory lifecycle |
| `ArtifactStore` | large/raw/binary outputs |
| `JobStore` | jobs, events, progress, cancellation |

## Error and Degradation Semantics

Stage failures map to `error-handling.md`.

General rules:

- resolver failures fail the request
- unsupported scope fails before acquisition
- provider unavailability may fail or degrade depending on required/optional
  role
- parser/chunker failures usually degrade to fallback
- redaction failures fail before public/vector output
- embedding failure before publish fails the generation unless policy allows
  partial degraded publish
- cleanup failure after publish records cleanup debt and degraded status

## Observability

Every stage emits:

- start event
- completion event
- duration
- counts
- warnings/errors
- degraded reason when applicable

Progress event shape is defined in `observability-contract.md`.

## Transport Crosswalk

| Pipeline Operation | CLI | MCP | REST |
|---|---|---|---|
| source run | `axon <source>` | `action=source` | `POST /v1/sources` |
| map | `axon map <source>` | `action=map` | `POST /v1/map` |
| refresh existing | `axon <source> --refresh` | `action=source refresh=force` | `POST /v1/sources/{id}/refresh` |
| watch | `axon watch <source>` | `action=watches subaction=create` | `POST /v1/watches` |
| status | `axon jobs get <job_id>` | `action=jobs subaction=get` | `GET /v1/jobs/{job_id}` |
| events | `axon jobs events <job_id>` | `action=jobs subaction=events` | `GET /v1/jobs/{job_id}/events` |

## Validation Checklist

Implementation is incomplete until:

- every transport creates the same `SourceRequest`
- every adapter emits `SourceDocument`
- every normal embed path consumes `PreparedDocument`
- source generations are committed only after publish checks
- search defaults to committed generation
- cleanup debt is created for stale generations/items
- all stages emit observable progress
- every provider boundary is mockable/testable
- `embed=false` never writes vectors
- `map` never writes vectors by default
- errors/degradation follow `error-handling.md`
- metadata follows `metadata-payload.md`
- chunking follows `chunking-contract.md`
