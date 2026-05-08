#!/usr/bin/env bash
# bench-ask.sh -- End-to-end perf harness for `axon ask` on Gemini headless.

set -euo pipefail

RUNS=30
MODE="cold"
PROMPTS_FILE=""
OUT=""

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

usage() {
    cat <<'EOF'
Usage: bench-ask.sh [options]

Options:
  --runs N              Sample count per prompt; min 30 (default: 30)
  --mode {cold|warm}    cold uses --no-server-url; warm requires AXON_ASK_SERVER_URL (default: cold)
  --prompts <file>      Prompt file (id|text per line; default: scripts/bench-prompts.txt)
  --out <path>          Output JSON path (default: docs/perf/results-<timestamp>-<sha>.json)
  -h, --help            Show this help
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --runs) RUNS="$2"; shift 2 ;;
        --mode) MODE="$2"; shift 2 ;;
        --prompts) PROMPTS_FILE="$2"; shift 2 ;;
        --out) OUT="$2"; shift 2 ;;
        -h|--help) usage; exit 0 ;;
        *) echo "Unknown option: $1" >&2; usage; exit 2 ;;
    esac
done

[[ "$RUNS" =~ ^[0-9]+$ && "$RUNS" -ge 30 ]] || {
    echo "Error: --runs must be an integer >= 30." >&2
    exit 2
}

case "$MODE" in
    cold|warm) ;;
    *) echo "Error: --mode must be cold or warm." >&2; exit 2 ;;
esac

command -v jq >/dev/null 2>&1 || { echo "Error: jq not found in PATH." >&2; exit 2; }

if [[ -z "$PROMPTS_FILE" ]]; then
    PROMPTS_FILE="$SCRIPT_DIR/bench-prompts.txt"
fi
[[ -f "$PROMPTS_FILE" ]] || { echo "Error: prompts file not found: $PROMPTS_FILE" >&2; exit 2; }

AXON_BIN="${AXON_BIN:-}"
if [[ -z "$AXON_BIN" ]]; then
    if [[ -x "$REPO_ROOT/target/release/axon" ]]; then
        AXON_BIN="$REPO_ROOT/target/release/axon"
    elif [[ -x "$REPO_ROOT/target/debug/axon" ]]; then
        AXON_BIN="$REPO_ROOT/target/debug/axon"
    elif command -v axon >/dev/null 2>&1; then
        AXON_BIN="$(command -v axon)"
    else
        echo "Error: axon binary not found. Build axon or set AXON_BIN." >&2
        exit 2
    fi
fi

if [[ -z "$OUT" ]]; then
    TS="$(date +%Y-%m-%d-%H%M%S)"
    SHA="$(git -C "$REPO_ROOT" rev-parse --short HEAD 2>/dev/null || echo nogit)"
    mkdir -p "$REPO_ROOT/docs/perf"
    OUT="$REPO_ROOT/docs/perf/results-${TS}-${SHA}.json"
fi

run_one_ask() {
    local prompt_text="$1"
    local extra_args=()

    if [[ "$MODE" == "warm" ]]; then
        [[ -n "${AXON_ASK_SERVER_URL:-}" ]] || {
            echo "Error: warm mode requires AXON_ASK_SERVER_URL." >&2
            return 2
        }
        extra_args+=(--server-url "$AXON_ASK_SERVER_URL")
    else
        extra_args+=(--no-server-url)
    fi

    local out
    if ! out=$("$AXON_BIN" ask "$prompt_text" --json "${extra_args[@]}" 2>/dev/null); then
        echo "{}"
        return 0
    fi

    jq -c '.timing_ms // {}' <<<"$out" 2>/dev/null || echo "{}"
}

echo "bench-ask: backend=gemini-headless runs=$RUNS mode=$MODE prompts=$PROMPTS_FILE" >&2
echo "bench-ask: out=$OUT" >&2
echo "bench-ask: axon=$AXON_BIN" >&2

results="[]"
while IFS='|' read -r prompt_id prompt_text || [[ -n "$prompt_id" ]]; do
    [[ -z "$prompt_id" || "$prompt_id" =~ ^# ]] && continue
    timings="[]"
    kept=0
    for ((i=0; i<RUNS; i++)); do
        t_json=$(run_one_ask "$prompt_text")
        [[ "$t_json" == "{}" ]] && continue
        timings=$(jq -c --argjson t "$t_json" '. + [$t]' <<<"$timings")
        kept=$((kept + 1))
    done

    entry=$(jq -n \
        --arg prompt_id "$prompt_id" \
        --arg mode "$MODE" \
        --arg status "$(if [[ "$kept" -gt 0 ]]; then echo measured; else echo unavailable_or_failed; fi)" \
        --argjson runs_requested "$RUNS" \
        --argjson samples "$kept" \
        --argjson timings "$timings" \
        '{backend:"gemini-headless", prompt_id:$prompt_id, mode:$mode, status:$status, runs_requested:$runs_requested, samples:$samples, timings:$timings}')
    results=$(jq --argjson r "$entry" '. + [$r]' <<<"$results")
done < "$PROMPTS_FILE"

jq -n \
    --arg git_sha "$(git -C "$REPO_ROOT" rev-parse HEAD 2>/dev/null || echo unknown)" \
    --arg git_branch "$(git -C "$REPO_ROOT" rev-parse --abbrev-ref HEAD 2>/dev/null || echo unknown)" \
    --arg timestamp "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
    --argjson runs "$RUNS" \
    --argjson results "$results" \
    '{
        schema: "axon-bench-ask/v2",
        backend: "gemini-headless",
        timestamp_utc: $timestamp,
        git_sha: $git_sha,
        git_branch: $git_branch,
        runs_per_prompt: $runs,
        results: $results
    }' > "$OUT"

echo "wrote $OUT" >&2
echo "$OUT"
