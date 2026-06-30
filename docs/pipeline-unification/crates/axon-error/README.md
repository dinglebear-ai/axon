# axon-error Crate Contract
Last Modified: 2026-06-30

## Purpose

`axon-error` owns the shared error taxonomy for the unified pipeline. Every
crate reports failures through this crate so CLI, REST, MCP, jobs, logs, and
progress streams can render the same error shape.

## Owns

- `ApiError`, `ErrorCode`, `ErrorStage`, `ErrorSeverity`, and stable JSON names
- retry, cooling, degradation, and fail-fast classifications
- structured error context attachments with redaction hints and secret
  classifications
- provider/store/parser/vector/job/source conversion helpers
- fake constructors for tests and schema snapshots

## Must Not Own

- CLI, MCP, or REST response rendering
- tracing/log emission
- provider clients, store clients, source adapters, or job scheduling
- secret detection or redaction implementation

## Public Modules

```text
lib.rs
api_error.rs
code.rs
stage.rs
severity.rs
retry.rs
degradation.rs
cooling.rs
context.rs
conversion.rs
testing.rs
```

## Public API

- `ApiError::new(code, stage, message)`
- `ApiError::with_context(key, value)`
- `ApiError::with_source_id(source_id)`
- `ApiError::with_job_id(job_id)`
- `ApiError::retry_policy() -> RetryPolicy`
- `ApiError::degradation_policy() -> DegradationPolicy`
- `ApiError::provider_cooling() -> Option<ProviderCooling>`
- `ErrorProjection` for `axon-api` envelopes
- `test_error(code, stage)` helpers under `testing`

## Dependencies Allowed

- `serde`, `thiserror`, `uuid`, `time` or `chrono`, and small utility crates

## Dependencies Forbidden

- any Axon crate, including `axon-api`, `axon-core`, `axon-services`,
  `axon-jobs`, `axon-cli`, `axon-mcp`, and `axon-web`
- concrete providers such as Qdrant, TEI, Gemini, Codex, OpenAI, Spider
- SQLite clients or transport frameworks

## Generated Artifacts

- contributes to [../../schemas/error-schema.md](../../schemas/error-schema.md)
- generated JSON schema for `ApiError`, `ErrorCode`, `ErrorStage`,
  `ErrorSeverity`, `RetryPolicy`, and `DegradationPolicy`

## Fixtures And Fakes

- stable fixture errors for retryable provider outage
- stable fixture errors for fatal config failure
- stable fixture errors for degraded parser/source acquisition
- fake context values that prove redaction works

## Tests

- enum JSON names are stable
- redacted display never leaks context marked secret
- every `ErrorCode` maps to severity, retry policy, and stage
- conversions preserve root cause class without exposing provider internals
- schema snapshots match generated docs

## Acceptance Criteria

- all crates can depend on `axon-error` without cycles
- no transport-specific rendering lives in this crate
- every emitted error can be converted into the shared envelope in `axon-api`
- every retry/degrade/cool decision is machine-readable
- `axon-error` carries redaction hints only; redaction implementation lives in
  `axon-core` or the renderer boundary

See [../README.md](../README.md) and
[../../foundation/crate-structure.md](../../foundation/crate-structure.md).
