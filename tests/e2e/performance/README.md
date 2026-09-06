# E2E performance contract

Performance qualification is report-only until a named owner promotes a metric after at least ten like-for-like baselines, adequate per-mode samples, and bounded variance. Correctness is never retried or weakened.

Fingerprints are split into machine, provider, and scenario buckets. Any runner, service/model, corpus, configuration, cardinality, concurrency, queue-depth, power, thermal, or contention mismatch makes a comparison invalid rather than a product regression.

The real measurement entrypoint is `scripts/e2e/measure-real-performance.py`. It acquires the exclusive performance group, uses allocation-owned hermetic namespaces, retains at most fifty raw samples per metric, and relies on the composed scenario's authoritative teardown. Embedding metrics remain explicitly unsupported until coordinated with the embedding optimization owner.
