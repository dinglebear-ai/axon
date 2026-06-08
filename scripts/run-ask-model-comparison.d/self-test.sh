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
    printf '{"env":{"AXON_LLM_BACKEND":"fake","AXON_OPENAI_API_KEY":"***"},"toml":{}}\n'
    exit 0
  fi
  echo "expected ask" >&2
  exit 2
fi
if [[ "${2:-}" == "--explain" ]]; then
  query="${@: -1}"
  printf '{"answer":"","diagnostics":{"context_chars":123},"explain":{"mode":"explain_only","llm_skipped":true,"query":%s,"context":{"context_char_budget":300000,"context_chars_used":123,"truncated_by_budget":false,"final_source_order":[]},"candidates":[]},"timing_ms":{"llm":0}}\n' "$(jq -Rn --arg q "$query" '$q')"
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
  echo "self-test passed"
}
