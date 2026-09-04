#!/usr/bin/env bash
# Shared argument, timing, and cleanup helpers for stress-crawler.sh.

usage() {
  cat <<'EOF'
usage: scripts/stress-crawler.sh [options]

Options:
  --mode plan|smoke|heavy     plan is non-mutating (default)
  --url URL                   required for heavy; smoke defaults to the Rust Book
  --max-pages N               heavy target, minimum 500 (default: 500)
  --concurrent-jobs N         queued source jobs including the site crawl (default: 8)
  --timeout-secs N            terminal-state deadline (default: 14400)
  --out-dir PATH              retained report/log directory
  -h, --help

Heavy mode requires:
  AXON_STRESS_CONFIRM=CRAWL_AND_DELETE_ISOLATED_STATE

The harness never starts Qdrant. Heavy mode requires an already-running,
non-loopback QDRANT_URL and deletes only its generated axon_stress_* collection.
EOF
}

die() {
  FAILURE_REASON="$*"
  echo "stress-crawler: $*" >&2
  exit 2
}

now_ms() {
  python3 -c 'import time; print(time.time_ns() // 1_000_000)'
}

require_uint() {
  local name="$1" value="$2"
  [[ "$value" =~ ^[0-9]+$ ]] || die "$name must be an unsigned integer"
}

parse_stress_args() {
  while [ "$#" -gt 0 ]; do
    case "$1" in
      --mode)
        MODE="${2:-}"
        shift 2
        ;;
      --url)
        TARGET_URL="${2:-}"
        shift 2
        ;;
      --max-pages)
        DESIRED_PAGES="${2:-}"
        shift 2
        ;;
      --concurrent-jobs)
        CONCURRENT_JOBS="${2:-}"
        shift 2
        ;;
      --timeout-secs)
        TIMEOUT_SECS="${2:-}"
        shift 2
        ;;
      --out-dir)
        OUTDIR="${2:-}"
        STATE_DIR="$OUTDIR/state"
        REPORT="$OUTDIR/report.json"
        shift 2
        ;;
      -h|--help)
        usage
        exit 0
        ;;
      *)
        die "unknown argument: $1"
        ;;
    esac
  done
}

cleanup() {
  local cleanup_status=$?
  local cleanup_qdrant_http="not-attempted"
  local cleanup_state_removed=false
  local failure_jobs="[]"
  local failure_attempts="[]"
  local failure_heartbeats="[]"
  local failure_events="[]"
  local failure_worker_observations="[]"
  local failure_worker_exit_report="null"
  local failure_qdrant="null"
  if [ -n "$WORKER_PID" ] && kill -0 "$WORKER_PID" 2>/dev/null; then
    kill -TERM "$WORKER_PID" 2>/dev/null || true
    wait "$WORKER_PID" 2>/dev/null || true
  fi
  if [ "$cleanup_status" -ne 0 ] && [ -f "$AXON_SQLITE_PATH" ]; then
    failure_jobs="$(
      sqlite3 -json "$AXON_SQLITE_PATH" \
        "SELECT j.job_id, j.kind, j.status, j.phase, j.source_id,
                j.parent_job_id, j.root_job_id, j.attempt,
                (SELECT a.attempt_id
                   FROM job_attempts a
                  WHERE a.job_id = j.job_id AND a.attempt = j.attempt
                  LIMIT 1) AS active_attempt_id,
                j.last_event_sequence,
                created_at, updated_at, started_at, finished_at, counts_json,
                heartbeat_json, last_error_json, error_json
           FROM jobs j
          ORDER BY created_at;" 2>/dev/null
    )"
    failure_attempts="$(
      sqlite3 -json "$AXON_SQLITE_PATH" \
        "SELECT attempt_id, job_id, attempt, status, worker_id, started_at,
                finished_at, heartbeat_at, error_json
           FROM job_attempts
          ORDER BY started_at, job_id, attempt;" 2>/dev/null
    )"
    failure_heartbeats="$(
      sqlite3 -json "$AXON_SQLITE_PATH" \
        "SELECT job_id, attempt, heartbeat_at, heartbeat_json
           FROM job_heartbeats
          ORDER BY heartbeat_at, job_id, attempt;" 2>/dev/null
    )"
    failure_events="$(
      sqlite3 -json "$AXON_SQLITE_PATH" \
        "SELECT job_id, sequence, status, phase, severity, message, details_json,
                timestamp
           FROM job_events
          WHERE severity IN ('failed', 'fatal')
          ORDER BY timestamp, job_id, sequence;" 2>/dev/null
    )"
    if [ -f "$OUTDIR/worker-observations.jsonl" ]; then
      failure_worker_observations="$(
        jq -s . "$OUTDIR/worker-observations.jsonl" 2>/dev/null
      )"
    fi
    if [ -s "$OUTDIR/logs/worker.json" ]; then
      failure_worker_exit_report="$(
        jq -c . "$OUTDIR/logs/worker.json" 2>/dev/null || printf 'null'
      )"
    fi
  fi
  if [ "$cleanup_status" -ne 0 ] && [ "$CLEANUP_ARMED" -eq 1 ]; then
    failure_qdrant="$(
      curl -fsS --max-time 30 \
        "${QDRANT_URL%/}/collections/$COLLECTION" 2>/dev/null || printf 'null'
    )"
  fi
  if [ "$CLEANUP_ARMED" -eq 1 ] && [[ "$COLLECTION" == axon_stress_* ]]; then
    curl -fsS --max-time 30 -X DELETE \
      "${QDRANT_URL%/}/collections/$COLLECTION" \
      >"$OUTDIR/logs/cleanup-qdrant.json" \
      2>"$OUTDIR/logs/cleanup-qdrant.stderr.log" || true
    cleanup_qdrant_http="$(
      curl -sS --max-time 30 -o "$OUTDIR/logs/cleanup-qdrant-check.json" \
        -w '%{http_code}' "${QDRANT_URL%/}/collections/$COLLECTION" 2>/dev/null \
        || printf 'request-failed'
    )"
  fi
  if [ -d "$STATE_DIR" ] && [[ "$STATE_DIR" == "$OUTDIR/state" ]]; then
    rm -r -- "$STATE_DIR"
  fi
  if [ ! -e "$STATE_DIR" ]; then
    cleanup_state_removed=true
  fi
  if [ "$cleanup_status" -ne 0 ] && [ ! -s "$REPORT" ]; then
    printf '%s\n' "${failure_jobs:-[]}" >"$OUTDIR/logs/failure-jobs.json"
    printf '%s\n' "${failure_attempts:-[]}" >"$OUTDIR/logs/failure-attempts.json"
    printf '%s\n' "${failure_heartbeats:-[]}" >"$OUTDIR/logs/failure-heartbeats.json"
    printf '%s\n' "${failure_events:-[]}" >"$OUTDIR/logs/failure-events.json"
    printf '%s\n' "${failure_worker_observations:-[]}" \
      >"$OUTDIR/logs/failure-worker-observations.json"
    printf '%s\n' "${failure_worker_exit_report:-null}" \
      >"$OUTDIR/logs/failure-worker-exit-report.json"
    printf '%s\n' "${failure_qdrant:-null}" >"$OUTDIR/logs/failure-qdrant.json"
    jq -n \
      --arg mode "$MODE" \
      --arg target_url "$TARGET_URL" \
      --arg collection "$COLLECTION" \
      --arg failure_reason "${FAILURE_REASON:-unexpected nonzero exit}" \
      --arg cleanup_qdrant_http "$cleanup_qdrant_http" \
      --argjson cleanup_state_removed "$cleanup_state_removed" \
      --slurpfile jobs "$OUTDIR/logs/failure-jobs.json" \
      --slurpfile attempts "$OUTDIR/logs/failure-attempts.json" \
      --slurpfile heartbeats "$OUTDIR/logs/failure-heartbeats.json" \
      --slurpfile failed_events "$OUTDIR/logs/failure-events.json" \
      --slurpfile worker_observations "$OUTDIR/logs/failure-worker-observations.json" \
      --slurpfile worker_exit_report "$OUTDIR/logs/failure-worker-exit-report.json" \
      --slurpfile qdrant_before_cleanup "$OUTDIR/logs/failure-qdrant.json" \
      '{
        outcome: "failed",
        mode: $mode,
        target: {url: $target_url},
        failure: {
          reason: $failure_reason,
          jobs: ($jobs[0] // []),
          attempts: ($attempts[0] // []),
          heartbeats: ($heartbeats[0] // []),
          failed_events: ($failed_events[0] // []),
          worker_observations: ($worker_observations[0] // []),
          worker_exit_report: ($worker_exit_report[0] // null)
        },
        publication_before_cleanup: {
          qdrant: ($qdrant_before_cleanup[0] // null)
        },
        cleanup: {
          collection: $collection,
          qdrant_get_after_delete_http: $cleanup_qdrant_http,
          qdrant_collection_absent: ($cleanup_qdrant_http == "404"),
          state_dir_absent: $cleanup_state_removed
        }
      }' >"$REPORT"
  fi
  return "$cleanup_status"
}
