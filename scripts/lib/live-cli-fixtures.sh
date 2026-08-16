#!/usr/bin/env bash
# Live harness fixture, dependency, provider, and operator-output helpers.

run_fixture_json() {
  local name="$1" logfile="$2"
  shift 2
  local stderr_log="${logfile%.json}.stderr.log" exit_code result started_ms
  started_ms="$(now_millis)"
  timeout "${TIMEOUT_SECS}s" "$AXON_BIN" "$@" >"$logfile" 2>"$stderr_log"
  exit_code=$?
  if [ "$exit_code" -eq 0 ] && jq -e . "$logfile" >/dev/null 2>&1; then
    result="PASS"
  else
    result="FAIL"
    failures=$((failures + 1))
  fi
  record "$name" "fixture" "$result" "$exit_code" "$*" "$logfile"
  record_timing "$started_ms" "fixture" "$name" "$*"
  [ "$result" = "PASS" ]
}

missing_live() {
  local name="$1" reason="$2"
  failures=$((failures + 1))
  record "$name" "live" "FAIL" "-" "$reason" "-"
}

skip_live() {
  local name="$1" reason="$2"
  record "$name" "live" "SKIP" "-" "$reason" "-"
}

require_fixture_value() {
  local name="$1" value="$2" fixture="$3"
  if [ -n "$value" ]; then
    return 0
  fi
  skip_live "$name" "blocked: prerequisite fixture '$fixture' was not created"
  return 1
}

run_live_provider_preflight() {
  local logfile="$OUTDIR/logs/live-provider-preflight.json"
  local stderr_log="$OUTDIR/logs/live-provider-preflight.stderr.log"
  if timeout "${TIMEOUT_SECS}s" "$AXON_BIN" doctor --json >"$logfile" 2>"$stderr_log" \
    && jq -e '
      .all_ok == true
      and ([.services.qdrant.ok, .services.tei.ok, .services.chrome.ok, .services.llm.ok]
        | all)
    ' "$logfile" >/dev/null 2>&1; then
    record "provider preflight" "preflight" "PASS" "0" \
      "doctor --json" "$logfile"
    return 0
  fi
  record "provider preflight" "preflight" "FAIL" "1" \
    "doctor --json" "$logfile"
  echo "provider preflight failed; refusing to start the expensive live matrix" >&2
  echo "details: $logfile and $stderr_log" >&2
  return 1
}

run_operator_output_contracts() {
  local operator_url
  local static_log="$OUTDIR/logs/live-operator-static.typescript"
  local motion_log="$OUTDIR/logs/live-operator-motion.typescript"
  local quiet_out="$OUTDIR/logs/live-operator-quiet.log"
  local quiet_err="$OUTDIR/logs/live-operator-quiet.stderr.log"
  local verbose_out="$OUTDIR/logs/live-operator-verbose.log"
  local verbose_err="$OUTDIR/logs/live-operator-verbose.stderr.log"
  if ! command -v script >/dev/null 2>&1; then
    missing_live "operator output contracts" "script is required for PTY assertions"
    return
  fi
  operator_url="$fixture_url?operator_output=$$"
  local -a base=(source "$operator_url" --scope page --render-mode chrome \
    --skip-embed --wait true --collection "$AXON_COLLECTION")

  if script --quiet --return --command \
      "$(printf '%q ' env TERM=xterm-256color "$AXON_BIN" --color always --motion never "${base[@]}")" \
      "$static_log" >/dev/null \
    && grep -Fq $'\033[' "$static_log" \
    && ! grep -Eq '⠋|⠙|⠹|⠸|⠼|⠴|⠦|⠧|⠇|⠏' "$static_log"; then
    record "operator static PTY" "contract" "PASS" "0" \
      "ANSI color with no animated spinner frames" "$static_log"
  else
    failures=$((failures + 1))
    record "operator static PTY" "contract" "FAIL" "1" \
      "ANSI color with no animated spinner frames" "$static_log"
  fi

  if script --quiet --return --command \
      "$(printf '%q ' env TERM=xterm-256color "$AXON_BIN" --color always --motion always "${base[@]}")" \
      "$motion_log" >/dev/null \
    && grep -Fq $'\033[' "$motion_log" \
    && grep -Fq 'COMMAND_EXIT_CODE="0"' "$motion_log"; then
    record "operator motion-enabled PTY" "contract" "PASS" "0" \
      "motion-enabled PTY preserved styled output and exited cleanly" "$motion_log"
    prove_option_behavior "@global" "--motion" \
      "motion-enabled PTY preserved styled output and exited cleanly"
  else
    failures=$((failures + 1))
    record "operator motion-enabled PTY" "contract" "FAIL" "1" \
      "motion-enabled PTY preserved styled output and exited cleanly" "$motion_log"
  fi

  if "$AXON_BIN" --quiet "${base[@]}" >"$quiet_out" 2>"$quiet_err" \
    && [ ! -s "$quiet_err" ]; then
    record "operator quiet output" "contract" "PASS" "0" \
      "quiet stderr is empty" "$quiet_err"
  else
    failures=$((failures + 1))
    record "operator quiet output" "contract" "FAIL" "1" \
      "quiet stderr is empty" "$quiet_err"
  fi

  if "$AXON_BIN" -v doctor >"$verbose_out" 2>"$verbose_err" \
    && grep -Fq 'info:' "$verbose_err"; then
    record "operator verbose output" "contract" "PASS" "0" \
      "verbose stderr contains operator diagnostics" "$verbose_err"
    prove_option_behavior "@global" "--verbose" \
      "verbose doctor emitted operator diagnostics"
  else
    failures=$((failures + 1))
    record "operator verbose output" "contract" "FAIL" "1" \
      "verbose stderr contains operator diagnostics" "$verbose_err"
  fi
}
