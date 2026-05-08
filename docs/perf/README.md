# Axon `ask` Performance Bench

This directory holds the harness and artifacts for benchmarking `axon ask`
end-to-end on the Gemini headless synthesis path.

## Quick start

Prereqs:
- `axon` binary built (`just build` or `cargo build --release --bin axon`)
- Infra services up: `rtk just services-up` (qdrant, tei, chrome)
- For warm mode: `axon serve` running and `AXON_ASK_SERVER_URL` set
- `jq`, `bash 4+`

Run:

```bash
rtk just bench-ask
rtk bash scripts/bench-ask.sh --runs 100 --mode warm
```

Artifacts land in `docs/perf/results-<timestamp>-<sha>.json`.

## What The Harness Measures

For each prompt it runs `axon ask --json` N times, parses the `timing_ms` object
from each response, and stores the raw timing objects under the
`gemini-headless` backend label.

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
        { "total_ms": 1234 }
      ]
    }
  ]
}
```

## Cold Vs Warm

- **cold**: `axon ask` runs with `--no-server-url`, forcing in-process startup each invocation.
- **warm**: assumes `axon serve` is already running and reuses it through `AXON_ASK_SERVER_URL`.

Do not average them. Report cold and warm side-by-side.

## Artifacts And `.gitignore`

`docs/perf/results-*.json` is git-ignored. Do not commit generated result files
without explicit review.

## Historical Baseline

`docs/perf/quality-parity-2026-05-07.md` is retained as historical comparison
evidence from the legacy completion-to-Gemini transition. It is not an active runtime guide.
