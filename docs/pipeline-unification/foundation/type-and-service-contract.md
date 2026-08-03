# Type and Service Contract Registry
Last Modified: 2026-06-30

## Contract

Every CamelCase item in the source pipeline is a concrete implementation
contract. It must be categorized, owned by one crate, defined in one detailed
contract, implemented with tests, and projected consistently through CLI, MCP,
REST, jobs, watches, logs, and storage.

This file is the registry. The detailed implementation contracts live in
`foundation/types/`.

## Detailed Contracts

| Contract | Owns |
|---|---|
| [types/dto-contract.md](types/dto-contract.md) | Serializable data shapes, field rules, IDs, references, metadata maps, serde behavior. |
| [types/enum-contract.md](types/enum-contract.md) | Exact enum variants, JSON names, unknown handling, extensibility rules. |
| [types/stage-result-contract.md](types/stage-result-contract.md) | Pipeline stage input/output shapes, degradation, persistence, event emission. |
| [types/trait-contract.md](types/trait-contract.md) | Executable boundary traits and required fake implementations. |
| [types/service-contract.md](types/service-contract.md) | `axon-services` orchestration service traits and method signatures. |
| [types/store-contract.md](types/store-contract.md) | Durable store traits, transactions, leases, schema ownership, reset behavior. |
| [types/provider-contract.md](types/provider-contract.md) | External/provider traits, capability docs, reservations, cooling, health, fakes. |

## Pipeline Type Registry

| Name | Kind | Owning Crate | Detailed Contract |
|---|---|---|---|
| `SourceRequest` | DTO | `axon-api` | [DTO](types/dto-contract.md) |
| `SourceResolver` | Trait | `axon-route` | [Trait](types/trait-contract.md) |
| `ResolvedSource` | DTO | `axon-api` | [DTO](types/dto-contract.md) |
| `SourceRouter` | Trait | `axon-route` | [Trait](types/trait-contract.md) |
| `RoutePlan` | DTO | `axon-api` | [DTO](types/dto-contract.md) |
| `SourcePlan` | DTO | `axon-api` | [DTO](types/dto-contract.md) |
| `SourceAdapter` | Trait | `axon-adapters` | [Trait](types/trait-contract.md) |
| `SourceAcquisition` | StageResult | `axon-api` | [Stage Result](types/stage-result-contract.md) |
| `SourceManifest` | DTO | `axon-api` | [DTO](types/dto-contract.md) |
| `ManifestItem` | DTO | `axon-api` | [DTO](types/dto-contract.md) |
| `SourceManifestDiff` | StageResult | `axon-api` | [Stage Result](types/stage-result-contract.md) |
| `SourceGeneration` | DTO/state | `axon-api` | [DTO](types/dto-contract.md) |
| `SourceEnrichment` | DTO/stage output | `axon-api` | [Stage Result](types/stage-result-contract.md) |
| `SourceDocument` | DTO | `axon-api` | [DTO](types/dto-contract.md) |
| `SourceParseFacts` | DTO | `axon-api` | [DTO](types/dto-contract.md) |
| `GraphCandidate` | DTO | `axon-api` | [DTO](types/dto-contract.md) |
| `SourceGraph` | Service boundary | `axon-graph` | [Service](types/service-contract.md), [Store](types/store-contract.md) |
| `DocumentPreparer` | Trait | `axon-document` | [Trait](types/trait-contract.md) |
| `ChunkRouter` | Trait | `axon-document` | [Trait](types/trait-contract.md) |
| `PreparedDocument` | DTO | `axon-api` | [DTO](types/dto-contract.md) |
| `PreparedChunk` | DTO | `axon-api` | [DTO](types/dto-contract.md) |
| `EmbeddingBatch` | DTO | `axon-api` | [DTO](types/dto-contract.md) |
| `EmbeddingProvider` | Provider trait | `axon-embedding` | [Provider](types/provider-contract.md) |
| `EmbeddingResult` | DTO | `axon-api` | [DTO](types/dto-contract.md) |
| `VectorPointBatch` | DTO | `axon-api` | [DTO](types/dto-contract.md) |
| `VectorStore` | Store/provider trait | `axon-vectors` | [Store](types/store-contract.md), [Provider](types/provider-contract.md) |
| `DocumentStatus` | DTO/state | `axon-api` | [DTO](types/dto-contract.md) |
| `GenerationPublisher` | Trait/service | `axon-ledger` | [Trait](types/trait-contract.md) |
| `CleanupDebt` | DTO/state | `axon-api` | [DTO](types/dto-contract.md) |

## Completion Checklist

Implementation is incomplete until:

- every registry entry has code in the owning crate
- every DTO has serde round-trip tests
- every enum has exact JSON-name tests
- every stage result has success/degraded/failed fixtures
- every trait has a fake implementation
- every service has a fake-backed integration test
- every store has transaction/reset tests
- every provider has capability/health/cooling tests
- CLI/MCP/REST use these types instead of transport-local domain clones
