# Implementation Checklist
Last Modified: 2026-06-30

## Contract

This checklist is the pre-plan execution outline. It is intentionally less
detailed than the implementation plan we will write next, but it fixes the
major build phases and exit criteria.

## Phase 0: Contract Freeze

- [ ] run structural doc checks
- [ ] review contradiction sweep
- [ ] verify current implementation sweep still matches code
- [ ] create/refresh GitHub issue body from the issue/PR draft
- [ ] commit the contract packet

Exit criteria:

- contracts link cleanly
- no known contradiction blocks implementation planning
- issue body points to the docs packet

## Phase 1: Workspace Skeleton

- PR0 plan: `docs/pipeline-unification/plans/2026-07-01-target-workspace-skeleton.md`
- [ ] add missing target crates
- [ ] keep transitional crates in the workspace until later responsibility-moving PRs
- [ ] add crate-local `src/CLAUDE.md` files
- [ ] enforce no `mod.rs`
- [ ] add dependency layering check for target crates

Exit criteria:

- workspace builds with placeholder crates
- crate dependency graph is acyclic
- crate source trees match `foundation/repo-structure.md`

## Phase 2: Shared Types And Schemas

- [ ] implement `axon-error`
- [ ] expand `axon-api`
- [ ] implement schema generation for DTOs, errors, events, CLI, MCP, OpenAPI,
  config, database, graph, vector payload, and provider capabilities
- [ ] add stable fixtures and snapshot tests

Exit criteria:

- schema commands generate deterministic artifacts
- transports consume shared DTOs

## Phase 3: Stores And Providers

- [ ] implement `LedgerStore`
- [ ] implement `GraphStore`
- [ ] implement `MemoryStore`
- [ ] implement `VectorStore`
- [ ] implement `EmbeddingProvider`
- [ ] implement `LlmProvider`
- [ ] implement `ArtifactStore`
- [ ] implement `JobStore`
- [ ] implement `ConfigStore`
- [ ] implement `CredentialProvider`
- [ ] implement `DocumentCache`
- [ ] implement `HealthProbe`
- [ ] implement `RateLimiter`
- [ ] implement `WatchStore`
- [ ] implement `SearchProvider`
- [ ] implement `FetchProvider`
- [ ] implement `RenderProvider`
- [ ] implement `NetworkCaptureProvider`
- [ ] implement `SecurityPolicy`
- [ ] keep `DocumentStatus` ledger-owned, not a separate store
- [ ] add strict fakes for all Phase 3 stores/providers
- [ ] add provider reservations/cooling/health
- [ ] generate complete provider capability schema and markdown artifacts
- [ ] enforce `axon-jobs` does not depend on `axon-services`

Exit criteria:

- fake-boundary tests can run without Qdrant, TEI, LLMs, browser, or live network
- provider saturation is observable and cannot starve interactive query/ask lanes
- `docs/reference/runtime/provider-capabilities.schema.json` is not a skeleton artifact
- source routing, URL normalization, authority entrypoints, and adapter registry work remain owned by Phase 4

## Phase 4: Source Resolver, Router, And Route-Time Adapter Registry

- [x] implement `SourceResolver`
- [x] implement `SourceRouter`
- [x] implement route-time adapter registry
- [x] declare scopes per route-time adapter
- [x] normalize URL/authority/alias behavior
- [x] route the live `index_source` entrypoint through `SourceResolver` and `SourceRouter`
- [x] reject unsupported scopes before data-plane checks or acquisition

Exit criteria:

- every source request reaches `SourceRouter` before acquisition dispatch
- route metadata supplies source kind, adapter, canonical URI, and scope in `SourceResult`
- broad source-family acquisition ports remain tracked by the planned PR 12-16 checklist

Source-family acquisition ports are not Phase 4 exit criteria. They remain
tracked by the planned source-family PRs:

- PR12: web page/site/docs crawl port
- PR13: Git provider port
- PR14: feeds/video/social port
- PR15: sessions + registry/package sources port
- PR16: CLI tools/scripts + MCP tool-call sources

## Phase 5: Parse, Graph, Document, And Index Pipeline

- [ ] implement parser registry and parse facts
- [ ] implement graph candidate ingestion
- [ ] implement `DocumentPreparer` and `ChunkRouter`
- [ ] port tree-sitter/code chunking and markdown/session/schema chunking
- [ ] implement vector point batch construction
- [ ] implement generation publish and cleanup debt execution

Exit criteria:

- one source pipeline handles crawl, embed, ingest, sessions, memory, and local
  watch/index paths
- current tree-sitter and payload correctness behavior is preserved

## Phase 6: Unified Jobs And Observability

- [ ] replace family-specific job tables with one durable job model
- [ ] preserve heartbeat, watchdog, panic guard, starvation detector, cancellation
- [ ] implement unified progress phases
- [ ] wire events into CLI, REST/SSE, MCP, logs, traces, and job rows
- [ ] enforce provider reservations across background and interactive work

Exit criteria:

- every async/detached operation is pollable by `job_id`
- embedding/LLM providers are protected from overload
- progress is visible without log scraping

## Phase 7: Surface Cutover

- [ ] implement clean-break CLI command model
- [ ] implement REST route contract
- [ ] implement MCP tool contract
- [ ] update web, palette, Android, and Chrome extension surfaces
- [ ] delete old commands/actions/routes/aliases

Exit criteria:

- CLI/MCP/REST/app parity tests pass
- removed surfaces are absent from generated schemas

## Phase 8: Pruning, Reset, And Empty-DB Cutover

- [ ] implement prune plans and receipts
- [ ] implement cleanup debt execution
- [ ] implement empty-store reset/dev bootstrap
- [ ] remove legacy migration/tombstone concerns

Exit criteria:

- stale vectors/artifacts/ledger/graph rows clean through `axon-prune`
- fresh reindex from empty DB is the supported cutover path

## Phase 9: Documentation And Release Readiness

- [ ] generate docs and schemas
- [ ] update quickstarts and development guides
- [ ] run fake-boundary, contract, and selected live smoke tests
- [ ] create final PR summary and review checklist

Exit criteria:

- docs match generated artifacts
- implementation is ready for review/merge
