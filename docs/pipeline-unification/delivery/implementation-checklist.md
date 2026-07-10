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

Verified 2026-07-10 against HEAD `5a4558cc7`: all 19 trait definitions below
exist in the workspace (checked via `grep -rl "trait <Name>\b" crates/`).
Checked as done; strict-fake coverage, reservations/cooling/health, and the
generated capability schema/markdown pass are tracked separately below since
they need per-boundary verification, not a single grep.

- [x] implement `LedgerStore` (`crates/axon-ledger/src/store.rs`)
- [x] implement `GraphStore` (`crates/axon-graph/src/store.rs`)
- [x] implement `MemoryStore` (`crates/axon-memory/src/store.rs`)
- [x] implement `VectorStore` (`crates/axon-vectors/src/store.rs`)
- [x] implement `EmbeddingProvider` (`crates/axon-embedding/src/provider.rs`)
- [x] implement `LlmProvider` (`crates/axon-llm/src/provider.rs`)
- [x] implement `ArtifactStore` (`crates/axon-core/src/boundary.rs`)
- [x] implement `JobStore` (`crates/axon-jobs/src/boundary.rs`)
- [x] implement `ConfigStore` (`crates/axon-core/src/boundary.rs`)
- [x] implement `CredentialProvider` (`crates/axon-authz/src/policy.rs`)
- [x] implement `DocumentCache` (`crates/axon-core/src/boundary.rs`)
- [x] implement `HealthProbe` (`crates/axon-core/src/boundary.rs`)
- [x] implement `RateLimiter` (`crates/axon-core/src/boundary.rs`)
- [x] implement `WatchStore` (`crates/axon-jobs/src/boundary.rs`)
- [x] implement `SearchProvider` (`crates/axon-adapters/src/boundary.rs`)
- [x] implement `FetchProvider` (`crates/axon-adapters/src/boundary.rs`)
- [x] implement `RenderProvider` (`crates/axon-adapters/src/boundary.rs`)
- [x] implement `NetworkCaptureProvider` (`crates/axon-adapters/src/boundary.rs`)
- [x] implement `SecurityPolicy` (`crates/axon-authz/src/policy.rs`)
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

Verified 2026-07-10 against HEAD `5a4558cc7`: all six boundary types below are
implemented; wiring every source family through this pipeline uniformly
(rather than command/service-specific paths) remains open per
`source-pipeline.md`'s "Partially implemented" snapshot.

- [x] implement parser registry and parse facts (`axon-parse`: docker/env/config/tool families)
- [x] implement graph candidate ingestion (`GraphStore::upsert_candidates`, wired via `axon-services::source::graph::write_baseline_graph`)
- [x] implement `DocumentPreparer` and `ChunkRouter` (`crates/axon-document/src/preparer.rs`, `chunk_router.rs`)
- [x] port tree-sitter/code chunking and markdown/session/schema chunking (`crates/axon-document/src/code.rs` + chunk router families)
- [x] implement vector point batch construction (`crates/axon-vectors/src/point.rs`)
- [x] implement generation publish and cleanup debt execution (`LedgerStore::publish_generation`, `prune::drain_cleanup_debt`)

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
