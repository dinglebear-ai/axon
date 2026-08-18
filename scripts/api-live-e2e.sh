#!/usr/bin/env bash
set -euo pipefail

base_url="${AXON_API_URL:-https://axon.dinglebear.ai}"
env_file="${AXON_ENV_FILE:-$HOME/.axon/.env}"
token="${AXON_HTTP_TOKEN:-$(sed -n 's/^AXON_HTTP_TOKEN=//p' "$env_file" | tail -1)}"
run_dir="${AXON_API_EVIDENCE_DIR:-$HOME/.agents/docs/sessions/axon-api-live/run_$(date +%Y%m%d_%H%M%S)}"
mutations=false
[[ "${1:-}" == "--mutations" ]] && mutations=true

[[ -n "$token" ]] || { echo "AXON_HTTP_TOKEN is required" >&2; exit 2; }
mkdir -p "$run_dir"

request() {
  local name=$1 method=$2 route=$3 body=${4-} expected=${5:-200}
  local output="$run_dir/$name.json" code
  if [[ -n "$body" ]]; then
    code=$(curl -sS -o "$output" -w '%{http_code}' --max-time 120 \
      -X "$method" -H "Authorization: Bearer $token" -H 'Content-Type: application/json' \
      --data "$body" "$base_url$route")
  else
    code=$(curl -sS -o "$output" -w '%{http_code}' --max-time 120 \
      -X "$method" -H "Authorization: Bearer $token" "$base_url$route")
  fi
  printf '%s\t%s\t%s\n' "$code" "$method" "$route" | tee -a "$run_dir/results.tsv"
  [[ "$code" == "$expected" ]]
  jq -e 'type == "object"' "$output" >/dev/null
}

unauth=$(curl -sS -o "$run_dir/unauthorized.json" -w '%{http_code}' --max-time 20 "$base_url/v1/status")
[[ "$unauth" == 401 ]]
printf '%s\tGET\t/v1/status (unauthenticated)\n' "$unauth" >>"$run_dir/results.tsv"

for route in status doctor providers capabilities collections sources jobs artifacts uploads graph/kinds memories; do
  request "get_${route//\//_}" GET "/v1/$route"
done

request query POST /v1/query '{"query":"watch scheduler lease handling","limit":2}'
request search POST /v1/search '{"query":"Axon source ingestion","limit":2}'
request ask POST /v1/ask '{"query":"Summarize Axon in one sentence","ask_chunk_limit":2}'

curl -sS -N --max-time 120 -H "Authorization: Bearer $token" -H 'Content-Type: application/json' \
  --data '{"query":"Summarize Axon in one sentence","ask_chunk_limit":2}' \
  "$base_url/v1/ask/stream" >"$run_dir/ask_stream.txt"
rg -q '^data: .*"kind":"final"' "$run_dir/ask_stream.txt"

if $mutations; then
  request source POST /v1/sources '{"source":"https://example.com","scope":"page"}'
  jq -e '.status == "completed" and .ledger.status == "completed"' "$run_dir/source.json" >/dev/null
fi

unset token
echo "$run_dir"
