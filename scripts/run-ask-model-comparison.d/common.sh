DEFAULT_QUESTIONS="$REPO_ROOT/reports/llm-ask-comparison-2026-06-07/questions-indexed-general.md"
DEFAULT_MODELS="current,gemini-flash,gpt-5.4-mini,gemini-3.1-flash-lite,gemma-local"

usage() {
  cat <<'USAGE'
Usage:
  scripts/run-ask-model-comparison.sh [options]

Runs the indexed general-knowledge question set through one or more Axon LLM
profiles, saving answer markdown files, stderr logs, and per-question timing.
Each run also captures an `axon ask --explain --diagnostics --json` retrieval
trace for the exact profile/question config before invoking synthesis.

Options:
  --questions PATH       Markdown question file (default: reports/.../questions-indexed-general.md)
  --out-dir PATH         Output directory (default: reports/.../run-YYYYmmdd-HHMMSS)
  --axon-bin PATH        Axon binary/script to run (default: target/release/axon, then scripts/axon)
  --models LIST          Comma-separated profiles: current,gemini-flash,gpt-5.4-mini,
                         gemini-3.1-flash-lite,gemma-local
                         (default: current,gemini-flash,gpt-5.4-mini,
                         gemini-3.1-flash-lite,gemma-local)
  --base-env PATH        Env file to copy for override profiles (default: ~/.axon/.env)
  --dry-run             Parse questions and print the planned runs without invoking axon
  --serial              Run profiles one at a time instead of in parallel
  --no-explain          Do not capture per-question ask --explain JSON traces
  --skip-preflight      Skip llama.cpp /v1/models reachability check
  --self-test           Run a local no-network smoke test with a fake axon binary
  -h, --help            Show this help

Environment overrides:
  GEMINI_FLASH_MODEL    Default: gemini-3.5-flash-low
  GPT_5_4_MINI_MODEL    Default: gpt-5.4-mini
  GEMINI_3_1_FLASH_LITE_MODEL
                         Default: gemini-3.1-flash-lite
  CLI_API_BASE_URL      Default: https://cli-api.tootie.tv/v1
  GEMMA_OPENAI_BASE_URL Default: http://127.0.0.1:8080/v1
  GEMMA_MODEL           Default: ggml-org/gemma-4-26B-A4B-it-GGUF:Q4_K_M
  GEMMA_CONTEXT_CHARS   Default: 128000

Output:
  run.json              Run metadata, selected model configs, per-question timing/results
  <profile>/QNN.md      Answer markdown for each model/question
  <profile>/QNN.stderr.log
  <profile>/QNN.explain.json
  <profile>/QNN.explain.stderr.log
USAGE
}

QUESTIONS_FILE="$DEFAULT_QUESTIONS"
OUT_DIR=""
AXON_BIN="${AXON_BIN:-}"
MODELS="$DEFAULT_MODELS"
BASE_ENV_FILE="${AXON_BASE_ENV_FILE:-$HOME/.axon/.env}"
DRY_RUN=0
SKIP_PREFLIGHT=0
SERIAL=0
CAPTURE_EXPLAIN=1
declare -A PROFILE_PROVIDER=()
declare -A PROFILE_MODEL=()
declare -A PROFILE_SETTINGS=()

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

safe_profile_label() {
  local raw="$1"
  local fallback="$2"
  local label
  label="$(printf '%s' "$raw" | slugify)"
  if [[ -z "$label" ]]; then
    label="$(printf '%s' "$fallback" | slugify)"
  fi
  printf '%s\n' "${label:-profile}"
}
