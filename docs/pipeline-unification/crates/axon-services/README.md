# axon-services Crate Contract
Last Modified: 2026-06-30

## Purpose

`axon-services` owns transport-neutral orchestration. It composes routing,
adapters, ledger, parsing, graph, document preparation, embedding, vector
storage, retrieval, LLM, memory, pruning, jobs, authz, and observability.

## Owns

- service traits and implementations for source, map, extract, ask, query,
  retrieve, search, memory, graph, jobs, providers, config, status, and prune
- `ServiceContext` / dependency container
- stage orchestration and transaction order
- conversion from API requests to domain boundary calls
- service-level validation, policy checks, and result DTO assembly

## Must Not Own

- transport-specific parsing/rendering
- domain internals that belong in lower crates
- duplicate DTOs instead of `axon-api`
- provider clients or stores outside injected boundaries

## Public Modules

```text
lib.rs
context.rs
source.rs
map.rs
extract.rs
ask.rs
query.rs
retrieve.rs
search.rs
memory.rs
graph.rs
jobs.rs
providers.rs
config.rs
status.rs
prune.rs
testing.rs
```

## Public API

- `ServiceContext`
- `SourceService`
- `MapService`
- `ExtractService`
- `AskService`
- `RetrievalService`
- `MemoryService`
- `GraphService`
- `JobService`
- `ProviderService`
- `PruneService`
- `FakeServiceContext`

## Dependencies Allowed

- all lower domain and provider boundary crates
- no transport crates

## Dependencies Forbidden

- `axon-cli`, `axon-mcp`, `axon-web`
- stdout/stderr rendering
- HTTP route or MCP tool registration

## Generated Artifacts

- service action registry feeding command/tool/REST schema generation
- service fixture catalog for parity tests

## Fixtures And Fakes

- in-memory service context with fake providers/stores
- source pipeline happy path fixture
- degraded provider fixture
- failed publish cleanup fixture

## Tests

- every transport action has one service entrypoint
- source pipeline stage order matches
  [../../foundation/source-pipeline.md](../../foundation/source-pipeline.md)
- errors, progress, document status, and cleanup debt are emitted consistently
- no service writes around injected stores/providers

## Acceptance Criteria

- CLI, MCP, REST, web, desktop, Android, and extension surfaces call services
- adding a source or action changes service registration once, not per transport
- stage results are explicit and observable

See [../README.md](../README.md) and
[../../foundation/types/service-contract.md](../../foundation/types/service-contract.md).
