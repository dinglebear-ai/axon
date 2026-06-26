#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BASE_CONFIG_PATH="${MCPORTER_CONFIG:-$REPO_ROOT/config/mcporter.json}"
SERVER="${MCP_SERVER:-axon}"
SELECTOR="${SERVER}.axon"

BASE_OUTDIR="${MCPORTER_OUTDIR:-$REPO_ROOT/.cache/mcporter-test}"
SUMMARY="$BASE_OUTDIR/summary.txt"
mkdir -p "$BASE_OUTDIR"
: >"$SUMMARY"

REAL_PAGE_URL="${REAL_PAGE_URL:-https://www.rust-lang.org/learn/get-started}"
pass=0
fail=0

OUTDIR=""
CONFIG_PATH=""
MCPORTER=()

EXPECTED_ROUTES="$(cat <<'EOF'
ask
brand
code_search
crawl:cancel
crawl:cleanup
crawl:clear
crawl:list
crawl:recover
crawl:start
crawl:status
diff
doctor
domains
elicit_demo
endpoints
embed:cancel
embed:cleanup
embed:clear
embed:list
embed:recover
embed:start
embed:status
evaluate
extract:cancel
extract:cleanup
extract:clear
extract:list
extract:recover
extract:start
extract:status
help
ingest:cancel
ingest:cleanup
ingest:clear
ingest:list
ingest:recover
ingest:start
ingest:status
map
memory:remember
memory:list
memory:search
memory:show
memory:link
memory:supersede
memory:context
query
research
retrieve
scrape
screenshot
search
sources
stats
status
summarize
suggest
vertical_scrape:capabilities
vertical_scrape:list
EOF
)"

DIRECT_ACTIONS_JSON='["ask","brand","code_search","diff","doctor","domains","elicit_demo","endpoints","evaluate","help","map","query","research","retrieve","scrape","screenshot","search","sources","stats","status","suggest","summarize"]'
EXPECTED_TOP_LEVEL_ACTIONS="$(cat <<'EOF'
ask
brand
code_search
crawl
diff
doctor
domains
elicit_demo
endpoints
embed
evaluate
extract
help
ingest
map
memory
query
research
retrieve
scrape
screenshot
search
sources
stats
status
summarize
suggest
vertical_scrape
EOF
)"

if ! command -v mcporter >/dev/null 2>&1; then
  echo "FAIL: mcporter not found in PATH" >&2
  exit 2
fi

if ! command -v jq >/dev/null 2>&1; then
  echo "FAIL: jq not found in PATH" >&2
  exit 2
fi

URL_MODE=0
if jq -e --arg server "$SERVER" '.mcpServers[$server].url? | type == "string"' "$BASE_CONFIG_PATH" >/dev/null; then
  URL_MODE=1
fi

trim() {
  local value="$1"
  value="${value#"${value%%[![:space:]]*}"}"
  value="${value%"${value##*[![:space:]]}"}"
  printf '%s' "$value"
}

record_pass() {
  local name="$1"
  echo "PASS $name" | tee -a "$SUMMARY"
  pass=$((pass + 1))
}

record_fail() {
  local name="$1"
  local logfile="$2"
  echo "FAIL $name (see $logfile)" | tee -a "$SUMMARY"
  fail=$((fail + 1))
}

run_case() {
  local name="$1"
  shift
  local logfile="$OUTDIR/${name}.log"
  if "$@" >"$logfile" 2>&1; then
    record_pass "$name"
  else
    record_fail "$name" "$logfile"
  fi
}

run_json_case() {
  local name="$1"
  local filter="$2"
  shift 2
  local logfile="$OUTDIR/${name}.log"
  if "$@" >"$logfile" 2>&1 && json_payload "$logfile" | jq -e "$filter" >/dev/null; then
    record_pass "$name"
  else
    record_fail "$name" "$logfile"
  fi
}

run_error_case() {
  local name="$1"
  local expected="$2"
  shift 2
  local logfile="$OUTDIR/${name}.log"
  "$@" >"$logfile" 2>&1 || true
  if json_payload "$logfile" | jq -er --arg expected "$expected" '.error | type == "string" and contains($expected)' >/dev/null; then
    record_pass "$name"
  else
    record_fail "$name" "$logfile"
  fi
}

call_tool() {
  "${MCPORTER[@]}" call "$SELECTOR" "$@" --output json
}

call_tool_json() {
  local payload="$1"
  "${MCPORTER[@]}" call "$SELECTOR" --args "$payload" --output json
}

call_tool_with_timeout() {
  local timeout_ms="$1"
  shift
  MCPORTER_CALL_TIMEOUT="$timeout_ms" "${MCPORTER[@]}" call "$SELECTOR" "$@" --output json
}

json_payload() {
  local file="$1"
  sed -n '/^[[:space:]]*{/,$p' "$file"
}

assert_ok() {
  local file="$1"
  local filter="$2"
  json_payload "$file" | jq -e "$filter" >/dev/null
}

normalize_discovered_routes() {
  local help_file="$1"
  json_payload "$help_file" | jq -r --argjson direct "$DIRECT_ACTIONS_JSON" '
    .data.inline.actions
    | to_entries[]
    | .key as $action
    | if (($direct | index($action)) != null) or (.value | length == 0) then
        $action
      else
        .value[] | "\($action):\(.)"
      end
  ' | sort -u
}

normalize_help_top_actions() {
  local help_file="$1"
  json_payload "$help_file" | jq -r '
    .data.inline.actions
    | keys[]
  ' | sort -u
}

normalize_description_actions() {
  local schema_file="$1"
  local description
  description="$(json_payload "$schema_file" | jq -r '.tools[] | select(.name == "axon") | .description')"
  if [[ -z "$description" || "$description" == "null" ]]; then
    return 1
  fi

  printf '%s\n' "$description" \
    | sed -n 's/.*Actions: //p' \
    | tr ',' '\n' \
    | while IFS= read -r line; do
        line="$(trim "$line")"
        [[ -n "$line" ]] && printf '%s\n' "$line"
      done \
    | sed 's/[.]$//' \
    | sort -u
}

assert_sorted_equals() {
  local expected="$1"
  local actual="$2"
  diff -u <(printf '%s\n' "$expected" | sed '/^$/d' | sort -u) <(printf '%s\n' "$actual" | sed '/^$/d' | sort -u)
}

extract_json_field() {
  local file="$1"
  local filter="$2"
  json_payload "$file" | jq -er "$filter"
}

build_suite_config() {
  local mode="$1"
  local runtime_root="$BASE_OUTDIR/runtime-$mode"
  local suite_config="$BASE_OUTDIR/mcporter-$mode.json"
  jq \
    --arg server "$SERVER" \
    --arg repo_root "$REPO_ROOT" \
    --arg axon_home "$HOME/.axon" \
    --arg data_dir "$runtime_root" \
    --arg log_file "$runtime_root/logs/axon.log" \
    --arg sqlite_path "$runtime_root/mcporter-jobs.db" \
    '.mcpServers[$server].args = ["-lc", ("exec \"" + $repo_root + "/scripts/mcporter-axon\"")]
    | .mcpServers[$server].env = ((.mcpServers[$server].env // {}) + {
        AXON_REPO_ROOT: $repo_root,
        AXON_HOME: $axon_home,
        AXON_DATA_DIR: $data_dir,
        AXON_CODE_SEARCH_ALLOWED_ROOTS: $repo_root,
        AXON_MCP_EMBED_ALLOWED_ROOTS: $repo_root,
        AXON_LOG_FILE: $log_file,
        AXON_SQLITE_PATH: $sqlite_path
      })
    ' \
    "$BASE_CONFIG_PATH" >"$suite_config"
  printf '%s\n' "$suite_config"
}

run_suite() {
  local mode="$1"
  local prefix="$mode"
  local expected_routes="$EXPECTED_ROUTES"
  local expected_top_level_actions="$EXPECTED_TOP_LEVEL_ACTIONS"

  CONFIG_PATH="$(build_suite_config "$mode")"
  OUTDIR="$BASE_OUTDIR/$mode"
  mkdir -p "$OUTDIR"
  MCPORTER=(mcporter --config "$CONFIG_PATH")

  echo "== Suite: $mode ==" | tee -a "$SUMMARY"
  echo "Config: $CONFIG_PATH" | tee -a "$SUMMARY"
  echo "Server: $SERVER" | tee -a "$SUMMARY"

  run_case "${prefix}_list_schema" "${MCPORTER[@]}" list "$SERVER" --schema --json
  run_case "${prefix}_action_help" call_tool action:help response_mode:inline

  local help_file="$OUTDIR/${prefix}_action_help.log"
  local schema_file="$OUTDIR/${prefix}_list_schema.log"

  echo "== $mode parity checks ==" | tee -a "$SUMMARY"
  run_case "${prefix}_schema_has_tool" assert_ok "$schema_file" '.status == "ok" and any(.tools[]; .name == "axon")'
  run_case "${prefix}_help_has_resource" assert_ok "$help_file" '.ok == true and (.data.inline.resources | index("axon://schema/mcp-tool")) != null'
  run_case "${prefix}_help_routes_match_expected" assert_sorted_equals "$expected_routes" "$(normalize_discovered_routes "$help_file")"
  run_case "${prefix}_tool_description_actions_match_expected" assert_sorted_equals "$expected_top_level_actions" "$(normalize_description_actions "$schema_file")"
  run_case "${prefix}_help_top_actions_match_description" assert_sorted_equals "$(normalize_description_actions "$schema_file")" "$(normalize_help_top_actions "$help_file")"

  echo "== $mode direct actions ==" | tee -a "$SUMMARY"
  run_json_case "${prefix}_status" '.ok == true and .action == "status" and .subaction == "status" and (((.data.data | type) == "object") or (.data.artifact.path | type == "string")) and (.data.response_mode | type == "string")' call_tool action:status
  run_json_case "${prefix}_help" '.ok == true and .action == "help" and .subaction == "help" and (.data.data.actions | type == "object")' call_tool action:help
  run_json_case "${prefix}_doctor" '.ok == true and .action == "doctor" and .subaction == "doctor" and .data.data.all_ok == true' call_tool action:doctor
  run_json_case "${prefix}_stats" '.ok == true and .action == "stats" and .subaction == "stats" and (.data.data.collection | type == "string") and (.data.data.collection | length) > 0 and (.data.data.counts | type == "object")' call_tool action:stats
  run_json_case "${prefix}_domains" '.ok == true and .action == "domains" and .subaction == "domains" and (.data.data.domains | type == "array") and .data.data.limit == 5' call_tool action:domains limit:5 offset:0
  run_json_case "${prefix}_sources" '.ok == true and .action == "sources" and .subaction == "sources" and (.data.data.urls | type == "array") and .data.data.limit == 5' call_tool action:sources limit:5 offset:0
  run_json_case "${prefix}_query" '.ok == true and .action == "query" and .subaction == "query" and (.data.data.results | type == "array") and .data.data.query == "rust mcp sdk"' call_tool action:query query:'rust mcp sdk' limit:3 offset:0
  if [[ "$URL_MODE" != "1" ]]; then
    run_json_case "${prefix}_code_search_no_freshness" '.ok == true and .action == "code_search" and .subaction == "code_search" and .data.data.content_trust == "untrusted_local_code" and (.data.data.results | type == "array") and .data.data.freshness.status == "stale"' call_tool action:code_search query:'freshness lease' cwd:"$REPO_ROOT" no_freshness:true limit:3 offset:0
  fi
  run_error_case "${prefix}_code_search_missing_cwd" "cwd is required for code_search MCP requests" call_tool action:code_search query:'freshness lease' no_freshness:true limit:3 offset:0
  run_json_case "${prefix}_map" ".ok == true and .action == \"map\" and .subaction == \"map\" and (.data.data.urls | type == \"array\") and .data.data.url == \"$REAL_PAGE_URL\"" call_tool action:map url:"$REAL_PAGE_URL" limit:5 offset:0
  run_json_case "${prefix}_scrape" ".ok == true and .action == \"scrape\" and .subaction == \"scrape\" and (((.data.data.url == \"$REAL_PAGE_URL\") and (.data.data.markdown | type == \"string\")) or ((.data.shape.url == \"$REAL_PAGE_URL\") and (.data.shape.markdown | type == \"string\") and (.data.artifact.path | type == \"string\")) or ((.data.inline.url == \"$REAL_PAGE_URL\") and (.data.inline.content | type == \"string\") and (.data.artifact.path | type == \"string\")))" call_tool action:scrape url:"$REAL_PAGE_URL"
  run_json_case "${prefix}_retrieve" ".ok == true and .action == \"retrieve\" and .subaction == \"retrieve\" and (((.data.data.url == \"$REAL_PAGE_URL\") and (.data.data.content | type == \"string\")) or ((.data.shape.url == \"$REAL_PAGE_URL\") and (.data.shape.content | type == \"string\") and (.data.artifact.path | type == \"string\")) or ((.data.inline.requested_url == \"$REAL_PAGE_URL\") and (.data.inline.content | type == \"string\") and (.data.artifact.path | type == \"string\")))" call_tool action:retrieve url:"$REAL_PAGE_URL"
  if [[ "$URL_MODE" == "1" ]]; then
    run_error_case "${prefix}_search_unavailable" "search requires AXON_SEARXNG_URL or TAVILY_API_KEY" call_tool action:search query:'rust programming language' limit:3 offset:0
    run_error_case "${prefix}_research_unavailable" "research requires AXON_SEARXNG_URL or TAVILY_API_KEY" call_tool action:research query:'rust async best practices' limit:3 offset:0
    run_error_case "${prefix}_ask_unavailable" "ask 'What is this repository?' failed" call_tool action:ask query:'What is this repository?'
    run_error_case "${prefix}_screenshot_unavailable" "screenshot failed" call_tool_with_timeout 180000 action:screenshot url:"$REAL_PAGE_URL"
  else
    run_json_case "${prefix}_search" '.ok == true and .action == "search" and .subaction == "search" and (.data.data.results | type == "array") and .data.data.query == "rust programming language"' call_tool action:search query:'rust programming language' limit:3 offset:0
    run_json_case "${prefix}_research" '.ok == true and .action == "research" and .subaction == "research" and (((.data.data.search_results | type) == "array" and (.data.data.summary | type) == "string") or (.data.response_mode == "path" and ((.data.shape.search_results | type) == "string" or (.data.shape.search_results | type) == "object") and (.data.shape.summary | type) == "string"))' call_tool action:research query:'rust async best practices' limit:3 offset:0
    run_json_case "${prefix}_ask" '.ok == true and .action == "ask" and .subaction == "ask" and (((.data.data.answer | type) == "string" and .data.data.query == "What is this repository?") or (.data.shape.query == "What is this repository?" and .data.shape.explain.llm_skipped == true))' call_tool action:ask query:'What is this repository?' explain:true response_mode:inline
    run_json_case "${prefix}_screenshot" '.ok == true and .action == "screenshot" and ((.data.data.path | type == "string") or (.data.path | type == "string"))' call_tool_with_timeout 180000 action:screenshot url:"$REAL_PAGE_URL"
  fi
  run_json_case "${prefix}_elicit_demo" '.ok == true and .action == "elicit_demo" and (.data.action | type == "string")' call_tool action:elicit_demo
  run_json_case "${prefix}_memory_remember" '.ok == true and .action == "memory" and (.data.memory.id | type == "string")' call_tool_json '{"action":"memory","subaction":"remember","body":"mcporter smoke memory content lives in Qdrant","project":"axon"}'
  local memory_id
  memory_id="$(extract_json_field "$OUTDIR/${prefix}_memory_remember.log" '.data.memory.id')"
  run_json_case "${prefix}_memory_replacement" '.ok == true and .action == "memory" and (.data.memory.id | type == "string")' call_tool_json '{"action":"memory","subaction":"remember","body":"mcporter smoke replacement memory content lives in Qdrant","project":"axon"}'
  local replacement_memory_id
  replacement_memory_id="$(extract_json_field "$OUTDIR/${prefix}_memory_replacement.log" '.data.memory.id')"
  run_json_case "${prefix}_memory_show" '.ok == true and .action == "memory" and ((.data.memory == null) or (.data.memory.id | type == "string"))' call_tool action:memory subaction:show id:"$memory_id"
  run_json_case "${prefix}_memory_link" '.ok == true and .action == "memory" and .subaction == "link" and (.data.edge.id | type == "string") and .data.edge.edge_type == "relates_to"' call_tool action:memory subaction:link source_id:"$replacement_memory_id" target_id:"$memory_id"
  run_json_case "${prefix}_memory_supersede" '.ok == true and .action == "memory" and .subaction == "supersede" and (.data.edge.id | type == "string") and .data.edge.edge_type == "supersedes"' call_tool action:memory subaction:supersede source_id:"$replacement_memory_id" target_id:"$memory_id"
  run_json_case "${prefix}_memory_list" '.ok == true and .action == "memory" and .subaction == "list" and (.data.memories | type == "array")' call_tool action:memory subaction:list project:axon status:active limit:3
  run_json_case "${prefix}_memory_search" '.ok == true and .action == "memory" and (.data.memories | type == "array")' call_tool action:memory subaction:search query:'mcporter smoke memory' project:axon limit:3
  run_json_case "${prefix}_memory_context" '.ok == true and .action == "memory" and .subaction == "context" and (.data.context.context | type == "string") and (.data.context.context | contains("trust=\"evidence_only\""))' call_tool action:memory subaction:context query:'mcporter smoke memory' project:axon limit:3 token_budget:1000

  echo "== $mode path-mode response ==" | tee -a "$SUMMARY"
  # Artifact-first response mode still persists large payloads to disk and returns
  # a path; the in-process server makes that path directly readable. (The standalone
  # `artifacts` MCP action was removed in 5.0.0.)
  run_json_case "${prefix}_help_path" '.ok == true and .action == "help" and .subaction == "help" and .data.response_mode == "path" and (.data.artifact.path | type == "string")' call_tool action:help response_mode:path

  echo "== $mode lifecycle start/status/cancel/list ==" | tee -a "$SUMMARY"
  run_json_case "${prefix}_crawl_start" '.ok == true and .action == "crawl" and .subaction == "start" and (.data.job_ids | type == "array") and ((.data.job_ids | length) > 0)' call_tool_json "{\"action\":\"crawl\",\"subaction\":\"start\",\"urls\":[\"$REAL_PAGE_URL\"],\"max_pages\":1}"
  local crawl_job_id
  crawl_job_id="$(extract_json_field "$OUTDIR/${prefix}_crawl_start.log" '.data.job_ids[0]')"
  run_json_case "${prefix}_crawl_status" '.ok == true and .action == "crawl" and .subaction == "status" and (((.data.job | type) == "object") or (.data.job == null))' call_tool action:crawl subaction:status job_id:"$crawl_job_id"
  run_json_case "${prefix}_crawl_cancel" '.ok == true and .action == "crawl" and .subaction == "cancel" and (.data.job_id | type == "string") and (.data.canceled | type == "boolean")' call_tool action:crawl subaction:cancel job_id:"$crawl_job_id"
  run_json_case "${prefix}_crawl_list" '.ok == true and .action == "crawl" and .subaction == "list" and (((.data.data.jobs | type) == "array" and .data.data.limit == 5) or ((.data.shape.jobs | type) == "object" and .data.shape.limit == 5))' call_tool action:crawl subaction:list limit:5 offset:0

  run_json_case "${prefix}_extract_start" '.ok == true and .action == "extract" and .subaction == "start" and (.data.job_id | type == "string")' call_tool_json "{\"action\":\"extract\",\"subaction\":\"start\",\"urls\":[\"$REAL_PAGE_URL\"],\"prompt\":\"Extract the page title.\",\"max_pages\":1}"
  local extract_job_id
  extract_job_id="$(extract_json_field "$OUTDIR/${prefix}_extract_start.log" '.data.job_id')"
  run_json_case "${prefix}_extract_status" '.ok == true and .action == "extract" and .subaction == "status" and .data.response_mode != null and (((.data.data.job | type) == "object") or (.data.data.job == null))' call_tool action:extract subaction:status job_id:"$extract_job_id"
  run_json_case "${prefix}_extract_cancel" '.ok == true and .action == "extract" and .subaction == "cancel" and (.data.job_id | type == "string") and (.data.canceled | type == "boolean")' call_tool action:extract subaction:cancel job_id:"$extract_job_id"
  run_json_case "${prefix}_extract_list" '.ok == true and .action == "extract" and .subaction == "list" and (((.data.data.jobs | type) == "array" and .data.data.limit == 5) or ((.data.shape.jobs | type) == "object" and .data.shape.limit == 5))' call_tool action:extract subaction:list limit:5 offset:0

  if [[ "$URL_MODE" == "1" ]]; then
    run_error_case "${prefix}_embed_start_unavailable" "local file embedding is disabled" call_tool_json "{\"action\":\"embed\",\"subaction\":\"start\",\"input\":\"$REPO_ROOT/docs/reference/mcp/overview.md\"}"
  else
    # A local path that passes validation is embedded IN-PROCESS (mirrors the CLI
    # guard) — never enqueued onto the shared jobs DB where a worker that cannot
    # see the host path could claim it. So embed.start of a local file returns a
    # completed status, not a job_id. embed status/cancel job-lifecycle is covered
    # by the crawl/extract smokes (shared plumbing).
    run_json_case "${prefix}_embed_start" '.ok == true and .action == "embed" and .subaction == "start" and .data.status == "completed" and (.data.input | type == "string")' call_tool_json "{\"action\":\"embed\",\"subaction\":\"start\",\"input\":\"$REPO_ROOT/docs/reference/mcp/overview.md\"}"
  fi
  run_json_case "${prefix}_embed_list" '.ok == true and .action == "embed" and .subaction == "list" and (((.data.data.jobs | type) == "array" and .data.data.limit == 5) or ((.data.shape.jobs | type) == "object" and .data.shape.limit == 5))' call_tool action:embed subaction:list limit:5 offset:0

  run_error_case "${prefix}_ingest_start_sessions_unavailable" "/v1/ingest/sessions/prepared" call_tool_json '{"action":"ingest","subaction":"start","source_type":"sessions","sessions":{"codex":true,"project":"axon_rust"}}'
  run_json_case "${prefix}_ingest_list" '.ok == true and .action == "ingest" and .subaction == "list" and (.data.data.jobs | type == "array") and .data.data.limit == 5' call_tool action:ingest subaction:list limit:5 offset:0

  echo "== $mode lifecycle maintenance ==" | tee -a "$SUMMARY"
  run_json_case "${prefix}_crawl_cleanup" '.ok == true and .action == "crawl" and .subaction == "cleanup" and (.data.deleted | type == "number")' call_tool action:crawl subaction:cleanup
  run_json_case "${prefix}_crawl_recover" '.ok == true and .action == "crawl" and .subaction == "recover" and (.data.recovered | type == "number")' call_tool action:crawl subaction:recover
  run_json_case "${prefix}_crawl_clear" '.ok == true and .action == "crawl" and .subaction == "clear" and (.data.deleted | type == "number")' call_tool action:crawl subaction:clear
  run_json_case "${prefix}_extract_cleanup" '.ok == true and .action == "extract" and .subaction == "cleanup" and (.data.deleted | type == "number")' call_tool action:extract subaction:cleanup
  run_json_case "${prefix}_extract_recover" '.ok == true and .action == "extract" and .subaction == "recover" and (.data.recovered | type == "number")' call_tool action:extract subaction:recover
  run_json_case "${prefix}_extract_clear" '.ok == true and .action == "extract" and .subaction == "clear" and (.data.deleted | type == "number")' call_tool action:extract subaction:clear
  run_json_case "${prefix}_embed_cleanup" '.ok == true and .action == "embed" and .subaction == "cleanup" and (.data.deleted | type == "number")' call_tool action:embed subaction:cleanup
  run_json_case "${prefix}_embed_recover" '.ok == true and .action == "embed" and .subaction == "recover" and (.data.recovered | type == "number")' call_tool action:embed subaction:recover
  run_json_case "${prefix}_embed_clear" '.ok == true and .action == "embed" and .subaction == "clear" and (.data.deleted | type == "number")' call_tool action:embed subaction:clear
  run_json_case "${prefix}_ingest_cleanup" '.ok == true and .action == "ingest" and .subaction == "cleanup" and (.data.deleted | type == "number")' call_tool action:ingest subaction:cleanup
  run_json_case "${prefix}_ingest_recover" '.ok == true and .action == "ingest" and .subaction == "recover" and (.data.recovered | type == "number")' call_tool action:ingest subaction:recover
  run_json_case "${prefix}_ingest_clear" '.ok == true and .action == "ingest" and .subaction == "clear" and (.data.deleted | type == "number")' call_tool action:ingest subaction:clear
}

if [[ "$URL_MODE" == "1" ]]; then
  # URL-mode configs target an already-running MCP HTTP server.
  run_suite url
else
  run_suite stdio
fi
echo "" | tee -a "$SUMMARY"
echo "Results: PASS=$pass FAIL=$fail" | tee -a "$SUMMARY"
echo "Summary: $SUMMARY"

if [[ "$fail" -gt 0 ]]; then
  exit 1
fi
