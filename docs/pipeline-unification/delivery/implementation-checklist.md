# Implementation Checklist
Last Modified: 2026-07-16

## Contract

This checklist is the pre-plan execution outline. It is intentionally less
detailed than the implementation plan we will write next, but it fixes the
major build phases and exit criteria.

## Phase 0: Contract Freeze

- [x] run structural doc checks
- [x] review contradiction sweep
- [x] verify current implementation sweep still matches code
- [ ] create/refresh GitHub issue body from the issue/PR draft
- [ ] commit the contract packet

Verified 2026-07-16 against live checkout `ae7b775a2` plus the shared dirty
closeout wave: `./target/debug/xtask docs check` passed 504 Markdown links,
115 removed-surface doc checks, and the 110-file final docs inventory. The
contradiction review has no unresolved blocker. Issue synchronization and a
commit remain intentionally open; this reconciliation did not mutate trackers
or commit shared work.

Exit criteria:

- contracts link cleanly
- no known contradiction blocks implementation planning
- issue body points to the docs packet

## Phase 1: Workspace Skeleton

- PR0 plan: `docs/pipeline-unification/plans/2026-07-01-target-workspace-skeleton.md`
- [x] add missing target crates
- [x] keep transitional crates in the workspace until later responsibility-moving PRs
- [x] add crate-local `src/CLAUDE.md` files
- [x] enforce no `mod.rs`
- [x] add dependency layering check for target crates

Current evidence: `cargo metadata --no-deps` lists the complete target crate
set with transitional `axon-extract`; every crate `src/` has `CLAUDE.md` and
the required sibling symlinks; no source `mod.rs` exists; repo-structure and
layering checks pass, and CI contains active `no-mod-rs` and repo-structure
gates.

Exit criteria:

- workspace builds with placeholder crates
- crate dependency graph is acyclic
- crate source trees match `foundation/repo-structure.md`

## Phase 2: Shared Types And Schemas

- [x] implement `axon-error`
- [x] expand `axon-api`
- [x] implement schema generation for DTOs, errors, events, CLI, MCP, OpenAPI,
  config, database, graph, vector payload, and provider capabilities
- [x] add stable fixtures and snapshot tests

Current evidence: `xtask::schemas` registers API, CLI, OpenAPI, MCP, config,
events, errors, database, graph, vector-payload, providers, and adapters; every
family has valid/invalid fixtures and tracked snapshots, and generated JSON and
Markdown carry source-input checksums. Final drift execution is tracked in
Phase 9 because compiling the current dirty generator is blocked by unrelated
in-flight `axon-adapters` errors.

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
- [x] keep `DocumentStatus` ledger-owned, not a separate store
- [x] add strict fakes for all Phase 3 stores/providers
- [x] add provider reservations/cooling/health
- [x] generate complete provider capability schema and markdown artifacts
- [x] enforce `axon-jobs` does not depend on `axon-services`

Current evidence: document status is implemented through `LedgerStore`; Phase
3 boundaries expose deterministic fakes; shared reservation managers cover
embedding, LLM, fetch/render/search, cooling, and interactive reserve tests;
provider schema/Markdown are populated generated artifacts; and both the Cargo
graph and layering checks reject `axon-jobs -> axon-services`.

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

- [x] replace family-specific job tables with one durable job model
- [x] preserve heartbeat, watchdog, panic guard, starvation detector, cancellation
- [x] implement unified progress phases
- [x] wire events into CLI, REST/SSE, MCP, logs, traces, and job rows
- [x] enforce provider reservations across background and interactive work

Current evidence: terminal migrations remove crawl/embed/extract/ingest,
session-watch, retired-watch, and freshness tables; the generated schema has
only `jobs` plus attempts/stages/events/heartbeats/artifacts/reservations and
canonical source-watch tables. State-machine, recovery, panic/watchdog,
cursor/event, CLI jobs, REST/SSE, MCP task progress, observe/tracing, and
interactive-reserve implementations and tests are present.

Exit criteria:

- every async/detached operation is pollable by `job_id`
- embedding/LLM providers are protected from overload
- progress is visible without log scraping

## Phase 7: Surface Cutover

- [x] implement clean-break CLI command model
- [x] implement REST route contract - `/v1/reset/plan` and `/v1/reset/exec`
  are routed, loopback/admin guarded, and covered by server route tests
- [x] implement MCP tool contract - the single-tool dispatcher handles
  `reset`, `collections`, `artifacts`, `uploads`, and `chat`, and the help
  payload/action-name parity test enforces the full action set
- [x] update web, palette, Android, and Chrome extension surfaces - source/job
  routes, opaque artifact identifiers, and canonical web options are consumed;
  removed `AXON_MCP_*` settings keys are gone from the app trees
- [x] delete old commands/actions/routes/aliases

Current evidence: the generated CLI inventory contains the clean source/jobs/
watch/prune/reset model and omits removed commands. Removed MCP actions and old
REST verb routes have negative schema/dispatch tests; `dedupe` and `purge` now
live only under prune. Reset plan/exec exists on CLI, REST, and MCP, and the
cross-surface resource/action fixtures report no CLI/REST/MCP operation
divergence after regeneration.

Exit criteria:

- CLI/MCP/REST/app parity tests pass
- removed surfaces are absent from generated schemas

## Phase 8: Pruning, Reset, And Empty-DB Cutover

- [x] implement prune plans and receipts - plans carry a `plan_id` and
  execution folds per-step results into receipts; public selector execution
  covers source/generation/collection vector deletes (generation-fenced via
  the ledger), while
  artifact/graph/memory/job-retention/cache selectors remain plan-only on the
  public surface and attach an explicit `prune.selector_unsupported` warning
  instead of reading as clean no-ops (the cleanup-debt drain executes vector,
  ledger, graph, memory, and job-retention steps)
- [x] implement cleanup debt execution
- [x] implement empty-store reset/dev bootstrap
- [x] remove legacy migration/tombstone concerns

Current evidence: cleanup debt drains in contract order through the source
pipeline. Reset inventories all target stores, defaults to dry-run, binds an
inventory checksum/plan id, requires `--yes`, recreates SQLite/Qdrant, writes a
receipt, and is guarded by doctor/startup incompatible-store checks. Terminal
drop migrations leave only the target schema; resumable reset/prune execution
and public-surface prune boundary completeness (artifact/cache delete
adapters) remain tracked in the metaplan.

Exit criteria:

- stale vectors/artifacts/ledger/graph rows clean through `axon-prune`
- fresh reindex from empty DB is the supported cutover path

## Phase 9: Documentation And Release Readiness

- [x] generate docs and schemas
- [x] update quickstarts and development guides
- [ ] run fake-boundary, contract, and selected live smoke tests - fake-boundary
  and contract suites compile and pass on the closeout tree; the live
  deployed-runtime CLI smoke across every canonical command group is still
  pending and gates final closeout
- [ ] create final PR summary and review checklist

Exit criteria:

- docs match generated artifacts
- implementation is ready for review/merge
