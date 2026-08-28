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

A defect invalidated every tuning run recorded between 05:51 and 06:03 UTC:
the rebuild issued was `cargo build --release -p axon-cli`, which builds the
library crate and never relinks the root `axon` binary. Those runs therefore
executed pre-overlap code, and the reported "overlap improved 77.8s to 69.8s"
was attributable to knob and geometry changes only.

Paired runs on one corpus (`1054ec892e0c`), one binary containing the
cross-pool overlap, and a freshly started loopback MLX process per run:

| Configuration | Wall | Metal idle | Vectorize span |
|---|---|---|---|
| Scheduler off | 69.02 s | 1.04% | 10.65 s -> 65.97 s |
| Scheduler on, pool 512, flush 1500 ms | 77.81 s | 0.74% | 14.87 s -> 75.49 s |

MLX Metal busy time was 53.36 s in both. Scheduler-off spends 55.3 s in
vectorization against 53.4 s of Metal work, i.e. 96.5% of the vectorization
window is accelerator compute. There is no scheduling gap left to recover, and
pool accumulation plus the flush deadline delay the first pool by 4.2 s and the
last by 9.5 s.

Task 10 requires at least a 5% median improvement to promote. The measured
result is a 12.7% regression, so `AXON_EMBED_SCHEDULER_ENABLED` remains `false`
by default and the scheduler code stays dormant behind it.

**Measured next bottleneck: acquisition, not embedding.** In the scheduler-off
run, fetching spans 6.94 s to 63.47 s (56.5 s for 200 pages, ~0.28 s/page)
while the whole 53.4 s of embedding fits inside that window. Wall time is
acquisition-bound. Further embedding-side tuning cannot move the total; crawl
concurrency and per-page fetch latency are where the remaining time is.
