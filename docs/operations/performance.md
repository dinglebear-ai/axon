---
title: "Performance"
created: 2026-02-25
updated: 2026-08-02
---

# Performance

Axon throughput is bounded by source acquisition, document preparation,
embedding capacity, Qdrant writes, provider reservations, and the durable job
scheduler. Tune one constrained boundary at a time and keep correctness gates
enabled.

## Performance profiles

The CLI exposes `high-stable`, `balanced`, `extreme`, and `max`
profiles. A profile applies a coordinated baseline; explicit CLI, environment,
or TOML settings may override individual knobs.

Start with `balanced` or `high-stable`. Use the more aggressive profiles
only after measuring provider headroom and failure rates.

```bash
time axon source https://docs.example.com   --scope site   --max-pages 200   --performance-profile high-stable   --wait true
```

## Acquisition tuning

For site sources, bound work with:

- `--max-pages`
- `--max-depth`
- `--budget PATH=N`
- a narrow start URL or path
- the appropriate `--render-mode`

HTTP is cheaper than Chrome. Use `chrome` only when rendered state is required;
`auto-switch` lets the web engine escalate when HTTP results are inadequate.
The web engine lives under `crates/axon-adapters/src/web_engine/`.

## Embedding tuning

Embedding throughput depends on model latency, provider batch limits, token
volume, and configured concurrency. Watch for:

- HTTP 413: batches are too large
- HTTP 429/503: provider saturation or throttling
- long waiting/cooling periods: reservation pressure
- high GPU memory use: model or batch size exceeds safe capacity

Embedding provider code lives in `crates/axon-embedding/`; vector writes and
Qdrant behavior live in `crates/axon-vectors/`.

## Job and provider concurrency

The unified scheduler enforces queue, concurrency, priority, reservation, and
cooling rules. Increase concurrency only when Qdrant, TEI, Chrome, CPU, memory,
and disk all have headroom. Site-scope source jobs use a separate conservative
Chrome/CDP rail.

Use job events and metrics rather than elapsed time alone:

```bash
axon jobs get <job-id>
axon jobs events <job-id>
curl -fsS http://127.0.0.1:8001/metrics
```

## Retrieval and ask tuning

Query and ask performance depends on embedding latency, Qdrant candidate count,
hybrid dense/sparse retrieval, reranking/context assembly, and LLM synthesis.
Tune retrieval limits before increasing synthesis context. Validate answer
quality whenever changing hybrid-search, chunking, or reranking settings.

## Benchmark method

Use repeatable source inputs and record:

- total duration
- pages/items discovered and changed
- prepared documents and vector points
- provider wait/cooling time
- warning/degradation counts
- peak CPU, memory, GPU memory, and disk I/O
- retrieval quality after indexing

Example:

```bash
/usr/bin/time -v axon source /path/to/project   --performance-profile high-stable   --wait true   --json > /tmp/axon-source-result.json
```

Run several repetitions and separate cold-cache from warm-cache results.

## Safety limits

Do not remove SSRF checks, authorization, redaction, cancellation polling,
generation publication rules, or cleanup-debt handling to gain throughput.
Those boundaries are part of correctness.

## Configuration references

- [Configuration](../guides/configuration.md)
- [Runtime providers](../reference/runtime/providers.md)
- [Runtime jobs](../reference/runtime/jobs.md)
- [Spider feature flags](../reference/spider-feature-flags.md)
