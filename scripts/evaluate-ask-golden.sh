#!/usr/bin/env bash
# Run docs/eval/ask-golden.jsonl through `axon evaluate --json`.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
GOLDEN="${GOLDEN:-$REPO_ROOT/docs/eval/ask-golden.jsonl}"
OUT="${OUT:-$REPO_ROOT/docs/perf/quality-parity-$(date -u +%Y-%m-%d).jsonl}"
AXON_BIN="${AXON_BIN:-$REPO_ROOT/target/release/axon}"

if [[ ! -x "$AXON_BIN" ]]; then
  if [[ -x "$REPO_ROOT/target/debug/axon" ]]; then
    AXON_BIN="$REPO_ROOT/target/debug/axon"
  elif command -v axon >/dev/null 2>&1; then
    AXON_BIN="$(command -v axon)"
  else
    echo "Error: axon binary not found. Set AXON_BIN or build axon." >&2
    exit 2
  fi
fi

command -v jq >/dev/null 2>&1 || { echo "Error: jq not found in PATH." >&2; exit 2; }
[[ -f "$GOLDEN" ]] || { echo "Error: golden set not found: $GOLDEN" >&2; exit 2; }

blank_count=$(jq -Rr 'fromjson? | select((.id // "") == "" or (.question // "") == "") | .id // "<blank>"' "$GOLDEN" | wc -l | tr -d '[:space:]')
if [[ "$blank_count" -gt 0 ]]; then
  echo "Error: golden set contains blank id or question." >&2
  exit 2
fi

dupes=$(jq -Rr 'fromjson? | .id' "$GOLDEN" | sort | uniq -d)
if [[ -n "$dupes" ]]; then
  echo "Error: golden set contains duplicate ids:" >&2
  echo "$dupes" >&2
  exit 2
fi

mkdir -p "$(dirname "$OUT")"
: > "$OUT"

JUDGE_MODEL="${JUDGE_MODEL:-${AXON_SYNTHESIS_HEADLESS_GEMINI_MODEL:-}}"

while IFS= read -r row; do
  id=$(jq -r '.id' <<<"$row")
  question=$(jq -r '.question' <<<"$row")
  backend="gemini-headless"
  agent="gemini"
      stdout_file="$(mktemp)"
      stderr_file="$(mktemp)"
      if "$AXON_BIN" evaluate "$question" --json >"$stdout_file" 2>"$stderr_file"; then
        output="$(cat "$stdout_file")"
        if scores=$(jq -c '.scores // {status:"parse_failed"}' <<<"$output"); then
          status=$(jq -r '.status // "parse_failed"' <<<"$scores")
        else
          scores='{"status":"parse_failed"}'
          status="parse_failed"
        fi
        jq -cn \
          --arg id "$id" \
          --arg backend "$backend" \
          --arg agent "$agent" \
          --arg status "$status" \
          --arg judge_backend "$backend" \
          --arg judge_agent "$agent" \
          --arg judge_model "$JUDGE_MODEL" \
          --argjson scores "$scores" \
          '{id:$id,backend:$backend,agent:$agent,status:$status,judge:{backend:$judge_backend,agent:$judge_agent,model:$judge_model},scores:$scores}' >> "$OUT"
      else
        error_tail="$(tail -c 512 "$stderr_file")"
        jq -cn \
          --arg id "$id" \
          --arg backend "$backend" \
          --arg agent "$agent" \
          --arg status "unavailable_or_failed" \
          --arg error "$error_tail" \
          --arg judge_backend "$backend" \
          --arg judge_agent "$agent" \
          --arg judge_model "$JUDGE_MODEL" \
          '{id:$id,backend:$backend,agent:$agent,status:$status,error:$error,judge:{backend:$judge_backend,agent:$judge_agent,model:$judge_model}}' >> "$OUT"
      fi
      rm -f "$stdout_file" "$stderr_file"
done < "$GOLDEN"

echo "$OUT"
