# Boundary Map

Last Modified: 2026-07-19

Each capability in Axon has one owning boundary (a trait + its primary
implementation crate). Transports translate requests and render responses;
they never own source acquisition, parsing, embedding, vector publishing, job
persistence, auth policy, or cleanup semantics.

> Contract source:
> [`docs/pipeline-unification/foundation/boundary-map.md`](../pipeline-unification/foundation/boundary-map.md).
> The crate-ownership rule is in [crate-ownership.md](crate-ownership.md).

## Boundary registry

| Boundary | Primary crate | Owns |
|---|---|---|
| `ApiError` / error taxonomy | `axon-error` | stable codes, stages, retry/degradation/cooling |
| Transport-neutral DTOs | `axon-api` | `SourceRequest`/`SourceResult`, job DTOs, MCP schema, envelopes |
| `SecurityPolicy` / `CredentialProvider` | `axon-authz` | SSRF, local-execution, auth scope, credential resolution |
| Config / paths / redaction / artifacts | `axon-core` | `ArtifactStore`, HTTP safety, time/id |
| `ObservabilitySink` | `axon-observe` | progress events, heartbeats, spans, metrics, log fields |
| `SourceResolver` / `SourceRouter` | `axon-route` | raw-source normalization, canonical URI, adapter/scope selection |
| `SourceAdapter` | `axon-adapters` | per-family acquisition/discovery/fetch |
| `SearchProvider` | `axon-adapters` | external web search (SearXNG, Tavily) |
| `FetchProvider` | `axon-adapters` | HTTP/file/package fetches |
| `RenderProvider` / `NetworkCaptureProvider` | `axon-adapters` | Chrome/CDP rendering, screenshots, endpoint discovery |
| `Parser` | `axon-parse` | parse facts + graph candidates (tree-sitter, markdown, schema, session, manifest) |
| `DocumentPreparer` / `ChunkRouter` | `axon-document` | source doc → prepared doc, chunk-profile selection |
| `LedgerStore` | `axon-ledger` | sources, manifests, generations, leases, cleanup debt |
| `GraphStore` | `axon-graph` | nodes/edges/evidence/merge/conflict |
| `MemoryStore` | `axon-memory` | memories, decay, reinforcement, review, forgetting |
| `EmbeddingProvider` | `axon-embedding` | embeddings + batching (TEI, OpenAI-compat) |
| `VectorStore` | `axon-vectors` | vector writes/search/delete/filter (Qdrant) |
| `RetrievalEngine` | `axon-retrieval` | query/retrieve/ask context assembly, ranking/fusion |
| `LlmProvider` | `axon-llm` | synthesis/extraction/judging/chat (Gemini, OpenAI-compat, Codex) |
| Prune / cleanup executor | `axon-prune` | cleanup-debt execution, dedupe |
| `JobStore` / `WatchStore` | `axon-jobs` | jobs, events, cancellation, progress, watches |
| Composition / runtime | `axon-services` | orchestration facade + ServiceContext (the source runner) |

Each boundary has at least one real implementation and an in-memory fake for
tests. Provider boundaries (Search/Fetch/Render/Embedding/Vector/Llm) expose
capability/health/limit metadata per the
[provider capability schema](../reference/runtime/provider-capabilities.schema.json).

## Source-pipeline boundary chain

```text
SourceRequest
  → SourceResolver → SourceRouter → SourceAdapter
  → LedgerStore (manifest/diff/generation)
  → DocumentPreparer / ChunkRouter / Parser
  → GraphStore (graph candidates)
  → EmbeddingProvider → VectorStore
  → LedgerStore (publish / cleanup debt)
```

## Anti-rules (what a boundary must not do)

- **Resolver/router** must not fetch full content.
- **SourceAdapter** must not write vectors, commit generations, persist graph
  facts, own job scheduling, or define transport command names.
- **DocumentPreparer** must not call the embedding provider.
- **Parser** must not persist graph data directly (it emits `GraphCandidate`).
- **VectorStore** does not own source freshness and is not the source ledger.
- **LedgerStore** does not perform semantic search and has no Qdrant/FS/LLM
  access (enforced by `cargo xtask check-layering`).
- **Transports** never import a domain crate's internal `::ops::*` modules.

## Retrieval vs. acquisition (separate verbs, separate boundaries)

| Verb | Boundaries used |
|---|---|
| `search` (external) | `SearchProvider` |
| `query` (indexed vector search) | `VectorStore` + `RetrievalEngine` |
| `retrieve` (stored content lookup) | `LedgerStore` + DocumentCache + `ArtifactStore` |
| `ask` (RAG answer) | `RetrievalEngine` + `LlmProvider` |
| `chat` (direct LLM) | `LlmProvider` only |

These stay separate in CLI, MCP, and REST — they are not folded into `source`.

## Review checklist

- A transport never imports a domain crate internal `ops` module.
- New source behavior starts with `SourceRequest`.
- Destructive behavior goes through `reset` or `prune` services.
- Observability is emitted through shared pipeline phases and events.
- Every provider boundary is fakeable for tests.

If a boundary moves or a new one is added, update this file and the relevant
`crates/<name>/src/CLAUDE.md` in the same PR.
