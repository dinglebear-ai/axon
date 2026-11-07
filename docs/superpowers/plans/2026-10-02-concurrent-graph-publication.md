# Concurrent graph publication implementation plan

**Goal:** Build graphs alongside embedding with atomic graph/ledger activation.
**Architecture:** Private durable graph stage, optimistic bulk activation, existing
shared graph merge rules on concurrent change, and durable disposal.
**Spec:** [approved design](../specs/2026-10-02-concurrent-graph-publication-design.md).
**Tech stack:** Rust, Tokio, SQLx, SQLite, existing provider reservations.

Task execution and rulings are tracked in bead `axon_rust-0t0a2.6`.

## Deliverables

### Graph staging

Own `crates/axon-graph/src/staging.rs` and focused child modules, the additive
sixth graph migration, and SQLite sidecars. Introduce `GraphStage::begin`,
`write_candidates`, `activate_in_tx`, `mark_disposable`, and bounded stage reaping.
Use the canonical graph merge in the hidden store. Journal candidate claims and
seed affected identities without overwriting prior stage writes. A conservative
graph revision detects concurrent mutations; replay preserves current claims.
Watch visibility/rollback/concurrent-edit tests fail before implementation, then
pass; run the complete graph library suite and upgrade/reopen tests.

### Ledger transaction composition

Own `crates/axon-ledger/src/sqlite/generation.rs`, `sqlite.rs`, and sidecars.
Expose caller-owned `publish_generation_in_tx`; retain the existing standalone
transaction wrapper. Prove rollback, baseline fencing, document-status carry,
cleanup debt and epoch atomicity with real SQLite.

### Pipeline integration

Own `crates/axon-services/src/source/executor` staging orchestration, graph helpers,
publication, result mapping, context wiring, reserved calls and cleanup worker.
Create one generation-owned stage; overlap stage writes with embedding without
advancing externally published heartbeat phases. Join before publication, filter
final baseline manifest dispositions, and activate graph plus ledger in one
`ImmediateTx`. Return the activated summary and remove redundant graph replay.
Fakes without a unified pool retain existing behavior. Add real SQLite pipeline
coverage proving staged events precede publication and cancellation cannot expose
uncommitted output. Add a blocking-provider concurrency integration test.

### Review and delivery

Regenerate schemas and source-pipeline docs. Run graph, ledger, services and jobs
checks plus formatting, monolith, layering, generated contracts and upgrade gates.
Run Lavra architecture/security/performance reviews over all touched files; fix
all surfaced issues with regression evidence. Publish exact-head CI, merge main,
deploy Axon alone on Tootie, verify revision/readiness, and force real Labby and
Claude documentation ingestions. Report timings, counts, staging/reconciliation
fallback, atomic publication and background disposal evidence separately.
