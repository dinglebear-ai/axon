#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage:
  scripts/test-ask-gemma4.sh "question" [-- extra axon ask args...]

Runs the normal `axon ask` retrieval/context path and routes synthesis to the
local llama.cpp OpenAI-compatible Gemma 4 server.

Defaults target the documented Gemma 4 26B-A4B 128k llama.cpp fit profile.
Override any of these env vars to experiment:

  LLAMA_CPP_URL=http://127.0.0.1:8080/v1
  LLAMA_CPP_MODEL=ggml-org/gemma-4-26B-A4B-it-GGUF:Q4_K_M

  AXON_ASK_MAX_CONTEXT_CHARS=128000
  AXON_ASK_CHUNK_LIMIT=20
  AXON_ASK_FULL_DOCS=1
  AXON_ASK_DOC_CHUNK_LIMIT=24
  AXON_ASK_BACKFILL_CHUNKS=1
  AXON_ASK_CANDIDATE_LIMIT=120
  AXON_ASK_HYBRID_CANDIDATES=100
  AXON_LLM_COMPLETION_CONCURRENCY=1

Examples:
  scripts/test-ask-gemma4.sh "How does Axon build ask context?"
  AXON_ASK_MAX_CONTEXT_CHARS=96000 scripts/test-ask-gemma4.sh "Explain hybrid search" -- --diagnostics
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

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
axon_bin="${AXON_BIN:-$repo_root/scripts/axon}"

if [[ ! -x "$axon_bin" ]]; then
  echo "axon runner is not executable: $axon_bin" >&2
  echo "set AXON_BIN=/path/to/axon or run from the repository checkout" >&2
  exit 1
fi

llama_url="${LLAMA_CPP_URL:-http://127.0.0.1:8080/v1}"
llama_model="${LLAMA_CPP_MODEL:-ggml-org/gemma-4-26B-A4B-it-GGUF:Q4_K_M}"

llama_base_url="${llama_url%/chat/completions}"
llama_base_url="${llama_base_url%/}"

if ! curl -fsS --max-time 3 "$llama_base_url/models" >/dev/null 2>&1; then
  echo "llama.cpp server is not reachable at $llama_base_url" >&2
  echo "start it with: docker compose -f docker-compose.llama.yaml up -d" >&2
  exit 1
fi

export AXON_LLM_BACKEND="openai-compat"
export AXON_OPENAI_BASE_URL="$llama_base_url"
export AXON_SYNTHESIS_OPENAI_MODEL="$llama_model"
export AXON_OPENAI_API_KEY="${AXON_OPENAI_API_KEY:-}"
export AXON_LLM_COMPLETION_CONCURRENCY="${AXON_LLM_COMPLETION_CONCURRENCY:-1}"
export AXON_LLM_COMPLETION_TIMEOUT_SECS="${AXON_LLM_COMPLETION_TIMEOUT_SECS:-300}"

# Match the current Gemma 4 26B-A4B llama.cpp 128k fit profile.
export AXON_ASK_MAX_CONTEXT_CHARS="${AXON_ASK_MAX_CONTEXT_CHARS:-128000}"
export AXON_ASK_CHUNK_LIMIT="${AXON_ASK_CHUNK_LIMIT:-20}"
export AXON_ASK_FULL_DOCS="${AXON_ASK_FULL_DOCS:-1}"
export AXON_ASK_DOC_CHUNK_LIMIT="${AXON_ASK_DOC_CHUNK_LIMIT:-24}"
export AXON_ASK_BACKFILL_CHUNKS="${AXON_ASK_BACKFILL_CHUNKS:-1}"
export AXON_ASK_CANDIDATE_LIMIT="${AXON_ASK_CANDIDATE_LIMIT:-120}"
export AXON_ASK_HYBRID_CANDIDATES="${AXON_ASK_HYBRID_CANDIDATES:-100}"
export AXON_ASK_DOC_FETCH_CONCURRENCY="${AXON_ASK_DOC_FETCH_CONCURRENCY:-1}"

echo "Gemma 4 ask smoke" >&2
echo "  model: $llama_model" >&2
echo "  url: $llama_base_url" >&2
echo "  max_context_chars: $AXON_ASK_MAX_CONTEXT_CHARS" >&2
echo "  chunk_limit: $AXON_ASK_CHUNK_LIMIT full_docs: $AXON_ASK_FULL_DOCS doc_chunk_limit: $AXON_ASK_DOC_CHUNK_LIMIT" >&2

"$axon_bin" ask "$question" --json "${extra_args[@]}"
