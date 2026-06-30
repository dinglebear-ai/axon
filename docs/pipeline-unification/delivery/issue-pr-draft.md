# Issue And PR Draft
Last Modified: 2026-06-30

## GitHub Issue Draft

Title:

```text
Unify Axon source pipeline across CLI, REST, MCP, jobs, watch, graph, memory, and vector storage
```

Body:

```markdown
## Goal

Make Axon use one clean-break source pipeline for every source and every
surface:

SourceRequest -> SourceResolver -> SourceRouter -> SourceAcquisition ->
SourceManifestDiff -> SourceGeneration -> SourceEnrichment -> SourceDocument ->
SourceParseFacts / GraphCandidate -> SourceGraph -> DocumentPreparer ->
PreparedDocument -> EmbeddingBatch -> EmbeddingProvider -> VectorPointBatch ->
VectorStore -> DocumentStatus -> GenerationPublisher -> CleanupDebt

## Why

Today the important low-level machinery is partially shared, but the lifecycle
is still split across crawl/embed/ingest/extract/code-search/watch/job-specific
paths. That makes progress, cleanup, refresh, source identity, metadata,
provider throughput, and observability harder than they need to be.

This issue is a clean break. There are no compatibility aliases, no tombstone
window, and no indexed-data migration requirement. Existing local data can be
discarded and reindexed after the metadata/payload model changes.

## Scope

- one source request/action model across CLI, REST, MCP, jobs, watch, and apps
- target crate split by pipeline responsibility
- SourceLedger for mutable/refreshable sources
- SourceGraph for repos/docs/packages/sessions/tools/agents/artifacts links
- source-specific adapters, parsers, chunking profiles, metadata, and scopes
- unified job model with heartbeat/progress/status
- provider boundaries for embedding, LLM, vector store, graph store, ledger
  store, memory store, artifacts, auth, cache, and observability
- generated schemas and docs for CLI, REST/OpenAPI, MCP, DTOs, config, events,
  errors, database, graph, vector payload, and provider capabilities
- clean CLI/MCP/REST/app surface cutover

## Implementation Contracts

The contract packet lives under `docs/pipeline-unification/`.

Start with:

- `README.md`
- `foundation/source-pipeline.md`
- `foundation/api-contract.md`
- `foundation/crate-structure.md`
- `crates/README.md`
- `delivery/current-implementation-sweep.md`
- `delivery/implementation-checklist.md`

## Non-Goals

- preserve old command/action/route aliases
- migrate old indexed Qdrant payloads
- keep obsolete crate names solely for compatibility
- create a second code-search-specific indexing path

## Acceptance Criteria

- every source enters through the shared source pipeline
- every adapter emits `SourceDocument`
- every mutable/refreshable source is ledger-owned
- every async/detached operation uses one durable job model
- every long-running operation emits shared progress/heartbeat events
- generated CLI/MCP/REST/schema docs match code
- old public surfaces are absent from generated schemas
```

## PR Draft

Title:

```text
Add pipeline unification implementation contracts
```

Summary:

```markdown
## Summary

- add the full `docs/pipeline-unification/` contract packet for the clean-break
  source pipeline refactor
- define target crate structure and per-crate implementation contracts
- define CLI, REST, MCP, app, schema, config, metadata, graph, memory, jobs,
  observability, errors, pruning, and testing contracts
- add current implementation sweep and implementation checklist to guide the
  follow-up build plan

## Verification

- markdown link check
- whitespace/tab check
- crate contract completeness check
- crate agent symlink check
```

Review focus:

- Are the crate boundaries acyclic and concrete enough to implement?
- Are current implementation strengths preserved in the target contracts?
- Are CLI/MCP/REST/app semantics aligned?
- Are source-specific optimizations hidden behind the shared pipeline?
- Are provider throughput and job observability explicit enough?
