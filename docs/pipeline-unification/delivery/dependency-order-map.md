# Pipeline Dependency and Order Map
Last Modified: 2026-06-30

## Contract

This map defines what must exist before each implementation area starts. It is
the guardrail against beginning the full refactor in the wrong order.

The implementation plan is ordered, but this file is stricter: if a row says
“must exist first,” implementation of the dependent area should not begin until
that prerequisite is merged or explicitly stubbed behind a contract test.

## Critical Path

```text
Contract packet
  -> axon-api DTO/enums
  -> schema/drift checks
  -> fake stores/providers
  -> SourceResolver/SourceRouter/adapter registry
  -> local-source + ledger-shaped spike
  -> LedgerStore/generation publish
  -> DocumentPreparer/ChunkRouter/payload builder
  -> unified jobs/observability
  -> source-family ports
  -> public surface cutover
  -> reset/prune/empty-DB cutover
  -> release readiness
```

## Dependency Rules

| Work Area | Must Exist First | Must Not Start Until | Proof |
|---|---|---|---|
| Public CLI cutover | `SourceRequest`, resolver/router, adapter registry, source job result, removal checks | old commands can be replaced without behavior loss | CLI schema/help generated and removed commands absent |
| MCP action cutover | shared DTOs, action registry, MCP schema generator, removal checks | old action families have source/job equivalents | generated MCP schema has only canonical actions |
| REST `/v1/sources` cutover | shared DTOs, OpenAPI generator, service entrypoint, job descriptors | crawl/embed/ingest routes have replacement flows | OpenAPI/client tests pass |
| Source adapter ports | adapter registry, source scopes, fake provider/store harness | source can emit `SourceDocument` without writing vectors | adapter fixtures pass |
| Local-source spike | `SourceRequest`, local adapter prototype, source ledger draft API, existing prepare path | no public surface wiring is required | local file/dir fixtures reach prepared docs with ledger draft metadata |
| Ledger publish | `LedgerStore`, generation DTOs, cleanup debt DTOs, fake store tests | vector payload builder can reference generation ids | interrupted generation is not searchable |
| Vector payload builder | metadata contract, redaction contract, vector schema fixtures, fake VectorStore | source docs and prepared docs have canonical ids | payload fixtures validate, secret fixture rejected |
| Unified jobs | job DTO/enums, provider reservations, observability sink, fake stores | source stages are named and counted consistently | heartbeat/progress tests pass |
| Provider reservations | provider capability schema, rate limiter boundary, fake providers | bulk background work can request capacity | starvation/backpressure tests pass |
| SourceGraph writes | graph schema, parser facts, graph candidate DTOs, fake GraphStore | adapters can emit evidence/merge keys | graph fixture validates nodes/edges/evidence |
| Memory integration | memory contract, MemoryStore fake, redaction, vector namespace | source/memory distinction is clear | memory recall/decay fixtures pass |
| Reset/prune cutover | reset DTOs, prune plans, cleanup debt, empty-DB policy | destructive operations have receipts | Tier 5 cutover tests pass |

## “Do Not Start X Until Y” List

- Source-family ports must not bypass
  `axon-services::source::routing::resolve_source_route`; acquisition receives
  an already validated route plan or a source-family bridge derived from it.
- Do not delete `embed`, `ingest`, `scrape`, `crawl`, or
  `code-search-watch` public surfaces until source routing, source jobs,
  generated schemas, and removal checks exist.
- Do not port GitHub/GitLab/Gitea ingestion until the local-source spike proves
  `SourceRequest -> adapter -> ledger draft -> SourceDocument -> prepare`.
- Do not port web crawl until URL normalization, authority mapping, and
  map/source scope behavior are implemented.
- Do not wire watch commands until ledger leases, job heartbeats, and provider
  backoff are observable.
- Do not make old vectors searchable through the new query path; cutover assumes
  reset/reindex into the target payload shape.
- Do not create source-specific payload fields outside the metadata registry.
- Do not add a new source without following
  `sources/new-source-contract.md`.
- Do not implement schema JSON without generated markdown from the same model.
- Do not run broad live smoke tests until fake-boundary tests prove behavior.

## Parallelizable Tracks

These can proceed in parallel after shared DTOs and enum registries exist:

- schema generator family modules
- fake store/provider implementations
- redaction/hash/schema fixtures
- source adapter capability definitions
- UI/client generated-schema consumption
- docs generator scaffolding

These should not proceed in parallel without coordination:

- ledger schema and job schema changes
- vector payload shape and query/retrieve filters
- public surface deletion and generated schema removal checks
- reset/prune behavior and empty-DB cutover tests

## First Three PRs

1. Shared source DTO/enums and schema fixtures.
2. Local-source ledger-shaped spike using existing prepare path.
3. Schema generator skeleton with enum/removal drift checks.

The full source-family migration starts only after those three PRs land.
