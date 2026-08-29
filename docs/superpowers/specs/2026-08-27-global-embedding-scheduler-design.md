# Global Embedding Scheduler Design

Date: 2026-08-27
Status: revised after Lavra engineering review

## Goal

Reduce cold source-index wall time by removing acquisition-wave-local embedding
barriers. A generation-scoped scheduler may form denser pools from prepared
documents across waves only when measurements prove this is the bottleneck.

The change preserves every source document and chunk, configured model/revision,
dtype, pooling, dimensions, vector-to-`ChunkId` identity, publication, rollback,
progress, artifact cleanup, and durable status semantics. No truncation,
sampling, summarization, or quality trade is permitted.

## Evidence Gate

The current `code.claude.com` run is about 50.43 seconds for 198 documents and
5,314 chunks, with roughly 44 seconds in embedding/vectorization. The executor
still prepares 16-item acquisition waves independently, but this is only a
hypothesis for remaining idle time.

Instrument the Apple MLX server before implementing the scheduler. Capture:

- useful and padded tokenizer tokens;
- row occupancy and token-budget occupancy;
- full and partial dispatches;
- request wall time and tokenize/serialize durations;
- synchronized Metal dispatch start/end intervals and their busy-time union;
- dispatcher idle gaps on one monotonic server clock;
- process epoch and request count.

Metal timing includes an explicit MLX synchronization/readback boundary.
Summed stage durations are not divided by wall time because stages overlap.
The harness snapshots metrics before and after each run, subtracts only within
one process epoch, and rejects unrelated request activity or invalid telemetry.

Implement the scheduler only if a pinned representative run shows one of:

- padding ratio at least 20%;
- mean row or token occupancy below 85%;
- measured Metal idle gaps at least 5% of request-span wall time.

If the gate fails, retain instrumentation and redirect work to the measured
Qdrant, SQLite, crawl, or serialization bottleneck.

## Current Guarantees to Preserve

- Artifacts are registered immediately after each producing operation.
- Split-document statuses are merged before durable publication.
- Completed work is absorbed before a later speculative acquisition failure.
- One-wave acquisition prefetch overlaps processing.
- Provider mutations quiesce before failed-generation deletion.

## Target Architecture and Layering

```text
one-wave-prefetch producer
  acquire -> reuse -> enrich -> normalize -> store -> prepare
                         |
                         v
executor/generation_work.rs
  FIFO bounded messages + chunk permits + prepared-byte permits
                         |
                         v
created_generation/scheduler.rs
  receive side effects -> form one active pool -> call vectorize
                         |
                         v
vectorize_prepared_pool
  embed -> build -> upsert -> cumulative durable statuses
                         |
                         v
incremental GenerationAccumulator absorption
```

`generation_work.rs` is a neutral executor sibling. Both
`created_generation` and `vectorize` depend inward on neutral prepared types;
`vectorize` never imports generation orchestration or side-effect types.

The producer retains source order and the existing acquisition prefetch. The
consumer alone owns `PipelineProgress`, cumulative document status state, and
`GenerationAccumulator`. There is one Metal owner and no detached task.

## Prepared Work and Memory Contract

```rust
pub(super) struct PreparedGenerationBatch {
    pub(super) prepared: Vec<PreparedDocument>,
    pub(super) side_effects: Option<PreparedBatchSideEffects>,
    pub(super) _chunk_permit: OwnedSemaphorePermit,
    pub(super) _byte_permit: OwnedSemaphorePermit,
}
```

The initial implementation exposes one environment-only experimental pool
size. Queue capacity is derived as two messages, each losslessly split to at
most one pool. It does not expose independent public queue/flush knobs.

For pool size `P`, chunk semaphore capacity is exactly `3 * P`: one active pool
plus two queued pool-sized messages. Prepared bytes are charged in 1-KiB units,
rounded up from owned UTF-8 text plus owned metadata/payload bytes. The initial
M5/48-GB profile budget is 1 GiB (`1_048_576` KiB permits), validated to fit
semaphore `u32` arithmetic. Both capacities are logged once as aggregates.

Permits remain held through embed, the overlapped built/upsert batch, durable
status write, and absorption; those vector/payload allocations are included in
the charged owned-byte estimate rather than treated as free overlap.

Selected generation side effects are appended to a private, process-lifetime
temporary spool and read back one record at a time during finalization. The
64-MiB limit is a per-record serialization/replay cap, not a total-memory bound:
the deduplication key set, document IDs, artifacts, output, graph candidates,
warnings, and counters remain in memory. If spool creation fails, side effects
remain in memory; an append plus replay failure aborts the generation.

A single materialized work item has an absolute 1-GiB owned-byte ceiling. The
exclusive path waits for all ordinary byte permits and streams non-embedding
metadata directly into the spool; it never materializes metadata above the
budget. Text must also fit the attested model/provider single-input limit. An
item above either limit fails clearly and is never truncated.

Splitting occurs before message construction. FIFO is authoritative; sequence
and continuation values are diagnostics, not vector-result attribution. The
first message owns one-time side effects; continuations own none. Permit
acquisition and channel send select against cancellation.

## Scheduling Policy

`created_generation/scheduler.rs` absorbs side effects on receipt, moves
prepared documents and permits into the active pool, and calls the narrow
lower-level `vectorize_prepared_pool(...)` function.

The evidence build supports only:

- `AXON_EMBED_SCHEDULER_ENABLED`, default `false`;
- `AXON_EMBED_SCHEDULER_POOL_INPUTS`, defaulting to the effective provider pool
  limit and validated against TEI request/in-flight limits;
- `AXON_EMBED_SCHEDULER_FLUSH_MS`, default `25`, clamped to 5,000 ms.

Acquisition wave-size controls are separate source-pipeline experiment knobs;
they are not scheduler pool limits.

The initial gather delay is an internal measured constant. A pool flushes at
its input target, producer close, or one absolute deadline derived from the
oldest item. New arrivals never extend that deadline. `embed=false`, closure,
zero-input work, and cancellation bypass or terminate the timer correctly.

Arrival order is preserved. The TEI client already sorts individual inputs
over the enlarged pool and restores by `ChunkId`; no redundant document-median
sort is added.

## Accumulation, Status, Progress, and SQLite

The producer registers acquisition, enrichment, and clean-output artifacts
immediately at each creation boundary. `PreparedBatchSideEffects` transfers
accumulation ownership; it is not the cleanup-registration trigger.

Archive items, artifact candidates, warnings, reused keys, and refreshed
manifest items are written to `GenerationSpool`. Final publication streams
those records in source order. The spool is a mode-0600 tempfile removed with
its process-lifetime temporary directory; it is not fsync-backed durable state
and does not create cleanup debt. Other accumulator state remains in memory.

The consumer absorbs side effects as FIFO messages arrive. It absorbs each
`VectorizeResult` immediately after its durable status write and returns
`anyhow::Result<()>`. There is no generation-wide `Vec<ScheduledBatchResult>`.

A generation-local cumulative `DocumentStatus` map merges documents split
across pools. Every ledger write contains cumulative counts. Each completed
pool writes its statuses before its vector result is absorbed, batching only
within that pool up to 100 rows. Cross-pool status buffering is deferred unless
measured SQLite admission cost proves it necessary.

Inventory producer and consumer SQLite writes and use the existing fair write
mechanism. Benchmark DB admission wait, transaction count, and duration. After
Batching/Embedding begins, producer progress updates are count-only. A monotonic
phase epoch and async durable-write mutex prevent older snapshots from landing
after newer ones.

## Failure and Cancellation

Pinned producer and consumer futures are driven by `tokio::select!`, not a
fail-fast claim around `tokio::join!`.

- Consumer failure cancels producer permits/sends and closes input. The
  supervisor then awaits cooperative producer shutdown; there is currently no
  hard timeout for a non-cooperative provider future.
- Producer failure closes the sender and lets already-prepared provider work
  reach a terminal/quiescent state so completed accounting is retained.
- Caller cancellation stops admitting new scheduler work. Already-started
  operations are awaited where their provider futures cooperate.
- Both errors use existing source redaction before persistence; the
  consumer/provider error remains primary.

Focused tests cover producer-first and consumer-first scheduler failures,
blocked permit/send cancellation, and durable current-publication accounting
when speculative next embedding fails. Non-cooperative provider shutdown and
remote-mutation quiescence remain future hardening work.

## MLX Service and Telemetry Security

Track the deployed compatibility server in `scripts/apple-mlx`. The initial
evidence path uses validated aggregate `/metrics` snapshots; Rust parsing of
per-response telemetry headers is deferred unless later evidence requires it.

The service binds to `127.0.0.1` by default. Any non-loopback bind requires an
explicit bearer token for `/embed`, `/info`, and `/metrics`, constant-time token
comparison, and no permissive CORS. Startup refuses non-loopback/no-token.

Request limits cover body bytes, row count, per-input bytes, JSON structure,
and aggregate tokens. Axon may split only rows between requests; splitting one
embedding text would change embedding semantics. Configure the server to accept
every valid prepared chunk. A single input above the attested provider/model
limit fails clearly and is never truncated.

Metrics contain aggregates only. Validate useful <= padded, partial <=
dispatches, epochs, duplicates, integer syntax, and upper bounds. Invalid
telemetry fails closed without logging raw values, source text, token IDs, URLs,
paths, credentials, or document/chunk IDs.

## Benchmark and Acceptance

The harness uses `umask 077`, a private trapped directory, quoted arguments,
`jq --arg/--argjson`, a validated job ID, bound SQLite parameters, loopback
time-bounded `curl`, separate sanitized stdout/stderr, and no `eval`, `set -x`,
environment dump, URL/header echo, or raw repository-local results.

Measure two states separately:

1. fresh-corpus/warm-service: identical empty Axon/Qdrant generation state,
   resident model, and pinned local corpus replay for the causal decision;
2. cold-service: restarted Axon/MLX, with startup reported separately.

The live `code.claude.com` crawl is final end-to-end confirmation. Record git
SHA, versions, power mode, thermal/memory pressure, and corpus hashes.

Equivalence requires identical sorted `(DocumentId, ChunkId, content hash)`
sets, counts, 1,024 dimensions, model revision, dtype, pooling, and explicit
no-truncation attestation. Compare vectors by `ChunkId` within a documented
numeric tolerance. Record peak Axon/MLX/Qdrant RSS at 100–250 ms, SQLite waits,
Qdrant requests/points/payload bytes, and upsert time.

Run three paired alternating trials, increasing to five pairs when variance
exceeds 2%. Default-on requires at least 5% median wall-time improvement, no
enabled run more than 3% slower than disabled median, no more than 15% peak RSS
regression, and no serious memory pressure, thermal throttling, corpus/vector
drift, or truncation.

## Deferred Until Evidence Requires It

- Rust per-response telemetry header parsing;
- public TOML queue/flush knobs and independent queue tuning;
- document-median or adaptive/token-aware scheduling;
- Prometheus history, request audit, mTLS, and benchmark signing;
- default-on cutover before pinned-corpus and live acceptance.

## Non-Goals

- No content, chunking, overlap, redaction, model, dtype, dimensions, pooling,
  or embedding-cache changes.
- No concurrent Metal forwards, detached tasks, unbounded queues, or new job.
- No publication semantic weakening or cleanup redesign.
