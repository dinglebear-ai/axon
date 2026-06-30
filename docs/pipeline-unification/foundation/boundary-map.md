# Boundary Map Contract
Last Modified: 2026-06-30

## Contract

This is the target boundary map. Several boundaries already exist in code, but
many are currently combined inside larger crates.

Axon boundaries are the seams where implementations can vary without changing
the source pipeline contract. A boundary deserves an abstraction when there are
multiple implementations, tests need a fake, process/network/security concerns
cross the boundary, provider capabilities change behavior, or failure/retry
semantics differ materially.

Boundaries must be explicit traits/services with capability reporting,
structured errors, and test fakes.

## Promotion Criteria

Promote code to a boundary when any of these are true:

- multiple implementations exist now
- tests need fake/in-memory implementation
- the boundary crosses process, network, filesystem, or security context
- provider capability negotiation changes behavior
- failure handling, retry, cooling, or auth semantics differ materially
- swapping implementation is a real product goal
- the boundary owns durable state
- the boundary emits public contract data

## Current Implementation Snapshot

Implemented today:

- `axon-vector` currently combines document preparation, chunk routing, TEI
  embedding, Qdrant payload construction, Qdrant search/delete, and hybrid
  retrieval behavior.
- `axon-core` owns config, HTTP helpers, artifacts, redaction, and the current
  `LlmProvider` implementations for Gemini headless, OpenAI-compatible, and
  Codex app-server.
- `axon-code-index` owns the specialized local code ledger, generation, manifest
  diff, lease, progress emission, and cleanup debt path.
- `axon-jobs` owns SQLite jobs, family workers, progress JSON persistence, watch
  tables, and current memory tables.
- `axon-services` composes current use cases, including memory orchestration and
  several current command-family service paths.

Planned by this contract:

- Split substitutable provider/state boundaries into explicit crates/traits:
  route, adapters, ledger, parse, graph, memory, document, embedding, vectors,
  retrieval, LLM, error, observe, prune, jobs, and services.
- Add fake/in-memory implementations for provider and durable-state boundaries
  where tests need them.

Do not abstract when:

- there is one simple pure helper
- the code is internal glue with no independent lifecycle
- abstraction would hide important pipeline order
- no tests or providers benefit from substitutability

## Boundary Registry

| Boundary | Primary Crate | Owns | Implementations |
|---|---|---|---|
| `ApiError`/error taxonomy | `axon-error` | stable codes, stages, retry/degradation/cooling semantics | shared constructors, test helpers |
| `ObservabilitySink` | `axon-observe` | progress events, heartbeats, spans, metrics, structured log fields | tracing/log sinks, no-op sink, test sink |
| `SourceResolver` | `axon-route` | raw source normalization, canonical URI, source kind | resolver chain, test fake |
| `SourceRouter` | `axon-route` | adapter/scope/provider selection | default router, test fake |
| `SourceAdapter` | `axon-adapters` | acquisition/discovery/fetch for one source family | web, local, git, registry, social, sessions, tools |
| `LedgerStore` | `axon-ledger` | sources, manifests, generations, leases, cleanup debt | SQLite, in-memory fake |
| `GraphStore` | `axon-graph` | graph nodes/edges/evidence/merge/conflict | SQLite, in-memory fake |
| `MemoryStore` | `axon-memory` | memories, decay, reinforcement, review, forgetting | SQLite, in-memory fake |
| `DocumentPreparer` | `axon-document` | source doc to prepared doc | default preparer, fake |
| `ChunkRouter` | `axon-document` | chunking profile selection | default router, fake |
| `Parser` | `axon-parse` | parse facts and graph candidates | tree-sitter, markdown, schema, session, manifest parsers |
| `EmbeddingProvider` | `axon-embedding` | embeddings and batching capability | TEI, OpenAI-compatible, fake |
| `VectorStore` | `axon-vectors` | vector writes/search/delete/filter indexes | Qdrant, fake |
| `RetrievalEngine` | `axon-retrieval` | query/retrieve/ask context assembly | hybrid retrieval, fake |
| `LlmProvider` | `axon-llm` | synthesis/extraction/judging/chat | Gemini CLI, OpenAI-compatible, Codex app-server, fake |
| `ArtifactStore` | `axon-core` | raw/large/binary artifacts | filesystem, object store later, fake |
| `JobStore` | `axon-jobs` | jobs/events/cancellation/progress persistence | SQLite, in-memory fake |
| `WatchStore` | `axon-jobs` | watch configs/runs/leases | SQLite, fake |
| `SearchProvider` | `axon-adapters` | external search | SearXNG, Tavily, fake |
| `FetchProvider` | `axon-adapters` | HTTP/file/package fetches | reqwest, git, registry clients, fake |
| `RenderProvider` | `axon-adapters` | browser rendering/screenshots | Chrome/CDP, fake |
| `NetworkCaptureProvider` | `axon-adapters` | endpoint discovery captures | Chrome/CDP, fake |
| `CredentialProvider` | `axon-authz` | credential resolution/redaction | env/config/keyring later, fake |
| `SecurityPolicy` | `axon-authz` | SSRF/local execution/auth policy | default policy, test policy |
| `RateLimiter` | provider crates | throughput/rate/backoff | token bucket, fake clock |

## Core Source Pipeline Boundaries

```text
SourceRequest
  -> SourceResolver
  -> SourceRouter
  -> SourceAdapter
  -> LedgerStore
  -> DocumentPreparer / ChunkRouter / Parser
  -> GraphStore
  -> EmbeddingProvider
  -> VectorStore
  -> LedgerStore publish / cleanup debt
```

Rules:

- Resolver/router do not fetch full content.
- Adapters do not write vectors.
- DocumentPreparer does not call embedding provider.
- Parsers do not persist graph directly.
- VectorStore does not own source freshness.
- LedgerStore does not perform semantic search.

## Provider Boundaries

Provider boundaries require capability documents.

Common capability fields:

| Field | Meaning |
|---|---|
| `provider_id` | stable provider id |
| `kind` | provider kind |
| `status` | ready/degraded/cooling/unavailable/disabled |
| `capabilities` | provider-specific features |
| `limits` | batch size, request size, rate, context window |
| `health` | last health check |
| `cooling` | cooldown/backoff state |
| `message` | redacted status message |

## Acquisition Boundaries

Adapters are acquisition boundaries.

Adapter responsibilities:

- declare source kinds/scopes/options
- validate scope-specific options
- discover items
- fetch/acquire changed items
- emit `SourceDocument`
- emit source-specific metadata
- emit parser/chunk hints
- declare degraded modes

Adapters must not:

- commit generations
- write vectors
- persist graph facts directly
- own job scheduling
- define transport command names

## Persistence Boundaries

| Store | Durable State | Transaction Requirements |
|---|---|---|
| `LedgerStore` | sources, items, generations, leases, cleanup debt | generation publish atomic from user perspective |
| `GraphStore` | nodes, edges, evidence, conflicts | idempotent candidate ingestion |
| `MemoryStore` | memories, links, decay/reinforcement events | memory lifecycle transitions atomic |
| `JobStore` | jobs, events, heartbeats, cancellation | event sequence monotonic per job |
| `ArtifactStore` | raw/large files, manifests, screenshots, WARC | content hash verification |
| `VectorStore` | vector points/payload indexes | idempotent upsert/delete by stable keys |

## Execution and Operations Boundaries

These cross local process/security boundaries and need explicit policy:

- CLI tool/script execution
- MCP server/tool invocation
- browser automation/rendering
- local filesystem reads
- network fetch/search
- credential lookup
- destructive prune/purge/delete
- artifact file writes

Execution boundary metadata:

- execution affinity
- side-effect class
- allowlist policy
- credential scope
- timeout
- byte/output limit
- redaction profile
- audit/progress events

## Retrieval Boundaries

Retrieval is not acquisition.

| Operation | Boundary |
|---|---|
| external web search | `SearchProvider` |
| indexed vector search | `VectorStore` + `RetrievalEngine` |
| stored content lookup | `LedgerStore` + `DocumentCache` + `ArtifactStore` |
| RAG answer | `RetrievalEngine` + `LlmProvider` |
| direct chat | `LlmProvider` only |

`search`, `query`, `retrieve`, and `ask` remain separate in CLI/MCP/REST.

## LLM Boundary

`LlmProvider` owns:

- chat completions
- streaming completions
- structured extraction
- summarization
- evaluation/judging
- optional source enrichment

Known provider families:

- Gemini CLI/headless
- OpenAI-compatible HTTP
- Codex app-server
- future ACP-compatible providers when needed
- fake provider for tests

LLM provider capability negotiation includes:

- context window
- streaming support
- JSON schema support
- tool/function calling support
- model id
- rate/concurrency limits
- isolation/security mode

## Artifact Boundary

Artifacts store data too large, binary, raw, or sensitive for inline results.

Artifact kinds:

- raw HTML
- markdown snapshots
- screenshots
- WARC archives
- Repomix outputs
- manifests
- network captures
- tool outputs
- LLM prompt/response artifacts when retained
- upload staging outputs

WARC means Web ARChive: a standard archive format for storing HTTP request and
response records from crawls.

## URL and Graph Boundaries

URL/source normalization is a boundary because authority mapping and canonical
identity affect every downstream store.

Graph is a boundary because it owns typed relationships:

- repo to docs
- package to repo
- package to docs
- source to source
- code file to dependency
- session to tool call
- tool call to artifact/external resource
- CLI/MCP tool to result

Do not encode graph relationships as unstructured vector payload strings only.
Payloads may reference graph ids.

## What Not To Abstract Yet

Do not create a separate boundary for:

- tiny pure formatting helpers
- one-off DTO conversion functions
- simple metadata merge helpers
- one transport renderer
- internal enum-to-string helpers

Promote later when criteria are met.

## Boundary Test Requirements

Every boundary has:

- fake implementation
- capability report
- health/degraded behavior
- structured error behavior
- redaction tests when content crosses it
- timeout/cancellation behavior when async/provider-backed
- contract test proving DTO shape

## Validation Checklist

Implementation is incomplete until:

- all promoted boundaries have traits/service contracts
- fakes exist for tests
- provider capabilities are visible in capabilities/status
- errors/cooling/retry are boundary-owned where appropriate
- transports call services, not provider internals
- adapters cannot write vectors directly
- parsers cannot persist graph directly
- source freshness lives in LedgerStore
- graph relationships live in GraphStore
- local/network/tool execution goes through security policy
