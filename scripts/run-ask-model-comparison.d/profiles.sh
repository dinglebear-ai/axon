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

copy_env_key_from_base() {
  local env_file="$1"
  local key="$2"
  if [[ -f "$BASE_ENV_FILE" ]]; then
    grep -m1 -E "^${key}=" "$BASE_ENV_FILE" >>"$env_file" || true
  fi
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
      copy_env_key_from_base "$env_file" AXON_OPENAI_API_KEY
      append_env_override "$env_file" AXON_LLM_COMPLETION_CONCURRENCY "1"
      ;;
    gpt-5.4-mini)
      append_env_override "$env_file" AXON_LLM_BACKEND "openai-compat"
      append_env_override "$env_file" AXON_OPENAI_BASE_URL "${CLI_API_BASE_URL:-https://cli-api.tootie.tv/v1}"
      append_env_override "$env_file" AXON_OPENAI_MODEL "${GPT_5_4_MINI_MODEL:-gpt-5.4-mini}"
      copy_env_key_from_base "$env_file" AXON_OPENAI_API_KEY
      append_env_override "$env_file" AXON_LLM_COMPLETION_CONCURRENCY "1"
      ;;
    gemini-3.1-flash-lite)
      append_env_override "$env_file" AXON_LLM_BACKEND "openai-compat"
      append_env_override "$env_file" AXON_OPENAI_BASE_URL "${CLI_API_BASE_URL:-https://cli-api.tootie.tv/v1}"
      append_env_override "$env_file" AXON_OPENAI_MODEL "${GEMINI_3_1_FLASH_LITE_MODEL:-gemini-3.1-flash-lite}"
      copy_env_key_from_base "$env_file" AXON_OPENAI_API_KEY
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

cli_api_overrides_json() {
  local model="$1"
  jq -n \
    --arg backend "openai-compat" \
    --arg base_url "${CLI_API_BASE_URL:-https://cli-api.tootie.tv/v1}" \
    --arg model "$model" \
    --arg concurrency "1" \
    '{
      AXON_LLM_BACKEND: $backend,
      AXON_OPENAI_BASE_URL: $base_url,
      AXON_OPENAI_MODEL: $model,
      AXON_OPENAI_API_KEY: "***",
      AXON_LLM_COMPLETION_CONCURRENCY: $concurrency
    }'
}

env_overrides_json() {
  local profile="$1"
  case "$profile" in
    current)
      jq -n '{}'
      ;;
    gemini-flash)
      cli_api_overrides_json "${GEMINI_FLASH_MODEL:-gemini-3.5-flash-low}"
      ;;
    gpt-5.4-mini)
      cli_api_overrides_json "${GPT_5_4_MINI_MODEL:-gpt-5.4-mini}"
      ;;
    gemini-3.1-flash-lite)
      cli_api_overrides_json "${GEMINI_3_1_FLASH_LITE_MODEL:-gemini-3.1-flash-lite}"
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
      if jq -e type >/dev/null <<<"$config_json"; then
        jq -c . <<<"$config_json"
      else
        echo "effective config capture for $profile returned invalid JSON" >&2
        return 1
      fi
    else
      echo "effective config capture failed for $profile" >&2
      cat "$stderr_file" >&2 2>/dev/null || true
      return 1
    fi
  else
    if config_json="$(AXON_ENV_FILE="$env_file" "$AXON_BIN" config list --json 2>"$stderr_file")"; then
      if jq -e type >/dev/null <<<"$config_json"; then
        jq -c . <<<"$config_json"
      else
        echo "effective config capture for $profile returned invalid JSON" >&2
        return 1
      fi
    else
      echo "effective config capture failed for $profile" >&2
      cat "$stderr_file" >&2 2>/dev/null || true
      return 1
    fi
  fi
}

profile_label() {
  case "$1" in
    current) echo "current-config" ;;
    gemini-flash) safe_profile_label "cli-api-${GEMINI_FLASH_MODEL:-gemini-3.5-flash-low}" "$1" ;;
    gpt-5.4-mini) safe_profile_label "cli-api-${GPT_5_4_MINI_MODEL:-gpt-5.4-mini}" "$1" ;;
    gemini-3.1-flash-lite) safe_profile_label "cli-api-${GEMINI_3_1_FLASH_LITE_MODEL:-gemini-3.1-flash-lite}" "$1" ;;
    gemma-local) echo "llamacpp-gemma-4-e4b-q4" ;;
    *) safe_profile_label "$1" "profile" ;;
  esac
}

profile_provider() {
  case "$1" in
    current) echo "current-config" ;;
    gemini-flash|gpt-5.4-mini|gemini-3.1-flash-lite) echo "${CLI_API_BASE_URL:-https://cli-api.tootie.tv/v1}" ;;
    gemma-local) echo "${GEMMA_OPENAI_BASE_URL:-http://127.0.0.1:8080/v1}" ;;
    *) echo "$1" ;;
  esac
}

profile_model() {
  case "$1" in
    current) echo "current-config" ;;
    gemini-flash) echo "${GEMINI_FLASH_MODEL:-gemini-3.5-flash-low}" ;;
    gpt-5.4-mini) echo "${GPT_5_4_MINI_MODEL:-gpt-5.4-mini}" ;;
    gemini-3.1-flash-lite) echo "${GEMINI_3_1_FLASH_LITE_MODEL:-gemini-3.1-flash-lite}" ;;
    gemma-local) echo "${GEMMA_MODEL:-ggml-org/gemma-4-E4B-it-GGUF:Q4_K_M}" ;;
    *) echo "$1" ;;
  esac
}

config_env_value() {
  local config_json="$1"
  local key="$2"
  jq -r --arg key "$key" '.effective_config.env[$key] // ""' <<<"$config_json"
}

selected_provider_from_config() {
  local profile="$1"
  local config_json="$2"
  local backend base
  backend="$(config_env_value "$config_json" AXON_LLM_BACKEND)"
  case "$backend" in
    openai-compat)
      base="$(config_env_value "$config_json" AXON_OPENAI_BASE_URL)"
      [[ -n "$base" ]] && echo "$base" || echo "<unset-openai-base-url>"
      ;;
    gemini-headless|gemini|headless|"")
      echo "gemini-headless"
      ;;
    *)
      if [[ "$profile" == "current" ]]; then
        echo "${backend:-<unset-backend>}"
      else
        profile_provider "$profile"
      fi
      ;;
  esac
}

selected_model_from_config() {
  local profile="$1"
  local config_json="$2"
  local backend model
  backend="$(config_env_value "$config_json" AXON_LLM_BACKEND)"
  case "$backend" in
    openai-compat)
      model="$(config_env_value "$config_json" AXON_OPENAI_MODEL)"
      ;;
    gemini-headless|gemini|headless|"")
      model="$(config_env_value "$config_json" AXON_HEADLESS_GEMINI_MODEL)"
      ;;
    *)
      model=""
      ;;
  esac
  if [[ -n "$model" ]]; then
    echo "$model"
  elif [[ "$profile" == "current" ]]; then
    echo "<default-model>"
  else
    profile_model "$profile"
  fi
}
