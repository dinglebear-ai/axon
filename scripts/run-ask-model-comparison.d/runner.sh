
selected_settings_from_config() {
  local config_json="$1"
  jq -r '
    .effective_config.env as $env
    | [
        "backend=" + ($env.AXON_LLM_BACKEND // "<unset>"),
        "completion_concurrency=" + ($env.AXON_LLM_COMPLETION_CONCURRENCY // "<unset>"),
        "ask_max_context_chars=" + ($env.AXON_ASK_MAX_CONTEXT_CHARS // "<tier-default>"),
        "ask_chunk_limit=" + ($env.AXON_ASK_CHUNK_LIMIT // "<tier-default>"),
        "ask_candidate_limit=" + ($env.AXON_ASK_CANDIDATE_LIMIT // "<tier-default>"),
        "ask_hybrid_candidates=" + ($env.AXON_ASK_HYBRID_CANDIDATES // "<tier-default>")
      ]
    | join(", ")
  ' <<<"$config_json"
}

validate_profiles() {
  local IFS=,
  local profile label
  local -A seen_profiles=()
  local -A seen_labels=()
  for profile in $MODELS; do
    case "$profile" in
      current|gemini-flash|gpt-5.4-mini|gemini-3.1-flash-lite|gemma-local) ;;
      *)
        echo "unknown model profile: $profile" >&2
        echo "valid profiles: current, gemini-flash, gpt-5.4-mini, gemini-3.1-flash-lite, gemma-local" >&2
        exit 2
        ;;
    esac
    if [[ -n "${seen_profiles[$profile]:-}" ]]; then
      echo "duplicate model profile selected: $profile" >&2
      exit 2
    fi
    seen_profiles["$profile"]=1
    label="$(profile_label "$profile")"
    if [[ -n "${seen_labels[$label]:-}" ]]; then
      echo "duplicate computed profile label selected: $label" >&2
      echo "profile labels must be unique after model-name slugification" >&2
      exit 2
    fi
    seen_labels["$label"]=1
  done
}

preflight() {
  if [[ ",$MODELS," == *",gemini-flash,"* || ",$MODELS," == *",gpt-5.4-mini,"* || ",$MODELS," == *",gemini-3.1-flash-lite,"* ]]; then
    local api_key="${AXON_OPENAI_API_KEY:-}"
    if [[ -z "$api_key" ]]; then
      api_key="$(base_env_value AXON_OPENAI_API_KEY)"
    fi
    if [[ -z "$api_key" ]]; then
      echo "cli-api profiles require a non-empty AXON_OPENAI_API_KEY in the environment or --base-env file." >&2
      echo "Set AXON_OPENAI_API_KEY or pass --base-env PATH with that key before running cli-api profiles." >&2
      echo "Current base env: $BASE_ENV_FILE" >&2
      exit 1
    fi
  fi

  if [[ "$SKIP_PREFLIGHT" -eq 1 || ",$MODELS," != *",gemma-local,"* ]]; then
    :
  else
    need curl
    local base="${GEMMA_OPENAI_BASE_URL:-http://127.0.0.1:8080/v1}"
    local attempts="${LLAMA_PREFLIGHT_ATTEMPTS:-60}"
    local delay="${LLAMA_PREFLIGHT_DELAY_SECS:-5}"
    local attempt=1
    while ! curl -fsS --max-time 4 "$base/models" >/dev/null; do
      if [[ "$attempt" -ge "$attempts" ]]; then
        echo "llama.cpp OpenAI-compatible endpoint is not reachable at $base/models" >&2
        echo "start it with: docker compose --env-file ~/.axon/.env -f docker-compose.llama.yaml up -d" >&2
        exit 1
      fi
      echo "waiting for llama.cpp model endpoint at $base/models (${attempt}/${attempts})" >&2
      sleep "$delay"
      attempt=$((attempt + 1))
    done
  fi
}

ensure_profile_env_file() {
  local profile="$1"
  local env_file="$TMP_ENV_DIR/${profile}.env"
  if [[ ! -f "$env_file" ]]; then
    write_override_env "$profile" "$env_file"
  fi
}

profile_env_file() {
  local profile="$1"
  echo "$TMP_ENV_DIR/${profile}.env"
}

run_axon_with_env_file() {
  local env_file="$1"
  shift
  AXON_ENV_FILE="$env_file" "$AXON_BIN" "$@"
}

register_profile() {
  local profile="$1"
  local env_file
  ensure_profile_env_file "$profile"
  env_file="$(profile_env_file "$profile")"
  register_profile_config "$profile" "$env_file"
}

run_axon_for_profile() {
  local profile="$1"
  shift
  local env_file
  ensure_profile_env_file "$profile"
  env_file="$(profile_env_file "$profile")"
  run_axon_with_env_file "$env_file" "$@"
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
    echo "Raw answer, stderr, and explain trace files may include retrieved snippets, internal URLs, local paths, and provider diagnostics. Review or redact this run directory before committing or sharing it."
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

register_profile_config() {
  local profile="$1"
  local env_file="${2:-}"
  local config_json provider model settings
  echo "capturing effective config: $(profile_label "$profile")" >&2
  if ! config_json="$(write_profile_config "$profile" "$env_file")"; then
    echo "failed to capture profile config: $profile" >&2
    exit 1
  fi
  printf '%s\n' "$config_json" >>"$OUT_DIR/profile-configs.jsonl"
  provider="$(selected_provider_from_config "$profile" "$config_json")"
  model="$(selected_model_from_config "$profile" "$config_json")"
  settings="$(selected_settings_from_config "$config_json")"
  PROFILE_PROVIDER["$profile"]="$provider"
  PROFILE_MODEL["$profile"]="$model"
  PROFILE_SETTINGS["$profile"]="$settings"
}

profile_results_file() {
  local profile="$1"
  echo "$OUT_DIR/$(profile_label "$profile").results.jsonl"
}

validate_explain_json() {
  local explain_file="$1"
  if [[ ! -s "$explain_file" ]]; then
    echo "explain output is empty"
    return 1
  fi
  if ! jq -e '.explain.context and (.explain.context | type == "object")' "$explain_file" >/dev/null 2>&1; then
    echo "explain output is not valid JSON with required .explain.context"
    return 1
  fi
}

run_profile_question() {
  local profile="$1"
  local question_id="$2"
  local question="$3"
  local profile_dir="$OUT_DIR/$(profile_label "$profile")"
  local answer_file="$profile_dir/${question_id}.md"
  local stderr_file="$profile_dir/${question_id}.stderr.log"
  local explain_file="$profile_dir/${question_id}.explain.json"
  local explain_stderr_file="$profile_dir/${question_id}.explain.stderr.log"
  local provider model started_at finished_at start_ns end_ns elapsed exit_code
  local explain_started_at explain_finished_at explain_start_ns explain_end_ns explain_elapsed explain_exit_code
  local explain_valid explain_error

  mkdir -p "$profile_dir"
  provider="${PROFILE_PROVIDER[$profile]:-$(profile_provider "$profile")}"
  model="${PROFILE_MODEL[$profile]:-$(profile_model "$profile")}"

  explain_started_at=""
  explain_finished_at=""
  explain_elapsed=""
  explain_exit_code=0
  explain_valid=0
  explain_error=""
  if [[ "$CAPTURE_EXPLAIN" -eq 1 ]]; then
    explain_started_at="$(date --iso-8601=seconds)"
    explain_start_ns="$(date +%s%N)"
    set +e
    run_axon_for_profile "$profile" ask --explain --diagnostics --json "$question" >"$explain_file" 2>"$explain_stderr_file"
    explain_exit_code=$?
    set -e
    explain_end_ns="$(date +%s%N)"
    explain_finished_at="$(date --iso-8601=seconds)"
    explain_elapsed="$(awk "BEGIN { printf \"%.3f\", ($explain_end_ns - $explain_start_ns) / 1000000000 }")"
    if [[ "$explain_exit_code" -eq 0 ]]; then
      if explain_error="$(validate_explain_json "$explain_file")"; then
        explain_valid=1
        explain_error=""
      fi
    else
      explain_error="explain command exited with code ${explain_exit_code}"
    fi
  fi

  started_at="$(date --iso-8601=seconds)"
  start_ns="$(date +%s%N)"
  set +e
  run_axon_for_profile "$profile" ask "$question" >"$answer_file" 2>"$stderr_file"
  exit_code=$?
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
    --arg explain_started_at "$explain_started_at" \
    --arg explain_finished_at "$explain_finished_at" \
    --arg explain_elapsed_seconds "$explain_elapsed" \
    --argjson explain_exit_code "$explain_exit_code" \
    --argjson explain_valid "$explain_valid" \
    --arg explain_error "$explain_error" \
    --arg started_at "$started_at" \
    --arg finished_at "$finished_at" \
    --arg elapsed_seconds "$elapsed" \
    --argjson exit_code "$exit_code" \
    --arg stdout_file "$answer_file" \
    --arg stderr_file "$stderr_file" \
    --arg explain_file "$explain_file" \
    --arg explain_stderr_file "$explain_stderr_file" \
    --argjson capture_explain "$CAPTURE_EXPLAIN" \
    '{
      question_id: $question_id,
      question: $question,
      profile: $profile,
      profile_label: $profile_label,
      provider: $provider,
      model: $model,
      explain_started_at: (if $capture_explain == 1 then $explain_started_at else null end),
      explain_finished_at: (if $capture_explain == 1 then $explain_finished_at else null end),
      explain_elapsed_seconds: (if $capture_explain == 1 and ($explain_elapsed_seconds | length) > 0 then ($explain_elapsed_seconds | tonumber) else null end),
      explain_exit_code: (if $capture_explain == 1 then $explain_exit_code else null end),
      explain_valid: (if $capture_explain == 1 then ($explain_valid == 1) else null end),
      explain_error: (if $capture_explain == 1 and ($explain_error | length) > 0 then $explain_error else null end),
      explain_file: (if $capture_explain == 1 then $explain_file else null end),
      explain_stderr_file: (if $capture_explain == 1 then $explain_stderr_file else null end),
      started_at: $started_at,
      finished_at: $finished_at,
      elapsed_seconds: ($elapsed_seconds | tonumber),
      exit_code: $exit_code,
      stdout_file: $stdout_file,
      stderr_file: $stderr_file
    }' >>"$(profile_results_file "$profile")"

  if [[ "$CAPTURE_EXPLAIN" -eq 1 ]]; then
    echo "  ${question_id}: explain=${explain_elapsed}s explain_exit=${explain_exit_code} explain_valid=${explain_valid} answer=${elapsed}s exit=${exit_code} file=${answer_file}" >&2
  else
    echo "  ${question_id}: ${elapsed}s exit=${exit_code} answer=${answer_file}" >&2
  fi
}

finalize_run_json() {
  local profiles_json results_json model_list_json
  profiles_json="$(jq -s . "$OUT_DIR/profile-configs.jsonl")"
  results_json="$(jq -s 'sort_by(.profile, .question_id)' "$OUT_DIR"/*.results.jsonl)"
  model_list_json="$(json_string_array_from_csv "$MODELS")"
  jq -n \
    --arg schema "axon-ask-model-comparison/v2" \
    --arg created_at "$(date --iso-8601=seconds)" \
    --arg questions_file "$QUESTIONS_FILE" \
    --arg out_dir "$OUT_DIR" \
    --arg axon_bin "$AXON_BIN" \
    --argjson capture_explain "$CAPTURE_EXPLAIN" \
    --arg execution_mode "$(if [[ "$SERIAL" -eq 1 ]]; then echo serial; else echo parallel; fi)" \
    --argjson models "$model_list_json" \
    --argjson profiles "$profiles_json" \
    --argjson results "$results_json" \
    '{
      schema: $schema,
      created_at: $created_at,
      questions_file: $questions_file,
      out_dir: $out_dir,
      axon_bin: $axon_bin,
      capture_explain: ($capture_explain == 1),
      result_schema_features: [
        "per_result_explain_valid",
        "per_result_explain_error",
        "top_level_capture_explain",
        "execution_mode"
      ],
      execution_mode: $execution_mode,
      selected_models: $models,
      profiles: $profiles,
      results: $results
    }' >"$OUT_DIR/run.json"
  rm -f "$OUT_DIR/profile-configs.jsonl" "$OUT_DIR"/*.results.jsonl
}

run_profile() {
  local profile="$1"
  local row question_id question provider model settings
  : >"$(profile_results_file "$profile")"
  provider="${PROFILE_PROVIDER[$profile]:-$(profile_provider "$profile")}"
  model="${PROFILE_MODEL[$profile]:-$(profile_model "$profile")}"
  settings="${PROFILE_SETTINGS[$profile]:-<unavailable>}"
  {
    echo "running profile: ${provider} / ${model}"
    echo "  label: $(profile_label "$profile")"
    echo "  settings: ${settings}"
  } >&2
  for row in "${QUESTIONS[@]}"; do
    question_id="${row%%$'\t'*}"
    question="${row#*$'\t'}"
    echo "  ${question_id}: starting" >&2
    run_profile_question "$profile" "$question_id" "$question"
  done
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
    echo "Planned comparison run"
    echo "  axon: $AXON_BIN"
    echo "  questions: $QUESTIONS_FILE (${#QUESTIONS[@]} questions)"
    echo "  out_dir: $OUT_DIR"
    echo "  models: $MODELS"
    return
  fi

  preflight
  mkdir -p "$OUT_DIR"
  write_run_readme
  extract_questions "$QUESTIONS_FILE" >"$OUT_DIR/questions.tsv"
  : >"$OUT_DIR/profile-configs.jsonl"
  {
    echo "Planned comparison run"
    echo "  axon: $AXON_BIN"
    echo "  questions: $QUESTIONS_FILE (${#QUESTIONS[@]} questions)"
    echo "  out_dir: $OUT_DIR"
    echo "  models: $MODELS"
  } >&2

  TMP_ENV_DIR="$(mktemp -d)"
  trap 'rm -rf "$TMP_ENV_DIR"' EXIT

  local IFS=,
  local profile row question_id question
  for profile in $MODELS; do
    register_profile "$profile"
  done

  if [[ "$SERIAL" -eq 1 ]]; then
    echo "running profiles serially" >&2
    for profile in $MODELS; do
      run_profile "$profile"
    done
  else
    echo "running profiles in parallel" >&2
    local pids=()
    for profile in $MODELS; do
      run_profile "$profile" &
      pids+=("$!")
    done
    local pid failed=0
    for pid in "${pids[@]}"; do
      if ! wait "$pid"; then
        failed=1
      fi
    done
    if [[ "$failed" -ne 0 ]]; then
      echo "one or more profile workers failed before run.json finalization" >&2
      exit 1
    fi
  fi

  finalize_run_json
  local failure_count
  failure_count="$(jq '[.results[] | select(.exit_code != 0 or (.explain_exit_code != null and .explain_exit_code != 0) or (.explain_valid == false))] | length' "$OUT_DIR/run.json")"
  {
    echo "Run complete"
    echo "  out_dir: $OUT_DIR"
    echo "  run_json: $OUT_DIR/run.json"
    echo "  result_count: $(jq '.results | length' "$OUT_DIR/run.json")"
    echo "  failures: $failure_count"
  } >&2
  echo "$OUT_DIR"
  if [[ "$failure_count" -ne 0 ]]; then
    exit 1
  fi
}
