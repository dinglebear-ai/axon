# axon-jobs — Agent Guide

`axon-jobs` owns the **single durable job runtime** for pipeline, scheduled watch,
and maintenance work: the `JobStore`/`JobRuntime` + SQLite implementation,
attempts, reservations, heartbeats, events, leases, cancellation, recovery, the
watch scheduler, and worker-lane coordination. Full contract (owns / API / deps /
tests):
[../../../docs/pipeline-unification/crates/axon-jobs/README.md](../../../docs/pipeline-unification/crates/axon-jobs/README.md)
· behavior spec:
[../../../docs/pipeline-unification/runtime/job-contract.md](../../../docs/pipeline-unification/runtime/job-contract.md).

## Status — live crate, unification at Phase 8
Today the runtime carries **per-family** job models (`crawl`/`embed`/`extract`/
`ingest` payload + dispatch modules), and works. At **Phase 8** these collapse
into **one unified source job model** — `job_kind`/`job_intent`, attempts, stages,
events, heartbeats, and reservations — so async and watch work are observable
through the same job shape. Jobs schedule and run **injected** workers; they must
not reimplement domain services.

## Module map
Current groups from `crates/axon-jobs/src/` (target modules in parens):
| Area | Owns |
|---|---|
| `backend.rs` · `store/` · `runtime.rs` | `JobBackend`/`JobStore` + `SqliteJobStore` + `JobRuntime` |
| `crawl.rs` · `embed.rs` · `extract.rs` · `ingest/` | **per-family payloads → collapse into one `job.rs`/`attempt.rs`** |
| `ops/` · `query.rs` · `cancel.rs` · `status.rs` | enqueue/lifecycle, query, cancellation, status |
| `workers/` | in-process worker lanes + watch scheduler (`worker.rs`/`reservation.rs`/`recovery.rs`) |
| `watch/` · `freshness/` | recurring watch triggers + freshness schedules (`scheduler.rs`/`watch.rs`) |
| `config_snapshot/` · `tx.rs` · `migrations/` · `service_job_conv.rs` | job config snapshots, txns, forward-only schema, `ServiceJob` conversion |

## Boundary — keep OUT of this crate
- Domain logic for source acquisition, parsing, embedding, vector writes, retrieval, LLM synthesis, or pruning — call injected boundaries/traits.
- Transport output formatting; provider implementation internals.
- Any dependency on `axon-services` (would create a cycle).

## Dependencies
- **Allowed:** `axon-api`, `axon-error`, `axon-core`, `axon-authz`, `axon-observe`, SQLite/migration crates, and injected worker traits/functions supplied by the composition layer.
- **Forbidden:** `axon-services`, transport crates (`axon-cli`/`axon-mcp`/`axon-web`), direct provider clients where a service/provider trait exists. Enforced by `cargo xtask check-layering`.

## Invariants (review checklist)
- Only one durable job shape exists; async and watch work share it.
- One `job_id` links logs, events, ledger rows, graph updates, vector payloads, and status output.
- Stale attempts recover without double-publishing generations; heartbeats are durable and recoverable.
- Provider reservations prevent embedding/LLM overload; cancellation is cooperative and leaves durable failure/degraded state.

## DTO ownership
Job/progress/event wire shapes (`JobStatus`, `ServiceJob`, …) live in
**`axon-api`**; this crate stores records and constructs `axon_api::ServiceJob`.
Transports never import this crate directly — they observe jobs through
`axon-services`/`axon-api`, never a domain crate's `::ops::*` or internals.

## Keep in sync when shapes change
`README.md` (crate contract) · `runtime/job-contract.md` ·
`runtime/observability-contract.md` · `schemas/database-schema.md` (job tables) ·
the job/status DTOs in `axon-api`.
