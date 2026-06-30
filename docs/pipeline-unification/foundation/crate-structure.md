# Crate Structure Contract
Last Modified: 2026-06-30

## Contract

This is the target crate structure. The current workspace still uses the
existing 13-crate shape.

The target workspace is organized by pipeline responsibility and provider
boundary. Crates expose typed public contracts and service entry points; they do
not expose transport-specific internals as the implementation surface.

No double-hyphen crate names. Use `axon-name`, not `axon-two-names`.

## Design Rules

- Keep domain logic below transports.
- Keep DTOs in `axon-api`, except cross-cutting error taxonomy in
  `axon-error` and observability event plumbing in `axon-observe`.
- Keep provider traits close to the boundary crate that owns them.
- Keep adapters in `axon-adapters`.
- Keep ledger, graph, memory, document preparation, embedding, vector storage,
  retrieval, LLM, jobs, errors, observability, services, MCP, web, and CLI
  separate enough to test.
- Transports call `axon-services` and `axon-api`, not domain internals.
- Provider boundaries must have fake/test implementations.
- Clean break: do not keep legacy crate names solely for compatibility.

## Current Implementation Snapshot

Implemented today:

- Current workspace crates are `axon-api`, `axon-authz`, `axon-cli`,
  `axon-code-index`, `axon-core`, `axon-crawl`, `axon-extract`, `axon-ingest`,
  `axon-jobs`, `axon-mcp`, `axon-services`, `axon-vector`, and `axon-web`.
- `axon-vector` currently owns responsibilities that the target split assigns
  to `axon-document`, `axon-embedding`, `axon-vectors`, and `axon-retrieval`.
- `axon-core` currently owns responsibilities that the target split assigns to
  `axon-llm` and part of `ArtifactStore`.
- `axon-code-index` currently owns the implemented local code ledger/generation
  path that the target split generalizes into `axon-ledger`.
- `axon-services` remains the composition layer and facade over current domain
  crates.

Planned by this contract:

- Introduce the target pipeline crates below as implementation work proceeds.
- Do not create double-hyphen crate names.
- Do not keep obsolete names solely for compatibility; this project is allowed
  a clean break.

## Current-to-Target Responsibility Map

Removed or renamed current crates are split by responsibility, not kept as
facades.

| Current Crate | Target Owner(s) | Notes |
|---|---|---|
| `axon-vector` | `axon-document`, `axon-embedding`, `axon-vectors`, `axon-retrieval` | Separate chunk preparation, embedding provider, vector store, and retrieval/RAG. |
| `axon-code-index` | `axon-ledger`, `axon-parse`, `axon-document`, `axon-jobs`, `axon-vectors` | Local code freshness becomes normal source ledger/generation/watch/query behavior. |
| `axon-crawl` | `axon-adapters`, `axon-route`, `axon-ledger`, `axon-document`, `axon-jobs` | Web crawl becomes a web adapter/source job, not a separate job family. |
| `axon-ingest` | `axon-adapters`, `axon-route`, `axon-ledger`, `axon-document`, `axon-jobs` | GitHub, feeds, Reddit, YouTube, registries, sessions, CLI/MCP tools become adapters. |
| `axon-extract` | `axon-llm`, `axon-parse`, `axon-adapters`, `axon-services` | Structured LLM extraction remains a top-level action, but vertical scraping/adapters move under source routing. |
| root `src/*` domain modules | target domain crates | Root crate keeps only binary/bootstrap glue. |

## Target Workspace

| Crate | Owns | Must Not Own |
|---|---|---|
| `axon-error` | typed error taxonomy, retry/degradation/cooling codes, conversions | transport rendering, provider clients |
| `axon-api` | transport-neutral DTOs, enums, envelopes, request/result shapes, error/progress projections | provider clients, runtime side effects |
| `axon-authz` | auth scopes, policy decisions, execution visibility | source acquisition logic |
| `axon-core` | config, paths, redaction, shared utilities, time/id helpers | domain orchestration |
| `axon-observe` | progress events, spans, metrics, structured log fields, heartbeat emitters | job scheduling, transport rendering |
| `axon-route` | source resolution, URL/source normalization, adapter routing | fetching/chunking/vector writes |
| `axon-adapters` | source adapters and acquisition implementations | chunk persistence/vector writes |
| `axon-ledger` | SourceLedger, manifests, diffs, generations, leases, cleanup debt | Qdrant search, graph semantics |
| `axon-parse` | source parsers, manifest parsers, schema/session/tool parsers | graph persistence |
| `axon-graph` | GraphStore trait, SQLite GraphStore, graph candidate ingestion | source fetching, vector search |
| `axon-memory` | MemoryStore, decay, reinforcement, review, forgetting | source adapter lifecycle |
| `axon-document` | DocumentPreparer, ChunkRouter, PreparedDocument construction | embedding provider calls |
| `axon-embedding` | EmbeddingProvider trait, batching, throughput, provider clients | VectorStore persistence |
| `axon-vectors` | VectorStore trait, Qdrant implementation, payload writes/search/delete | embedding generation |
| `axon-retrieval` | query/retrieve/ask context assembly, ranking/fusion | source acquisition |
| `axon-llm` | LlmProvider trait and provider implementations | vector store ownership |
| `axon-prune` | cleanup, purge, dedupe, cleanup debt execution | source discovery |
| `axon-jobs` | JobStore, workers, events, cancellation, heartbeats | transport rendering |
| `axon-services` | orchestration facade and use-case entry points | low-level provider internals |
| `axon-mcp` | MCP server/tool schema and routing | source/vector internals |
| `axon-web` | REST server, OpenAPI, panel routes | CLI rendering |
| `axon-cli` | argv parsing, human/JSON rendering, completions | domain logic |
| `axon` | binary bootstrap | business logic |

## Final Workspace Layout

Target directory shape:

```text
Cargo.toml
src/main.rs
src/lib.rs
crates/
  axon-error/
  axon-api/
  axon-authz/
  axon-core/
  axon-observe/
  axon-route/
  axon-adapters/
  axon-ledger/
  axon-parse/
  axon-graph/
  axon-memory/
  axon-document/
  axon-embedding/
  axon-vectors/
  axon-retrieval/
  axon-llm/
  axon-prune/
  axon-jobs/
  axon-services/
  axon-mcp/
  axon-web/
  axon-cli/
```

Rules:

- The root `axon` crate remains a binary/bootstrap crate only.
- Every crate has `src/lib.rs`; crates with meaningful internal boundaries also
  have `src/CLAUDE.md`.
- Public modules are re-exported intentionally from `lib.rs`; transports do not
  import `ops`, `internal`, `store_schema`, or provider-client internals.
- No crate uses `mod.rs`; follow the workspace sibling-file module convention.
- Generated transport artifacts live under transport crates or app folders, not
  domain crates.

## Cargo Workspace Contract

Root `Cargo.toml` owns:

- workspace member list
- workspace package version/edition/license/authors
- shared dependency versions
- lint policy
- release profile policy

Every crate `Cargo.toml` must:

- inherit `version`, `edition`, `license`, and `authors` from workspace
- depend on sibling crates through `{ workspace = true }`
- keep optional provider dependencies behind named features
- avoid duplicate dependency versions outside `[workspace.dependencies]`
- avoid build scripts unless the crate truly needs codegen/assets

Workspace members must be listed in dependency order where practical. The
detailed per-crate implementation contracts live in
[../crates/README.md](../crates/README.md) and each
`../crates/<crate>/README.md`; this file owns the high-level dependency order
and crate ownership table.

```toml
members = [
  "crates/axon-error",
  "crates/axon-api",
  "crates/axon-authz",
  "crates/axon-core",
  "crates/axon-observe",
  "crates/axon-route",
  "crates/axon-adapters",
  "crates/axon-ledger",
  "crates/axon-parse",
  "crates/axon-graph",
  "crates/axon-memory",
  "crates/axon-document",
  "crates/axon-embedding",
  "crates/axon-vectors",
  "crates/axon-retrieval",
  "crates/axon-llm",
  "crates/axon-prune",
  "crates/axon-jobs",
  "crates/axon-services",
  "crates/axon-mcp",
  "crates/axon-web",
  "crates/axon-cli",
  ".",
]
```

No removed current crate remains in `members` after cutover:

- `crates/axon-vector`
- `crates/axon-code-index`
- `crates/axon-crawl`
- `crates/axon-ingest`
- `crates/axon-extract`

## Crate Public API Contracts

Each crate exposes a small stable surface:

| Crate | Required Public Surface |
|---|---|
| `axon-error` | `ApiError`, `ErrorCode`, stage/severity/retry/degradation taxonomy, provider cooling errors, conversion helpers. |
| `axon-api` | DTOs, enums, envelopes, pagination, error/status/progress projections, capability documents. |
| `axon-authz` | `SecurityPolicy`, auth scopes, execution-affinity decisions, redaction visibility helpers. |
| `axon-core` | config loader, path helpers, id/time helpers, redaction utilities, HTTP safety helpers, artifact primitives if not split. |
| `axon-observe` | `SourceProgressEvent`, heartbeat helpers, tracing span builders, metric instruments, redacted structured logging fields. |
| `axon-route` | `SourceResolver`, `SourceRouter`, `ResolvedSource`, canonical URI/source id utilities. |
| `axon-adapters` | `SourceAdapter`, adapter registry, adapter capability documents, adapter option schemas. |
| `axon-ledger` | `LedgerStore`, source/item/generation models, manifest diffing, leases, cleanup debt. |
| `axon-parse` | `Parser`, parse facts, graph candidates, manifest/schema/session/tool parsers. |
| `axon-graph` | `GraphStore`, graph node/edge/evidence models, merge/conflict APIs. |
| `axon-memory` | `MemoryStore`, memory lifecycle services, decay/reinforcement/review policies. |
| `axon-document` | `DocumentPreparer`, `ChunkRouter`, prepared document/chunk constructors/builders, chunk metadata builders. Serializable shapes remain in `axon-api`. |
| `axon-embedding` | `EmbeddingProvider`, embedding batches, embedding capability docs, scheduler-facing limits. |
| `axon-vectors` | `VectorStore`, collection specs, vector point batches, Qdrant implementation, vector payload builder. |
| `axon-retrieval` | `RetrievalEngine`, query/retrieve/ask context assembly, ranking/fusion/citation DTOs. |
| `axon-llm` | `LlmProvider`, completion/chat/extract/judge requests, streaming deltas, provider implementations. |
| `axon-prune` | prune plans, cleanup debt execution, dedupe, vector/artifact/ledger cleanup services. |
| `axon-jobs` | `JobStore`, worker runtime, scheduler, reservations, events, heartbeats, recovery. |
| `axon-services` | source/query/ask/watch/memory/prune/job use-case entry points only. |
| `axon-mcp` | MCP tool schema, request conversion, auth wrapper, response rendering. |
| `axon-web` | REST router, OpenAPI, SSE, panel/static routes, HTTP response rendering. |
| `axon-cli` | clap parser, command-to-DTO conversion, human/JSON rendering, completions. |

## Per-Crate Implementation Contracts

The detailed crate-level contracts are maintained in
[../crates/README.md](../crates/README.md) and each
`../crates/<crate>/README.md`. The summaries below are retained as a compact
dependency-layer reference. When a module list or owned API drifts, update the
per-crate README first and then update this summary.

### `axon-error`

Purpose: shared error taxonomy and error construction.

Required public modules:

```text
lib.rs
api_error.rs
code.rs
stage.rs
severity.rs
retry.rs
degradation.rs
cooling.rs
context.rs
conversion.rs
testing.rs
```

Must expose:

- `ApiError`
- `ErrorCode`
- `ErrorStage`
- `ErrorSeverity`
- `RetryPolicy`
- `DegradationPolicy`
- `ProviderCooling`
- structured context attachments
- redaction-aware display/debug behavior
- conversions from common provider/store/parser errors
- fake/test constructors

Must not expose:

- CLI/MCP/REST response rendering
- provider clients
- Qdrant/TEI/SQLite/Gemini/Codex concrete clients
- job scheduling

Dependency rule: `axon-error` is below `axon-api` so every crate can depend on
the same error taxonomy without creating cycles.

### `axon-api`

Purpose: transport-neutral contract types.

Required public modules:

```text
lib.rs
envelope.rs
error.rs
source.rs
job.rs
progress.rs
capability.rs
provider.rs
document.rs
graph.rs
memory.rs
retrieval.rs
prune.rs
artifact.rs
config.rs
```

Must expose:

- request/result DTOs used by CLI, MCP, REST, jobs, watches, and services
- `SuccessEnvelope<T>` and `ErrorEnvelope`
- serializable projections for `axon-error::ApiError`
- source/job/progress/status enums
- capability documents for adapters/providers/stores
- pagination/cursor types
- serializable graph/memory/document/vector/prune DTOs

Must not expose:

- provider clients
- Qdrant/TEI/SQLite/Gemini/Codex concrete types
- filesystem/network side effects
- CLI/MCP/REST rendering helpers

Dependency rule: no Axon domain crate dependencies. If a domain crate needs a
shared type, move the type here. `axon-api` may depend on `axon-error` for
error projections.

### `axon-authz`

Purpose: authorization, execution-affinity, and security-policy decisions.

Required public modules:

```text
lib.rs
scope.rs
policy.rs
decision.rs
visibility.rs
testing.rs
```

Must expose:

- `AuthScope`
- `ExecutionAffinity`
- `SecurityDecision`
- `SecurityPolicy`
- local path permission decisions
- tool execution permission decisions
- visibility/redaction policy helpers
- fake/test policy

Must not own:

- source fetching
- SSRF HTTP client implementation
- credential storage
- transport-specific auth middleware

### `axon-core`

Purpose: shared runtime primitives that do not belong to one domain boundary.

Required public modules:

```text
lib.rs
config.rs
paths.rs
ids.rs
time.rs
redact.rs
http_safety.rs
artifact.rs
fs.rs
diagnostics.rs
testing.rs
```

Must expose:

- config loading and effective config snapshots
- path/data-dir helpers
- id/time providers
- redaction primitives
- safe HTTP/URL helpers used by providers/adapters
- artifact handle primitives until/unless `axon-artifacts` is split out
- test clock/id providers

Must not own:

- source pipeline orchestration
- LLM provider implementations after `axon-llm` exists
- embedding/vector/retrieval logic
- transport rendering

### `axon-observe`

Purpose: shared observability event model, tracing, metrics, and heartbeat
helpers.

Required public modules:

```text
lib.rs
event.rs
heartbeat.rs
progress.rs
span.rs
metrics.rs
log_fields.rs
sink.rs
redacted.rs
testing.rs
```

Must expose:

- `SourceProgressEvent`
- heartbeat builders
- monotonic per-job sequence helpers
- tracing span constructors
- bounded-label metric instruments
- structured log field builders
- event sink trait and no-op/test sinks
- redaction-aware observability helpers

Must not own:

- job persistence
- worker scheduling
- transport-specific status rendering
- source pipeline orchestration

### `axon-route`

Purpose: source normalization and adapter routing.

Required public modules:

```text
lib.rs
resolver.rs
router.rs
canonical.rs
authority.rs
source_id.rs
patterns.rs
testing.rs
```

Must expose:

- `SourceResolver`
- `SourceRouter`
- `ResolvedSource`
- `RoutePlan`
- canonical URI utilities
- source id/fingerprint utilities
- adapter candidate matching
- authority map lookup hooks
- fake resolver/router

Must not own:

- full content fetch
- adapter execution
- vector writes
- ledger mutation beyond lookup-friendly ids

### `axon-adapters`

Purpose: acquisition/discovery/fetch for every source family.

Required public modules:

```text
lib.rs
adapter.rs
registry.rs
capability.rs
options.rs
web.rs
local.rs
git.rs
registry_adapters.rs
feed.rs
social.rs
video.rs
sessions.rs
tools.rs
testing.rs
```

Must expose:

- `SourceAdapter`
- `AdapterRegistry`
- adapter capability documents
- adapter option schemas
- source discovery/fetch result types
- source-family adapters for web/local/git/registry/feed/social/video/sessions/tools
- fixture-backed fake adapters

Must emit:

- source manifest candidates
- fetched/acquired items
- `SourceDocument` inputs or raw acquired item values for `axon-document`
- adapter metadata and degraded warnings

Must not own:

- chunking
- embedding
- vector persistence
- generation publishing
- graph persistence

### `axon-ledger`

Purpose: mutable source lifecycle state.

Required public modules:

```text
lib.rs
store.rs
sqlite.rs
source.rs
item.rs
manifest.rs
generation.rs
lease.rs
cleanup_debt.rs
status.rs
testing.rs
```

Must expose:

- `LedgerStore`
- source rows and source item rows
- manifest diffing
- generation creation/commit/read APIs
- source/item leases
- cleanup debt CRUD
- document status join keys
- SQLite implementation
- in-memory fake

Must not own:

- Qdrant searches/deletes directly
- parser/chunker behavior
- graph semantics
- job scheduling

### `axon-parse`

Purpose: structured parse facts and graph candidates.

Required public modules:

```text
lib.rs
parser.rs
registry.rs
facts.rs
graph_candidate.rs
code.rs
markdown.rs
manifest.rs
schema.rs
session.rs
tool.rs
testing.rs
```

Must expose:

- `Parser`
- parser registry
- parse fact DTOs
- graph candidate DTOs
- parsers for code, Markdown, dependency manifests, schemas/OpenAPI, sessions,
  CLI output, MCP tool schemas, env examples, compose files

Must not own:

- graph persistence
- source fetching
- embedding
- vector writes

### `axon-graph`

Purpose: source graph storage and graph candidate ingestion.

Required public modules:

```text
lib.rs
store.rs
sqlite.rs
node.rs
edge.rs
evidence.rs
candidate.rs
merge.rs
query.rs
testing.rs
```

Must expose:

- `GraphStore`
- SQLite graph store
- node/edge/evidence models
- candidate ingestion
- merge/conflict rules
- graph query APIs
- in-memory fake

Must not own:

- source fetching
- vector retrieval
- source ledger lifecycle

### `axon-memory`

Purpose: durable memory lifecycle.

Required public modules:

```text
lib.rs
store.rs
memory.rs
link.rs
decay.rs
reinforce.rs
review.rs
context.rs
testing.rs
```

Must expose:

- `MemoryStore`
- memory CRUD/lifecycle service
- decay/reinforcement policy
- memory context builder
- graph-link integration points
- vector indexing request builder
- fake memory store

Must not own:

- general SourceGraph storage
- LLM synthesis
- vector provider implementation

### `axon-document`

Purpose: normalize `SourceDocument` into `PreparedDocument`.

Required public modules:

```text
lib.rs
source_document.rs
prepared_document.rs
chunk.rs
chunk_router.rs
preparer.rs
metadata.rs
code.rs
markdown.rs
structured.rs
testing.rs
```

Must expose:

- `SourceDocument`
- `PreparedDocument`
- `PreparedChunk`
- `DocumentPreparer`
- `ChunkRouter`
- content-kind chunk profiles
- deterministic document/chunk id builders
- chunk metadata builders
- fake preparer/chunker

Must not own:

- embedding provider calls
- vector point construction
- source ledger mutation
- graph persistence

### `axon-embedding`

Purpose: embedding provider boundary and batching.

Required public modules:

```text
lib.rs
provider.rs
batch.rs
capability.rs
tei.rs
openai_compat.rs
fake.rs
testing.rs
```

Must expose:

- `EmbeddingProvider`
- `EmbeddingBatch`
- `EmbeddingResult`
- embedding provider capability document
- TEI provider
- OpenAI-compatible embeddings provider
- deterministic fake provider

Must not own:

- global scheduler fairness
- vector payload construction
- Qdrant upserts
- source/job orchestration

### `axon-vectors`

Purpose: vector storage boundary.

Required public modules:

```text
lib.rs
store.rs
collection.rs
point.rs
payload.rs
filter.rs
query.rs
delete.rs
qdrant.rs
fake.rs
testing.rs
```

Must expose:

- `VectorStore`
- collection specs
- vector point batch types
- payload builder that consumes `PreparedDocument` + embeddings + metadata
- filter builders
- search/delete/upsert APIs
- Qdrant implementation
- fake vector store

Must not own:

- embedding generation
- source freshness
- ledger cleanup debt ownership
- ask synthesis

### `axon-retrieval`

Purpose: retrieval, ranking, citations, and ask context assembly.

Required public modules:

```text
lib.rs
engine.rs
query.rs
retrieve.rs
ask_context.rs
rank.rs
hybrid.rs
citation.rs
trace.rs
testing.rs
```

Must expose:

- `RetrievalEngine`
- query/retrieve request execution
- hybrid ranking/fusion
- citation assembly
- ask context builder
- retrieval trace DTOs
- fake retrieval engine

Must not own:

- source acquisition
- vector provider implementation internals
- LLM provider implementation

### `axon-llm`

Purpose: LLM provider boundary.

Required public modules:

```text
lib.rs
provider.rs
completion.rs
chat.rs
extract.rs
judge.rs
stream.rs
capability.rs
gemini.rs
openai_compat.rs
codex.rs
fake.rs
testing.rs
```

Must expose:

- `LlmProvider`
- completion/chat/extract/judge DTOs
- streaming delta DTOs
- provider capability document
- Gemini headless provider
- OpenAI-compatible provider
- Codex app-server provider
- fake LLM provider

Must not own:

- retrieval
- vector storage
- source orchestration

### `axon-prune`

Purpose: cleanup, prune, purge, and dedupe execution.

Required public modules:

```text
lib.rs
plan.rs
selector.rs
execute.rs
cleanup_debt.rs
dedupe.rs
report.rs
testing.rs
```

Must expose:

- prune plan builder
- prune selectors
- cleanup debt executor
- dedupe planner/executor
- dry-run report DTOs
- fake executor

Must not own:

- source discovery
- vector store implementation
- ledger store implementation

### `axon-jobs`

Purpose: unified job model and worker runtime.

Required public modules:

```text
lib.rs
store.rs
sqlite.rs
job.rs
attempt.rs
event.rs
heartbeat.rs
scheduler.rs
reservation.rs
worker.rs
runtime.rs
testing.rs
```

Must expose:

- `JobStore`
- unified job/attempt/event/heartbeat models
- provider reservation scheduler
- worker runtime
- retry/recovery/cancellation APIs
- SQLite implementation
- in-memory fake

Must not own:

- source-specific business logic
- transport rendering
- provider implementation internals

Workers call `axon-services` or service traits, but domain crates must not call
back into `axon-jobs` except through job/progress traits explicitly passed in.

### `axon-services`

Purpose: orchestration facade.

Required public modules:

```text
lib.rs
context.rs
source.rs
query.rs
retrieve.rs
ask.rs
extract.rs
memory.rs
jobs.rs
watch.rs
prune.rs
providers.rs
system.rs
testing.rs
```

Must expose:

- service context containing public boundary traits
- source lifecycle service
- query/retrieve/ask services
- extract service
- memory service
- job/watch/prune/provider/system services
- test context builder with fakes

Must not own:

- low-level provider clients
- parser/chunker internals
- vector payload construction
- transport request parsing/rendering

### `axon-mcp`

Purpose: MCP transport projection.

Required public modules:

```text
lib.rs
server.rs
schema.rs
request.rs
response.rs
auth.rs
handlers.rs
testing.rs
```

Must expose:

- one `axon` MCP tool schema
- action/subaction request conversion to `axon-api`
- response envelope rendering
- MCP auth wrapper
- schema tests

Must not import:

- adapter internals
- vector internals
- ledger internals
- provider clients

### `axon-web`

Purpose: REST/OpenAPI/panel transport projection.

Required public modules:

```text
lib.rs
router.rs
openapi.rs
handlers.rs
sse.rs
auth.rs
response.rs
panel.rs
testing.rs
```

Must expose:

- canonical REST router
- OpenAPI generation
- SSE job/progress streams
- auth middleware integration
- response/error rendering
- panel/static route composition

Must not import:

- adapter internals
- vector internals
- ledger internals
- provider clients

### `axon-cli`

Purpose: CLI transport projection.

Required public modules:

```text
lib.rs
parser.rs
commands.rs
render.rs
json.rs
progress.rs
help.rs
completions.rs
testing.rs
```

Must expose:

- clap parser for canonical commands only
- command-to-DTO conversion
- human renderer
- JSON renderer
- progress renderer
- shell completions
- parser/help golden tests

Must not import:

- adapter internals
- vector internals
- ledger internals
- provider clients

### Root `axon`

Purpose: binary bootstrap.

Required files:

```text
src/main.rs
src/lib.rs
build.rs
```

Must expose:

- `run()` re-export from `axon-cli` if needed by tests
- environment/config bootstrap
- web asset embedding build glue if still needed

Must not own:

- command logic
- service logic
- provider logic

## Module Layout Per Crate

Default module layout:

```text
src/lib.rs
src/error.rs
src/types.rs
src/capability.rs
src/service.rs
src/testing.rs
```

Use these only when needed:

| Module | Use |
|---|---|
| `store.rs` | Durable-store trait/implementation facade. |
| `sqlite.rs` | SQLite implementation internals. |
| `qdrant.rs` | Qdrant implementation internals. |
| `provider.rs` | Provider trait and common provider logic. |
| `registry.rs` | Adapter/parser/provider registry. |
| `fake.rs` | Fake implementation when small. |
| `testing/` | Larger fake builders and contract test helpers. |
| `internal/` | Private helpers; never imported by other crates. |

Transport crates may use `handlers/`, `routes/`, or `commands/`. Domain crates
should prefer boundary names over transport names.

## Dependency Direction

Allowed general direction:

```text
axon-error
  -> axon-api
  -> axon-core / axon-authz / axon-observe
  -> axon-route
  -> axon-adapters / axon-ledger / axon-parse / axon-graph / axon-memory
  -> axon-document
  -> axon-embedding / axon-vectors / axon-retrieval / axon-llm / axon-prune
  -> axon-jobs
  -> axon-services
  -> axon-cli / axon-mcp / axon-web
  -> axon
```

Rules:

- `axon-api` depends on no Axon domain crates.
- `axon-error` is below `axon-api`; every crate may depend on it.
- `axon-observe` may be used across crates for events/log fields/spans but must
  not depend on high-level domain orchestration.
- `axon-cli`, `axon-mcp`, and `axon-web` are sibling transports.
- `axon-services` may compose domain crates.
- Domain crates must not import transport crates.
- Provider boundary crates may depend on `axon-api` and `axon-core`.
- Cycles are forbidden.

## Dependency Matrix

Legend:

- `yes`: allowed normal dependency.
- `types`: allowed only for DTO/error/capability types.
- `test`: allowed only as dev-dependency.
- `no`: forbidden.

| Crate | May Depend On |
|---|---|
| `axon-error` | external serde/thiserror/time-style crates only |
| `axon-api` | `axon-error`, external serde/schemars/time/uuid-style crates only; no Axon domain crates |
| `axon-authz` | `axon-error`, `axon-api`, `axon-core` |
| `axon-core` | `axon-error`, `axon-api` for shared primitive DTOs only |
| `axon-observe` | `axon-error`, `axon-api`, `axon-core` |
| `axon-route` | `axon-error`, `axon-api`, `axon-core`, `axon-authz`, `axon-observe`, `axon-graph` types, `axon-ledger` types |
| `axon-adapters` | `axon-error`, `axon-api`, `axon-core`, `axon-route`, `axon-authz`, `axon-observe`, `axon-parse` types |
| `axon-ledger` | `axon-error`, `axon-api`, `axon-core`, `axon-observe`, `axon-route` types |
| `axon-parse` | `axon-error`, `axon-api`, `axon-core`, `axon-observe`, `axon-document` types only if needed |
| `axon-graph` | `axon-error`, `axon-api`, `axon-core`, `axon-observe`, `axon-parse` types, `axon-ledger` types |
| `axon-memory` | `axon-error`, `axon-api`, `axon-core`, `axon-observe`, `axon-graph`, `axon-embedding`, `axon-vectors` |
| `axon-document` | `axon-error`, `axon-api`, `axon-core`, `axon-observe`, `axon-parse` |
| `axon-embedding` | `axon-error`, `axon-api`, `axon-core`, `axon-observe` |
| `axon-vectors` | `axon-error`, `axon-api`, `axon-core`, `axon-observe`, `axon-embedding` types |
| `axon-retrieval` | `axon-error`, `axon-api`, `axon-core`, `axon-observe`, `axon-vectors`, `axon-embedding`, `axon-graph`, `axon-memory`, `axon-llm` types |
| `axon-llm` | `axon-error`, `axon-api`, `axon-core`, `axon-observe` |
| `axon-prune` | `axon-error`, `axon-api`, `axon-core`, `axon-observe`, `axon-ledger`, `axon-vectors`, `axon-graph`, `axon-memory`, `axon-document` |
| `axon-jobs` | `axon-error`, `axon-api`, `axon-core`, `axon-observe`, provider boundary crates, domain service traits |
| `axon-services` | all domain/boundary crates; no transport crates |
| `axon-mcp` | `axon-error`, `axon-api`, `axon-core`, `axon-authz`, `axon-observe`, `axon-services` |
| `axon-web` | `axon-error`, `axon-api`, `axon-core`, `axon-authz`, `axon-observe`, `axon-services` |
| `axon-cli` | `axon-error`, `axon-api`, `axon-core`, `axon-observe`, `axon-services` |
| `axon` | `axon-cli` only, plus bootstrap/build dependencies |

Forbidden dependency examples:

- `axon-api -> axon-services`
- `axon-api -> axon-core` if it introduces runtime behavior
- `axon-error -> axon-api`
- `axon-observe -> axon-services`
- `axon-core -> axon-services`
- `axon-ledger -> axon-vectors`
- `axon-vectors -> axon-ledger`
- `axon-document -> axon-embedding`
- `axon-adapters -> axon-vectors`
- `axon-mcp -> axon-adapters`
- `axon-web -> axon-vectors`
- `axon-cli -> axon-ledger`

When a dependency feels necessary but violates the matrix, promote the shared
type into `axon-api` or introduce a narrow trait in the lower boundary crate.

## Layering Enforcement

The workspace must include an automated layering check.

Required check command:

```text
cargo xtask check-layering
```

The check must fail when:

- a transport crate imports a domain crate directly instead of `axon-services`
- a domain crate imports `axon-cli`, `axon-mcp`, or `axon-web`
- `axon-error` imports any Axon crate
- `axon-api` imports any Axon crate except `axon-error`
- `axon-core` imports `axon-services` or a high-level domain crate
- `axon-observe` imports `axon-services` or transport crates
- a crate imports another crate's `internal`, `ops`, `store_schema`, `handlers`,
  or `commands` modules
- a removed crate remains in workspace members after cutover
- a forbidden feature name such as `legacy-*` or `compat-*` appears
- a cycle appears in `cargo metadata`

The check should be data-driven from this contract or a generated allowlist
checked into the repo. Manual review is not enough.

Allowed import examples:

```rust
use axon_api::source::SourceRequest;
use axon_services::source::run_source;
use axon_embedding::provider::EmbeddingProvider;
```

Forbidden import examples:

```rust
use axon_vector::ops::tei::embed_prepared_docs;
use axon_ledger::store_schema::PROJECTS_TABLE;
use axon_adapters::web::internal::crawl_page;
use axon_web::server::handlers::jobs;
```

## Pipeline Stage Mapping

| Pipeline Stage | Primary Crate | Supporting Crates |
|---|---|---|
| request parse | `axon-cli`, `axon-mcp`, `axon-web` | `axon-api`, `axon-error` |
| observe/error | `axon-observe`, `axon-error` | all stages |
| resolve | `axon-route` | `axon-graph`, `axon-ledger` |
| route | `axon-route` | `axon-adapters`, `axon-authz` |
| acquire | `axon-adapters` | `axon-core`, `axon-llm`, `axon-route` |
| ledger diff/generation | `axon-ledger` | `axon-jobs` |
| parse facts | `axon-parse` | `axon-document` |
| graph ingest | `axon-graph` | `axon-parse` |
| prepare/chunk | `axon-document` | `axon-parse` |
| embed | `axon-embedding` | `axon-jobs` |
| vector write/search | `axon-vectors` | `axon-retrieval` |
| retrieve/ask | `axon-retrieval` | `axon-vectors`, `axon-graph`, `axon-llm`, `axon-memory` |
| memory | `axon-memory` | `axon-embedding`, `axon-vectors`, `axon-graph` |
| cleanup | `axon-prune` | `axon-ledger`, `axon-vectors`, `axon-graph`, `axon-document` |
| jobs/workers | `axon-jobs` | `axon-services` |
| orchestration | `axon-services` | all domain crates |

## Public API Rules

Each crate exposes:

- typed request/result structs or service traits
- explicit errors using `error-handling.md`
- capability structs when applicable
- fake/test implementations for boundaries

Each crate hides:

- transport-specific rendering
- ad hoc JSON blobs when typed DTO exists
- direct access to internal ops modules from transports
- secrets/config internals not needed by callers

## Boundary Traits

Required traits:

| Trait | Crate |
|---|---|
| `SourceResolver` | `axon-route` |
| `SourceAdapter` | `axon-adapters` |
| `LedgerStore` | `axon-ledger` |
| `GraphStore` | `axon-graph` |
| `MemoryStore` | `axon-memory` |
| `DocumentPreparer` | `axon-document` |
| `ChunkRouter` | `axon-document` |
| `EmbeddingProvider` | `axon-embedding` |
| `VectorStore` | `axon-vectors` |
| `LlmProvider` | `axon-llm` |
| `ArtifactStore` | `axon-core` |
| `JobStore` | `axon-jobs` |
| `WatchStore` | `axon-jobs` |
| `SearchProvider` | `axon-adapters` |
| `FetchProvider` | `axon-adapters` |
| `RenderProvider` | `axon-adapters` |

## Old Crate Removal Map

| Current/Legacy Area | Target |
|---|---|
| code index/watch specifics | `axon-ledger`, `axon-document`, `axon-jobs`, `axon-cli` |
| crawl/scrape split | `axon-adapters` web adapter + `axon-route` scopes |
| ingest split | `axon-adapters` registry/git/social/session adapters |
| vector ops | `axon-vectors`, `axon-retrieval`, `axon-embedding` |
| memory feature | `axon-memory` |
| extraction | `axon-llm`, `axon-parse`, top-level `extract` service |
| route/action DTOs | `axon-api` |

Current crate disposition:

| Current Crate | Disposition |
|---|---|
| `axon-api` | keep; expand into full transport-neutral contract crate |
| `axon-authz` | keep; expand security/execution policy boundary |
| `axon-core` | keep; shrink to config/paths/redaction/runtime primitives |
| `axon-vector` | remove after splitting into document/embedding/vectors/retrieval |
| `axon-code-index` | remove after ledger/document/jobs/watch source path absorbs it |
| `axon-crawl` | remove after web adapter/source jobs absorb it |
| `axon-ingest` | remove after adapters/source jobs absorb it |
| `axon-extract` | remove or shrink into parser/LLM extraction service pieces |
| `axon-jobs` | keep; convert to unified job model |
| `axon-services` | keep; keep orchestration only |
| `axon-mcp` | keep; target schema only |
| `axon-web` | keep; target REST/OpenAPI only |
| `axon-cli` | keep; target parser/help only |

Because the cutover assumes empty stores, no current crate is retained solely to
read old tables, old Qdrant payloads, or old job rows.

## Implementation Order

Recommended implementation order:

1. Freeze `axon-api` DTOs/enums/envelopes.
2. Add `axon-route` canonical source normalization.
3. Add/expand `axon-ledger` for every mutable source.
4. Add `axon-document` and move chunking behind `DocumentPreparer`.
5. Add `axon-parse` for manifests/schemas/sessions/tool outputs.
6. Add `axon-graph` boundary and SQLite implementation.
7. Split `EmbeddingProvider` and `VectorStore`.
8. Move source adapters into `axon-adapters`.
9. Move memory into `axon-memory`.
10. Move cleanup into `axon-prune`.
11. Make `axon-services` the orchestration facade.
12. Cut CLI/MCP/REST to the new surfaces.
13. Remove old commands/actions/routes.

## Feature Flag Policy

Feature flags are for optional provider integrations and test/runtime footprint,
not compatibility.

Allowed features:

| Feature | Meaning |
|---|---|
| `qdrant` | Qdrant VectorStore implementation. |
| `sqlite` | SQLite store implementations. |
| `chrome` | Chrome/CDP RenderProvider. |
| `tei` | Native TEI EmbeddingProvider. |
| `openai-compat` | OpenAI-compatible LLM/embedding providers. |
| `gemini` | Gemini headless LLM provider. |
| `codex-provider` | Codex app-server LLM provider. |
| `test-fakes` | Fake/in-memory providers for integration tests. |

Forbidden features:

- `legacy-*`
- `compat-*`
- old command/action/route feature gates
- dual-pipeline feature gates

Feature ownership:

| Feature | Owning Crate | May Enable |
|---|---|---|
| `qdrant` | `axon-vectors` | Qdrant client and integration tests |
| `sqlite` | store crates | SQLite implementations |
| `chrome` | `axon-adapters`/`axon-web` | CDP render/screenshot support |
| `tei` | `axon-embedding` | TEI native provider |
| `openai-compat` | `axon-embedding`, `axon-llm` | OpenAI-compatible providers |
| `gemini` | `axon-llm` | Gemini headless provider |
| `codex-provider` | `axon-llm` | Codex app-server provider |
| `test-fakes` | all boundary crates | fake/in-memory implementations |

Features must not change public command/action/route names. They may only
enable/disable provider implementations or heavyweight optional integrations.

## Services Facade Rules

`axon-services` is allowed to orchestrate. It is not allowed to become the new
monolith.

Rules:

- service functions accept `axon-api` request DTOs or narrowly typed service
  requests
- service functions return `axon-api` result DTOs
- service functions compose domain crates through public traits/services
- no service function builds Qdrant payloads directly
- no service function parses CLI/MCP/REST transport shapes
- no service function reaches into domain `internal`/`ops` modules
- shared validation belongs in services only when it spans multiple boundaries

Required service groups:

| Group | Owns |
|---|---|
| `source` | source lifecycle create/refresh/map/watch request orchestration |
| `query` | query request orchestration |
| `retrieve` | known source/document/chunk retrieval |
| `ask` | retrieval plus LLM synthesis orchestration |
| `extract` | structured extraction orchestration |
| `memory` | memory lifecycle orchestration |
| `jobs` | job status/events/cancel/retry/recover orchestration |
| `prune` | prune plan/execute orchestration |
| `providers` | provider capability/health orchestration |

## Generated Code Ownership

Generated artifacts must have one owner:

| Artifact | Owner |
|---|---|
| OpenAPI JSON | `axon-web` |
| generated TypeScript REST client | app/client workspace, generated from `axon-web` OpenAPI |
| MCP tool schema JSON | `axon-mcp`, sourced from `axon-api` DTOs |
| CLI completions | `axon-cli` |
| JSON schema snapshots | `axon-api` |
| fixture/golden outputs | owning crate's `tests/golden/` |

Generated artifacts must not become source-of-truth contracts. If generated
output disagrees with `axon-api` DTOs or these contracts, fix the generator or
DTOs.

## Cross-Crate Test Layout

Each boundary crate owns:

```text
tests/
  contract.rs
  fake.rs
  golden/
```

Transport parity tests live in `axon-services` or a dedicated integration test
crate if needed:

```text
crates/axon-services/tests/transport_parity.rs
```

Layering tests live in `xtask`:

```text
xtask/src/check_layering.rs
```

No domain crate test may require a live Qdrant/TEI/Chrome/LLM provider unless
the test name and feature clearly mark it as live.

## Testing Requirements

Every boundary crate needs:

- fake in-memory implementation where practical
- contract tests for request/result shapes
- error/degradation tests
- redaction tests where it touches content
- capability reporting tests
- no-cycle/layering check

Transport tests verify:

- CLI/MCP/REST route to same DTOs
- removed commands/actions/routes are absent or fail before side effects
- JSON/envelope shapes match
- progress events match

## Hard Break Position

This restructure is a clean break.

Do not add:

- compatibility crates
- compatibility aliases
- restored old commands
- hidden action shims
- duplicate old/new source pipelines

## Validation Checklist

Implementation is incomplete until:

- target crates exist or current crates clearly map to target names
- layering checks prevent transport-to-domain-internal imports
- all provider boundaries have fakes
- `axon-services` is a thin orchestration facade
- CLI/MCP/REST share `axon-api`
- source acquisition goes through `axon-route` and `axon-adapters`
- document preparation goes through `axon-document`
- vector writes go through `axon-vectors`
- memory lifecycle is isolated in `axon-memory`
- cleanup is isolated in `axon-prune`
