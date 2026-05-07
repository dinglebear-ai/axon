# Axon `ask` Performance Bench

This directory holds the harness and artifacts for benchmarking `axon ask` end-to-end across ACP backends.

## Quick start

Prereqs:
- `axon` binary built (`just build` or `cargo build --release --bin axon`)
- Infra services up: `rtk just services-up` (qdrant, tei, chrome)
- For warm mode: `axon serve` running on `AXON_MCP_HTTP_HOST:PORT`
- `jq`, `bash 4+`

Run:

```bash
rtk just bench-ask                      # default: 30 runs, both modes
rtk just bench-ask 100 warm             # 100 runs, warm only
rtk bash scripts/bench-ask.sh --help    # full options
```

Artifacts land in `docs/perf/results-<timestamp>-<sha>.json`.

## What the harness measures

For each `(backend, agent, prompt_id, mode)` combination it runs `axon ask --json` N times, parses the `timing_ms` object from each response, and computes per-field statistics with a bootstrap percentile CI.

Output schema (numerical-only):

```jsonc
{
  "schema": "axon-bench-ask/v1",
  "timestamp_utc": "...",
  "git_sha": "...",
  "git_branch": "...",
  "backend": "acp",
  "runs_per_combination": 30,
  "warmup_discarded_per_warm_combination": 3,
  "bootstrap_resamples": 1000,
  "results": [
    {
      "backend": "acp",
      "agent": "claude",
      "prompt_id": "nl-canonical",
      "mode": "warm",
      "runs_requested": 30,
      "samples": 30,
      "warmup_discarded": 3,
      "metrics": {
        "total_ms":       { "count": 30, "p50": ..., "p95": ..., "p99": ...,
                            "p50_ci": { "lo": ..., "hi": ... }, "p95_ci": {...}, "p99_ci": {...} },
        "llm_total_ms":   { ... },
        "retrieval_ms":   { ... },
        "context_build_ms": { ... }
      }
    }
  ]
}
```

The harness validates the artifact before writing: any string longer than 200 chars or any forbidden top-level key (`query`, `answer`, `chunk_text`, `url`, `source`) aborts the run and deletes the file. Bench output is **numerical only** by design.

## Statistical interpretation

| Percentile | Min samples for ±10% CI |
|------------|-------------------------|
| p50        | 30                      |
| p95        | 100–200                 |
| p99        | 500+                    |

The harness rejects `--runs < 30` outright. For p95 work, pass `--runs 100`; for p99, pass `--runs 500`. CI half-widths are reported in `metrics.<field>.{p50,p95,p99}_ci` — read `hi - lo` to size the noise floor.

### Why first 3 warm runs are discarded

Cold caches (ACP adapter session pool, Qdrant page cache, reqwest connection pool, JIT) inflate the first runs by 10–100% over the steady-state. The harness discards `WARMUP_DISCARD=3` runs in `warm` mode before counting samples. `cold` mode does NOT discard — every run is intentionally cold-start and counted.

### Cold vs warm

- **cold**: `axon ask` runs with `--no-server-url`, forcing in-process startup each invocation. Captures end-to-end including service handshake.
- **warm**: assumes `axon serve` is already running and reuses it. Captures steady-state retrieval+synthesis cost.

Don't average them. Report cold and warm side-by-side.

### LLM nondeterminism caveat

`llm_total_ms` typically swings 20–40% across identical prompts due to KV cache hits, sampler stochasticity, and provider load. **For stable signal, watch `retrieval_ms` and `context_build_ms`**, which are deterministic-ish. Use `llm_total_ms` to flag regressions only when the change exceeds ~50% over baseline.

## Bootstrap percentile CI

For each per-field sample array, the harness:

1. Computes the empirical p50/p95/p99 by linear interpolation.
2. Resamples the array with replacement 1000 times (deterministic LCG seed for reproducibility).
3. Computes the same percentile on each resample, takes the 2.5%–97.5% quantiles for the 95% CI.

This is the standard non-parametric approach for latency tails — no distributional assumption, robust to long-tail outliers. Implementation lives inline in `scripts/bench-ask.sh` (jq).

## Artifacts and `.gitignore`

`docs/perf/results-*.json` is git-ignored. The validator enforces numerical-only content, but **never commit a results file** without an explicit review — once a chunk_text or URL leaks into git history it's effectively impossible to remove. Baseline files that you do want to commit should NOT use the `results-` prefix; name them `baseline-<date>-<sha>.json` and add explicitly with `git add -f`.

## Backends and the headless gap

Today only `--backend acp` is wired. `--backend headless` (per bead `6dl`) is not yet shipped and the harness rejects it with a clear error. Extend by adding the `headless` execution path in `run_one_ask()` once 6dl lands.

## See also

- `bd show axon_rust-yv0` — bead with research and acceptance criteria
- `bd show axon_rust-nm9` — sub-stage timing instrumentation that produces `timing_ms`
- `bd show axon_rust-6dl` — pending headless CLI synthesis path
