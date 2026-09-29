#!/usr/bin/env bash
# Record successful effects, expected denials, handled service errors and skips separately.
record_pass() {
  local name="$1"
  if [[ "$name" == *_skipped_* ]]; then
    echo "SKIP $name" | tee -a "$SUMMARY"
    skip=$((skip + 1))
    return
  fi
  echo "PASS $name" | tee -a "$SUMMARY"
  pass=$((pass + 1))
}

record_fail() {
  local name="$1"
  local logfile="$2"
  echo "FAIL $name (see $logfile)" | tee -a "$SUMMARY"
  fail=$((fail + 1))
}

run_case() {
  local name="$1"
  shift
  local logfile="$OUTDIR/${name}.log"
  if "$@" >"$logfile" 2>&1; then
    record_pass "$name"
  else
    record_fail "$name" "$logfile"
  fi
}

run_json_case() {
  local name="$1"
  local filter="$2"
  shift 2
  local logfile="$OUTDIR/${name}.log"
  if "$@" >"$logfile" 2>&1 && json_payload "$logfile" | jq -e "$filter" >/dev/null; then
    record_pass "$name"
  else
    record_fail "$name" "$logfile"
  fi
}

run_error_case() {
  local name="$1"
  local expected="$2"
  shift 2
  local logfile="$OUTDIR/${name}.log"
  "$@" >"$logfile" 2>&1 || true
  if json_payload "$logfile" | jq -er --arg expected "$expected" '.error | type == "string" and contains($expected)' >/dev/null; then
    echo "EXPECTED_ERROR $name" | tee -a "$SUMMARY"
    expected_error=$((expected_error + 1))
  else
    record_fail "$name" "$logfile"
  fi
}

# Like run_json_case but tolerant of mcporter's non-zero exit on MCP error
# responses: asserts the filter against whatever envelope came back (a valid
# `ok:true` success OR a structured `error` string both count as "the tool
# responded"). Use for routes whose success depends on non-deterministic state
# (job phase, provider availability) where either a success or a structured
# error is an acceptable, non-crashing response.
run_envelope_case() {
  local name="$1"
  local filter="$2"
  shift 2
  local logfile="$OUTDIR/${name}.log"
  "$@" >"$logfile" 2>&1 || true
  if json_payload "$logfile" | jq -e "$filter" >/dev/null 2>&1; then
    if json_payload "$logfile" | jq -e '.ok == true' >/dev/null; then
      record_pass "$name"
    elif [[ "$name" == *_guard ]]; then
      echo "EXPECTED_ERROR $name" | tee -a "$SUMMARY"
      expected_error=$((expected_error + 1))
    else
      echo "HANDLED_ERROR $name" | tee -a "$SUMMARY"
      handled_error=$((handled_error + 1))
    fi
  else
    record_fail "$name" "$logfile"
  fi
}


