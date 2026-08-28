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
