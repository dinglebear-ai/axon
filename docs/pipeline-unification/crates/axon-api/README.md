# axon-api Crate Contract
Last Modified: 2026-06-30

## Purpose

`axon-api` owns transport-neutral DTOs, enums, envelopes, and schema-exportable
contracts. CLI, REST, MCP, jobs, watches, apps, and services speak through these
types instead of inventing surface-local shapes.

## Owns

- request/result DTOs for source, map, extract, ask, query, retrieve, search,
  memory, graph, jobs, providers, pruning, config, and status
- `SuccessEnvelope<T>`, `ErrorEnvelope`, pagination, cursors, and warnings
- serializable projections of `axon-error`
- closed enums and wire names used by all transports
- schema derivation annotations and snapshot fixture values

## Must Not Own

- provider clients, stores, routing behavior, parsing, chunking, embedding, or
  orchestration logic
- CLI formatting, MCP server registration, Axum routes, or app state
- concrete Qdrant/SQLite/TEI/Gemini/Codex types

## Public Modules

```text
lib.rs
envelope.rs
error.rs
source.rs
job.rs
progress.rs
capability.rs
provider.rs
document.rs
graph.rs
memory.rs
retrieval.rs
prune.rs
artifact.rs
config.rs
schema.rs
testing.rs
```

## Public API

- `SourceRequest`, `SourceResult`, `SourceStatus`
- `MapRequest`, `MapResult`
- `ExtractRequest`, `ExtractResult`
- `AskRequest`, `AskResult`, `QueryRequest`, `QueryResult`,
  `RetrieveRequest`, `RetrieveResult`
- `JobRequest`, `JobStatus`, `JobEvent`, `JobProgress`, `JobHeartbeat`
- `ProviderCapability`, `ProviderHealth`, `ProviderReservation`
- `DocumentStatus`, `PreparedDocumentDto`, `VectorPayloadDto`
- `GraphNodeDto`, `GraphEdgeDto`, `GraphEvidenceDto`
- `MemoryRecordDto`, `MemoryContextDto`, `MemoryReviewDto`
- `SuccessEnvelope<T>` and `ErrorEnvelope`

## Dependencies Allowed

- `axon-error`
- serde/schema crates, `uuid`, `time` or `chrono`, `url`
- tiny value-object crates that do not add runtime side effects

## Dependencies Forbidden

- all domain crates except `axon-error`
- Axum, rmcp, clap, Qdrant, SQLite clients, TEI clients, LLM clients
- filesystem/network process side effects

## Generated Artifacts

- [../../schemas/api-dto-schema.md](../../schemas/api-dto-schema.md)
- OpenAPI component schemas
- MCP input/output schemas
- CLI command schema DTO references
- JSON fixtures for transport parity tests

## Fixtures And Fakes

- canonical `SourceRequest` per source kind
- canonical `JobProgress` for every phase
- canonical success/error envelopes
- canonical graph, memory, vector payload, and provider capability DTOs

## Tests

- every DTO serializes and deserializes with stable JSON names
- schema generation is deterministic
- transport fixtures share the same DTO snapshots
- enum additions fail unless schema fixtures are updated

## Acceptance Criteria

- no transport, provider, or store imports exist
- every external surface can express its requests and responses using this crate
- implementation crates do not define duplicate public DTOs for the same concept

See [../README.md](../README.md) and
[../../foundation/api-contract.md](../../foundation/api-contract.md).
