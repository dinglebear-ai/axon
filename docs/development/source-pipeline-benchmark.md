---
title: "Source Pipeline Scheduler Benchmark"
created: 2026-08-28
updated: 2026-08-29
---

# Source pipeline scheduler benchmark

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
bash scripts/bench-source-pipeline.sh
```

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
