#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage:
  scripts/test-ask-gemma4.sh "question" [-- extra axon ask args...]

Runs the normal `axon ask` retrieval/context path, but swaps the headless Gemini
subprocess for a temporary shim that forwards synthesis to the local llama.cpp
OpenAI-compatible Gemma 4 server.

Defaults are intentionally conservative for the current Gemma 4 server context.
Override any of these env vars to experiment:

  LLAMA_CPP_URL=http://127.0.0.1:8080/v1/chat/completions
  LLAMA_CPP_MODEL=ggml-org/gemma-4-E4B-it-GGUF:Q4_K_M
  LLAMA_CPP_MAX_TOKENS=1024
  LLAMA_CPP_TEMPERATURE=0.2

  AXON_ASK_MAX_CONTEXT_CHARS=20000
  AXON_ASK_CHUNK_LIMIT=6
  AXON_ASK_FULL_DOCS=1
  AXON_ASK_DOC_CHUNK_LIMIT=24
  AXON_ASK_BACKFILL_CHUNKS=1
  AXON_ASK_CANDIDATE_LIMIT=80
  AXON_ASK_HYBRID_CANDIDATES=80
  AXON_LLM_COMPLETION_CONCURRENCY=1

Examples:
  scripts/test-ask-gemma4.sh "How does Axon build ask context?"
  AXON_ASK_MAX_CONTEXT_CHARS=30000 scripts/test-ask-gemma4.sh "Explain hybrid search" -- --diagnostics
USAGE
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" || $# -eq 0 ]]; then
  usage
  exit 0
fi

question=$1
shift

extra_args=()
if [[ "${1:-}" == "--" ]]; then
  shift
  extra_args=("$@")
elif [[ $# -gt 0 ]]; then
  extra_args=("$@")
fi

need() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "missing required command: $1" >&2
    exit 127
  fi
}

need curl
need jq

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
axon_bin="${AXON_BIN:-$repo_root/scripts/axon}"

if [[ ! -x "$axon_bin" ]]; then
  echo "axon runner is not executable: $axon_bin" >&2
  echo "set AXON_BIN=/path/to/axon or run from the repository checkout" >&2
  exit 1
fi

llama_url="${LLAMA_CPP_URL:-http://127.0.0.1:8080/v1/chat/completions}"
llama_model="${LLAMA_CPP_MODEL:-ggml-org/gemma-4-E4B-it-GGUF:Q4_K_M}"

if ! curl -fsS --max-time 3 "${llama_url%/chat/completions}/models" >/dev/null 2>&1; then
  echo "llama.cpp server is not reachable at ${llama_url%/chat/completions}" >&2
  echo "start it with: docker compose -f docker-compose.llama.yaml up -d" >&2
  exit 1
fi

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT
shim="$tmp_dir/gemini-llama-shim"

cat >"$shim" <<'SHIM'
#!/usr/bin/env bash
set -euo pipefail

model="${LLAMA_CPP_MODEL:-ggml-org/gemma-4-E4B-it-GGUF:Q4_K_M}"
while [[ $# -gt 0 ]]; do
  case "$1" in
    --model)
      model="${2:-$model}"
      shift 2
      ;;
    *)
      shift
      ;;
  esac
done

prompt="$(cat)"
url="${LLAMA_CPP_URL:-http://127.0.0.1:8080/v1/chat/completions}"
max_tokens="${LLAMA_CPP_MAX_TOKENS:-1024}"
temperature="${LLAMA_CPP_TEMPERATURE:-0.2}"

payload="$(jq -n \
  --arg model "$model" \
  --arg prompt "$prompt" \
  --argjson max_tokens "$max_tokens" \
  --argjson temperature "$temperature" \
  '{
    model: $model,
    messages: [{ role: "user", content: $prompt }],
    max_tokens: $max_tokens,
    temperature: $temperature,
    stream: false
  }')"

response="$(curl -fsS "$url" \
  -H 'Content-Type: application/json' \
  --data-binary "$payload")"

text="$(jq -r '.choices[0].message.content // .choices[0].text // empty' <<<"$response")"
if [[ -z "${text//[[:space:]]/}" ]]; then
  echo "Gemma 4 shim received an empty completion: $response" >&2
  exit 1
fi

jq -nc --arg text "$text" '{type:"message",role:"assistant",content:$text}'
jq -nc --arg text "$text" '{type:"result",status:"success",response:$text}'
SHIM
chmod 700 "$shim"

export AXON_HEADLESS_GEMINI_CMD="$shim"
export AXON_HEADLESS_GEMINI_MODEL="$llama_model"
export AXON_LLM_COMPLETION_CONCURRENCY="${AXON_LLM_COMPLETION_CONCURRENCY:-1}"
export AXON_LLM_COMPLETION_TIMEOUT_SECS="${AXON_LLM_COMPLETION_TIMEOUT_SECS:-300}"

# Keep the total prompt inside the current 12k-token llama.cpp server context.
# Axon's lower clamp is 20k chars, so this is the smallest supported context
# budget without changing Rust config parsing.
export AXON_ASK_MAX_CONTEXT_CHARS="${AXON_ASK_MAX_CONTEXT_CHARS:-20000}"
export AXON_ASK_CHUNK_LIMIT="${AXON_ASK_CHUNK_LIMIT:-6}"
export AXON_ASK_FULL_DOCS="${AXON_ASK_FULL_DOCS:-1}"
export AXON_ASK_DOC_CHUNK_LIMIT="${AXON_ASK_DOC_CHUNK_LIMIT:-24}"
export AXON_ASK_BACKFILL_CHUNKS="${AXON_ASK_BACKFILL_CHUNKS:-1}"
export AXON_ASK_CANDIDATE_LIMIT="${AXON_ASK_CANDIDATE_LIMIT:-80}"
export AXON_ASK_HYBRID_CANDIDATES="${AXON_ASK_HYBRID_CANDIDATES:-80}"
export AXON_ASK_DOC_FETCH_CONCURRENCY="${AXON_ASK_DOC_FETCH_CONCURRENCY:-1}"

echo "Gemma 4 ask smoke" >&2
echo "  model: $llama_model" >&2
echo "  url: ${llama_url%/chat/completions}" >&2
echo "  max_context_chars: $AXON_ASK_MAX_CONTEXT_CHARS" >&2
echo "  chunk_limit: $AXON_ASK_CHUNK_LIMIT full_docs: $AXON_ASK_FULL_DOCS doc_chunk_limit: $AXON_ASK_DOC_CHUNK_LIMIT" >&2

"$axon_bin" ask "$question" --json "${extra_args[@]}"
