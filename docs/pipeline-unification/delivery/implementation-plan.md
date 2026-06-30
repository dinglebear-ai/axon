# Pipeline Unification Implementation Plan
Last Modified: 2026-06-30

## Contract

This is the implementation plan for the clean-break pipeline unification. The
contracts under `docs/pipeline-unification/` are authoritative; this file orders
that work into shippable phases and names the exact proof required before
moving forward.

This is not a rewrite plan. Existing working code should be moved, extracted,
renamed, and wrapped behind the new boundaries where it still fits. Rewrite only
the parts whose ownership model conflicts with the contracts: public routing,
job lifecycle, ledger ownership, payload construction, schema generation,
cleanup, and observability.

No compatibility aliases are required. Existing indexed data, legacy jobs, old
payloads, and old local stores can be reset and reindexed after the cutover.

## Guardrails

- Implement one boundary at a time behind contract tests.
- Preserve useful current implementation behavior before deleting old paths.
- Do not wire a new public surface until its DTO, generated schema, tests, and
  removal checks exist.
- Every phase must leave the workspace buildable.
- Every source adapter emits `SourceDocument`, never `PreparedDocument` or
  vector points directly.
- Every async/detached operation creates one `job_id`.
- Every mutable or refreshable source is ledger-owned before it is searchable.
- Every vector payload is produced from the shared payload builder.
- Provider reservations protect TEI, Qdrant, LLMs, browser rendering, search,
  and other shared provider boundaries from overload.

## Phase 0: Contract Freeze And Issue Sync

Goal: make the docs packet and GitHub issue the shared implementation source of
truth.

Tasks:

- Run structural doc checks.
- Keep `docs/pipeline-unification/delivery/current-implementation-sweep.md`
  aligned with the current codebase.
- Keep `docs/pipeline-unification/delivery/implementation-checklist.md` as the
  phase checklist.
- Update GitHub issue #298 to point to the docs packet instead of carrying
  stale contract text.
- Add this implementation plan to the docs index.

Proof:

- `git diff --check`
- targeted contract consistency checks for canonical enums, job kinds, vector
  payload generation fields, and removed surface names
- issue #298 links to the canonical docs packet

## Phase 1: Shared DTO And Enum Spine

Goal: add the transport-neutral data model before moving behavior.

Tasks:

- Implement `axon-api::source` with `SourceRequest`, `ResolvedSource`,
  `SourceResult`, source scopes, source intents, refresh/watch policies,
  execution mode, output mode, and source options.
- Move canonical enums from markdown into Rust-owned registries.
- Add serde/schemars/utoipa coverage for DTOs.
- Add fixtures for minimal and full source requests.
- Add removed-field rejection tests for old `EmbedRequest`/`IngestRequest`/
  `CrawlRequest` names once the schema generator lands.

Proof:

- `cargo test -p axon-api source`
- generated schema fixture snapshots once `xtask schemas` exists

## Phase 2: Schema Generator And Contract Drift Checks

Goal: stop hand-maintaining mirrored schema lists.

Tasks:

- Add `cargo xtask schemas generate`.
- Implement family generators for API DTOs, errors, events, CLI, MCP, OpenAPI,
  config, database, graph, vector payload, and providers.
- Emit generated markdown and JSON artifacts.
- Validate valid and invalid fixtures.
- Fail when canonical enum projections drift.
- Fail when removed commands/actions/routes/config keys/DTO fields reappear.

Proof:

- `cargo xtask schemas generate --check`
- generated docs include source inputs and checksums
- CI fails on intentionally stale fixture in a local negative test

## Phase 3: Stores, Providers, And Fakes

Goal: make the durable and external boundaries testable before rewiring source
flows.

Tasks:

- Add or reshape `LedgerStore`, `GraphStore`, `MemoryStore`, `VectorStore`,
  `EmbeddingProvider`, `LlmProvider`, `ArtifactStore`, `JobStore`,
  `ConfigStore`, `CredentialProvider`, `DocumentCache`, `HealthProbe`,
  `RateLimiter`, `UrlResolver`, and `AuthorityRegistry`.
- Add in-memory/fake implementations for each boundary.
- Implement provider reservations, cooling, health, and backpressure.
- Add fake-boundary tests for provider saturation and graceful degradation.

Proof:

- fake tests run without Qdrant, TEI, LLMs, browser, or network
- provider saturation does not starve interactive query/ask paths

## Phase 4: Source Resolver, Router, And Adapter Registry

Goal: route every source target through one resolver/router before acquisition.

Tasks:

- Implement `SourceResolver`.
- Implement `SourceRouter`.
- Implement adapter capability/scopes registry.
- Normalize URI/URL/path/package/repo/session/tool inputs.
- Add authority mapping and URL entrypoint resolution.
- Keep `map` as a first-class action/route.

Proof:

- resolver fixtures cover local paths, scheme-less docs domains, GitHub
  shorthand, full git URLs, registry package IDs, Reddit, YouTube, RSS, session
  exports, CLI tools, and MCP tools
- ambiguous inputs return reason/confidence and warnings

## Phase 5: One Source Vertical Spike

Goal: prove one real source can flow through the target shape without beginning
the full migration.

Selected spike: local file/directory source.

Why this vertical:

- It already uses the shared `SourceDocument -> PreparedDoc ->
  embed_prepared_docs` path.
- It exercises source resolution, path metadata, content kind routing,
  document preparation, code/markdown chunking, embedding, and vector payload
  construction.
- It is cheap to test with local fixtures and does not need network, GitHub,
  browser, Reddit, YouTube, or registry credentials.

Spike boundary:

- Add `SourceRequest` DTOs and tests.
- Add a local-source adapter prototype behind an internal module or test-only
  harness.
- Convert a local file/directory request into the existing prepare path.
- Do not remove old `embed`, `code-search`, or watch commands.
- Do not wire public CLI/MCP/REST to the new path yet.
- Do not migrate job tables or ledger tables yet.

Proof:

- local Markdown and Rust fixtures produce `SourceDocument`/prepared chunk
  metadata with canonical source fields
- existing `prepare_embed_docs` behavior remains unchanged
- spike notes list what can be moved versus rewritten for local embed/watch

## Phase 6: Ledger-Owned Source Lifecycle

Goal: generalize code-index generation safety to all mutable sources.

Tasks:

- Implement source/generation/item/manifest/document/cleanup tables.
- Make `SourceLedger` own freshness, manifest diffing, generation publish, and
  cleanup debt.
- Use committed generations for search.
- Prevent generation churn when providers are unavailable before first write.
- Move stale cleanup out of custom Qdrant scroll paths.

Proof:

- interrupted generation is not searchable
- publish is atomic from the user perspective
- cleanup failure records debt and does not unpublish new generation

## Phase 7: Document, Parser, Graph, And Payload Pipeline

Goal: make every source prepare and store through one document pipeline while
keeping source-specific optimizations.

Tasks:

- Implement parser registry and graph candidate ingestion.
- Implement `DocumentPreparer` and `ChunkRouter`.
- Move code/tree-sitter, Markdown, transcript, structured, session, package,
  API/schema, and plain-text chunking behind the router.
- Implement shared vector payload builder.
- Add source-specific metadata registries.

Proof:

- mixed local/repo fixtures route code and Markdown to the right chunkers
- vector payload fixtures validate and contain no secrets
- graph facts are optional and degradation is explicit

## Phase 8: Unified Jobs And Observability

Goal: replace family-specific queue semantics with one job model.

Tasks:

- Implement one durable jobs table family.
- Model work with `job_kind`, `job_intent`, attempts, stages, events,
  heartbeats, and artifacts.
- Wire `ObservabilitySink` into jobs and domain crates.
- Preserve watchdog, panic guard, cancellation, recovery, and starvation
  detector behavior.
- Emit progress to CLI, REST/SSE, MCP, logs, traces, and job rows.

Proof:

- every detached operation is pollable by `job_id`
- heartbeats prove liveness without log scraping
- failed/degraded events include structured `ApiError`/warning payloads

## Phase 9: Port Source Families

Goal: move each source family onto the shared source pipeline.

Order:

1. local file/directory
2. local watch/code index
3. web page/site/docs crawl
4. GitHub/GitLab/Gitea/generic git
5. RSS/Atom/JSON feeds
6. YouTube
7. Reddit
8. sessions
9. registry/package sources
10. CLI tools/scripts
11. MCP server/tool calls
12. memory documents where shared preparation is useful

For each source:

- implement adapter capability and scopes
- emit `SourceDocument`
- emit parser/graph facts where supported
- use ledger generation semantics when mutable
- use shared payload builder
- add source-specific fixtures
- update docs and generated schemas

Proof:

- adapter has success, auth failure, degraded, and skipped fixtures
- source-specific metadata appears only through approved fields

## Phase 10: Surface Cutover

Goal: hard-break public surfaces after internals are ready.

Tasks:

- Implement `axon <source>`, `axon watch <source>`, and `axon watch exec
  <source>`.
- Keep `extract` as structured LLM extraction.
- Keep `map` as a first-class command/action/endpoint.
- Remove old `embed`, `ingest`, `scrape`, `crawl`, `code-search-watch`,
  `purge`, and legacy MCP action families from normal public surfaces.
- Update CLI help, MCP tool schema, REST OpenAPI, web, Palette, Android, and
  Chrome extension contracts.

Proof:

- removed surfaces are absent from generated schemas and help
- new surfaces map to shared DTOs
- no compatibility aliases remain

## Phase 11: Reset, Prune, And Empty-DB Cutover

Goal: make the clean break operationally simple.

Tasks:

- Implement reset plan/exec with receipts.
- Implement prune plans and cleanup debt execution.
- Make old stores block unified workers until reset.
- Recreate fresh SQLite schema and Qdrant payload/index shape.

Proof:

- Tier 5 cutover tests pass
- fresh reindex from empty DB is the supported path

## Phase 12: Release Readiness

Goal: prove the refactor is complete enough to merge.

Tasks:

- Generate docs and schemas.
- Run fake-boundary tests.
- Run selected live smoke tests for local, web, git, ask/query, and reset.
- Run mandatory PR reviews.
- Update #298 with final status and follow-up issues for optional backends such
  as Repomix.

Proof:

- docs match generated artifacts
- PR checklist is complete
- no known contract gaps remain
