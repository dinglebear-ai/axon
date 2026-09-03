#!/bin/bash
set -euo pipefail
umask 077

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)
BENCH_WORK_DIR=

cleanup_work_dir() {
  if [[ -n ${BENCH_WORK_DIR:-} && -d $BENCH_WORK_DIR ]]; then
    rm -rf -- "$BENCH_WORK_DIR"
  fi
}

sanitize_text() {
  sed -E \
    -e 's#https?://[^[:space:]]+#<redacted-url>#g' \
    -e 's#([Aa]uthorization:|[Bb]earer|[Tt]oken|[Kk]ey|[Ss]ecret)[^[:space:]]*#<redacted-secret>#g' \
    -e 's#[A-Za-z0-9_=-]{32,}#<redacted-value>#g'
}

validate_job_id() {
  [[ ${1-} =~ ^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$ ]]
}

# `cargo build -p <lib-crate>` compiles a library and its dependencies but never
# selects the root `axon` package, so it leaves a stale `target/release/axon` in
# place and still exits zero. A benchmark run against that stale binary silently
# measures code that is not in the working tree, so refuse to start.
reject_stale_binary() {
  local binary=$1 newer
  newer=$(find crates src build.rs Cargo.toml Cargo.lock \
    -newer "$binary" \
    \( -name '*.rs' -o -name 'Cargo.toml' -o -name 'Cargo.lock' \) \
    -print -quit 2>/dev/null)
  if [[ -n $newer ]]; then
    echo "benchmark binary is older than $newer; rebuild with 'cargo build --release --bin axon'" >&2
    return 1
  fi
}

validate_safe_source() {
  local source=${1-}
  [[ $source != *'@'* ]] || return 1
  [[ $source != *'$('* ]] || return 1
  [[ $source != *'`'* ]] || return 1
}

benchmark_config_path() {
  printf '%s\n' "${AXON_BENCH_CONFIG_PATH:-${AXON_CONFIG_PATH:-${HOME}/.axon/config.toml}}"
}

corpus_hash_from_sqlite() {
  local database=$1
  local job_id=$2
  python3 - "$database" "$job_id" <<'PY'
import hashlib
import json
import sqlite3
import sys

database, job_id = sys.argv[1:]
with sqlite3.connect(database) as connection:
    rows = connection.execute(
        """
        SELECT i.source_item_key, i.content_hash
        FROM source_items AS i
        JOIN sources AS s
          ON s.source_id = i.source_id
         AND s.committed_generation = i.generation
        WHERE i.source_id = (SELECT source_id FROM jobs WHERE job_id = ?)
        ORDER BY i.source_item_key
        """,
        (job_id,),
    ).fetchall()
if not rows:
    raise SystemExit("completed benchmark has no committed corpus rows")
payload = json.dumps(rows, separators=(",", ":"), ensure_ascii=False).encode()
print(hashlib.sha256(payload).hexdigest())
PY
}

acquisition_timings_from_log() {
  local log_file=$1
  if [[ ! -f $log_file ]]; then
    echo "benchmark acquisition telemetry log is missing" >&2
    return 1
  fi
  jq -e -s '[
    .[]
    | select(.message == "web acquisition batch timing")
    | {
        timestamp,
        lane,
        item_count: (.item_count | tonumber),
        concurrency: (.concurrency | tonumber),
        wall_ms: (.wall_ms | tonumber),
        first_completion_ms: (.first_completion_ms | tonumber),
        item_p50_ms: (.item_p50_ms | tonumber),
        item_p95_ms: (.item_p95_ms | tonumber),
        item_max_ms: (.item_max_ms | tonumber),
        max_completion_gap_ms: (.max_completion_gap_ms | tonumber),
        slot_occupancy: ((.slot_occupancy_permille | tonumber) / 1000)
      }
  ]
  | if length == 0
    then error("benchmark log contains no web acquisition batch timing records")
    else .
    end' "$log_file"
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
  local work_dir stdout_file stderr_file before_file after_file state_dir acquisition_file config_path

  [[ -n $source ]] || { echo 'AXON_BENCH_SOURCE is required' >&2; return 2; }
  validate_safe_source "$source" || { echo 'source rejected by benchmark safety policy' >&2; return 2; }
  [[ -x $axon_bin ]] || { echo 'benchmark binary is not executable' >&2; return 2; }
  reject_stale_binary "$axon_bin" || return 2
  config_path=$(benchmark_config_path)
  [[ -f $config_path ]] || {
    echo "benchmark config is missing: $config_path" >&2
    echo 'set AXON_BENCH_CONFIG_PATH to the tuned config.toml to benchmark' >&2
    return 2
  }

  work_dir=$(mktemp -d "${TMPDIR:-/tmp}/axon-source-bench.XXXXXX")
  BENCH_WORK_DIR=$work_dir
  trap cleanup_work_dir EXIT HUP INT TERM
  stdout_file=$work_dir/stdout.json
  stderr_file=$work_dir/stderr.log
  before_file=$work_dir/metrics-before.json
  after_file=$work_dir/metrics-after.json
  acquisition_file=$work_dir/acquisition-timings.json
  state_dir=$work_dir/state
  mkdir -m 700 "$state_dir"

  metrics_get "$before_file"
  local start_ns end_ns status job_id corpus_hash
  # Wall-clock nanoseconds are process-independent on every supported macOS
  # Python runtime; independently launched monotonic clocks have not always
  # shared an epoch.
  start_ns=$(python3 -c 'import time; print(time.time_ns())')
  set +e
  AXON_DATA_DIR="$state_dir" AXON_SQLITE_PATH="$state_dir/jobs.db" \
    AXON_CONFIG_PATH="$config_path" \
    "$axon_bin" source "$source" --scope site --max-pages 200 --wait true \
    --cache false --json --performance-profile max --quiet --collection "$collection" \
    >"$stdout_file" 2>"$stderr_file"
  status=$?
  set -e
  end_ns=$(python3 -c 'import time; print(time.time_ns())')
  metrics_get "$after_file"

  if [[ $status -ne 0 ]]; then
    sanitize_text <"$stderr_file" >&2
    return "$status"
  fi
  job_id=$(jq -r '.job_id // .data.job_id // .id // empty' "$stdout_file")
  validate_job_id "$job_id" || { echo 'benchmark output did not contain a valid job id' >&2; return 3; }

  corpus_hash=$(corpus_hash_from_sqlite "$state_dir/jobs.db" "$job_id")
  acquisition_timings_from_log "$state_dir/logs/axon.log" >"$acquisition_file"

  PYTHONPATH="$SCRIPT_DIR" python3 - "$before_file" "$after_file" "$job_id" \
    "$corpus_hash" "$start_ns" "$end_ns" "$state_dir/jobs.db" \
    "$acquisition_file" >"$output" <<'PY'
from datetime import datetime
import json
import sqlite3
import sys
from mlx_metrics import evidence_gate, metrics_delta

before_path, after_path, job_id, corpus_hash, start_ns, end_ns, database, acquisition_path = sys.argv[1:]
with open(before_path, encoding="utf-8") as handle:
    before = json.load(handle)
with open(after_path, encoding="utf-8") as handle:
    after = json.load(handle)
with open(acquisition_path, encoding="utf-8") as handle:
    acquisition_batches = json.load(handle)
if before.get("requests") != 0:
    raise SystemExit("exclusive MLX benchmark service was already used")
expected_requests = after.get("requests", 0) - before.get("requests", 0)
if expected_requests <= 0:
    raise SystemExit("benchmark issued no MLX requests")
delta = metrics_delta(before, after, expected_requests=expected_requests)
passed, reasons = evidence_gate(delta)
with sqlite3.connect(database) as connection:
    rows = connection.execute(
        """
        SELECT phase, started_at, completed_at
        FROM job_stages
        WHERE job_id = ? AND started_at IS NOT NULL AND completed_at IS NOT NULL
        ORDER BY phase
        """,
        (job_id,),
    ).fetchall()
    event_rows = connection.execute(
        """
        SELECT phase, MIN(timestamp), MAX(timestamp), COUNT(*)
        FROM job_events
        WHERE job_id = ?
        GROUP BY phase
        ORDER BY phase
        """,
        (job_id,),
    ).fetchall()
stage_seconds = {}
for phase, started_at, completed_at in rows:
    started = datetime.fromisoformat(started_at.replace("Z", "+00:00"))
    completed = datetime.fromisoformat(completed_at.replace("Z", "+00:00"))
    stage_seconds[phase] = round((completed - started).total_seconds(), 6)
benchmark_started = datetime.fromtimestamp(int(start_ns) / 1_000_000_000).astimezone()
phase_windows = {}
for phase, first_at, last_at, count in event_rows:
    first = datetime.fromisoformat(first_at.replace("Z", "+00:00"))
    last = datetime.fromisoformat(last_at.replace("Z", "+00:00"))
    phase_windows[phase] = {
        "first_offset_seconds": round((first - benchmark_started).total_seconds(), 6),
        "last_offset_seconds": round((last - benchmark_started).total_seconds(), 6),
        "events": count,
    }
report = {
    "job_id": job_id,
    "wall_seconds": (int(end_ns) - int(start_ns)) / 1_000_000_000,
    "corpus_hash": corpus_hash,
    "model_contract": {
        "dimensions": None,
        "truncation": False,
        "dimensions_status": "not asserted; verify against the indexed collection",
        "truncation_source": "Axon TEI provider request contract",
    },
    "metrics_epoch": delta.epoch,
    "padding_ratio": delta.padding_ratio,
    "row_occupancy": delta.row_occupancy,
    "token_occupancy": delta.token_occupancy,
    "metal_idle_ratio": delta.metal_idle_ratio,
    "evidence_gate": passed,
    "evidence_reasons": reasons,
    "stage_seconds": stage_seconds,
    "phase_windows": phase_windows,
    "acquisition_batches": acquisition_batches,
}
print(json.dumps(report, sort_keys=True))
if not passed:
    raise SystemExit(4)
PY

  cleanup_work_dir
  BENCH_WORK_DIR=
  trap - EXIT HUP INT TERM
}

if [[ ${AXON_BENCH_LIBRARY_MODE:-0} != 1 ]]; then
  run_benchmark
fi
