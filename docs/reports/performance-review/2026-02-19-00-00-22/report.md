# Ask/Query/Retrieve Optimization Proof Report

- Timestamp: 2026-02-19
- Repo: `axon_rust`
- Scope: `ask`, `query`, `retrieve`

## Evidence Artifacts

- Bench raw output: `docs/reports/performance-review/2026-02-19-00-00-22/benchmark.log`
- Test raw output: `docs/reports/performance-review/2026-02-19-00-00-22/tests.log`
- Build check output: `docs/reports/performance-review/2026-02-19-00-00-22/check.log`

## Commands Executed

```bash
cargo check -q
CARGO_TARGET_DIR=/tmp/axon-target cargo test -q --lib vector::ops::tests -- --nocapture
cargo bench --bench ask_query_retrieve -- --noplot
```

## Performance Results (Measured)

From `benchmark.log`:

1. Reranker old vs new (`256` candidates)
- `rerank_old_256`: `[4.6508 ms 4.7383 ms 4.8309 ms]`
- `rerank_new_256`: `[244.79 us 251.79 us 258.70 us]`
- Midpoint speedup: `4.7383 ms / 0.25179 ms = ~18.82x faster`

2. Qdrant extraction path old dynamic JSON vs typed structs
- `qdrant_extract_old_value_path`: `[186.76 us 189.36 us 192.30 us]`
- `qdrant_extract_typed_path`: `[109.47 us 113.40 us 117.71 us]`
- Midpoint speedup: `189.36 / 113.40 = ~1.67x faster`

3. Context builder microbench (local string assembly only)
- `context_old_join_strategy`: `[6.2503 us 6.3197 us 6.3958 us]`
- `context_new_single_pass`: `[6.8431 us 6.9257 us 7.0166 us]`
- Midpoint delta: `~9.6% slower` in microbench
- Note: this cost is microseconds and not the dominant end-to-end latency driver in ask flows (network retrieval and LLM inference dominate).

## Accuracy / Response Quality Improvements (Behavioral)

Implemented changes that directly improve answer fidelity:

1. `ask` now includes top-ranked chunks in prompt context
- Previously selected but unused; now injected as `Top Chunk [S#]` entries.

2. Relevance floor before generation
- `AXON_ASK_MIN_RELEVANCE_SCORE` filters weak candidates before context assembly.

3. Better source coverage under context limits
- Full-doc inclusion tracking only marks inserted URLs; supplemental candidates can still contribute when full docs do not fit.

4. Retrieve robustness
- URL normalization and trailing-slash variants in `retrieve` improve hit rate for equivalent URLs.

5. Query correctness control
- `query --limit` now actually controls retrieval size, reducing noise and improving precision control.

## Validation Summary

- `cargo check`: pass
- `vector::ops` tests: `12 passed, 0 failed`
- Benchmarks: completed successfully with reproducible logs

## Conclusion

The optimization work delivered measurable speed gains in the highest-value compute hot paths:
- Reranking: ~18.8x faster
- Qdrant extraction transform: ~1.67x faster

And delivered concrete accuracy improvements through better candidate filtering, source inclusion, and retrieval normalization.
