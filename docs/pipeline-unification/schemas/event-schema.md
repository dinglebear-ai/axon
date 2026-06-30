# Event Schema Contract
Last Modified: 2026-06-30

## Contract

`axon-observe` owns event, heartbeat, span, and metric schemas. These schemas
drive CLI progress, REST job events/SSE, MCP status responses, logs, traces,
and job persistence.

## Generated Artifacts

```text
docs/reference/runtime/events.schema.json
docs/reference/runtime/events.md
```

Generator:

```bash
cargo xtask schemas events
cargo xtask schemas events --check
```

## Required Schemas

- `SourceProgressEvent`
- `JobHeartbeat`
- `StreamEvent`
- `StageCounts`
- `EventTiming`
- `ThroughputSnapshot`
- `RetrySnapshot`
- `ProviderWaitSnapshot`
- `MetricDescriptor`
- `TraceFieldSet`
- `CurrentItem`
- `LogFieldSet`
- `SpanFieldSet`

## Root Artifact Shape

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "https://axon.local/schemas/runtime/events.schema.json",
  "title": "AxonRuntimeEvents",
  "x-axon": {
    "contract_version": "2026-06-30",
    "generated_by": "cargo xtask schemas events",
    "owner_crates": ["axon-observe", "axon-api"],
    "source_inputs": ["crates/axon-observe/src", "crates/axon-api/src/progress.rs"]
  },
  "$defs": {
    "SourceProgressEvent": {},
    "JobHeartbeat": {},
    "StreamEvent": {}
  }
}
```

## SourceProgressEvent Shape

```json
{
  "type": "object",
  "required": [
    "event_id",
    "sequence",
    "job_id",
    "phase",
    "status",
    "severity",
    "visibility",
    "message",
    "timestamp"
  ],
  "properties": {
    "event_id": { "type": "string", "pattern": "^evt_" },
    "sequence": { "type": "integer", "minimum": 0 },
    "job_id": { "type": "string", "pattern": "^job_" },
    "phase": { "$ref": "#/$defs/PipelinePhase" },
    "status": { "$ref": "#/$defs/EventStatus" },
    "severity": { "$ref": "#/$defs/Severity" },
    "visibility": { "$ref": "#/$defs/Visibility" },
    "message": { "type": "string" },
    "counts": { "$ref": "#/$defs/StageCounts" },
    "timing": { "$ref": "#/$defs/EventTiming" },
    "throughput": { "$ref": "#/$defs/ThroughputSnapshot" },
    "current": { "$ref": "#/$defs/CurrentItem" },
    "retry": { "$ref": "#/$defs/RetrySnapshot" },
    "error": { "$ref": "#/$defs/ApiError" }
  },
  "additionalProperties": false
}
```

## Required Enums

- `PipelinePhase`
- `EventStatus`
- `Severity`
- `Visibility`
- `MetricKind`
- `HeartbeatState`

Enum values must match `runtime/observability-contract.md`.

## Required Phase Enum Values

`PipelinePhase` must include:

```text
queued
requested
resolving
routing
authorizing
planning
leasing
discovering
diffing
fetching
rendering
enriching
normalizing
parsing
graphing
preparing
batching
embedding
vectorizing
upserting
publishing
cleaning
retrieving
synthesizing
evaluating
complete
degraded
failed
canceled
```

## StageCounts Shape

```json
{
  "type": "object",
  "properties": {
    "items_total": { "type": ["integer", "null"], "minimum": 0 },
    "items_done": { "type": "integer", "minimum": 0 },
    "items_failed": { "type": "integer", "minimum": 0 },
    "documents_total": { "type": ["integer", "null"], "minimum": 0 },
    "documents_done": { "type": "integer", "minimum": 0 },
    "chunks_total": { "type": ["integer", "null"], "minimum": 0 },
    "chunks_done": { "type": "integer", "minimum": 0 },
    "bytes_total": { "type": ["integer", "null"], "minimum": 0 },
    "bytes_done": { "type": "integer", "minimum": 0 }
  },
  "additionalProperties": false
}
```

## CurrentItem Shape

```json
{
  "type": "object",
  "properties": {
    "source_item_key": { "type": ["string", "null"] },
    "document_id": { "$ref": "#/$defs/DocumentId" },
    "chunk_id": { "$ref": "#/$defs/ChunkId" },
    "artifact_id": { "$ref": "#/$defs/ArtifactId" },
    "adapter": { "type": ["string", "null"] },
    "path": { "type": ["string", "null"] },
    "label": { "type": ["string", "null"] }
  },
  "additionalProperties": false
}
```

## JobHeartbeat Shape

```json
{
  "type": "object",
  "required": [
    "job_id",
    "sequence",
    "phase",
    "status",
    "heartbeat_at",
    "last_progress_at"
  ],
  "properties": {
    "job_id": { "$ref": "#/$defs/JobId" },
    "sequence": { "type": "integer", "minimum": 0 },
    "phase": { "$ref": "#/$defs/PipelinePhase" },
    "status": { "$ref": "#/$defs/LifecycleStatus" },
    "heartbeat_at": { "$ref": "#/$defs/Timestamp" },
    "last_progress_at": { "$ref": "#/$defs/Timestamp" },
    "counts": { "$ref": "#/$defs/StageCounts" },
    "provider_wait": { "$ref": "#/$defs/ProviderWaitSnapshot" }
  },
  "additionalProperties": false
}
```

## StreamEvent Shape

SSE and MCP streaming use the same logical event envelope.

```json
{
  "type": "object",
  "required": ["event_id", "event_type", "timestamp"],
  "properties": {
    "event_id": { "type": "string", "pattern": "^evt_" },
    "event_type": {
      "type": "string",
      "enum": ["progress", "token", "citation", "artifact", "warning", "error", "final"]
    },
    "timestamp": { "$ref": "#/$defs/Timestamp" },
    "job_id": { "$ref": "#/$defs/JobId" },
    "data": { "type": "object", "additionalProperties": true }
  },
  "additionalProperties": false
}
```

Stream rules:

- `progress` data validates as `SourceProgressEvent`
- `error` data validates as `ApiError`
- `final` data validates as the route-specific result DTO
- stream-only fields must be absent from durable job event rows unless they are
  valid `SourceProgressEvent` fields

## Sink Contract

`axon-observe` exposes `ObservabilitySink`:

```rust
#[async_trait]
pub trait ObservabilitySink: Send + Sync {
    async fn emit(&self, event: SourceProgressEvent) -> Result<()>;
    async fn heartbeat(&self, heartbeat: JobHeartbeat) -> Result<()>;
}
```

Schemas must validate both durable job-store event rows and public REST/MCP/CLI
event projections.

## Rules

- every event has `event_id`, `sequence`, `job_id`, `phase`, `status`,
  `severity`, `visibility`, `message`, and `timestamp`
- every active job emits heartbeat-compatible events
- labels are bounded
- sensitive fields are redacted before event construction
- event sequence is monotonic per job
- public events never include unredacted secrets
- every SSE event maps to `StreamEvent`
- every durable job event maps to `SourceProgressEvent`

## Acceptance Criteria

- event examples in `observability-contract.md` validate
- CLI progress renderer uses the same schema fields
- REST SSE events use the same schema fields
- MCP progress responses use the same schema fields
- job-store event payloads round-trip through this schema
- active jobs can be monitored from heartbeats alone
- stream event fixtures validate for ask, research, summarize, and job progress

## Drift Checks

Fail when:

- phase registry differs from observability contract
- event schema differs from `axon-observe`
- job store persists fields not represented here
- SSE and MCP event payloads differ
- examples in observability docs fail validation

## Validation Fixtures

Required fixtures:

```text
crates/axon-observe/tests/fixtures/schema/progress_embedding.valid.json
crates/axon-observe/tests/fixtures/schema/heartbeat_waiting.valid.json
crates/axon-observe/tests/fixtures/schema/stream_token.valid.json
crates/axon-observe/tests/fixtures/schema/stream_final_ask.valid.json
crates/axon-observe/tests/fixtures/schema/missing_event_id.invalid.json
crates/axon-observe/tests/fixtures/schema/bad_phase.invalid.json
crates/axon-observe/tests/fixtures/schema/secret_message.invalid.json
```
