# Axon `ask` Performance Bench

This directory holds the harness and artifacts for benchmarking `axon ask`
end-to-end on the Gemini headless synthesis path.

## Quick start

Prereqs:
- `axon` binary built (`just build` or `cargo build --release --bin axon`)
- Infra services up: `rtk just services-up` (qdrant, tei, chrome)
- For warm mode: `axon serve` running and `AXON_SERVER_URL` set
- `jq`, `bash 4+`

Run:

```bash
rtk just bench-ask
rtk bash scripts/bench-ask.sh --runs 100 --mode warm
```

Artifacts land in `docs/perf/results-<timestamp>-<sha>.json`.

## What The Harness Measures

For each prompt it runs `axon ask --json --diagnostics` N times, parses the
`timing_ms` object from each response, and stores the raw timing objects under
the `gemini-headless` backend label. Diagnostics are intentional: without them
the response keeps only the legacy five buckets and omits TEI, Qdrant, rerank,
full-doc fetch, TTFT, and normalization sub-stage timings.

Output schema:

```json
{
  "schema": "axon-bench-ask/v2",
  "backend": "gemini-headless",
  "runs_per_prompt": 30,
  "results": [
    {
      "backend": "gemini-headless",
      "prompt_id": "nl-canonical",
      "mode": "cold",
      "runs_requested": 30,
      "samples": 30,
      "timings": [
        { "retrieval": 100, "context_build": 40, "llm": 1100, "total": 1234 }
      ]
    }
  ]
}
```

The artifact keeps short run metadata (`backend`, `prompt_id`, `mode`, git SHA,
timestamp) plus numerical/boolean timing values. The harness rejects forbidden
content-bearing keys (`query`, `prompt`, `answer`, `chunk_text`, `url`,
`source`) and strings longer than 100 characters before writing a successful
result.

## Cold Vs Warm

- **cold**: `axon ask` runs with `--local`, forcing in-process startup each invocation.
- **warm**: assumes `axon serve` is already running and reuses it through `AXON_SERVER_URL`.

Do not average them. Report cold and warm side-by-side.

## Artifacts And `.gitignore`

`docs/perf/results-*.json` is git-ignored. Do not commit generated result files
without explicit review.

## Historical Baseline

`docs/perf/quality-parity-2026-05-07.md` is retained as historical comparison
evidence from the legacy completion-to-Gemini transition. It is not an active runtime guide.
