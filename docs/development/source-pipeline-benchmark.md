---
title: "Source Pipeline Scheduler Benchmark"
created: 2026-08-28
updated: 2026-09-03
---

# Source pipeline scheduler benchmark

`scripts/bench-source-pipeline.sh` is the controlled source-pipeline harness.
It runs the locally built release binary in-process with a private Axon state
directory, a caller-selected Qdrant collection, and embedding-cache reads and
writes disabled. It is not a generic production smoke test.

## Reproducibility contract

Before collecting evidence:

1. Build the root binary with `cargo build --release --bin axon`. Building a
   library crate does not relink `target/release/axon`; the harness rejects a
   binary older than Rust sources or Cargo manifests.
2. Use an exclusive embedding endpoint. The automated evidence gate currently
   requires the loopback MLX compatibility server because its `/metrics`
   endpoint supplies epoch-scoped accelerator counters.
3. Use a unique Qdrant collection for each arm and the same tuned
   `config.toml`. Set `AXON_BENCH_COLLECTION` and `AXON_BENCH_CONFIG_PATH`
   explicitly when the defaults are not dedicated to the benchmark.
4. Keep the service warm but the corpus cold: the model may already reside on
   the accelerator, while Axon receives a new state directory and runs with
   `--cache false`. This measures full acquisition, preparation, embedding,
   publication, and graph finalization without model-download startup.
5. Compare only runs with the same committed-corpus hash and equivalent
   document, chunk, vector, and graph counts. A partial crawl or provider error
   is not a performance result.

The root binary no longer proxies source commands to a separate server. The
harness invokes `target/release/axon` directly and therefore measures the
checked-out code. Do not substitute an installed `axon` binary unless its
revision is recorded and matches the intended arm.

The evidence phase uses a pinned local replay and aggregate MLX metric deltas.
It is intentionally smaller than the final acceptance matrix: if the telemetry
does not show padding >=20%, row/token occupancy <85%, or synchronized Metal
idle time >=5%, scheduler implementation stops and optimization moves to the
measured bottleneck.

The harness creates a mode-0700 temporary state directory, uses a private
SQLite database, never prints the source URL or subprocess output, rejects URL
userinfo and command-substitution syntax, and sanitizes failures. MLX metrics
must come from loopback and remain in one process epoch with an otherwise idle,
freshly started service. Every request issued by the isolated crawl is validated
as one uncontaminated aggregate delta.

```bash
AXON_BENCH_SOURCE=https://code.claude.com \
AXON_BENCH_AXON_BIN=target/release/axon \
AXON_BENCH_COLLECTION=axon_scheduler_evidence_$(date +%s) \
bash scripts/bench-source-pipeline.sh
```

The JSON result contains wall time, committed-corpus hash, SQLite stage and
phase windows, per-wave acquisition latency/occupancy, and MLX aggregate
padding, occupancy, and idle ratios. Raw source content and URLs are not
included. Temporary state is removed on exit.

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

## 2026-08-28 evidence gate

A fresh live `code.claude.com` baseline used an empty Axon state, a unique
Qdrant collection, and a freshly started loopback MLX process. It completed in
72.992 seconds. The same-process aggregate reported 7.21% padding, 93.56% row
occupancy, 46.21% token occupancy, and 46.10% synchronized Metal idle time.

The gate passed on both token occupancy below 85% and Metal idle above 5%.
This authorizes the scheduler implementation. These values are hypothesis
evidence, not cutover evidence; the Task 10 pinned comparison must use the
SQLite-derived committed-corpus hash and exact vector/ID parity checks.

## 2026-08-28 Task 10 cutover decision: scheduler stays off

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

Task 10 requires at least a 5% median improvement to promote. No scheduler arm
improves on scheduler-off in any thermal epoch, so `AXON_EMBED_SCHEDULER_ENABLED`
remains `false` by default and the scheduler code stays dormant behind it.

### The 2026-08-28 02:23-02:26 results are the best recorded, at about 50 s

Three consecutive full cold crawls on the persistent `~/.axon` state finished in
49.53 s, 49.81 s, and 50.24 s (jobs `d8b07e91`, `22f5e96d`, `e6490380`), beating
the 59 s reference. Those runs used the main checkout's binary with the
cleanup-debt drain fix, length-aware packing, and `AXON_DOCUMENT_BATCH_SIZE=80`,
against the launchd MLX service on port 8084. The persistent embedding cache was
off (`providers.embedding.cache-enabled = false`, no env override, and no cache
row written since 2026-08-27T13:45Z), so those runs embedded the corpus for real.

Every number in the tables above is from 2026-08-28 07:00-11:45Z and lands at 69
to 80 s. The cause is not configuration. It is the crawl:

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
