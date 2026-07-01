# axon-error — Agent Guide

`axon-error` is the **lowest shared error boundary** for the unified pipeline:
the typed error taxonomy (`ApiError`, `ErrorCode`, `ErrorStage`, `ErrorSeverity`)
plus retry / cooling / degradation classifications and redaction-aware context.
Every crate reports failures through it so CLI, REST, MCP, jobs, logs, and
progress streams render one error shape. Full contract (owns / API / deps / tests):
[../../../docs/pipeline-unification/crates/axon-error/README.md](../../../docs/pipeline-unification/crates/axon-error/README.md)
· behavior spec:
[../../../docs/pipeline-unification/runtime/error-handling.md](../../../docs/pipeline-unification/runtime/error-handling.md).

## Status — PR0 skeleton
Modules below are **markers only**. Real implementation lands in **Phase 1
(Shared DTO And Enum Spine)** as the error taxonomy is built. Do not add
transport rendering, provider clients, stores, or job scheduling here.

## Module map
| File | Owns |
|---|---|
| `api_error.rs` | `ApiError` — the shared error type; `ApiError::new` / `with_context` / `with_source_id` / `with_job_id` |
| `code.rs` | `ErrorCode` — closed error-code enum with stable JSON names |
| `stage.rs` | `ErrorStage` — pipeline stage each error is attributed to |
| `severity.rs` | `ErrorSeverity` — severity classification per code |
| `retry.rs` | `RetryPolicy` — machine-readable retry/fail-fast classification |
| `degradation.rs` | `DegradationPolicy` — graceful-degradation decisions |
| `cooling.rs` | `ProviderCooling` — provider saturation / cool-down classification |
| `context.rs` | structured context attachments with redaction hints + secret classifications |
| `conversion.rs` | provider/store/parser/vector/job/source → `ApiError` conversion helpers |
| `testing.rs` | fixture errors + `test_error(code, stage)` fakes for tests and schema snapshots |

## Boundary — keep OUT of this crate
- CLI / MCP / REST response rendering.
- tracing / log emission.
- provider clients, store clients, source adapters, job scheduling.
- secret **detection** or redaction **implementation** — this crate carries
  redaction *hints* only; the implementation lives in `axon-core` or the renderer.

## Dependencies
- **Allowed:** external crates only — `serde`, `thiserror`, `uuid`, `time`/`chrono`, small utility crates.
- **Forbidden:** every Axon crate (including `axon-api`, `axon-core`, `axon-observe`, `axon-services`, `axon-jobs`, transports); concrete providers (Qdrant, TEI, Gemini, Codex, OpenAI, Spider); SQLite clients or transport frameworks. Enforced by `cargo xtask check-layering`.

## Invariants (review checklist)
- **Below `axon-api`** — every crate can depend on it without cycles; it depends on no higher crate.
- Every `ErrorCode` maps to a **severity, retry policy, and stage** — every retry/degrade/cool decision is machine-readable.
- Enum JSON names are **stable**; schema snapshots match generated docs.
- **Display/Debug is redaction-safe** — context marked secret never leaks.
- Conversions preserve root-cause class **without exposing provider internals**.
- Every emitted error is convertible into the shared envelope in `axon-api`.

## DTO ownership
The serializable envelope/projection (`ErrorEnvelope`, `ErrorProjection`) lives in
**`axon-api`**; this crate defines the taxonomy and exposes `ErrorProjection` for
`axon-api` envelopes — it does not own transport-facing response shapes.

## Keep in sync when shapes change
`README.md` (crate contract) · `runtime/error-handling.md` ·
`schemas/error-schema.md` (generated `ApiError`/`ErrorCode`/`ErrorStage`/
`ErrorSeverity`/`RetryPolicy`/`DegradationPolicy` schemas + fixtures) · the error
projection components in `axon-api`.
