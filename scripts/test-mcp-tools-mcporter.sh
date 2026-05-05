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
GRAPH_ENTITY="${GRAPH_ENTITY:-Axon}"
GRAPH_BUILD_URL="${GRAPH_BUILD_URL:-https://www.rust-lang.org/learn}"

pass=0
fail=0

OUTDIR=""
CONFIG_PATH=""
MCPORTER=()

EXPECTED_ROUTES="$(cat <<'EOF'
artifacts:clean
artifacts:delete
artifacts:grep
artifacts:head
artifacts:list
artifacts:read
artifacts:search
artifacts:wc
ask
crawl:cancel
crawl:cleanup
crawl:clear
crawl:list
crawl:recover
crawl:start
crawl:status
doctor
domains
elicit_demo
embed:cancel
embed:cleanup
embed:clear
embed:list
embed:recover
embed:start
embed:status
export
extract:cancel
extract:cleanup
extract:clear
extract:list
extract:recover
extract:start
extract:status
graph:build
graph:explore
graph:stats
graph:status
help
ingest:cancel
ingest:cleanup
ingest:clear
ingest:list
ingest:recover
ingest:start
ingest:status
map
query
refresh:cancel
refresh:cleanup
refresh:clear
refresh:list
refresh:recover
refresh:schedule
refresh:schedule:create
refresh:schedule:delete
refresh:schedule:disable
refresh:schedule:enable
refresh:schedule:list
refresh:start
refresh:status
research
retrieve
scrape
screenshot
search
sources
stats
status
EOF
)"

DIRECT_ACTIONS_JSON='["ask","doctor","domains","elicit_demo","export","help","map","query","research","retrieve","scrape","screenshot","search","sources","stats","status"]'
EXPECTED_TOP_LEVEL_ACTIONS="$(cat <<'EOF'
artifacts
ask
crawl
doctor
domains
elicit_demo
embed
export
extract
graph
help
ingest
map
query
refresh
research
retrieve
scrape
screenshot
search
sources
stats
status
EOF
)"

LITE_EXPECTED_ROUTES="$(printf '%s\n' "$EXPECTED_ROUTES" | grep -Ev '^(export|graph:|refresh:)')"
LITE_EXPECTED_TOP_LEVEL_ACTIONS="$(printf '%s\n' "$EXPECTED_TOP_LEVEL_ACTIONS" | grep -Ev '^(export|graph|refresh)$')"

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
    | if $action == "refresh_schedule" then
        .value[] | "refresh:schedule:\(.)"
      elif (($direct | index($action)) != null) or (.value | length == 0) then
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
    | select(. != "refresh_schedule")
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
  local lite_value="$2"
  local runtime_root="$BASE_OUTDIR/runtime-$mode"
  local suite_config="$BASE_OUTDIR/mcporter-$mode.json"
  jq \
    --arg server "$SERVER" \
    --arg lite "$lite_value" \
    --arg repo_root "$REPO_ROOT" \
    --arg data_dir "$runtime_root" \
    --arg log_file "$runtime_root/axon/logs/axon.log" \
    --arg sqlite_path "$runtime_root/axon/mcporter-jobs.db" \
    '.mcpServers[$server].env = ((.mcpServers[$server].env // {}) + {
        AXON_LITE: $lite,
        AXON_REPO_ROOT: $repo_root,
        AXON_DATA_DIR: $data_dir,
        AXON_LOG_FILE: $log_file,
        AXON_SQLITE_PATH: $sqlite_path
      })
     | if ((.mcpServers[$server].args // []) | length) > 1 then
         .mcpServers[$server].args[1] |= sub("export AXON_LITE=\\\"\\$\\{AXON_LITE:-0\\}\\\""; "export AXON_LITE=" + $lite)
       else
         .
       end
    ' \
    "$BASE_CONFIG_PATH" >"$suite_config"
  printf '%s\n' "$suite_config"
}

run_suite() {
  local mode="$1"
  local lite_value="$2"
  local prefix="$mode"
  local expected_routes="$EXPECTED_ROUTES"
  local expected_top_level_actions="$EXPECTED_TOP_LEVEL_ACTIONS"
  if [[ "$lite_value" == "1" ]]; then
    expected_routes="$LITE_EXPECTED_ROUTES"
    expected_top_level_actions="$LITE_EXPECTED_TOP_LEVEL_ACTIONS"
  fi

  CONFIG_PATH="$(build_suite_config "$mode" "$lite_value")"
  OUTDIR="$BASE_OUTDIR/$mode"
  mkdir -p "$OUTDIR"
  MCPORTER=(mcporter --config "$CONFIG_PATH")

  echo "== Suite: $mode (AXON_LITE=$lite_value) ==" | tee -a "$SUMMARY"
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
  if [[ "$lite_value" == "0" ]]; then
    run_json_case "${prefix}_export" '.ok == true and .action == "export"' call_tool action:export
  elif [[ "$URL_MODE" == "1" ]]; then
    run_error_case "${prefix}_export_unavailable" "unknown variant \`export\`" call_tool action:export
  else
    run_error_case "${prefix}_export_unavailable" "export is not available in lite mode because it requires Postgres-backed history" call_tool action:export
  fi
  run_json_case "${prefix}_query" '.ok == true and .action == "query" and .subaction == "query" and (.data.data.results | type == "array") and .data.data.query == "rust mcp sdk"' call_tool action:query query:'rust mcp sdk' limit:3 offset:0
  run_json_case "${prefix}_map" ".ok == true and .action == \"map\" and .subaction == \"map\" and (.data.data.urls | type == \"array\") and .data.data.url == \"$REAL_PAGE_URL\"" call_tool action:map url:"$REAL_PAGE_URL" limit:5 offset:0
  run_json_case "${prefix}_scrape" ".ok == true and .action == \"scrape\" and .subaction == \"scrape\" and (((.data.data.url == \"$REAL_PAGE_URL\") and (.data.data.markdown | type == \"string\")) or ((.data.shape.url == \"$REAL_PAGE_URL\") and (.data.shape.markdown | type == \"string\") and (.data.artifact.path | type == \"string\")))" call_tool action:scrape url:"$REAL_PAGE_URL"
  run_json_case "${prefix}_retrieve" ".ok == true and .action == \"retrieve\" and .subaction == \"retrieve\" and (((.data.data.url == \"$REAL_PAGE_URL\") and (.data.data.content | type == \"string\")) or ((.data.shape.url == \"$REAL_PAGE_URL\") and (.data.shape.content | type == \"string\") and (.data.artifact.path | type == \"string\")))" call_tool action:retrieve url:"$REAL_PAGE_URL"
  if [[ "$URL_MODE" == "1" ]]; then
    run_error_case "${prefix}_search_unavailable" "TAVILY_API_KEY is required for search" call_tool action:search query:'rust programming language' limit:3 offset:0
    run_error_case "${prefix}_research_unavailable" "TAVILY_API_KEY is required for research" call_tool action:research query:'rust async best practices' limit:3 offset:0
    run_error_case "${prefix}_ask_unavailable" "ask 'What is this repository?' failed" call_tool action:ask query:'What is this repository?'
    run_error_case "${prefix}_screenshot_unavailable" "screenshot failed" call_tool_with_timeout 180000 action:screenshot url:"$REAL_PAGE_URL"
  else
    run_json_case "${prefix}_search" '.ok == true and .action == "search" and .subaction == "search" and (.data.data.results | type == "array") and .data.data.query == "rust programming language"' call_tool action:search query:'rust programming language' limit:3 offset:0
    run_json_case "${prefix}_research" '.ok == true and .action == "research" and .subaction == "research" and (((.data.data.search_results | type) == "array" and (.data.data.summary | type) == "string") or (.data.response_mode == "path" and (.data.shape.search_results | type) == "string"))' call_tool action:research query:'rust async best practices' limit:3 offset:0
    run_json_case "${prefix}_ask" '.ok == true and .action == "ask" and .subaction == "ask" and (.data.data.answer | type == "string") and .data.data.query == "What is this repository?"' call_tool action:ask query:'What is this repository?'
    run_json_case "${prefix}_screenshot" '.ok == true and .action == "screenshot" and ((.data.data.path | type == "string") or (.data.path | type == "string"))' call_tool_with_timeout 180000 action:screenshot url:"$REAL_PAGE_URL"
  fi
  run_json_case "${prefix}_elicit_demo" '.ok == true and .action == "elicit_demo" and (.data.action | type == "string")' call_tool action:elicit_demo

  echo "== $mode artifacts ==" | tee -a "$SUMMARY"
  run_json_case "${prefix}_help_path" '.ok == true and .action == "help" and .subaction == "help" and .data.response_mode == "path" and (.data.artifact.path | type == "string")' call_tool action:help response_mode:path
  local help_path_file="$OUTDIR/${prefix}_help_path.log"
  local artifact_path
  artifact_path="$(extract_json_field "$help_path_file" '.data.artifact.path')"
  run_json_case "${prefix}_artifacts_head" '.ok == true and .action == "artifacts" and .subaction == "head" and (.data.path | type == "string") and (.data.head | type == "string")' call_tool action:artifacts subaction:head path:"$artifact_path" limit:10
  run_json_case "${prefix}_artifacts_wc" '.ok == true and .action == "artifacts" and .subaction == "wc" and (.data.path | type == "string") and (.data.bytes | type == "number") and (.data.lines | type == "number")' call_tool action:artifacts subaction:wc path:"$artifact_path"
  run_json_case "${prefix}_artifacts_read" '.ok == true and .action == "artifacts" and .subaction == "read" and (.data.path | type == "string") and (.data.content | type == "string")' call_tool action:artifacts subaction:read path:"$artifact_path" full:true limit:20 offset:0
  run_json_case "${prefix}_artifacts_grep" '.ok == true and .action == "artifacts" and .subaction == "grep" and (.data.path | type == "string") and .data.pattern == "action" and (.data.matches | type == "array")' call_tool action:artifacts subaction:grep path:"$artifact_path" pattern:'action' limit:10
  run_json_case "${prefix}_artifacts_list" '.ok == true and .action == "artifacts" and .subaction == "list" and (((.data.response_mode == "path") and (.data.artifact.path | type == "string")) or ((.data.response_mode == "auto-inline") and (.data.data.files | type == "array")))' call_tool action:artifacts subaction:list
  run_json_case "${prefix}_artifacts_search" '.ok == true and .action == "artifacts" and .subaction == "search" and (((.data.data.matches | type) == "array" and .data.data.pattern == "action") or ((.data.artifact.path | type) == "string" and .data.response_mode == "path"))' call_tool action:artifacts subaction:search pattern:'action' limit:10
  run_json_case "${prefix}_artifacts_clean" '.ok == true and .action == "artifacts" and .subaction == "clean" and .data.max_age_hours == 24 and (.data.files | type == "array")' call_tool action:artifacts subaction:clean max_age_hours:24
  run_json_case "${prefix}_artifacts_delete" '.ok == true and .action == "artifacts" and .subaction == "delete" and (.data.deleted | type == "string") and (.data.bytes_freed | type == "number")' call_tool action:artifacts subaction:delete path:"$artifact_path"

  echo "== $mode lifecycle start/status/cancel/list ==" | tee -a "$SUMMARY"
  run_json_case "${prefix}_crawl_start" '.ok == true and .action == "crawl" and .subaction == "start" and (.data.job_ids | type == "array") and ((.data.job_ids | length) > 0)' call_tool_json "{\"action\":\"crawl\",\"subaction\":\"start\",\"urls\":[\"$REAL_PAGE_URL\"],\"max_pages\":1}"
  local crawl_job_id
  crawl_job_id="$(extract_json_field "$OUTDIR/${prefix}_crawl_start.log" '.data.job_ids[0]')"
  run_json_case "${prefix}_crawl_status" '.ok == true and .action == "crawl" and .subaction == "status" and (((.data.job | type) == "object") or (.data.job == null))' call_tool action:crawl subaction:status job_id:"$crawl_job_id"
  run_json_case "${prefix}_crawl_cancel" '.ok == true and .action == "crawl" and .subaction == "cancel" and (.data.job_id | type == "string") and (.data.canceled | type == "boolean")' call_tool action:crawl subaction:cancel job_id:"$crawl_job_id"
  run_json_case "${prefix}_crawl_list" '.ok == true and .action == "crawl" and .subaction == "list" and (.data.data.jobs | type == "array") and .data.data.limit == 5' call_tool action:crawl subaction:list limit:5 offset:0

  run_json_case "${prefix}_extract_start" '.ok == true and .action == "extract" and .subaction == "start" and (.data.job_id | type == "string")' call_tool_json "{\"action\":\"extract\",\"subaction\":\"start\",\"urls\":[\"$REAL_PAGE_URL\"],\"prompt\":\"Extract the page title.\",\"max_pages\":1}"
  local extract_job_id
  extract_job_id="$(extract_json_field "$OUTDIR/${prefix}_extract_start.log" '.data.job_id')"
  run_json_case "${prefix}_extract_status" '.ok == true and .action == "extract" and .subaction == "status" and .data.response_mode != null and (((.data.data.job | type) == "object") or (.data.data.job == null))' call_tool action:extract subaction:status job_id:"$extract_job_id"
  run_json_case "${prefix}_extract_cancel" '.ok == true and .action == "extract" and .subaction == "cancel" and (.data.job_id | type == "string") and (.data.canceled | type == "boolean")' call_tool action:extract subaction:cancel job_id:"$extract_job_id"
  run_json_case "${prefix}_extract_list" '.ok == true and .action == "extract" and .subaction == "list" and (.data.data.jobs | type == "array") and .data.data.limit == 5' call_tool action:extract subaction:list limit:5 offset:0

  if [[ "$URL_MODE" == "1" ]]; then
    run_error_case "${prefix}_embed_start_unavailable" "local file embedding via MCP is disabled" call_tool_json "{\"action\":\"embed\",\"subaction\":\"start\",\"input\":\"$REPO_ROOT/docs/MCP.md\"}"
  else
    run_json_case "${prefix}_embed_start" '.ok == true and .action == "embed" and .subaction == "start" and (.data.job_id | type == "string")' call_tool_json "{\"action\":\"embed\",\"subaction\":\"start\",\"input\":\"$REPO_ROOT/docs/MCP.md\"}"
    local embed_job_id
    embed_job_id="$(extract_json_field "$OUTDIR/${prefix}_embed_start.log" '.data.job_id')"
    run_json_case "${prefix}_embed_status" '.ok == true and .action == "embed" and .subaction == "status" and .data.response_mode != null and (((.data.data.job | type) == "object") or (.data.data.job == null))' call_tool action:embed subaction:status job_id:"$embed_job_id"
    run_json_case "${prefix}_embed_cancel" '.ok == true and .action == "embed" and .subaction == "cancel" and (.data.job_id | type == "string") and (.data.canceled | type == "boolean")' call_tool action:embed subaction:cancel job_id:"$embed_job_id"
  fi
  run_json_case "${prefix}_embed_list" '.ok == true and .action == "embed" and .subaction == "list" and (.data.data.jobs | type == "array") and .data.data.limit == 5' call_tool action:embed subaction:list limit:5 offset:0

  run_json_case "${prefix}_ingest_start" '.ok == true and .action == "ingest" and .subaction == "start" and (.data.job_id | type == "string")' call_tool_json '{"action":"ingest","subaction":"start","source_type":"sessions","sessions":{"codex":true,"project":"axon_rust"}}'
  local ingest_job_id
  ingest_job_id="$(extract_json_field "$OUTDIR/${prefix}_ingest_start.log" '.data.job_id')"
  run_json_case "${prefix}_ingest_status" '.ok == true and .action == "ingest" and .subaction == "status" and .data.response_mode != null and (((.data.data.job | type) == "object") or (.data.data.job == null))' call_tool action:ingest subaction:status job_id:"$ingest_job_id"
  run_json_case "${prefix}_ingest_cancel" '.ok == true and .action == "ingest" and .subaction == "cancel" and (.data.job_id | type == "string") and (.data.canceled | type == "boolean")' call_tool action:ingest subaction:cancel job_id:"$ingest_job_id"
  run_json_case "${prefix}_ingest_list" '.ok == true and .action == "ingest" and .subaction == "list" and (.data.data.jobs | type == "array") and .data.data.limit == 5' call_tool action:ingest subaction:list limit:5 offset:0

  if [[ "$lite_value" == "0" ]]; then
    run_json_case "${prefix}_refresh_start" '.ok == true and .action == "refresh" and .subaction == "start" and (.data.job_id | type == "string")' call_tool_json "{\"action\":\"refresh\",\"subaction\":\"start\",\"url\":\"$REAL_PAGE_URL\"}"
    local refresh_job_id
    refresh_job_id="$(extract_json_field "$OUTDIR/${prefix}_refresh_start.log" '.data.job_id')"
    run_json_case "${prefix}_refresh_status" '.ok == true and .action == "refresh" and .subaction == "status" and .data.response_mode != null and (((.data.data.job | type) == "object") or (.data.data.job == null))' call_tool action:refresh subaction:status job_id:"$refresh_job_id"
    run_json_case "${prefix}_refresh_cancel" '.ok == true and .action == "refresh" and .subaction == "cancel" and (.data.job_id | type == "string") and (.data.canceled | type == "boolean")' call_tool action:refresh subaction:cancel job_id:"$refresh_job_id"
    run_json_case "${prefix}_refresh_list" '.ok == true and .action == "refresh" and .subaction == "list" and (.data.data.jobs | type == "array") and .data.data.limit == 5' call_tool action:refresh subaction:list limit:5 offset:0
  fi

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
  if [[ "$lite_value" == "0" ]]; then
    run_json_case "${prefix}_refresh_cleanup" '.ok == true and .action == "refresh" and .subaction == "cleanup" and (.data.deleted | type == "number")' call_tool action:refresh subaction:cleanup
    run_json_case "${prefix}_refresh_recover" '.ok == true and .action == "refresh" and .subaction == "recover" and (.data.recovered | type == "number")' call_tool action:refresh subaction:recover
    run_json_case "${prefix}_refresh_clear" '.ok == true and .action == "refresh" and .subaction == "clear" and (.data.deleted | type == "number")' call_tool action:refresh subaction:clear
  fi

  local schedule_name="mcporter-smoke-$mode-$$"
  if [[ "$lite_value" == "0" ]]; then
    echo "== $mode refresh schedules ==" | tee -a "$SUMMARY"
    run_json_case "${prefix}_refresh_schedule_list" '.ok == true and .action == "refresh" and .subaction == "schedule" and (.data.data.schedules | type == "array")' call_tool action:refresh subaction:schedule schedule_subaction:list
    run_json_case "${prefix}_refresh_schedule_create" '.ok == true and .action == "refresh" and .subaction == "schedule" and (.data.created.name | type == "string")' call_tool action:refresh subaction:schedule schedule_subaction:create schedule_name:"$schedule_name" url:"$REAL_PAGE_URL"
    run_json_case "${prefix}_refresh_schedule_disable" '.ok == true and .action == "refresh" and .subaction == "schedule" and (.data.name | type == "string") and .data.enabled == false and (.data.updated | type == "boolean")' call_tool action:refresh subaction:schedule schedule_subaction:disable schedule_name:"$schedule_name"
    run_json_case "${prefix}_refresh_schedule_enable" '.ok == true and .action == "refresh" and .subaction == "schedule" and (.data.name | type == "string") and .data.enabled == true and (.data.updated | type == "boolean")' call_tool action:refresh subaction:schedule schedule_subaction:enable schedule_name:"$schedule_name"
    run_json_case "${prefix}_refresh_schedule_delete" '.ok == true and .action == "refresh" and .subaction == "schedule" and (.data.name | type == "string") and (.data.deleted | type == "boolean")' call_tool action:refresh subaction:schedule schedule_subaction:delete schedule_name:"$schedule_name"
  fi

  if [[ "$lite_value" == "0" ]]; then
    echo "== $mode graph ==" | tee -a "$SUMMARY"
    run_json_case "${prefix}_graph_status" '.ok == true and .action == "graph" and .subaction == "status" and (.data.response_mode | type == "string") and ((.data.data.counts | type == "object") or (.data.artifact.path | type == "string"))' call_tool action:graph subaction:status
    run_json_case "${prefix}_graph_stats" '.ok == true and .action == "graph" and .subaction == "stats" and (.data.response_mode | type == "string") and ((.data.data.rows | type == "array") or (.data.artifact.path | type == "string"))' call_tool action:graph subaction:stats
    run_json_case "${prefix}_graph_explore" '.ok == true and .action == "graph" and .subaction == "explore" and (.data.response_mode | type == "string") and ((.data.data.entity | type == "string") or (.data.artifact.path | type == "string"))' call_tool action:graph subaction:explore entity:"$GRAPH_ENTITY"
    run_json_case "${prefix}_graph_build" '.ok == true and .action == "graph" and .subaction == "build" and (.data.response_mode | type == "string") and ((.data.data.queued | type == "number") or (.data.artifact.path | type == "string"))' call_tool action:graph subaction:build url:"$GRAPH_BUILD_URL"
  fi
}

if [[ "$URL_MODE" == "1" ]]; then
  # URL-mode configs target an already-running MCP HTTP server. CI starts that
  # server in default lite mode, so run the suite that matches the live process.
  run_suite lite 1
else
  run_suite full 0
  echo "" | tee -a "$SUMMARY"
  run_suite lite 1
fi
echo "" | tee -a "$SUMMARY"
echo "Results: PASS=$pass FAIL=$fail" | tee -a "$SUMMARY"
echo "Summary: $SUMMARY"

if [[ "$fail" -gt 0 ]]; then
  exit 1
fi
