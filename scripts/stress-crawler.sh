#!/usr/bin/env bash
set -uo pipefail

# Isolated, opt-in live stress harness for Axon's unified source pipeline.
# The default plan mode performs no crawl and mutates no Axon/Qdrant state.

ROOT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
MODE="plan"
TARGET_URL="${AXON_STRESS_URL:-}"
DESIRED_PAGES="${AXON_STRESS_MAX_PAGES:-500}"
CONCURRENT_JOBS="${AXON_STRESS_CONCURRENT_JOBS:-8}"
TIMEOUT_SECS="${AXON_STRESS_TIMEOUT_SECS:-14400}"
MAP_TIMEOUT_SECS="${AXON_STRESS_MAP_TIMEOUT_SECS:-900}"
MAP_MAX_PAGES="${AXON_STRESS_MAP_MAX_PAGES:-0}"
WORKER_CONCURRENCY="${AXON_STRESS_WORKER_CONCURRENCY:-8}"
SOURCE_CONCURRENCY="${AXON_STRESS_SOURCE_CONCURRENCY:-4}"
MIN_COMPLETION_PERCENT="${AXON_STRESS_MIN_COMPLETION_PERCENT:-90}"
AXON_BIN="${AXON_BIN:-$ROOT_DIR/target/debug/axon}"
RUN_STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
OUTDIR="${AXON_STRESS_OUTDIR:-$ROOT_DIR/.cache/stress-crawler/$RUN_STAMP}"
COLLECTION="${AXON_STRESS_COLLECTION:-axon_stress_${RUN_STAMP,,}_$$_${RANDOM}}"
STATE_DIR="$OUTDIR/state"
REPORT="$OUTDIR/report.json"
WORKER_PID=""
CLEANUP_ARMED=0
DISCOVERY_STRATEGY="map"
FAILURE_REASON=""

# shellcheck disable=SC1091
source "$ROOT_DIR/scripts/lib/stress-crawler-runtime.sh"
parse_stress_args "$@"

case "$MODE" in
  plan|smoke|heavy) ;;
  *) die "invalid mode '$MODE' (expected plan, smoke, or heavy)" ;;
esac
for pair in \
  "max-pages:$DESIRED_PAGES" \
  "concurrent-jobs:$CONCURRENT_JOBS" \
  "timeout-secs:$TIMEOUT_SECS" \
  "map-timeout-secs:$MAP_TIMEOUT_SECS" \
  "map-max-pages:$MAP_MAX_PAGES" \
  "worker-concurrency:$WORKER_CONCURRENCY" \
  "source-concurrency:$SOURCE_CONCURRENCY" \
  "min-completion-percent:$MIN_COMPLETION_PERCENT"; do
  require_uint "${pair%%:*}" "${pair#*:}"
done
[ "$CONCURRENT_JOBS" -ge 2 ] || die "concurrent-jobs must be at least 2"
[ "$WORKER_CONCURRENCY" -ge 2 ] || die "worker concurrency must be at least 2"
[ "$SOURCE_CONCURRENCY" -ge 2 ] || die "source concurrency must be at least 2"
[ "$MIN_COMPLETION_PERCENT" -le 100 ] || die "min completion percent cannot exceed 100"
[[ "$COLLECTION" =~ ^axon_stress_[a-z0-9_]+$ ]] \
  || die "collection must use the generated axon_stress_* safety prefix"

if [ "$MODE" = "smoke" ]; then
  TARGET_URL="${TARGET_URL:-https://doc.rust-lang.org/book/}"
  DESIRED_PAGES="${AXON_STRESS_MAX_PAGES:-2}"
  CONCURRENT_JOBS="${AXON_STRESS_CONCURRENT_JOBS:-2}"
  MAP_MAX_PAGES="${AXON_STRESS_MAP_MAX_PAGES:-16}"
elif [ "$MODE" = "heavy" ]; then
  [ -n "$TARGET_URL" ] || die "--url is required in heavy mode"
  [ "$DESIRED_PAGES" -ge 500 ] || die "heavy mode requires --max-pages >= 500"
  [ "${AXON_STRESS_CONFIRM:-}" = "CRAWL_AND_DELETE_ISOLATED_STATE" ] \
    || die "heavy mode requires AXON_STRESS_CONFIRM=CRAWL_AND_DELETE_ISOLATED_STATE"
fi

if [ -n "$TARGET_URL" ] && [[ ! "$TARGET_URL" =~ ^https?:// ]]; then
  die "target URL must use http:// or https://"
fi

# Load endpoint/credential env without evaluating shell content. Runtime paths
# are replaced below before any Axon command can mutate state.
# shellcheck disable=SC1091
source "$ROOT_DIR/scripts/lib/axon-env.sh"
load_axon_env_file "$ROOT_DIR"
QDRANT_URL="${QDRANT_URL:-${AXON_QDRANT_URL:-}}"
command -v jq >/dev/null 2>&1 || die "jq is required"

if [ "$MODE" = "plan" ]; then
  jq -n \
    --arg mode "$MODE" \
    --arg target_url "$TARGET_URL" \
    --arg collection "$COLLECTION" \
    --arg state_dir "$STATE_DIR" \
    --arg qdrant_url "${QDRANT_URL:-not-configured}" \
    --argjson desired_pages "$DESIRED_PAGES" \
    --argjson concurrent_jobs "$CONCURRENT_JOBS" \
    '{
      mode: $mode,
      mutates: false,
      target_url: (if $target_url == "" then null else $target_url end),
      desired_pages: $desired_pages,
      concurrent_jobs: $concurrent_jobs,
      collection: $collection,
      state_dir: $state_dir,
      qdrant_url: $qdrant_url,
      qdrant_runtime: "external-existing-only",
      cleanup: ["generated Qdrant collection", "isolated AXON_DATA_DIR"],
      next: "rerun with --mode smoke, or provide --url and confirmation for --mode heavy"
    }'
  exit 0
fi

command -v curl >/dev/null 2>&1 || die "curl is required"
command -v sqlite3 >/dev/null 2>&1 || die "sqlite3 is required"
[ -x "$AXON_BIN" ] || die "Axon binary is not executable: $AXON_BIN"
[ -n "$QDRANT_URL" ] || die "QDRANT_URL is required"

qdrant_host="$(sed -E 's#^[a-zA-Z]+://([^/@]+@)?([^/:]+).*#\2#' <<<"$QDRANT_URL")"
case "$qdrant_host" in
  localhost|127.*|::1|axon-qdrant)
    die "stress runs require external Qdrant; refusing endpoint host '$qdrant_host'"
    ;;
esac

mkdir -p "$OUTDIR/logs" "$STATE_DIR"
export AXON_DATA_DIR="$STATE_DIR"
export AXON_SQLITE_PATH="$STATE_DIR/jobs.db"
export AXON_CONFIG_PATH="$STATE_DIR/config.toml"
export AXON_ENV_FILE="$STATE_DIR/.env"
export AXON_COLLECTION="$COLLECTION"
export AXON_OUTPUT_DIR="$STATE_DIR/output"
export AXON_ARTIFACT_ROOT="$STATE_DIR/artifacts"
export AXON_ARTIFACT_BIN_DIR="$STATE_DIR/artifacts/bin"
unset AXON_HOME AXON_SERVER_URL
install -m 0600 /dev/null "$AXON_CONFIG_PATH"
install -m 0600 /dev/null "$AXON_ENV_FILE"

trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

if curl -fsS --max-time 10 "${QDRANT_URL%/}/collections/$COLLECTION" >/dev/null 2>&1; then
  die "generated collection already exists; refusing collision: $COLLECTION"
fi
CLEANUP_ARMED=1

"$AXON_BIN" config set jobs.auto-worker false --json \
  >"$OUTDIR/logs/config-auto-worker.json"
"$AXON_BIN" config set pipeline.unified-worker-concurrency "$WORKER_CONCURRENCY" --json \
  >"$OUTDIR/logs/config-worker-concurrency.json"
"$AXON_BIN" config set pipeline.max-active-source-jobs "$SOURCE_CONCURRENCY" --json \
  >"$OUTDIR/logs/config-source-concurrency.json"

map_started_ms="$(now_ms)"
timeout "${MAP_TIMEOUT_SECS}s" \
  "$AXON_BIN" map "$TARGET_URL" --max-pages "$MAP_MAX_PAGES" --render-mode http --json \
  >"$OUTDIR/map.json" 2>"$OUTDIR/logs/map.stderr.log" \
  || die "URL discovery failed; see $OUTDIR/logs/map.stderr.log"
map_finished_ms="$(now_ms)"
mapped_urls="$(jq -r '(.urls // []) | length' "$OUTDIR/map.json")"
mapped_urls="${mapped_urls:-0}"
if [ "$MODE" = "smoke" ] \
  && [ "${TARGET_URL%/}" = "https://doc.rust-lang.org/book" ] \
  && [ "$mapped_urls" -lt 2 ]; then
  cp -- "$OUTDIR/map.json" "$OUTDIR/map-discovery.json"
  jq '.urls = [
        "https://doc.rust-lang.org/stable/book",
        "https://doc.rust-lang.org/stable/book/ch01-00-getting-started.html"
      ]
      | .discovery_strategy = "bounded-map-plus-explicit-rust-book-seeds"' \
    "$OUTDIR/map-discovery.json" >"$OUTDIR/map.json"
  DISCOVERY_STRATEGY="bounded-map-plus-explicit-rust-book-seeds"
fi
discovered_urls="$(jq -r '(.urls // []) | length' "$OUTDIR/map.json")"
discovered_urls="${discovered_urls:-0}"
[ "$discovered_urls" -gt 0 ] || die "map discovered no crawlable URLs"

target_pages="$DESIRED_PAGES"
target_basis="requested"
if [ "$discovered_urls" -lt "$DESIRED_PAGES" ]; then
  target_pages="$discovered_urls"
  target_basis="evidence-backed-map-maximum"
fi
if [ "$MODE" = "heavy" ] && [ "$target_pages" -lt 500 ]; then
  echo "stress-crawler: map found only $target_pages URLs; using the captured evidence-backed maximum" >&2
fi

page_job_count=$((CONCURRENT_JOBS - 1))
if [ "$discovered_urls" -le "$page_job_count" ]; then
  page_job_count=$((discovered_urls - 1))
fi
[ "$page_job_count" -ge 1 ] \
  || die "target must expose at least two mapped URLs to exercise concurrent source jobs"

jobs_file="$OUTDIR/job-ids.txt"
: >"$jobs_file"
enqueue_started_ms="$(now_ms)"
"$AXON_BIN" source "$TARGET_URL" --scope site --max-pages "$target_pages" \
  --batch-concurrency "$SOURCE_CONCURRENCY" --wait false \
  --collection "$COLLECTION" --json \
  >"$OUTDIR/logs/enqueue-primary.json" 2>"$OUTDIR/logs/enqueue-primary.stderr.log" \
  || die "primary crawl enqueue failed"
primary_job_id="$(jq -r '.job_id // empty' "$OUTDIR/logs/enqueue-primary.json")"
[ -n "$primary_job_id" ] || die "primary crawl enqueue returned no job_id"
printf '%s\n' "$primary_job_id" >>"$jobs_file"

mapfile -t page_urls < <(
  jq -r --arg root "$TARGET_URL" \
    '[.urls[] | select(. != $root)] | unique | .[]' "$OUTDIR/map.json" \
    | head -n "$page_job_count"
)
for index in "${!page_urls[@]}"; do
  enqueue_log="$OUTDIR/logs/enqueue-page-$((index + 1)).json"
  "$AXON_BIN" source "${page_urls[$index]}" --scope page --wait false \
    --collection "$COLLECTION" --json \
    >"$enqueue_log" 2>"$OUTDIR/logs/enqueue-page-$((index + 1)).stderr.log" \
    || die "concurrent page enqueue $((index + 1)) failed"
  job_id="$(jq -r '.job_id // empty' "$enqueue_log")"
  [ -n "$job_id" ] || die "concurrent page enqueue $((index + 1)) returned no job_id"
  printf '%s\n' "$job_id" >>"$jobs_file"
done
enqueue_finished_ms="$(now_ms)"

actual_jobs="$(wc -l <"$jobs_file" | tr -d ' ')"
[ "$actual_jobs" -ge 2 ] || die "fewer than two jobs were enqueued"

worker_started_ms="$(now_ms)"
"$AXON_BIN" jobs worker --idle-exit-secs 30 --json \
  >"$OUTDIR/logs/worker.json" 2>"$OUTDIR/logs/worker.stderr.log" &
WORKER_PID=$!

deadline_epoch=$(( $(date +%s) + TIMEOUT_SECS ))
worker_observations="$OUTDIR/worker-observations.jsonl"
: >"$worker_observations"
while :; do
  terminal=0
  failed=0
  while IFS= read -r job_id; do
    get_log="$OUTDIR/logs/job-$job_id.json"
    if "$AXON_BIN" jobs get "$job_id" --json >"$get_log" 2>"$get_log.stderr"; then
      status="$(jq -r '.status // "unknown"' "$get_log")"
      case "$status" in
        completed) terminal=$((terminal + 1)) ;;
        failed|canceled)
          terminal=$((terminal + 1))
          failed=$((failed + 1))
          ;;
      esac
    fi
  done <"$jobs_file"
  worker_alive=false
  if kill -0 "$WORKER_PID" 2>/dev/null; then
    worker_alive=true
  fi
  tracked_jobs="$(
    sqlite3 -json "$AXON_SQLITE_PATH" \
      "SELECT j.job_id, j.status, j.phase, j.source_id, j.parent_job_id,
              j.root_job_id, j.attempt,
              (SELECT a.attempt_id
                 FROM job_attempts a
                WHERE a.job_id = j.job_id AND a.attempt = j.attempt
                LIMIT 1) AS active_attempt_id,
              j.updated_at, j.last_event_sequence, j.heartbeat_json
         FROM jobs j
        WHERE j.job_id IN ($(sed "s/.*/'&'/" "$jobs_file" | paste -sd, -))
        ORDER BY j.created_at;" 2>/dev/null
  )"
  durable_worker_active="$(
    sqlite3 "$AXON_SQLITE_PATH" \
      "SELECT CASE WHEN EXISTS(
         SELECT 1 FROM jobs
          WHERE status IN ('queued', 'pending', 'waiting', 'blocked', 'running', 'canceling')
            AND kind IN ('provider_probe', 'extract', 'source', 'memory')
         LIMIT 1
       ) THEN 'true' ELSE 'false' END;" 2>/dev/null
  )"
  durable_worker_active="${durable_worker_active:-false}"
  jq -cn \
    --argjson sampled_at_ms "$(now_ms)" \
    --argjson worker_process_alive "$worker_alive" \
    --argjson tracked_terminal "$terminal" \
    --argjson tracked_failed "$failed" \
    --argjson durable_worker_active "$durable_worker_active" \
    --argjson tracked_jobs "${tracked_jobs:-[]}" \
    '{
      sampled_at_ms: $sampled_at_ms,
      worker_process_alive: $worker_process_alive,
      tracked_terminal: $tracked_terminal,
      tracked_failed: $tracked_failed,
      durable_worker_active: $durable_worker_active,
      tracked_jobs: $tracked_jobs
    }' >>"$worker_observations"
  [ "$terminal" -eq "$actual_jobs" ] && break
  if [ "$(date +%s)" -ge "$deadline_epoch" ]; then
    die "timed out with $terminal/$actual_jobs jobs terminal"
  fi
  if ! kill -0 "$WORKER_PID" 2>/dev/null; then
    die "worker exited before all jobs became terminal ($terminal/$actual_jobs)"
  fi
  sleep 5
done
worker_finished_ms="$(now_ms)"
wait "$WORKER_PID" || die "worker exited non-zero"
WORKER_PID=""

jobs_json="$OUTDIR/jobs.json"
while IFS= read -r job_id; do
  jq -c . "$OUTDIR/logs/job-$job_id.json"
done <"$jobs_file" | jq -s . >"$jobs_json"

sqlite3 -json "$AXON_SQLITE_PATH" \
  "SELECT job_id, kind, status, phase, created_at, started_at, finished_at,
          ROUND((julianday(finished_at)-julianday(created_at))*86400.0, 3) AS latency_secs,
          counts_json, error_json
     FROM jobs
    WHERE job_id IN ($(sed "s/.*/'&'/" "$jobs_file" | paste -sd, -));" \
  >"$OUTDIR/sqlite-jobs.json"
jq -n '{
  status: "not_applicable",
  reason: "source plans do not request durable scheduler reservations; embedding capacity is managed in memory"
}' >"$OUTDIR/provider-reservations.json"
sqlite3 -json "$AXON_SQLITE_PATH" \
  "SELECT
      (SELECT COUNT(*) FROM graph_nodes) AS nodes,
      (SELECT COUNT(*) FROM graph_edges) AS edges,
      (SELECT COUNT(*) FROM graph_evidence) AS evidence;" \
  >"$OUTDIR/graph-counts.json"
sqlite3 -json "$AXON_SQLITE_PATH" \
  "SELECT COUNT(*) AS error_events
     FROM job_events
    WHERE severity = 'error' OR status = 'failed';" \
  >"$OUTDIR/error-counts.json"

curl -fsS --max-time 30 \
  "${QDRANT_URL%/}/collections/$COLLECTION" \
  >"$OUTDIR/qdrant-collection.json"
qdrant_points="$(jq -r '.result.points_count // 0' "$OUTDIR/qdrant-collection.json")"
redaction_skips="$(
  awk '
    /skipping chunk with secret-redaction-forbidden payload value/ { count += 1 }
    END { print count + 0 }
  ' "$OUTDIR/logs/worker.stderr.log"
)"

finished_ms="$(now_ms)"
jq -n \
  --arg mode "$MODE" \
  --arg target_url "$TARGET_URL" \
  --arg collection "$COLLECTION" \
  --arg discovery_strategy "$DISCOVERY_STRATEGY" \
  --arg target_basis "$target_basis" \
  --arg primary_job_id "$primary_job_id" \
  --arg qdrant_host "$qdrant_host" \
  --argjson desired_pages "$DESIRED_PAGES" \
  --argjson mapped_urls "$mapped_urls" \
  --argjson discovered_urls "$discovered_urls" \
  --argjson target_pages "$target_pages" \
  --argjson actual_jobs "$actual_jobs" \
  --argjson map_ms "$((map_finished_ms - map_started_ms))" \
  --argjson enqueue_ms "$((enqueue_finished_ms - enqueue_started_ms))" \
  --argjson worker_ms "$((worker_finished_ms - worker_started_ms))" \
  --argjson total_ms "$((finished_ms - map_started_ms))" \
  --argjson qdrant_points "$qdrant_points" \
  --argjson redaction_skips "$redaction_skips" \
  --argjson min_completion_percent "$MIN_COMPLETION_PERCENT" \
  --slurpfile jobs "$jobs_json" \
  --slurpfile sqlite_jobs "$OUTDIR/sqlite-jobs.json" \
  --slurpfile providers "$OUTDIR/provider-reservations.json" \
  --slurpfile graph "$OUTDIR/graph-counts.json" \
  --slurpfile errors "$OUTDIR/error-counts.json" \
  '
    ($jobs[0]) as $job_rows
    | ($sqlite_jobs[0] | map(.latency_secs // 0) | sort) as $latencies
    | ($latencies | length) as $latency_count
    | ($job_rows | map(.counts.documents_done // 0) | add // 0) as $documents_done
    | ($job_rows | map(.counts.documents_total // 0) | add // 0) as $documents_total
    | ($job_rows | map(.counts.chunks_done // 0) | add // 0) as $chunks_done
    | ($job_rows | map(.counts.chunks_total // 0) | add // 0) as $chunks_total
    | ($job_rows[] | select(.job_id == $primary_job_id) | .counts.documents_done // 0) as $primary_documents
    | {
        mode: $mode,
        target: {
          url: $target_url,
          desired_pages: $desired_pages,
          mapped_urls: $mapped_urls,
          discovered_urls: $discovered_urls,
          discovery_strategy: $discovery_strategy,
          target_pages: $target_pages,
          target_basis: $target_basis
        },
        isolation: {
          collection: $collection,
          qdrant_host: $qdrant_host,
          data_dir_cleaned_on_exit: true,
          collection_cleaned_on_exit: true
        },
        timing_ms: {
          map: $map_ms,
          enqueue: $enqueue_ms,
          worker: $worker_ms,
          total: $total_ms
        },
        throughput: {
          documents_per_second: (if $worker_ms > 0 then ($documents_done * 1000 / $worker_ms) else 0 end),
          chunks_per_second: (if $worker_ms > 0 then ($chunks_done * 1000 / $worker_ms) else 0 end),
          vector_points_per_second: (if $worker_ms > 0 then ($qdrant_points * 1000 / $worker_ms) else 0 end)
        },
        jobs: {
          requested_concurrent: $actual_jobs,
          terminal: ($job_rows | length),
          completed: ($job_rows | map(select(.status == "completed")) | length),
          failed: ($job_rows | map(select(.status == "failed" or .status == "canceled")) | length),
          documents_done: $documents_done,
          documents_total: $documents_total,
          chunks_done: $chunks_done,
          chunks_total: $chunks_total,
          latency: {
            per_job: $sqlite_jobs[0],
            min_secs: (if $latency_count > 0 then $latencies[0] else 0 end),
            p50_secs: (
              if $latency_count > 0
              then $latencies[((($latency_count * 0.50) | ceil) - 1)]
              else 0 end
            ),
            p95_secs: (
              if $latency_count > 0
              then $latencies[((($latency_count * 0.95) | ceil) - 1)]
              else 0 end
            ),
            max_secs: (
              if $latency_count > 0
              then $latencies[$latency_count - 1]
              else 0 end
            )
          }
        },
        publication: {
          prepared_chunks: $chunks_done,
          qdrant_points: $qdrant_points,
          point_delta_after_policy: ($chunks_done - $qdrant_points),
          secret_redaction_skips: $redaction_skips,
          graph: ($graph[0][0] // {nodes: 0, edges: 0, evidence: 0})
        },
        provider_reservations: ($providers[0] // []),
        errors: ($errors[0][0] // {error_events: 0}),
        verification: {
          all_jobs_completed: (($job_rows | length) == $actual_jobs and all($job_rows[]; .status == "completed")),
          terminal_counts_balanced: ($documents_done == $documents_total and $chunks_done == $chunks_total),
          primary_completion_percent: (if $target_pages > 0 then ($primary_documents * 100 / $target_pages) else 0 end),
          primary_met_threshold: ($primary_documents * 100 >= $target_pages * $min_completion_percent),
          qdrant_has_points: ($qdrant_points > 0),
          graph_published: (($graph[0][0].nodes // 0) > 0 and ($graph[0][0].evidence // 0) > 0),
          durable_provider_reservations_applicable: false,
          no_error_events: (($errors[0][0].error_events // 0) == 0)
        }
      }
  ' >"$REPORT"

jq -e '
  .verification
  | .all_jobs_completed
    and .terminal_counts_balanced
    and .primary_met_threshold
    and .qdrant_has_points
    and .graph_published
    and .no_error_events
' "$REPORT" >/dev/null || die "stress verification failed; see $REPORT"

jq -e \
  --argjson map_max_ms "$((MAP_TIMEOUT_SECS * 1000 + 60000))" \
  --argjson worker_max_ms "$((TIMEOUT_SECS * 1000 + 60000))" \
  --argjson total_max_ms "$(((MAP_TIMEOUT_SECS + TIMEOUT_SECS) * 1000 + 120000))" \
  '
  (.timing_ms.map >= 0 and .timing_ms.map <= $map_max_ms)
  and (.timing_ms.enqueue >= 0 and .timing_ms.enqueue < 900000)
  and (.timing_ms.worker >= 0 and .timing_ms.worker <= $worker_max_ms)
  and (.timing_ms.total >= .timing_ms.worker and .timing_ms.total <= $total_max_ms)
' "$REPORT" >/dev/null || die "stress timing sanity check failed; see $REPORT"

cat "$REPORT"
