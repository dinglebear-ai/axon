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

- [ ] add missing target crates
- [ ] remove obsolete crates from workspace when their responsibilities move
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
- [ ] add fakes for all stores/providers
- [ ] add provider reservations/cooling/health

Exit criteria:

- fake-boundary tests can run without Qdrant, TEI, LLMs, or live network
- provider saturation is observable and does not overload TEI/LLM backends

## Phase 4: Source Routing And Acquisition

- [ ] implement `SourceResolver`
- [ ] implement `SourceRouter`
- [ ] implement adapter registry
- [ ] port web/local/git/registry/reddit/youtube/rss/session/CLI/MCP sources
- [ ] declare scopes per adapter
- [ ] normalize URL/authority/alias behavior

Exit criteria:

- every source emits `SourceDocument`
- no adapter writes vectors or commits generations directly

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
