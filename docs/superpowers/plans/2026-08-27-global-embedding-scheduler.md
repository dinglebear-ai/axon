# Global Embedding Scheduler Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> `superpowers:subagent-driven-development` or `superpowers:executing-plans`.
> Track every checkbox and preserve unrelated dirty-worktree changes.

**Goal:** Prove whether Metal is underfilled, then—only if the evidence gate
passes—stream prepared chunks across acquisition waves into one bounded
generation scheduler without changing content, vectors, durability, or quality.

**Architecture:** A one-wave-prefetch producer sends neutral prepared work
through a chunk-and-byte-bounded FIFO. A generation-layer consumer absorbs
side effects immediately, pools work, calls a narrow vectorization function,
persists cumulative statuses, and incrementally updates the accumulator.

**Spec:** `docs/superpowers/specs/2026-08-27-global-embedding-scheduler-design.md`

## Global Constraints

- Never truncate, cap, sample, summarize, skip, or alter source content.
- Preserve model revision, dtype, pooling, 1,024 dimensions, chunk IDs, and
  vector-point IDs.
- One Metal-forward owner only.
- Register artifacts immediately at each creation boundary.
- Quiesce issued upserts before failed-generation deletion.
- Hold chunk and byte permits through durable completion and absorption.
- Default scheduler off; implementation begins only after the evidence gate.
- Stage only task-owned files; test first and commit each reviewable task.

---

### Task 1: Track, Secure, and Instrument the Apple MLX Server

**Files:**
- Create: `scripts/apple-mlx/mlx_tei_direct.py`
- Create: `scripts/apple-mlx/test_mlx_tei_direct.py`
- Create: `scripts/apple-mlx/README.md`

**Produces:** loopback/auth binding policy, total request limits, process-epoch
metrics, synchronized Metal busy intervals, token/row occupancy, and aggregate
`/metrics` snapshots.

- [ ] Copy `/Users/jmagar/.local/bin/mlx-tei-direct-v2` into the repository and
  diff-review it for machine paths, credentials, and stale experiments.
- [ ] Add pure `unittest.TestCase` tests under `AXON_MLX_TEST_MODE=1` for useful
  and padded token totals, row/token occupancy, dispatch counts, and empty data.
- [ ] Time Metal only across an explicit `mx.eval`/readback boundary. Record
  monotonic dispatch intervals, request wall time, and a random process epoch.
- [ ] Protect counters without holding a lock during tokenization or Metal work.
- [ ] Default bind to `127.0.0.1`. Refuse non-loopback startup without a bearer
  token; require constant-time token validation on `/embed`, `/info`, `/metrics`.
- [ ] Enforce body bytes, JSON structure, input rows, per-input bytes, and
  aggregate-token limits. Return 4xx on overflow; never truncate.
- [ ] Test loopback/no-token, non-loopback refusal, invalid/valid tokens,
  malformed/deep JSON, every exact limit, one-over-limit, disconnect during
  tokenization, and no permissive CORS.
- [ ] Add a parity smoke test for the current 700-token no-truncation probe,
  model identity, 1,024 dimensions, and vector order.
- [ ] Document an explicit loopback LaunchAgent command. Tailscale exposure is
  allowed only with authentication; benchmarks always use loopback.

Run:

```bash
AXON_MLX_TEST_MODE=1 python3 -m unittest -v scripts/apple-mlx/test_mlx_tei_direct.py
curl --connect-timeout 2 --max-time 5 -fsS http://127.0.0.1:8084/info
curl --connect-timeout 2 --max-time 5 -fsS http://127.0.0.1:8084/metrics
```

Commit: `feat(embedding): secure and instrument Apple MLX dispatches`

### Task 2: Validate the Telemetry Evidence Gate

**Files:**
- Create: `scripts/test-mlx-metrics.py`
- Modify: `scripts/apple-mlx/test_mlx_tei_direct.py`

**Produces:** strict before/after metric-delta validation. Rust response-header
parsing is explicitly deferred.

- [ ] Test and implement validation for epoch equality, request-count isolation,
  useful <= padded, partial <= dispatches, zero consistency, duplicate fields,
  negative text, non-integers, oversized integers, and duration upper bounds.
- [ ] Never log raw invalid values or endpoint URLs; return a stable reason code.
- [ ] Compute the union of Metal busy intervals and true idle gaps. Never infer
  idle time by summing overlapping stage durations.
- [ ] Add a concurrent-probe test proving unrelated requests invalidate a run.
- [ ] Run one pinned replay and record whether padding >=20%, occupancy <85%, or
  Metal idle >=5%. If none holds, stop scheduler work after Task 4 and redirect
  to the measured bottleneck.

Run: `python3 -m unittest -v scripts/test-mlx-metrics.py`

Commit: `bench(embedding): validate MLX scheduler evidence`

### Task 3: Build the Minimal Hardened Evidence Benchmark

**Files:**
- Create: `scripts/bench-source-pipeline.sh`
- Create: `scripts/test-bench-source-pipeline.sh`
- Create: `docs/development/source-pipeline-benchmark.md`

- [ ] In shell tests, require `umask 077`, private `mktemp -d`, traps for EXIT
  and signals, quoted expansion, and `jq --arg/--argjson`. Reject `eval`,
  `set -x`, command strings, and environment dumps.
- [ ] Validate job-ID syntax before a bound SQLite query. Use loopback curl with
  connect/total timeouts and separate sanitized stdout/stderr.
- [ ] Test sources containing spaces, quotes, command-substitution syntax,
  URL userinfo, and a subprocess error containing a fake secret. No secret,
  URL, header, source text, inline result, or environment may reach JSON output.
- [ ] Add one pinned local replay with identical empty Axon/Qdrant state. Emit
  wall time, corpus hashes, model contract, process epoch, request isolation,
  and only the three evidence-gate measurements.
- [ ] Stop when this minimal gate fails. Detailed RSS, SQLite, Qdrant,
  vector-equivalence, cold-service, paired comparison, and live-crawl modes are
  Task 10 work only after the scheduler hypothesis earns implementation.

Run: `bash scripts/test-bench-source-pipeline.sh`

Commit: `bench(source): add hardened scheduler evidence harness`

### Task 4: Make the No-Truncation Contract Explicit

**Files:**
- Modify: `crates/axon-embedding/src/tei/client.rs`
- Modify: `crates/axon-embedding/src/tei/client_tests.rs`

- [ ] Add a failing mock-server test proving a long input is sent with an
  explicit no-truncation contract and returns its full-token embedding.
- [ ] Replace or configure the current `truncate: true` request behavior. If a
  provider cannot attest no truncation, fail clearly instead of silently losing
  content.
- [ ] Split only rows between requests for Task 1 limits. Never split one
  embedding text; configure the server for every valid prepared chunk and fail
  clearly if one input exceeds the attested model/provider maximum.
- [ ] Re-run 413 splitting, retry, concurrency, and input-order tests.

Run: `OPENSSL_DIR=/Users/jmagar/.local/opt/openssl-3.5.2 cargo test -p axon-embedding`

Commit: `fix(embedding): enforce lossless TEI requests`

### Task 5: Separate Pretracked Side Effects and Add Neutral Work Types

**Files:**
- Create: `crates/axon-services/src/source/executor/generation_work.rs`
- Create: `crates/axon-services/src/source/executor/generation_work_tests.rs`
- Create: `crates/axon-services/src/source/executor/generation_spool.rs`
- Create: `crates/axon-services/src/source/executor/generation_spool_tests.rs`
- Modify: `crates/axon-services/src/source/executor.rs`
- Modify: `crates/axon-services/src/source/executor/generation_state.rs`
- Create: `crates/axon-services/src/source/executor/generation_state_tests.rs`
- Modify: `crates/axon-services/src/source/executor/created_generation/batches.rs`

- [ ] Add failpoint tests after acquisition artifact creation, enrichment,
  normalization, clean-output storage, preparation, and closed-channel send.
  Assert every artifact created before failure is already cleanup-owned.
- [ ] Introduce `PreparedBatchSideEffects` as accumulation ownership only.
  Name the method `absorb_pretracked_side_effects`; keep immediate `track` calls
  exactly at current production boundaries.
- [ ] Add `absorb_vectorized` and equivalence tests using private child-module
  access—no production test-only getters.
- [ ] Implement a mode-0600 generation-scoped `GenerationSpool` for bulky side
  effects, archive/output data, graph candidates, refreshed manifests, and
  cumulative ID/status state. Cap its in-memory read/write window at 64 MiB and
  use its indexed table for deduplication. Finalization streams it in source
  order; failure removes it or records cleanup debt.
- [ ] Put `PreparedGenerationBatch` and neutral split/message types in
  `generation_work.rs`. Neither `created_generation` nor `vectorize` imports
  orchestration types from the other.
- [ ] Keep a temporary `ProcessedBatch` wrapper only for the disabled path;
  Task 9 must delete it and the compatibility absorber.

Run:

```bash
OPENSSL_DIR=/Users/jmagar/.local/opt/openssl-3.5.2 cargo test -p axon-services artifact_tracking
OPENSSL_DIR=/Users/jmagar/.local/opt/openssl-3.5.2 cargo test -p axon-services generation_state
cargo xtask check-layering
```

Commit: `refactor(source): define pretracked generation work`

### Task 6: Implement Cancellation-Aware Chunk-and-Byte Backpressure

**Files:**
- Modify: `crates/axon-services/src/source/executor/generation_work.rs`
- Modify: `crates/axon-services/src/source/executor/generation_work_tests.rs`
- Modify: `crates/axon-services/src/source/executor/vectorize/batching.rs`

**Interface:**

```rust
pub(super) async fn send(
    &self,
    prepared: Vec<PreparedDocument>,
    side_effects: PreparedBatchSideEffects,
    cancel: &CancellationToken,
) -> anyhow::Result<()>;
```

- [ ] For pool size `P`, set channel capacity to two and chunk permits to
  `3 * P`, covering exactly one active plus two queued pools. Reject overflow.
  Split each message losslessly to at most one pool before permits.
- [ ] Charge owned prepared text plus metadata/payload bytes in ceiling 1-KiB
  units against an internal 1-GiB M5/48-GB profile budget (`1_048_576` permits),
  including the overlapped built vector/payload batch and validated against
  `u32`. Log effective capacities once. Together with the 64-MiB spool window
  and 128-MiB fixed runtime/index allowance, scheduler-owned memory is capped at
  1,216 MiB.
- [ ] Acquire chunk and estimated-owned-byte permits with `tokio::select!`
  against cancellation; do the same for channel send.
- [ ] Move permits into the active pool and release only after upsert, durable
  status write, and absorption—not when the envelope is received.
- [ ] Enforce an absolute 1-GiB owned-byte ceiling per materialized item. The
  exclusive gate waits for all ordinary permits and streams non-embedding
  metadata directly to the spool; text must fit the attested single-input model
  limit. Test exact/over limits, huge metadata, zero chunks, slow consumer,
  closed receiver, cancellation, and no circular wait.
- [ ] At pool sizes 512, 1,024, and 2,048, assert exact capacities, at most two
  resident queued envelopes, active-pool progress, and permit release only after
  durable completion/absorption.
- [ ] Run a sustained many-wave test and allocator/RSS sampling test. Assert
  transient charged bytes <=1 GiB, spool window <=64 MiB, fixed allowance
  <=128 MiB, total scheduler-owned live bytes <=1,216 MiB, and permits release
  only after spool durability.
- [ ] Log only sequence/chunk/byte aggregates and cancellation reason.

Run: `OPENSSL_DIR=/Users/jmagar/.local/opt/openssl-3.5.2 cargo test -p axon-services generation_work`

Commit: `feat(source): bound prepared work by chunks and bytes`

### Task 7: Extract Pool Vectorization and Cumulative Status Writes

**Files:**
- Modify: `crates/axon-services/src/source/executor/vectorize.rs`
- Modify: `crates/axon-services/src/source/executor/vectorize/pipeline.rs`
- Modify: `crates/axon-services/src/source/executor/vectorize_tests.rs`

**Interface:**

```rust
pub(super) async fn vectorize_prepared_pool(
    runtime: &TargetLocalSourceRuntime,
    input: &SourcePipelineInput<'_>,
    generation: &SourceGenerationId,
    collection: CollectionSpec,
    emitter: &SourceEventEmitter,
    coordinator: &ProgressCoordinator,
    prepared: Vec<PreparedDocument>,
    cumulative: &mut HashMap<DocumentId, DocumentStatus>,
    progress: &mut PipelineProgress,
    cancel: &CancellationToken,
) -> anyhow::Result<VectorizeResult>;
```

- [ ] Force one document across three pools. After every SQLite write and at
  finalization, assert cumulative—not last-window—chunk/vector counts.
- [ ] At each pool completion, write cumulative statuses in batches up to 100,
  then absorb that vector result immediately. Do not buffer across pools.
  Measure DB admission wait and transaction count/duration; defer cross-pool
  buffering unless those measurements justify it.
- [ ] Preserve the existing embed/build/upsert overlap and input-level TEI sort;
  do not add document-median sorting or a second Metal call.
- [ ] Add a late-failure test proving earlier durable work is absorbed and later
  failed-generation cleanup remains complete.

Run: `OPENSSL_DIR=/Users/jmagar/.local/opt/openssl-3.5.2 cargo test -p axon-services vectorize`

Commit: `refactor(source): vectorize prepared pools cumulatively`

### Task 8: Implement the Generation Scheduler and Progress Serialization

**Files:**
- Create: `crates/axon-services/src/source/executor/created_generation/scheduler.rs`
- Create: `crates/axon-services/src/source/executor/created_generation/scheduler_tests.rs`
- Modify: `crates/axon-services/src/source/executor/progress.rs`
- Modify: `crates/axon-services/src/source/executor/progress_tests.rs`

**Interface:**

```rust
pub(super) async fn run_generation_scheduler(
    runtime: &TargetLocalSourceRuntime,
    input: &SourcePipelineInput<'_>,
    receiver: PreparedBatchReceiver,
    accumulator: &mut GenerationAccumulator,
    progress: &mut PipelineProgress,
    cancel: &CancellationToken,
) -> anyhow::Result<()>;
```

- [ ] Absorb pretracked side effects on FIFO receipt. Absorb each vector result
  immediately after durable status completion. Never return scheduled results.
- [ ] Use one `sleep_until(oldest_deadline)`; arrivals never reset it. Test
  continuous trickle, closure/deadline race, zero delay, cancellation,
  `embed=false`, and zero chunks.
- [ ] Once vector phases begin, producer progress is count-only. Serialize
  durable writes with an async mutex and monotonic epoch; a deliberately delayed
  older write may not overwrite a newer phase/count.
- [ ] Add SQLite contention tests with a small busy timeout, concurrent progress
  and status work, and existing fair admission. No starvation or permit hoarding.

Run:

```bash
OPENSSL_DIR=/Users/jmagar/.local/opt/openssl-3.5.2 cargo test -p axon-services generation_scheduler
OPENSSL_DIR=/Users/jmagar/.local/opt/openssl-3.5.2 cargo test -p axon-services progress
cargo xtask check-layering
```

Commit: `feat(source): stream bounded generation embedding pools`

### Task 9: Integrate Fail-Fast Orchestration and Quiescent Cleanup

**Files:**
- Modify: `crates/axon-services/src/source/executor/created_generation.rs`
- Modify: `crates/axon-services/src/source/executor/created_generation/batches.rs`
- Modify: `crates/axon-services/src/source/executor/created_generation/batches_tests.rs`
- Modify: `crates/axon-services/src/source/executor.rs`

- [ ] Preserve the current acquisition N+1 prefetch. Test the three-event chain:
  acquisition N+1 overlaps preparation N, and preparation N+1 overlaps embed N.
- [ ] Drive pinned producer/consumer futures with `tokio::select!`. On consumer
  failure cancel/close waits. Drop a non-cooperative producer after a bounded
  cooperative exit only when no artifact mutation is in flight; otherwise
  drive the write terminal or persist durable cleanup debt. On producer failure,
  close input and quiesce already issued provider work.
- [ ] Fence provider reservations and in-flight upserts before failed-generation
  deletion. Test an upsert that ignores local cancellation and completes after
  cancellation; the final vector count for the failed generation must be zero.
- [ ] Fence artifact-store writes too. Test a put that ignores local cancellation
  and completes late; assert zero surviving failed-generation artifacts or a
  durable, idempotent cleanup-debt record.
- [ ] Test blocked acquisition, blocked permit/send, producer failure during
  upsert, consumer failure, simultaneous failures, and caller cancellation.
- [ ] Sanitize combined errors through the established source redactor before
  persistence; cover URL userinfo, API keys, headers, paths, and source excerpts.
- [ ] Delete the temporary `ProcessedBatch` compatibility wrapper and ambiguous
  absorber. Keep only explicit pretracked-side-effect and vector methods.
- [ ] Add scheduler-on/off differential tests for manifests, artifacts, warnings,
  graph candidates, lifecycle statuses, IDs, counts, publication, cleanup, and
  vectors by `ChunkId`.

Run both modes:

```bash
OPENSSL_DIR=/Users/jmagar/.local/opt/openssl-3.5.2 AXON_EMBED_SCHEDULER_ENABLED=false cargo test -p axon-services source::executor
OPENSSL_DIR=/Users/jmagar/.local/opt/openssl-3.5.2 AXON_EMBED_SCHEDULER_ENABLED=true cargo test -p axon-services source::executor
```

Commit: `feat(source): overlap preparation with quiescent embedding`

### Task 10: Benchmark Matrix, Stability, and Cutover Decision

**Files:**
- Modify: `docs/development/source-pipeline-benchmark.md`
- Modify: `config.example.toml` only if the scheduler earns default-on status

- [ ] Run the evidence gate first. If it fails, leave scheduler code disabled
  and document the measured next bottleneck.
- [ ] If it passes, benchmark pool 512/1,024/2,048 where compatible with
  effective TEI request/in-flight limits. Record effective values once per run.
- [ ] Run pinned fresh-corpus/warm-service paired trials, then cold-service, then
  one fresh live `code.claude.com` full crawl+embed+upsert confirmation.
- [ ] Sample Axon/MLX/Qdrant RSS every 100–250ms and reject >15% aggregate peak
  regression, serious/critical pressure, watchdog restart, or thermal throttle.
- [ ] Require exact corpus/ID/content-hash equality, vector equivalence, no
  truncation, identical model contract, >=5% median improvement, and tail gate.
- [ ] Run `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  focused tests, `cargo xtask check-layering`, release build, and a 30-minute
  stability smoke with cancellation and cleanup assertions.
- [ ] Only after every gate passes, promote the winning enable/pool defaults.
  Public queue/flush knobs, adaptive scheduling, and Rust response telemetry
  remain separate follow-ups unless this matrix proves they are necessary.

Commit: `perf(source): validate global embedding scheduler cutover`

## Explicitly Deferred Work

- Rust per-response telemetry headers: aggregate epoch deltas answer the gate.
- Public queue/flush knobs: avoid invalid combinations and premature support.
- Document-median/adaptive scheduling: TEI already sorts individual inputs.
- Prometheus history, request audit, mTLS, benchmark signing: unnecessary for a
  loopback evidence build; revisit only for remote production exposure.
- Default-on before acceptance: forbidden by the quality and stability gates.
