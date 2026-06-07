#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

DEFAULT_QUESTIONS="$REPO_ROOT/reports/llm-ask-comparison-2026-06-07/questions-indexed-general.md"
DEFAULT_MODELS="current,gemini-flash,gemma-local"

usage() {
  cat <<'USAGE'
Usage:
  scripts/run-ask-model-comparison.sh [options]

Runs the indexed general-knowledge question set through one or more Axon LLM
profiles, saving answer markdown files, stderr logs, and per-question timing.

Options:
  --questions PATH       Markdown question file (default: reports/.../questions-indexed-general.md)
  --out-dir PATH         Output directory (default: reports/.../run-YYYYmmdd-HHMMSS)
  --axon-bin PATH        Axon binary/script to run (default: target/release/axon, then scripts/axon)
  --models LIST          Comma-separated profiles: current,gemini-flash,gemma-local
                         (default: current,gemini-flash,gemma-local)
  --base-env PATH        Env file to copy for override profiles (default: ~/.axon/.env)
  --dry-run             Parse questions and print the planned runs without invoking axon
  --skip-preflight      Skip llama.cpp /v1/models reachability check
  --self-test           Run a local no-network smoke test with a fake axon binary
  -h, --help            Show this help

Environment overrides:
  GEMINI_FLASH_MODEL    Default: gemini-3.5-flash-low
  CLI_API_BASE_URL      Default: https://cli-api.tootie.tv/v1
  GEMMA_OPENAI_BASE_URL Default: http://127.0.0.1:8080/v1
  GEMMA_MODEL           Default: ggml-org/gemma-4-E4B-it-GGUF:Q4_K_M
  GEMMA_CONTEXT_CHARS   Default: 300000

Output:
  run.json              Run metadata, selected model configs, per-question timing/results
  <profile>/QNN.md      Answer markdown for each model/question
  <profile>/QNN.stderr.log
USAGE
}

QUESTIONS_FILE="$DEFAULT_QUESTIONS"
OUT_DIR=""
AXON_BIN="${AXON_BIN:-}"
MODELS="$DEFAULT_MODELS"
BASE_ENV_FILE="${AXON_BASE_ENV_FILE:-$HOME/.axon/.env}"
DRY_RUN=0
SKIP_PREFLIGHT=0

need() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "missing required command: $1" >&2
    exit 127
  fi
}

json_string_array_from_csv() {
  local csv="$1"
  jq -Rn --arg csv "$csv" '$csv | split(",") | map(select(length > 0))'
}

resolve_axon_bin() {
  if [[ -n "$AXON_BIN" ]]; then
    return
  fi
  if [[ -x "$REPO_ROOT/target/release/axon" ]]; then
    AXON_BIN="$REPO_ROOT/target/release/axon"
  elif [[ -x "$REPO_ROOT/scripts/axon" ]]; then
    AXON_BIN="$REPO_ROOT/scripts/axon"
  elif command -v axon >/dev/null 2>&1; then
    AXON_BIN="$(command -v axon)"
  else
    echo "axon binary not found. Build axon or pass --axon-bin PATH." >&2
    exit 2
  fi
}

slugify() {
  tr '[:upper:]' '[:lower:]' \
    | sed -E 's/[^a-z0-9]+/-/g; s/^-+//; s/-+$//; s/-{2,}/-/g'
}

extract_questions() {
  local file="$1"
  awk '
    /^## Questions[[:space:]]*$/ { in_questions=1; next }
    /^## Answer Key[[:space:]]*$/ { in_questions=0 }
    in_questions && /^[0-9]+[.][[:space:]]+/ {
      id=sprintf("Q%02d", ++count)
      text=$0
      sub(/^[0-9]+[.][[:space:]]+/, "", text)
      printf "%s\t%s\n", id, text
    }
  ' "$file"
}

append_env_override() {
  local env_file="$1"
  local key="$2"
  local value="$3"
  printf '%s=%q\n' "$key" "$value" >>"$env_file"
}

write_override_env() {
  local profile="$1"
  local env_file="$2"

  if [[ -f "$BASE_ENV_FILE" ]]; then
    grep -vE '^(AXON_LLM_BACKEND|AXON_OPENAI_BASE_URL|AXON_OPENAI_MODEL|AXON_OPENAI_API_KEY|AXON_ASK_|AXON_LLM_COMPLETION_)=' "$BASE_ENV_FILE" >"$env_file" || true
  else
    : >"$env_file"
  fi

  case "$profile" in
    gemini-flash)
      append_env_override "$env_file" AXON_LLM_BACKEND "openai-compat"
      append_env_override "$env_file" AXON_OPENAI_BASE_URL "${CLI_API_BASE_URL:-https://cli-api.tootie.tv/v1}"
      append_env_override "$env_file" AXON_OPENAI_MODEL "${GEMINI_FLASH_MODEL:-gemini-3.5-flash-low}"
      append_env_override "$env_file" AXON_LLM_COMPLETION_CONCURRENCY "1"
      ;;
    gemma-local)
      append_env_override "$env_file" AXON_LLM_BACKEND "openai-compat"
      append_env_override "$env_file" AXON_OPENAI_BASE_URL "${GEMMA_OPENAI_BASE_URL:-http://127.0.0.1:8080/v1}"
      append_env_override "$env_file" AXON_OPENAI_MODEL "${GEMMA_MODEL:-ggml-org/gemma-4-E4B-it-GGUF:Q4_K_M}"
      append_env_override "$env_file" AXON_OPENAI_API_KEY ""
      append_env_override "$env_file" AXON_LLM_COMPLETION_CONCURRENCY "1"
      append_env_override "$env_file" AXON_ASK_MAX_CONTEXT_CHARS "${GEMMA_CONTEXT_CHARS:-300000}"
      append_env_override "$env_file" AXON_ASK_CHUNK_LIMIT "${GEMMA_CHUNK_LIMIT:-20}"
      append_env_override "$env_file" AXON_ASK_CANDIDATE_LIMIT "${GEMMA_CANDIDATE_LIMIT:-120}"
      append_env_override "$env_file" AXON_ASK_HYBRID_CANDIDATES "${GEMMA_HYBRID_CANDIDATES:-100}"
      append_env_override "$env_file" AXON_ASK_DOC_FETCH_CONCURRENCY "${GEMMA_DOC_FETCH_CONCURRENCY:-1}"
      ;;
    *)
      echo "internal error: no override env for profile $profile" >&2
      exit 2
      ;;
  esac
  chmod 600 "$env_file"
}

env_overrides_json() {
  local profile="$1"
  case "$profile" in
    current)
      jq -n '{}'
      ;;
    gemini-flash)
      jq -n \
        --arg backend "openai-compat" \
        --arg base_url "${CLI_API_BASE_URL:-https://cli-api.tootie.tv/v1}" \
        --arg model "${GEMINI_FLASH_MODEL:-gemini-3.5-flash-low}" \
        --arg concurrency "1" \
        '{
          AXON_LLM_BACKEND: $backend,
          AXON_OPENAI_BASE_URL: $base_url,
          AXON_OPENAI_MODEL: $model,
          AXON_LLM_COMPLETION_CONCURRENCY: $concurrency
        }'
      ;;
    gemma-local)
      jq -n \
        --arg backend "openai-compat" \
        --arg base_url "${GEMMA_OPENAI_BASE_URL:-http://127.0.0.1:8080/v1}" \
        --arg model "${GEMMA_MODEL:-ggml-org/gemma-4-E4B-it-GGUF:Q4_K_M}" \
        --arg api_key_redacted "" \
        --arg concurrency "1" \
        --arg context "${GEMMA_CONTEXT_CHARS:-300000}" \
        --arg chunks "${GEMMA_CHUNK_LIMIT:-20}" \
        --arg candidates "${GEMMA_CANDIDATE_LIMIT:-120}" \
        --arg hybrid "${GEMMA_HYBRID_CANDIDATES:-100}" \
        --arg doc_fetch "${GEMMA_DOC_FETCH_CONCURRENCY:-1}" \
        '{
          AXON_LLM_BACKEND: $backend,
          AXON_OPENAI_BASE_URL: $base_url,
          AXON_OPENAI_MODEL: $model,
          AXON_OPENAI_API_KEY: $api_key_redacted,
          AXON_LLM_COMPLETION_CONCURRENCY: $concurrency,
          AXON_ASK_MAX_CONTEXT_CHARS: $context,
          AXON_ASK_CHUNK_LIMIT: $chunks,
          AXON_ASK_CANDIDATE_LIMIT: $candidates,
          AXON_ASK_HYBRID_CANDIDATES: $hybrid,
          AXON_ASK_DOC_FETCH_CONCURRENCY: $doc_fetch
        }'
      ;;
    *)
      jq -n --arg profile "$profile" '{unknown_profile: $profile}'
      ;;
  esac
}

capture_effective_config() {
  local profile="$1"
  local env_file="${2:-}"
  local config_json stderr_file
  stderr_file="$TMP_ENV_DIR/${profile}.config.stderr.log"
  if [[ "$profile" == "current" ]]; then
    if config_json="$("$AXON_BIN" config list --json 2>"$stderr_file")"; then
      jq -c . <<<"$config_json"
    else
      jq -n --arg status "unavailable" --arg stderr "$(cat "$stderr_file" 2>/dev/null || true)" \
        '{status:$status, stderr:$stderr}'
    fi
  else
    if config_json="$(AXON_ENV_FILE="$env_file" "$AXON_BIN" config list --json 2>"$stderr_file")"; then
      jq -c . <<<"$config_json"
    else
      jq -n --arg status "unavailable" --arg stderr "$(cat "$stderr_file" 2>/dev/null || true)" \
        '{status:$status, stderr:$stderr}'
    fi
  fi
}

profile_label() {
  case "$1" in
    current) echo "current-config" ;;
    gemini-flash) echo "cli-api-gemini-3.5-flash-low" ;;
    gemma-local) echo "llamacpp-gemma-4-e4b-q4" ;;
    *) echo "$1" ;;
  esac
}

profile_provider() {
  case "$1" in
    current) echo "current-config" ;;
    gemini-flash) echo "${CLI_API_BASE_URL:-https://cli-api.tootie.tv/v1}" ;;
    gemma-local) echo "${GEMMA_OPENAI_BASE_URL:-http://127.0.0.1:8080/v1}" ;;
    *) echo "$1" ;;
  esac
}

profile_model() {
  case "$1" in
    current) echo "current-config" ;;
    gemini-flash) echo "${GEMINI_FLASH_MODEL:-gemini-3.5-flash-low}" ;;
    gemma-local) echo "${GEMMA_MODEL:-ggml-org/gemma-4-E4B-it-GGUF:Q4_K_M}" ;;
    *) echo "$1" ;;
  esac
}

validate_profiles() {
  local IFS=,
  local profile
  for profile in $MODELS; do
    case "$profile" in
      current|gemini-flash|gemma-local) ;;
      *)
        echo "unknown model profile: $profile" >&2
        echo "valid profiles: current, gemini-flash, gemma-local" >&2
        exit 2
        ;;
    esac
  done
}

preflight() {
  if [[ "$SKIP_PREFLIGHT" -eq 1 || ",$MODELS," != *",gemma-local,"* ]]; then
    return
  fi
  need curl
  local base="${GEMMA_OPENAI_BASE_URL:-http://127.0.0.1:8080/v1}"
  if ! curl -fsS --max-time 4 "$base/models" >/dev/null; then
    echo "llama.cpp OpenAI-compatible endpoint is not reachable at $base/models" >&2
    echo "start it with: docker compose --env-file ~/.axon/.env -f docker-compose.llama.yaml up -d" >&2
    exit 1
  fi
}

write_run_readme() {
  local readme="$OUT_DIR/README.md"
  {
    echo "# Axon Ask Model Comparison Run"
    echo
    echo "Created: $(date --iso-8601=seconds)"
    echo
    echo "- Questions: $QUESTIONS_FILE"
    echo "- Axon: $AXON_BIN"
    echo "- Models: $MODELS"
    echo "- Run JSON: run.json"
    echo
    echo "Temporary env files are generated outside this directory and removed on exit."
  } >"$readme"
}

write_profile_config() {
  local profile="$1"
  local env_file="${2:-}"
  local label overrides effective
  label="$(profile_label "$profile")"
  overrides="$(env_overrides_json "$profile")"
  effective="$(capture_effective_config "$profile" "$env_file")"
  jq -n \
    --arg profile "$profile" \
    --arg label "$label" \
    --arg provider "$(profile_provider "$profile")" \
    --arg model "$(profile_model "$profile")" \
    --argjson overrides "$overrides" \
    --argjson effective "$effective" \
    '{
      profile: $profile,
      label: $label,
      provider: $provider,
      model: $model,
      env_overrides: $overrides,
      effective_config: $effective
    }'
}

run_profile_question() {
  local profile="$1"
  local question_id="$2"
  local question="$3"
  local profile_dir="$OUT_DIR/$(profile_label "$profile")"
  local answer_file="$profile_dir/${question_id}.md"
  local stderr_file="$profile_dir/${question_id}.stderr.log"
  local provider model started_at finished_at start_ns end_ns elapsed exit_code

  mkdir -p "$profile_dir"
  provider="$(profile_provider "$profile")"
  model="$(profile_model "$profile")"

  started_at="$(date --iso-8601=seconds)"
  start_ns="$(date +%s%N)"
  set +e
  if [[ "$profile" == "current" ]]; then
    "$AXON_BIN" ask "$question" >"$answer_file" 2>"$stderr_file"
    exit_code=$?
  else
    local env_file="$TMP_ENV_DIR/${profile}.env"
    AXON_ENV_FILE="$env_file" "$AXON_BIN" ask "$question" >"$answer_file" 2>"$stderr_file"
    exit_code=$?
  fi
  set -e
  end_ns="$(date +%s%N)"
  finished_at="$(date --iso-8601=seconds)"
  elapsed="$(awk "BEGIN { printf \"%.3f\", ($end_ns - $start_ns) / 1000000000 }")"

  {
    printf '## %s\n\n' "$question_id"
    printf '**Question:** %s\n\n' "$question"
    printf '**Provider:** `%s`  \n' "$provider"
    printf '**Model:** `%s`  \n' "$model"
    printf '**Elapsed:** `%ss`  \n' "$elapsed"
    printf '**Exit code:** `%s`\n\n' "$exit_code"
    printf '%s\n\n' '---'
    cat "$answer_file"
  } >"$answer_file.tmp"
  mv "$answer_file.tmp" "$answer_file"

  jq -cn \
    --arg question_id "$question_id" \
    --arg question "$question" \
    --arg profile "$profile" \
    --arg profile_label "$(profile_label "$profile")" \
    --arg provider "$provider" \
    --arg model "$model" \
    --arg started_at "$started_at" \
    --arg finished_at "$finished_at" \
    --arg elapsed_seconds "$elapsed" \
    --argjson exit_code "$exit_code" \
    --arg stdout_file "$answer_file" \
    --arg stderr_file "$stderr_file" \
    '{
      question_id: $question_id,
      question: $question,
      profile: $profile,
      profile_label: $profile_label,
      provider: $provider,
      model: $model,
      started_at: $started_at,
      finished_at: $finished_at,
      elapsed_seconds: ($elapsed_seconds | tonumber),
      exit_code: $exit_code,
      stdout_file: $stdout_file,
      stderr_file: $stderr_file
    }' >>"$OUT_DIR/results.jsonl"
}

finalize_run_json() {
  local profiles_json results_json model_list_json
  profiles_json="$(jq -s . "$OUT_DIR/profile-configs.jsonl")"
  results_json="$(jq -s . "$OUT_DIR/results.jsonl")"
  model_list_json="$(json_string_array_from_csv "$MODELS")"
  jq -n \
    --arg schema "axon-ask-model-comparison/v1" \
    --arg created_at "$(date --iso-8601=seconds)" \
    --arg questions_file "$QUESTIONS_FILE" \
    --arg out_dir "$OUT_DIR" \
    --arg axon_bin "$AXON_BIN" \
    --argjson models "$model_list_json" \
    --argjson profiles "$profiles_json" \
    --argjson results "$results_json" \
    '{
      schema: $schema,
      created_at: $created_at,
      questions_file: $questions_file,
      out_dir: $out_dir,
      axon_bin: $axon_bin,
      selected_models: $models,
      profiles: $profiles,
      results: $results
    }' >"$OUT_DIR/run.json"
  rm -f "$OUT_DIR/profile-configs.jsonl" "$OUT_DIR/results.jsonl"
}

run_all() {
  [[ -f "$QUESTIONS_FILE" ]] || { echo "questions file not found: $QUESTIONS_FILE" >&2; exit 2; }
  need jq
  resolve_axon_bin
  [[ -x "$AXON_BIN" ]] || { echo "axon binary is not executable: $AXON_BIN" >&2; exit 2; }
  validate_profiles

  mapfile -t QUESTIONS < <(extract_questions "$QUESTIONS_FILE")
  if [[ "${#QUESTIONS[@]}" -eq 0 ]]; then
    echo "no questions found in $QUESTIONS_FILE" >&2
    exit 2
  fi

  if [[ -z "$OUT_DIR" ]]; then
    OUT_DIR="$REPO_ROOT/reports/llm-ask-comparison-2026-06-07/run-$(date +%Y%m%d-%H%M%S)"
  fi

  if [[ "$DRY_RUN" -eq 1 ]]; then
    echo "axon: $AXON_BIN"
    echo "questions: $QUESTIONS_FILE (${#QUESTIONS[@]} questions)"
    echo "out_dir: $OUT_DIR"
    echo "models: $MODELS"
    return
  fi

  preflight
  mkdir -p "$OUT_DIR"
  write_run_readme
  extract_questions "$QUESTIONS_FILE" >"$OUT_DIR/questions.tsv"
  : >"$OUT_DIR/profile-configs.jsonl"
  : >"$OUT_DIR/results.jsonl"

  TMP_ENV_DIR="$(mktemp -d)"
  trap 'rm -rf "$TMP_ENV_DIR"' EXIT

  local IFS=,
  local profile row question_id question env_file
  for profile in $MODELS; do
    if [[ "$profile" != "current" ]]; then
      env_file="$TMP_ENV_DIR/${profile}.env"
      write_override_env "$profile" "$env_file"
      write_profile_config "$profile" "$env_file" >>"$OUT_DIR/profile-configs.jsonl"
    else
      write_profile_config "$profile" "" >>"$OUT_DIR/profile-configs.jsonl"
    fi
  done

  for profile in $MODELS; do
    echo "running profile: $(profile_label "$profile")" >&2
    for row in "${QUESTIONS[@]}"; do
      question_id="${row%%$'\t'*}"
      question="${row#*$'\t'}"
      echo "  $question_id" >&2
      run_profile_question "$profile" "$question_id" "$question"
    done
  done

  finalize_run_json
  echo "wrote $OUT_DIR" >&2
  echo "$OUT_DIR"
}

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
echo "answer for: $2"
SH
  chmod 700 "$fake"

  QUESTIONS_FILE="$questions"
  OUT_DIR="$out"
  AXON_BIN="$fake"
  MODELS="current,gemini-flash,gemma-local"
  BASE_ENV_FILE="$tmp/base.env"
  SKIP_PREFLIGHT=1
  printf 'AXON_OPENAI_API_KEY=fake\n' >"$BASE_ENV_FILE"
  run_all >/dev/null

  [[ -f "$out/run.json" ]] || { echo "self-test missing run.json" >&2; exit 1; }
  [[ ! -f "$out/timing.tsv" ]] || { echo "self-test should not write timing.tsv" >&2; exit 1; }
  jq -e '.results | length == 6' "$out/run.json" >/dev/null || {
    echo "self-test expected 6 JSON results" >&2
    exit 1
  }
  jq -e '.profiles | length == 3 and all(.[]; has("effective_config"))' "$out/run.json" >/dev/null || {
    echo "self-test expected model configs in JSON" >&2
    exit 1
  }
  [[ -f "$out/current-config/Q01.md" ]] || { echo "self-test missing answer markdown" >&2; exit 1; }
  grep -q "answer for: What is alpha?" "$out/current-config/Q01.md" || {
    echo "self-test answer body missing" >&2
    exit 1
  }
  if find "$out" -name '*.env' -print -quit | grep -q .; then
    echo "self-test leaked env file into report output" >&2
    exit 1
  fi
  echo "self-test passed"
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --questions) QUESTIONS_FILE="$2"; shift 2 ;;
    --out-dir) OUT_DIR="$2"; shift 2 ;;
    --axon-bin) AXON_BIN="$2"; shift 2 ;;
    --models) MODELS="$2"; shift 2 ;;
    --base-env) BASE_ENV_FILE="$2"; shift 2 ;;
    --dry-run) DRY_RUN=1; shift ;;
    --skip-preflight) SKIP_PREFLIGHT=1; shift ;;
    --self-test) self_test; exit 0 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown option: $1" >&2; usage; exit 2 ;;
  esac
done

run_all
