# API DTO Schema Contract
Last Modified: 2026-06-30

## Contract

`axon-api` owns transport-neutral DTO schemas. CLI, MCP, REST, jobs, stores, and
providers consume these schemas directly or through generated projections.

## Generated Artifacts

```text
docs/reference/api/schemas.json
docs/reference/api/dto.md
docs/reference/api/enums.md
```

Generator:

```bash
cargo xtask schemas api
cargo xtask schemas api --check
```

## Required Families

- source DTOs
- ledger DTOs
- document/prepared document DTOs
- embedding/vector DTOs
- graph DTOs
- memory DTOs
- retrieval/ask DTOs
- job/watch DTOs
- artifact/upload DTOs
- prune/reset DTOs
- provider capability DTOs
- config projection DTOs
- success/error envelopes

## Root Artifact Shape

`docs/reference/api/schemas.json` is a schema bundle:

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "https://axon.local/schemas/api/schemas.schema.json",
  "title": "AxonApiSchemas",
  "x-axon": {
    "owner_crates": ["axon-api", "axon-error", "axon-observe"],
    "generated_by": "cargo xtask schemas api",
    "contract_version": "2026-06-30",
    "source_inputs": [
      "crates/axon-api/src",
      "crates/axon-error/src",
      "crates/axon-observe/src"
    ]
  },
  "$defs": {
    "SourceRequest": {},
    "SourceResult": {},
    "SourceProgressEvent": {},
    "ApiError": {}
  }
}
```

The generator must emit one `$defs` entry for every public DTO exported by
`axon-api`, plus projections for `axon-error::ApiError` and
`axon-observe::SourceProgressEvent`.

## Required DTO Definition Shape

Every DTO definition includes:

```json
{
  "type": "object",
  "required": ["field_name"],
  "properties": {
    "field_name": {
      "type": "string",
      "description": "Generated from Rust doc comment.",
      "x-axon": {
        "rust_type": "SourceId",
        "visibility": "public",
        "source_crate": "axon-api"
      }
    }
  },
  "additionalProperties": false
}
```

DTOs with extension maps may set `additionalProperties=true` only on the
specific extension property, never on the whole DTO.

## DTO Registry Source

`axon-api` must expose a generated or static DTO registry:

```rust
pub struct DtoSchemaSpec {
    pub name: &'static str,
    pub rust_type: &'static str,
    pub module: &'static str,
    pub family: DtoFamily,
    pub transport_exposed: bool,
    pub store_exposed: bool,
    pub fields: &'static [DtoFieldSpec],
    pub examples: &'static [SchemaExample],
    pub extension_points: &'static [DtoExtensionPoint],
    pub forbidden_fields: &'static [&'static str],
}

pub struct DtoFieldSpec {
    pub name: &'static str,
    pub rust_type: &'static str,
    pub json_type: JsonType,
    pub required: bool,
    pub visibility: Visibility,
    pub extension_point: bool,
    pub description: &'static str,
}
```

```rust
pub struct DtoExtensionPoint {
    pub field: &'static str,
    pub max_keys: usize,
    pub max_value_bytes: usize,
    pub allowed_value_types: &'static [JsonType],
    pub redaction: RedactionPolicy,
}
```

The schema generator fails if a public DTO lacks a registry entry or if the
registry entry does not match the generated JSON schema.

## Required Enum Definitions

The schema bundle must include all enums from
`foundation/types/enum-contract.md`, including:

- `SourceIntent`
- `SourceRefreshPolicy`
- `SourceWatchPolicy`
- `ExecutionMode`
- `ResponseMode`
- `ArtifactMode`
- `SourceKind`
- `SourceScope`
- `ItemKind`
- `ContentKind`
- `PipelinePhase`
- `JobKind`
- `LifecycleStatus`
- `DocumentLifecycleStatus`
- `DiffKind`
- `EnrichmentKind`
- `CleanupDebtKind`
- `ProviderKind`
- `HealthStatus`
- `Visibility`
- `Severity`
- `JobPriority`
- `AuthorityLevel`
- `ExecutionAffinity`
- `SafetyClass`
- `CredentialKind`
- `ArtifactKind`
- `CachePolicy`
- `ChunkProfile`

Every enum definition is:

```json
{
  "type": "string",
  "enum": ["snake_case_value"],
  "x-axon": {
    "rust_enum": "SourceKind",
    "owner_crate": "axon-api"
  }
}
```

## Required `$defs`

The generated bundle must include at minimum:

| Family | Required `$defs` |
|---|---|
| Envelope | `SuccessEnvelope`, `ErrorEnvelope`, `Page`, `PollDescriptor`, `JobDescriptor` |
| Source | `SourceRequest`, `ResolvedSource`, `RoutePlan`, `SourcePlan`, `SourceResult` |
| Ledger | `SourceManifest`, `ManifestItem`, `SourceManifestDiff`, `SourceGeneration`, `CleanupDebt` |
| Document | `SourceDocument`, `PreparedDocument`, `PreparedChunk`, `DocumentStatus` |
| Parse/Graph | `SourceParseFacts`, `GraphCandidate`, `GraphNode`, `GraphEdge`, `GraphEvidence` |
| Embedding/Vector | `EmbeddingBatch`, `EmbeddingResult`, `VectorPointBatch`, `VectorSearchRequest`, `VectorSearchResult` |
| Retrieval | `QueryRequest`, `QueryResult`, `RetrievalRequest`, `RetrievalResult`, `AskRequest`, `AskResult`, `ChatRequest`, `ChatResult`, `EvaluationRequest`, `EvaluationResult`, `SuggestRequest`, `SuggestResult` |
| Discovery/Synthesis | `SearchRequest`, `SearchResult`, `ResearchRequest`, `ResearchResult`, `SummarizeRequest`, `SummarizeResult`, `EndpointDiscoveryRequest`, `EndpointDiscoveryResult`, `BrandRequest`, `BrandResult`, `DiffRequest`, `DiffResult`, `ScreenshotRequest`, `ScreenshotResult`, `ExtractRequest`, `ExtractResult` |
| Runtime | `JobSummary`, `JobEventPage`, `WatchRequest`, `WatchResult`, `WatchDescriptor`, `SourceProgressEvent`, `TraceContext` |
| Operations | `ArtifactRef`, `ArtifactListRequest`, `ArtifactResult`, `UploadCreateRequest`, `UploadResult`, `PruneRequest`, `PruneExecuteRequest`, `PrunePlan`, `PruneResult`, `DedupeRequest`, `DedupeResult`, `PurgeRequest`, `PurgeResult`, `CollectionListRequest`, `CollectionResult`, `ProviderCapability`, `HealthReport` |
| Errors | `ApiError`, `SourceError`, `SourceWarning` |

## Required Envelope Definitions

Every transport projects from these envelope definitions:

```json
{
  "SuccessEnvelope": {
    "type": "object",
    "required": ["ok", "request_id", "contract_version", "data", "warnings", "trace"],
    "properties": {
      "ok": { "const": true },
      "request_id": { "$ref": "#/$defs/RequestId" },
      "contract_version": { "type": "string" },
      "data": {},
      "warnings": {
        "type": "array",
        "items": { "$ref": "#/$defs/SourceWarning" }
      },
      "pagination": { "$ref": "#/$defs/Page" },
      "job": { "$ref": "#/$defs/JobDescriptor" },
      "artifacts": {
        "type": "array",
        "items": { "$ref": "#/$defs/ArtifactRef" }
      },
      "trace": { "$ref": "#/$defs/TraceContext" }
    },
    "additionalProperties": false
  }
}
```

Error envelopes use the same correlation fields plus `ApiError`.

## Forbidden DTO Forks

These are forbidden:

- REST-only copies of request/result DTOs
- MCP-only copies of request/result DTOs
- CLI-only copies of request/result DTOs
- app-specific copies in generated clients that rename fields
- untyped `serde_json::Value` request bodies except explicit `body`,
  `metadata`, or `options` extension points
- compatibility DTOs for removed commands/routes/actions

## Validation Fixtures

Required fixtures:

```text
crates/axon-api/tests/fixtures/schema/source_request.valid.json
crates/axon-api/tests/fixtures/schema/source_request.full.valid.json
crates/axon-api/tests/fixtures/schema/source_request.unknown-field.invalid.json
crates/axon-api/tests/fixtures/schema/prepared_document.valid.json
crates/axon-api/tests/fixtures/schema/api_error.valid.json
crates/axon-api/tests/fixtures/schema/source_progress_event.valid.json
crates/axon-api/tests/fixtures/schema/success_envelope.valid.json
crates/axon-api/tests/fixtures/schema/extension_too_large.invalid.json
```

Every externally exposed DTO has at least one valid fixture. High-risk DTOs
have invalid fixtures for unknown field, wrong enum casing, missing required id,
and oversized inline content.

## Cross-Schema References

The API schema is the source for:

- MCP request/result DTO refs
- OpenAPI component schemas
- CLI command `maps_to_dto`
- job payload schemas
- provider capability schemas

No transport schema may define an object with the same name differently.

## Acceptance Criteria

- `schemas.json` contains every required `$defs` entry
- every `$defs` entry has generated markdown documentation
- every request DTO has `additionalProperties=false` unless explicitly exempt
- every extension map has a documented max size policy
- every id field references a typed id schema
- every public field has a doc comment or generated description

## Rules

- request DTOs deny unknown fields unless explicitly extensible
- extensible fields are named `metadata`, `options`, or `body`
- all ids use typed id schemas
- all timestamps are RFC3339 strings
- all enums serialize as snake_case
- large content is represented by `ContentRef` or `ArtifactRef`
- extension maps have explicit max key/value limits
- generated clients preserve field names exactly

## Drift Checks

Fail when:

- DTO exists without schema
- schema exists without DTO
- enum variants differ from enum contract
- examples in surface contracts fail validation
- transport schemas define private DTO copies
- extension point limits differ from registry
- generated client DTOs rename or omit fields
