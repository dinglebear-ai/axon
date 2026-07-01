# axon-api — Agent Guide

`axon-api` is the **transport-neutral DTO / enum / envelope / schema hub**. CLI,
REST, MCP, jobs, watches, apps, and services speak through these types instead of
inventing surface-local shapes. It currently depends on serialization/schema
helpers only and must not depend on Axon domain crates, so the retrieval/vector
layer and the services facade can depend on it without a cycle. Full contract
(owns / API / deps / tests):
[../../../docs/pipeline-unification/crates/axon-api/README.md](../../../docs/pipeline-unification/crates/axon-api/README.md)
· behavior spec:
[../../../docs/pipeline-unification/foundation/api-contract.md](../../../docs/pipeline-unification/foundation/api-contract.md).

## Status — PR0 skeleton, partly live
The `source.rs` DTO spike is **already real, not a marker** — it defines
`SourceIntent`, `SourceRefreshPolicy`, and the transport-neutral source shapes as
the first narrow clean-break spike. The remaining request/result/envelope
families fold in during **Phase 1 (Shared DTO And Enum Spine)** as bead
`axon_rust-23dw.2` continues, moving canonical enums out of markdown into
Rust-owned registries. Do not add provider clients, stores, or runtime side
effects.

## Module map
| File | Owns |
|---|---|
| `source.rs` | **live spike** — `SourceIntent`, `SourceRefreshPolicy`, transport-neutral source DTOs |
| `result.rs` | ask/query/evaluate result contracts (former `services::types::service::query`) |
| `explain.rs` | ask-explain trace types (former `core::ask_explain`) |
| `contract.rs` | shared contract types + `contract_tests.rs` |
| `diff.rs` | diff DTOs |
| `ingest.rs` | ingest request/result DTOs |
| `job_dto.rs` / `job_status.rs` / `job_progress.rs` | `JobRequest`, `JobStatus`, `JobEvent`, `JobProgress`, `JobHeartbeat` |
| `service_job.rs` | `ServiceJob` — the job-runtime handoff shape |
| `mcp_schema.rs` | MCP wire-contract input/output schema source of truth |
| `purge.rs` | purge/delete-url DTOs |

## Boundary — keep OUT of this crate
- provider clients, stores, routing behavior, parsing, chunking, embedding, orchestration.
- CLI formatting, MCP server registration, Axum routes, app state.
- concrete Qdrant / SQLite / TEI / Gemini / Codex types.
- filesystem / network / process side effects.

## Dependencies
- **Currently allowed by manifest:** serde/schemars/utoipa, `uuid`, `chrono`,
  `percent-encoding`, `serde_json`, `similar`, and tracing/value-object helpers
  with no runtime side effects.
- **Target direction:** `axon-error` may become the only Axon dependency when
  shared `ErrorEnvelope` / `SuccessEnvelope<T>` shapes move here.
- **Forbidden:** Axum, rmcp, clap, Qdrant/SQLite/TEI/LLM clients, and Axon
  domain crates. Treat this as a target dependency contract; PR0 only enforces
  empty dependencies for the new marker crates.

## Invariants (review checklist)
- **No transport, provider, store, or domain-crate imports.**
- Every DTO serializes/deserializes with **stable JSON names**; schema generation is deterministic.
- **Enum additions fail** unless schema fixtures are updated; transport fixtures share the same DTO snapshots.
- Implementation crates **depend on these DTOs** rather than redefining the same concept.

## DTO ownership
This **is** the DTO home. Wire DTOs for every surface live here — including the
serializable projection of `axon-error::ApiError` (`ErrorEnvelope`) and the
`SuccessEnvelope<T>` wrapper. If a domain crate needs a shared type, move the type
here rather than duplicating it.

## Keep in sync when shapes change
`README.md` (crate contract) · `foundation/api-contract.md` ·
`foundation/types/dto-contract.md` · `schemas/api-dto-schema.md` (OpenAPI + MCP
schemas + transport-parity fixtures). Keep JSON names stable unless the
clean-break contract explicitly changes them.
