self_test() {
  local tmp fake questions out
  tmp="$(mktemp -d)"
  trap 'rm -rf "$tmp"' EXIT
  questions="$tmp/questions.md"
  out="$tmp/out"
  fake="$tmp/axon"

  cat >"$questions" <<'MD'
# Test Questions

## Questions

1. What is alpha?

2. What is beta?

## Answer Key
MD

  cat >"$fake" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
if [[ "${1:-}" != "ask" ]]; then
  if [[ "${1:-}" == "config" && "${2:-}" == "list" ]]; then
    if [[ "${FAKE_AXON_CONFIG_FAIL:-0}" == "1" ]]; then
      echo "fake config capture failed" >&2
      exit 17
    fi
    printf '{"env":{"AXON_LLM_BACKEND":"fake","AXON_OPENAI_API_KEY":"***"},"toml":{}}\n'
    exit 0
  fi
  echo "expected ask" >&2
  exit 2
fi
if [[ "${2:-}" == "--explain" ]]; then
  case "${FAKE_AXON_EXPLAIN_MODE:-ok}" in
    empty)
      exit 0
      ;;
    missing-context)
      printf '{"explain":{"mode":"explain_only"}}\n'
      exit 0
      ;;
    exit-nonzero)
      echo "fake explain failure" >&2
      exit 19
      ;;
  esac
  query="${@: -1}"
  printf '{"answer":"","diagnostics":{"context_chars":123},"explain":{"mode":"explain_only","llm_skipped":true,"query":%s,"context":{"context_char_budget":128000,"context_chars_used":123,"truncated_by_budget":false,"final_source_order":[]},"candidates":[]},"timing_ms":{"llm":0}}\n' "$(jq -Rn --arg q "$query" '$q')"
  exit 0
fi
echo "answer for: $2"
SH
  chmod 700 "$fake"

  QUESTIONS_FILE="$questions"
  OUT_DIR="$out"
  AXON_BIN="$fake"
  MODELS="$DEFAULT_MODELS"
  BASE_ENV_FILE="$tmp/base.env"
  SKIP_PREFLIGHT=1
  GEMINI_FLASH_MODEL="../bad/model"
  printf 'AXON_OPENAI_API_KEY=fake-secret-value\n' >"$BASE_ENV_FILE"
  local stdout_log stderr_log
  stdout_log="$tmp/stdout.log"
  stderr_log="$tmp/stderr.log"
  run_all >"$stdout_log" 2>"$stderr_log"

  [[ -f "$out/run.json" ]] || { echo "self-test missing run.json" >&2; exit 1; }
  [[ ! -f "$out/timing.tsv" ]] || { echo "self-test should not write timing.tsv" >&2; exit 1; }
  jq -e '.results | length == 10' "$out/run.json" >/dev/null || {
    echo "self-test expected 10 JSON results" >&2
    exit 1
  }
  jq -e '.schema == "axon-ask-model-comparison/v2" and .capture_explain == true and .execution_mode == "parallel"' "$out/run.json" >/dev/null || {
    echo "self-test expected v2 parallel explain-enabled run schema" >&2
    exit 1
  }
  jq -e '.result_schema_features | index("per_result_explain_valid") and index("top_level_capture_explain")' "$out/run.json" >/dev/null || {
    echo "self-test expected explicit result schema feature metadata" >&2
    exit 1
  }
  jq -e '.profiles | length == 5 and all(.[]; has("effective_config"))' "$out/run.json" >/dev/null || {
    echo "self-test expected model configs in JSON" >&2
    exit 1
  }
  jq -e '.profiles[] | select(.profile=="gemini-flash") | .env_overrides.AXON_OPENAI_API_KEY == "***"' "$out/run.json" >/dev/null || {
    echo "self-test expected gemini-flash API key to be preserved and redacted in config metadata" >&2
    exit 1
  }
  jq -e '.profiles[] | select(.profile=="gemini-flash") | .label == "cli-api-bad-model" and .env_overrides.AXON_OPENAI_MODEL == "../bad/model"' "$out/run.json" >/dev/null || {
    echo "self-test expected dynamic model labels to be slugified while preserving model metadata" >&2
    exit 1
  }
  jq -e '.profiles[] | select(.profile=="gpt-5.4-mini") | .env_overrides.AXON_OPENAI_API_KEY == "***" and .env_overrides.AXON_OPENAI_MODEL == "gpt-5.4-mini"' "$out/run.json" >/dev/null || {
    echo "self-test expected gpt-5.4-mini cli-api config metadata" >&2
    exit 1
  }
  jq -e '.profiles[] | select(.profile=="gemini-3.1-flash-lite") | .env_overrides.AXON_OPENAI_API_KEY == "***" and .env_overrides.AXON_OPENAI_MODEL == "gemini-3.1-flash-lite"' "$out/run.json" >/dev/null || {
    echo "self-test expected gemini-3.1-flash-lite cli-api config metadata" >&2
    exit 1
  }
  [[ -f "$out/current-config/Q01.md" ]] || { echo "self-test missing answer markdown" >&2; exit 1; }
  [[ -d "$out/cli-api-bad-model" ]] || { echo "self-test missing slugified dynamic profile directory" >&2; exit 1; }
  [[ ! -e "$tmp/bad" ]] || { echo "self-test wrote outside output directory through dynamic profile label" >&2; exit 1; }
  [[ -f "$out/current-config/Q01.explain.json" ]] || { echo "self-test missing explain JSON" >&2; exit 1; }
  jq -e '.results[] | select(.profile=="current" and .question_id=="Q01") | .explain_file | endswith("Q01.explain.json")' "$out/run.json" >/dev/null || {
    echo "self-test expected explain metadata in run.json" >&2
    exit 1
  }
  jq -e 'all(.results[]; .explain_valid == true and .explain_error == null)' "$out/run.json" >/dev/null || {
    echo "self-test expected successful explain validation in run.json" >&2
    exit 1
  }
  grep -q "answer for: What is alpha?" "$out/current-config/Q01.md" || {
    echo "self-test answer body missing" >&2
    exit 1
  }
  if find "$out" -name '*.env' -print -quit | grep -q .; then
    echo "self-test leaked env file into report output" >&2
    exit 1
  fi
  if grep -R "fake-secret-value" "$out" >/dev/null 2>&1; then
    echo "self-test leaked base env secret into report output" >&2
    exit 1
  fi
  grep -q "Planned comparison run" "$stderr_log" || {
    echo "self-test expected planned-run summary in stderr" >&2
    exit 1
  }
  grep -q "capturing effective config" "$stderr_log" || {
    echo "self-test expected config-capture progress in stderr" >&2
    exit 1
  }
  grep -q "running profiles in parallel" "$stderr_log" || {
    echo "self-test expected parallel profile execution in stderr" >&2
    exit 1
  }
  grep -q "Run complete" "$stderr_log" || {
    echo "self-test expected final summary in stderr" >&2
    exit 1
  }
  grep -q "run.json" "$stderr_log" || {
    echo "self-test expected run.json path in stderr" >&2
    exit 1
  }

  local no_explain_out no_explain_stdout no_explain_stderr
  no_explain_out="$tmp/no-explain-out"
  no_explain_stdout="$tmp/no-explain-stdout.log"
  no_explain_stderr="$tmp/no-explain-stderr.log"
  (
    QUESTIONS_FILE="$questions"
    OUT_DIR="$no_explain_out"
    AXON_BIN="$fake"
    MODELS="current"
    BASE_ENV_FILE="$tmp/base.env"
    SKIP_PREFLIGHT=1
    SERIAL=1
    CAPTURE_EXPLAIN=0
    run_all
  ) >"$no_explain_stdout" 2>"$no_explain_stderr"
  jq -e '.schema == "axon-ask-model-comparison/v2" and .capture_explain == false and .execution_mode == "serial" and (.results | length == 2)' "$no_explain_out/run.json" >/dev/null || {
    echo "self-test expected serial no-explain JSON shape" >&2
    exit 1
  }
  jq -e 'all(.results[]; .explain_file == null and .explain_valid == null and .explain_error == null)' "$no_explain_out/run.json" >/dev/null || {
    echo "self-test expected no-explain result metadata to be null" >&2
    exit 1
  }
  if find "$no_explain_out" -name '*.explain.json' -print -quit | grep -q .; then
    echo "self-test expected --no-explain to skip explain files" >&2
    exit 1
  fi

  local explain_fail_out explain_fail_stdout explain_fail_stderr
  explain_fail_out="$tmp/explain-fail-out"
  explain_fail_stdout="$tmp/explain-fail-stdout.log"
  explain_fail_stderr="$tmp/explain-fail-stderr.log"
  if (
    QUESTIONS_FILE="$questions"
    OUT_DIR="$explain_fail_out"
    AXON_BIN="$fake"
    MODELS="current"
    BASE_ENV_FILE="$tmp/base.env"
    SKIP_PREFLIGHT=1
    SERIAL=1
    CAPTURE_EXPLAIN=1
    FAKE_AXON_EXPLAIN_MODE=missing-context
    export FAKE_AXON_EXPLAIN_MODE
    run_all
  ) >"$explain_fail_stdout" 2>"$explain_fail_stderr"; then
    echo "self-test expected invalid explain JSON to fail the run" >&2
    exit 1
  fi
  [[ -f "$explain_fail_out/run.json" ]] || { echo "self-test expected explain failure run.json for accounting" >&2; exit 1; }
  jq -e 'all(.results[]; .explain_valid == false and (.explain_error | test("explain\\.context")))' "$explain_fail_out/run.json" >/dev/null || {
    echo "self-test expected explain validation failures in run.json" >&2
    exit 1
  }
  grep -q "failures: 2" "$explain_fail_stderr" || {
    echo "self-test expected explain failures in final failure count" >&2
    exit 1
  }

  local dup_profile_out dup_label_out config_fail_out
  dup_profile_out="$tmp/dup-profile-out"
  if (
    QUESTIONS_FILE="$questions"
    OUT_DIR="$dup_profile_out"
    AXON_BIN="$fake"
    MODELS="current,current"
    BASE_ENV_FILE="$tmp/base.env"
    SKIP_PREFLIGHT=1
    run_all
  ) >"$tmp/dup-profile-stdout.log" 2>"$tmp/dup-profile-stderr.log"; then
    echo "self-test expected duplicate profile rejection" >&2
    exit 1
  fi
  grep -q "duplicate model profile selected: current" "$tmp/dup-profile-stderr.log" || {
    echo "self-test expected duplicate profile error" >&2
    exit 1
  }
  [[ ! -f "$dup_profile_out/run.json" ]] || { echo "self-test duplicate profile should not finalize run.json" >&2; exit 1; }

  dup_label_out="$tmp/dup-label-out"
  if (
    QUESTIONS_FILE="$questions"
    OUT_DIR="$dup_label_out"
    AXON_BIN="$fake"
    MODELS="gemini-flash,gemini-3.1-flash-lite"
    BASE_ENV_FILE="$tmp/base.env"
    SKIP_PREFLIGHT=1
    GEMINI_FLASH_MODEL="same/model"
    GEMINI_3_1_FLASH_LITE_MODEL="same/model"
    run_all
  ) >"$tmp/dup-label-stdout.log" 2>"$tmp/dup-label-stderr.log"; then
    echo "self-test expected duplicate label rejection" >&2
    exit 1
  fi
  grep -q "duplicate computed profile label selected: cli-api-same-model" "$tmp/dup-label-stderr.log" || {
    echo "self-test expected duplicate label error" >&2
    exit 1
  }
  [[ ! -f "$dup_label_out/run.json" ]] || { echo "self-test duplicate label should not finalize run.json" >&2; exit 1; }

  config_fail_out="$tmp/config-fail-out"
  if (
    QUESTIONS_FILE="$questions"
    OUT_DIR="$config_fail_out"
    AXON_BIN="$fake"
    MODELS="current"
    BASE_ENV_FILE="$tmp/base.env"
    SKIP_PREFLIGHT=1
    FAKE_AXON_CONFIG_FAIL=1
    export FAKE_AXON_CONFIG_FAIL
    run_all
  ) >"$tmp/config-fail-stdout.log" 2>"$tmp/config-fail-stderr.log"; then
    echo "self-test expected effective config capture failure" >&2
    exit 1
  fi
  grep -q "effective config capture failed for current" "$tmp/config-fail-stderr.log" || {
    echo "self-test expected effective config failure error" >&2
    exit 1
  }
  [[ ! -f "$config_fail_out/run.json" ]] || { echo "self-test config failure should not finalize run.json" >&2; exit 1; }
  echo "self-test passed"
}
