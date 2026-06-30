# First Implementation PR Scope
Last Modified: 2026-06-30

## Contract

The first real implementation PR must be intentionally boring. Its job is to
make the shared source DTO/enums real and testable, not to move crawl/embed/
ingest behavior yet.

This PR should not delete old commands, route public traffic through the new
pipeline, migrate existing indexed data, or wire MCP/REST/CLI to a partial
implementation.

## Scope

Include:

- `axon-api::source` DTOs:
  - `SourceRequest`
  - `ResolvedSource`
  - `SourceResult`
  - source intent/refresh/watch/execution/output enums
  - source kind/scope enums
  - source limits/options metadata
- serde round-trip tests for minimal and full source requests
- local file/directory request fixtures
- schema fixture stubs or generated-schema TODOs for these DTOs
- documentation updates linking the DTOs to the pipeline contracts
- source-ledger migration WIP only if it is required by the chosen spike and is
  not wired into public runtime behavior yet

Exclude:

- public CLI command changes
- MCP action changes
- REST route changes
- replacing existing embed/ingest/crawl/scrape handlers
- moving source adapters
- Qdrant payload shape changes
- generation publish logic
- job runtime rewrite
- reset/prune cutover

## Acceptance Criteria

- `cargo test -p axon-api source`
- `./target/debug/xtask check`
- no new public command/action/route
- no compatibility aliases
- no runtime behavior change outside inert DTO/schema/migration scaffolding
- docs explain which behavior is still current-state versus target-state

## Follow-Up PRs

PR 2: local-source ledger-shaped spike.

```text
SourceRequest(local path)
  -> local adapter prototype
  -> source ledger draft/generation row
  -> SourceDocument
  -> existing prepare_source_document path
  -> PreparedDocument
```

PR 2 still does not wire CLI/MCP/REST. It proves the internal shape and records
what can be moved versus rewritten for local embed/watch.

PR 3: schema generator skeleton with enum/removal drift checks.

PR 3 creates the mechanism that keeps the DTOs, CLI, MCP, REST, OpenAPI,
events, errors, vector payload, and docs from drifting while the larger
migration proceeds.
