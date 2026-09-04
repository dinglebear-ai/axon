---
title: "Source Pipeline Scheduler Benchmark"
created: 2026-08-28
updated: 2026-09-03
---

# Source pipeline scheduler benchmark

## Executable benchmark contracts

The harness has two deliberately separate modes. `pinned-replay` consumes a
versioned local replay fixture and supports deterministic pipeline comparisons;
it does not establish live web performance. `live-cold-crawl` fetches the live
site and supports cold-crawl qualification; network variance means it must be
run in paired, back-to-back trials. The JSON records `benchmark_mode` and the
corresponding `acceptance_claim` so results cannot silently cross those evidence
boundaries.

```bash
# Deterministic replay
AXON_BENCH_MODE=pinned-replay \
AXON_BENCH_REPLAY_FIXTURE=/absolute/path/to/versioned-replay \
AXON_BENCH_AXON_BIN=target/release/axon \
bash scripts/bench-source-pipeline.sh

# Live cold crawl
AXON_BENCH_MODE=live-cold-crawl \
AXON_BENCH_SOURCE=https://code.claude.com \
AXON_BENCH_AXON_BIN=target/release/axon \
bash scripts/bench-source-pipeline.sh
```

Both modes invoke the selected release binary directly with `--cache false`,
`--scope site`, and no page or depth cap. By default the harness generates a
globally unique `axon_bench_*` collection, proves it does not already exist,
and treats it as owned. The collection is deleted on success, command failure,
HUP, INT, and TERM. Temporary SQLite state is deleted on those same paths.
Collection deletion failure fails an otherwise successful run. Set
`AXON_BENCH_RETAIN_COLLECTION=1` only when residual inspection is intentional.
`AXON_BENCH_RETAIN_WORK_DIR=1` preserves the isolated SQLite database and logs
for a failed evidence gate; the default still removes all local benchmark state.
An explicit `AXON_BENCH_COLLECTION` is accepted only with
`AXON_BENCH_OWN_COLLECTION=1` and the reserved `axon_bench_` prefix; this keeps
the cleanup boundary unambiguous.

The result captures the provider's observed `/info` contract rather than model
constants. It also captures the fully resolved `axon config list --json`
throughput configuration with provenance and a SHA-256 hash. Keys that can hold
credentials are replaced with `<redacted-secret>` and endpoints with
`<redacted-endpoint>` before either the manifest or hash is emitted. Never put
credentials in benchmark mode, source, collection, output, or comparison
variables.

`metal_busy_interval.seconds` is the union of provider-reported accelerator
busy intervals within the single validated metrics epoch. It is not summed
request duration. `wall_minus_metal_busy_seconds` is the wall-clock process
interval less that union and is diagnostic timing, not sufficient evidence to rank paired runs. The timing
object distinguishes:

- `critical_path`: benchmark process wall time;
- `stage_active`: persisted active stage durations;
- `event_windows`: observational first/last event windows, never active time;
- `stage_overlap_seconds`: intersections among active stage intervals;
- `stage_union_seconds`: the union of active stage intervals; and
- `unattributed_critical_path_seconds`: job critical-path time outside that union.

The canonical job critical path is the interval from the earliest persisted
start to the latest persisted completion for that job. Duplicate and
overlapping stage rows contribute once to the union; their excess active sum is
reported as overlap. Null/incomplete rows are excluded, and rows or events for
other jobs are never considered. The result also emits top-level
`critical_path_seconds`, `overlap_seconds`, `unattributed_seconds`,
`unattributed_ratio`, and `attribution_ratio`. Attribution below 95% sets
`attribution_gate`, `evidence_gate`, and `environment_comparable` false and adds
`critical_path_attribution_below_95_percent` to the evidence reasons. A result
with no completed timing interval fails attribution rather than passing
vacuously.

Queue wait, reservation wait, provider work, retries, publication, and
checkpoint time are stage or telemetry intervals when the runtime emits them;
they must not be inferred by adding overlapping event windows.

The environment record includes machine/OS/CPU identity, machine load, the
observed provider identity fingerprint, and provider load when `/info` exposes
it. A control result's `environment.fingerprint_sha256` must be supplied as
`AXON_BENCH_COMPARISON_ENV_SHA256` for the candidate. `environment_comparable`
is true only when that stable fingerprint matches and one-minute load is at or
below `AXON_BENCH_MAX_LOAD` (default 8). Missing baselines fail closed as false.
Even matching environment and corpus hashes do not prove document, chunk, vector, and graph equivalence. Every run remains `single_arm_diagnostic`, with `ranking_eligible=false`, until authoritative paired equivalence is established separately.

Record provider ownership and ensure no unrelated clients use the exclusive
metrics epoch. Run multiple paired trials and report median and range; never
rank results whose environment gate is false.

Executable fake-tool coverage validates exact Axon argv, generated collection
ownership, secret redaction, and cleanup after success, Axon failure, and TERM:

```bash
bash -n scripts/bench-source-pipeline.sh
bash -n scripts/test-bench-source-pipeline.sh
bash scripts/test-bench-source-pipeline.sh
```

The evidence phase uses a pinned local replay and aggregate MLX metric deltas.
It is intentionally smaller than the final acceptance matrix: if the telemetry
does not show padding >=20%, row/token occupancy <85%, or synchronized Metal
idle time >=5%, scheduler implementation stops and optimization moves to the
measured bottleneck.

The harness creates a mode-0700 temporary state directory, uses a private
SQLite database, never prints a raw source URL or unsanitized subprocess output,
rejects URL userinfo and command-substitution syntax, and prints only sanitized
stderr when the benchmark subprocess fails. MLX metrics
must come from loopback and remain in one process epoch with an otherwise idle,
freshly started service. Every request issued by the isolated crawl is validated
as one uncontaminated aggregate delta.

## Interpreting live crawl results

Live `code.claude.com` acquisition is deliberately retained because it exposes
pipeline starvation, but it is not deterministic. Cloudflare responses and
network latency have moved the fetch phase by tens of seconds across otherwise
equivalent runs. Use paired, interleaved arms and repeated medians; never rank a
change from one absolute wall-clock sample.

For tootie's RTX 4070 TEI deployment, use the manual cold-crawl control in
`docs/perf/code-claude-cold-crawl-2026-08-12.md`. Record the exact TEI container
image and command, GPU identity/activity, relevant TEI 429/restart counts, Axon
configuration, collection name, state directory, and result counts. The MLX
accelerator fields emitted by this harness are not interchangeable with NVIDIA
telemetry.

The 2026-09-03 RTX 4070 validation used 189 documents, 6,876 vector points,
9,124 graph nodes, and 4,656 edges/evidence records. Raising TEI's input
admission capacity from 128 to 1,024 eliminated `no permits available` 429s.
Batching parser-produced graph node reads/writes reduced the comparable
publishing-to-graph-tracking interval from 9.27 seconds to 2.47 seconds. A
dynamic edge-batching experiment regressed that interval to 4.47 seconds and
was rejected. These phase comparisons are diagnostic evidence, not a claim
that unrelated live crawl wall times are directly comparable.
The final scheduler comparison, if earned by this gate, separately measures a
pinned fresh-corpus/warm-service run, cold-service startup, and a live full
crawl. It adds corpus/vector equivalence, RSS, thermal state, SQLite admission,
and Qdrant publication diagnostics.

## Qdrant write-path sweep

`scripts/qdrant-tune.py` replays the frozen `code.claude.com` Markdown corpus
through isolated, benchmark-owned collections. A real sweep requires
`--execute`; without it the script only prints the configuration matrix.

```bash
cargo build --release --bin axon
python3 scripts/qdrant-tune.py --execute \
  --binary target/release/axon \
  --source ~/.axon/output/markdown \
  --qdrant-url http://tootie:53333 \
  --grpc-url http://tootie:53334 \
  --tei-url http://tootie:52000 \
  --repetitions 3 \
  --output /tmp/axon-qdrant-sweep.json
```

The harness alternates forward and reverse variant order on successive
repetitions, defaults to three samples per variant, and reports median, minimum,
and maximum wall time. The report includes the frozen-corpus SHA-256, document
count, credential-redacted endpoints, binary path, per-run Qdrant point/index
state, source-command write receipt, a stable digest of every stored point,
payload, and vector, and retrieval overlap. The digest excludes only the
execution-specific `job_id` and `embedded_at` payload fields. Do not rank failed
runs, unequal corpus hashes or counts, non-green
collections, or variants with fewer than the requested samples. Collections
are deleted on exit unless `--keep-collections` is supplied; deletion is
restricted to names with the `axon_qdrant_bench_` prefix.

The equivalence gate is deliberately strict: every successful arm must produce
the same point count, have a matching source-command write receipt, produce the
same full stored-data digest, reach a green collection, complete the requested
repetition count, and retain exact top-10 result overlap for every fixed query.
A lower overlap is diagnostic
output, not a valid speed winner. Invalid evidence is still written for
diagnosis, but the harness exits with status 2 and emits no timing summaries
for arms without successful samples.

## 2026-08-28 evidence gate

A fresh live `code.claude.com` baseline used an empty Axon state, a unique
Qdrant collection, and a freshly started loopback MLX process. It completed in
72.992 seconds. The same-process aggregate reported 7.21% padding, 93.56% row
occupancy, 46.21% token occupancy, and 46.10% synchronized Metal idle time.

The gate passed on both token occupancy below 85% and Metal idle above 5%.
This authorizes the scheduler implementation. These values are hypothesis
evidence, not cutover evidence; the Task 10 pinned comparison must use the
SQLite-derived committed-corpus hash and exact vector/ID parity checks.

## 2026-08-28 Task 10 decision: no throughput promotion; safety default retained

The 2026-08-28 evidence gate above is superseded as cutover evidence. It was
measured before the Apple MLX dispatch geometry was pinned to 16 rows / 8,192
tokens and before the TEI client request-capacity knobs were widened. Under
those corrected settings the pre-scheduler path already runs the accelerator
essentially saturated, so the underfill the gate detected no longer exists.

### The stale-binary defect

Every tuning run recorded between 05:51 and 06:03 UTC is void. The rebuild
issued was `cargo build --release -p axon-cli`. `axon-cli` is a library package;
the root `axon` package owns the `axon` bin target and depends on `axon-cli`, so
`-p axon-cli` builds that library and everything beneath it and never selects
the bin target. No link step runs and Cargo exits zero, leaving a stale
`target/release/axon`. Those runs measured pre-overlap code, and the reported
"overlap improved 77.8s to 69.8s" was attributable to knob and geometry changes
only. `scripts/bench-source-pipeline.sh` now refuses to start when any Rust
source or manifest is newer than the binary. Always build with
`cargo build --release --bin axon`.

### Measured comparison

Three arms over one committed corpus (`1054ec892e0c`), one harness, and a
freshly started loopback MLX process per run:

| Arm | Wall | Metal busy | Wall - Metal |
|---|---|---|---|
| Scheduler off (sample 1) | 69.02 s | 53.36 s | 15.66 s |
| Scheduler off (sample 2) | 78.23 s | 58.72 s | 19.51 s |
| Scheduler on, cross-pool overlap | 77.81 s | 53.36 s | 24.45 s |
| Scheduler on, serial per-pool | 97.22 s | 59.11 s | 38.11 s |

The cross-pool overlap is a real improvement to the scheduler path: it removes
13.7 s of non-accelerator overhead relative to the serial per-pool path. It is
still worse than not scheduling. Scheduler-off spends 55.3 s in vectorization
against 53.4 s of Metal work, so 96.5% of that window is already accelerator
compute; pool accumulation and the flush deadline only add latency.

Task 10 requires at least a 5% median improvement to claim a throughput
promotion, and no scheduler arm met that bar in any thermal epoch. The bounded
scheduler is nevertheless the default for memory-admission and pipeline-safety
reasons, not as a benchmark-backed speedup; set
`AXON_EMBED_SCHEDULER_ENABLED=false` only as a rollback switch.

### The 2026-08-28 02:23-02:26 results are the best recorded, at about 50 s

Three consecutive full cold crawls on the persistent `~/.axon` state finished in
49.53 s, 49.81 s, and 50.24 s (jobs `d8b07e91`, `22f5e96d`, `e6490380`), beating
the 59 s reference. Those runs used the main checkout's binary with the
cleanup-debt drain fix, length-aware packing, and `AXON_DOCUMENT_BATCH_SIZE=80`,
against the launchd MLX service on port 8084. The persistent embedding cache was
off (`providers.embedding.cache-enabled = false`, no env override, and no cache
row written since 2026-08-27T13:45Z), so those runs embedded the corpus for real.

The scheduler-off and cross-pool-overlap arms in the 2026-08-28 07:00-11:45Z
window land at 69 to 80 s. The observed spread tracks crawl duration rather
than the tested configuration changes:

| Run | Fetch phase | Wall |
|---|---|---|
| `22f5e96d` (02:24Z) | 42.66 s | 49.81 s |
| `e6490380` (02:25Z) | 42.88 s | 50.24 s |
| scheduler-off reproduction (07:0xZ) | 56.5 s | 69.02 s |
| warm-state reproduction (11:39Z) | 65.28 s | 79.56 s |

The fetch phase swings from 42.7 s to 65.3 s, a 53% spread, and embedding fits
inside that window in every case. Wall time tracks the crawl almost exactly.

Reproduction attempts ruled out the other candidates: both MLX servers run
`LiquidAI/LFM2.5-Embedding-350M` at 1,024 dimensions, a warm persistent state
directory still embedded the full 1,694,770-token corpus (70.11 s), and the
older v2 server was slower, not faster (79.56 s).

**Absolute wall times are not comparable across sessions.** Only paired runs
taken back to back under the same network and thermal conditions support a
ranking. Rank by `wall - metal_busy` when the accelerator time is available.

### Run-to-run variance invalidates single-run rankings

Metal busy time varied from 53.36 s to 59.11 s across these runs on a
byte-identical token count (1,694,770 useful tokens), a 10.8% spread worth about
9 s of wall time. Rank arms by `wall - metal_busy` within one thermal epoch, or
take repeated samples.

This band is wider than the gaps that separated most configurations in the
2026-08-28 sweep. Pool 512 vs 1,024, native batch 16 vs 20, and pipeline depth
2 vs 3 were each decided on one run apiece with 4-6 s between them. Those
rankings are **not** established and must not be treated as settled; only
extremes such as the 5,000 ms flush deadline are distinguishable.

### Current hypothesis: acquisition variance dominates observed wall time

In the scheduler-off samples, fetching spans 6.5 s to 71.9 s (200 pages,
roughly 0.3 s/page) while the entire 53-59 s of embedding fits inside that
window. Live crawl variance therefore dominated the observed wall time in
these runs. This is a hypothesis, not a causal bottleneck result: paired
back-to-back trials using the new acquisition timing and occupancy records are
required before ranking further acquisition or embedding changes.
