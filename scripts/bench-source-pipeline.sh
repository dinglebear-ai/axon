#!/bin/bash
set -euo pipefail
umask 077

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)

sanitize_text() {
  sed -E \
    -e 's#https?://[^[:space:]]+#<redacted-url>#g' \
    -e 's#([Aa]uthorization:|[Bb]earer|[Tt]oken|[Kk]ey|[Ss]ecret)[^[:space:]]*#<redacted-secret>#g' \
    -e 's#[A-Za-z0-9_=-]{32,}#<redacted-value>#g'
}

validate_job_id() {
  [[ ${1-} =~ ^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$ ]]
}

validate_safe_source() {
  local source=${1-}
  [[ $source != *'@'* ]] || return 1
  [[ $source != *'$('* ]] || return 1
  [[ $source != *'`'* ]] || return 1
}

metrics_get() {
  local output=$1
  [[ ${AXON_BENCH_MLX_URL:-http://127.0.0.1:8084} == http://127.0.0.1:* ]] || return 2
  curl --connect-timeout 2 --max-time 5 --silent --show-error --fail \
    "${AXON_BENCH_MLX_URL:-http://127.0.0.1:8084}/metrics" >"$output"
  jq -e 'type == "object" and (.epoch | type == "string")' "$output" >/dev/null
}

run_benchmark() {
  local source=${AXON_BENCH_SOURCE:-}
  local axon_bin=${AXON_BENCH_AXON_BIN:-target/release/axon}
  local collection=${AXON_BENCH_COLLECTION:-axon_scheduler_evidence}
  local output=${AXON_BENCH_OUTPUT:-/dev/stdout}
  local work_dir stdout_file stderr_file before_file after_file state_dir

  [[ -n $source ]] || { echo 'AXON_BENCH_SOURCE is required' >&2; return 2; }
  validate_safe_source "$source" || { echo 'source rejected by benchmark safety policy' >&2; return 2; }
  [[ -x $axon_bin ]] || { echo 'benchmark binary is not executable' >&2; return 2; }

  work_dir=$(mktemp -d "${TMPDIR:-/tmp}/axon-source-bench.XXXXXX")
  trap 'rm -rf -- "$work_dir"' EXIT HUP INT TERM
  stdout_file=$work_dir/stdout.json
  stderr_file=$work_dir/stderr.log
  before_file=$work_dir/metrics-before.json
  after_file=$work_dir/metrics-after.json
  state_dir=$work_dir/state
  mkdir -m 700 "$state_dir"

  metrics_get "$before_file"
  local start_ns end_ns status job_id corpus_hash
  start_ns=$(python3 -c 'import time; print(time.monotonic_ns())')
  set +e
  AXON_DATA_DIR="$state_dir" AXON_SQLITE_PATH="$state_dir/jobs.db" \
    "$axon_bin" source "$source" --scope site --max-pages 200 --wait true \
    --json --performance-profile max --quiet --collection "$collection" \
    >"$stdout_file" 2>"$stderr_file"
  status=$?
  set -e
  end_ns=$(python3 -c 'import time; print(time.monotonic_ns())')
  metrics_get "$after_file"

  if [[ $status -ne 0 ]]; then
    sanitize_text <"$stderr_file" >&2
    return "$status"
  fi
  job_id=$(jq -r '.job_id // .data.job_id // .id // empty' "$stdout_file")
  validate_job_id "$job_id" || { echo 'benchmark output did not contain a valid job id' >&2; return 3; }

  corpus_hash=$(jq -cS '
      [.. | objects | select(has("document_id") and has("chunk_id")) |
       [.document_id, .chunk_id, (.content_hash // "")]] | sort
    ' "$stdout_file" | shasum -a 256 | awk '{print $1}')

  PYTHONPATH="$SCRIPT_DIR" python3 - "$before_file" "$after_file" "$job_id" \
    "$corpus_hash" "$start_ns" "$end_ns" >"$output" <<'PY'
import json
import sys
from mlx_metrics import evidence_gate, metrics_delta

before_path, after_path, job_id, corpus_hash, start_ns, end_ns = sys.argv[1:]
with open(before_path, encoding="utf-8") as handle:
    before = json.load(handle)
with open(after_path, encoding="utf-8") as handle:
    after = json.load(handle)
if before.get("requests") != 0:
    raise SystemExit("exclusive MLX benchmark service was already used")
expected_requests = after.get("requests", 0) - before.get("requests", 0)
if expected_requests <= 0:
    raise SystemExit("benchmark issued no MLX requests")
delta = metrics_delta(before, after, expected_requests=expected_requests)
passed, reasons = evidence_gate(delta)
print(json.dumps({
    "job_id": job_id,
    "wall_seconds": (int(end_ns) - int(start_ns)) / 1_000_000_000,
    "corpus_hash": corpus_hash,
    "model_contract": {"dimensions": 1024, "truncation": False},
    "metrics_epoch": delta.epoch,
    "padding_ratio": delta.padding_ratio,
    "row_occupancy": delta.row_occupancy,
    "token_occupancy": delta.token_occupancy,
    "metal_idle_ratio": delta.metal_idle_ratio,
    "evidence_gate": passed,
    "evidence_reasons": reasons,
}, sort_keys=True))
PY
}

if [[ ${AXON_BENCH_LIBRARY_MODE:-0} != 1 ]]; then
  run_benchmark
fi
