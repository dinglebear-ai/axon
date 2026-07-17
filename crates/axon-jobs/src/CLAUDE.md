# axon-jobs — Agent Guide

`axon-jobs` owns the **single durable job runtime** for pipeline, scheduled watch,
and maintenance work: the `JobStore`/`JobRuntime` + SQLite implementation,
attempts, reservations, heartbeats, events, leases, cancellation, recovery, the
watch scheduler, and worker-lane coordination. Full contract (owns / API / deps /
tests):
[../../../docs/pipeline-unification/crates/axon-jobs/README.md](../../../docs/pipeline-unification/crates/axon-jobs/README.md)
· behavior spec:
[../../../docs/pipeline-unification/runtime/job-contract.md](../../../docs/pipeline-unification/runtime/job-contract.md).

## Status — unified runtime
The crate stores every durable operation in one job model with canonical
`JobKind`, attempts, stages, events, heartbeats, artifacts, reservations, and
recovery state. Source watches schedule canonical Source jobs and retain their
job IDs in watch-run history. Jobs run injected workers; this crate does not
reimplement domain services.

## Module map
Current groups from `crates/axon-jobs/src/`:
| Area | Owns |
|---|---|
| `boundary.rs` · `unified.rs` · `store.rs` · `runtime.rs` | `JobStore` contract, unified SQLite operations, pool ownership, and runtime composition |
| `state_machine.rs` · `status.rs` · `limits.rs` | lifecycle transitions, canonical status, and admission limits |
| `workers.rs` · `workers/` | in-process worker lanes, provider reservations, recovery, and watch scheduling |
| `watch_store.rs` · `workers/watch_scheduler.rs` | source-watch store + scheduler (`axon_source_watches` / `axon_source_watch_runs`) |
| `config_snapshot.rs` · `config_snapshot_store.rs` · `migrations/` | job config snapshots and forward-only unified schema |

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
