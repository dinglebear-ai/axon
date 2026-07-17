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
artifacts:content
artifacts:get
artifacts:list
ask
brand
capabilities
chat:chat
collections:get
collections:list
diff
doctor
endpoints
evaluate
extract:start
graph:edge
graph:kinds
graph:node
graph:query
graph:resolve
graph:source
help
jobs:cancel
jobs:cleanup
jobs:clear
jobs:events
jobs:get
jobs:list
jobs:recover
jobs:retry
jobs:status
jobs:stream
map
memory:remember
memory:list
memory:search
memory:show
memory:link
memory:supersede
memory:context
memory:reinforce
memory:contradict
memory:pin
memory:archive
memory:forget
memory:review
memory:compact
memory:import
memory:export
providers:get
providers:list
prune:exec
prune:plan
query
research
reset:exec
reset:plan
resolve
retrieve
screenshot
search
source
status
summarize
suggest
uploads:abort
uploads:complete
uploads:create
uploads:get
uploads:list
uploads:put_content
watch:create
watch:delete
watch:exec
watch:get
watch:history
watch:list
watch:pause
watch:resume
watch:status
watch:update
EOF
)"

DIRECT_ACTIONS_JSON='["ask","brand","capabilities","diff","doctor","endpoints","evaluate","help","map","query","research","resolve","retrieve","screenshot","search","source","status","suggest","summarize"]'
EXPECTED_TOP_LEVEL_ACTIONS="$(cat <<'EOF'
artifacts
ask
brand
capabilities
chat
collections
diff
doctor
endpoints
evaluate
extract
graph
help
jobs
map
memory
providers
prune
query
research
reset
resolve
retrieve
screenshot
search
source
status
summarize
suggest
uploads
watch
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

# The stdio wrapper execs ./target/debug/axon, so build once up front to
# validate this checkout rather than whatever binary happened to exist.
cargo build --bin axon >/dev/null

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
  json_payload "$schema_file" | jq -r '
    .tools[]
    | select(.name == "axon")
    | .inputSchema.properties.action.enum[]
  ' | sort -u
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
  mkdir -p "$runtime_root/logs"
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
        AXON_SOURCE_LOCAL_ALLOWED_ROOTS: $repo_root,
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
  run_json_case "${prefix}_status" '.ok == true and .action == "status" and .subaction == "status" and (((.data.data | type) == "object") or (.data.artifact.artifact_id | type == "string")) and (.data.response_mode | type == "string")' call_tool action:status
  run_json_case "${prefix}_help" '.ok == true and .action == "help" and .subaction == "help" and (.data.data.actions | type == "object")' call_tool action:help
  run_json_case "${prefix}_doctor" '.ok == true and .action == "doctor" and .subaction == "doctor" and (((.data.data.all_ok | type) == "boolean") or ((.data.shape.all_ok | type) == "boolean"))' call_tool action:doctor
  run_json_case "${prefix}_capabilities" '.ok == true and .action == "capabilities" and .subaction == "capabilities" and (.data.data.actions | type == "array") and (.data.data.providers | type == "array")' call_tool action:capabilities
  run_json_case "${prefix}_providers_list" '.ok == true and .action == "providers" and .subaction == "list" and (.data.data.providers | type == "array")' call_tool action:providers subaction:list
  run_json_case "${prefix}_resolve" '.ok == true and .action == "resolve" and .subaction == "resolve" and .data.data.source == "https://example.com"' call_tool action:resolve source:'https://example.com'
  run_json_case "${prefix}_graph_kinds" '.ok == true and .action == "graph" and .subaction == "kinds" and (((.data.data | type) == "object") or ((.data.inline | type) == "object"))' call_tool action:graph subaction:kinds
  run_json_case "${prefix}_query" '(.ok == true and .action == "query" and .subaction == "query" and (.data.data.results | type == "array") and .data.data.query == "rust mcp sdk") or ((.error | type) == "string" and (.error | contains("TEI transport error")))' call_tool action:query query:'rust mcp sdk' limit:3 offset:0
  run_json_case "${prefix}_source_detached" '.ok == true and .action == "source" and .subaction == "source" and (((.data.inline.job.id | type) == "string") or ((.data.inline.job_id | type) == "string") or ((.data.data.job.job_id | type) == "string"))' call_tool action:source source:"$REAL_PAGE_URL" scope:page detached:true response_mode:inline
  run_json_case "${prefix}_map" ".ok == true and .action == \"map\" and .subaction == \"map\" and (.data.data.urls | type == \"array\") and .data.data.url == \"$REAL_PAGE_URL\"" call_tool action:map url:"$REAL_PAGE_URL" limit:5 offset:0
  run_json_case "${prefix}_retrieve" ".ok == true and .action == \"retrieve\" and .subaction == \"retrieve\" and (((.data.data.url == \"$REAL_PAGE_URL\") and ((.data.data.content | type) == \"string\" or (.data.data.chunks | type) == \"array\")) or ((.data.shape.url == \"$REAL_PAGE_URL\") and (.data.artifact.artifact_id | type == \"string\")) or ((.data.inline.requested_url == \"$REAL_PAGE_URL\") and (.data.artifact.artifact_id | type == \"string\")))" call_tool action:retrieve url:"$REAL_PAGE_URL"
  if [[ "$URL_MODE" == "1" ]]; then
    run_error_case "${prefix}_search_unavailable" "search requires AXON_SEARXNG_URL or TAVILY_API_KEY" call_tool action:search query:'rust programming language' limit:3 offset:0
    run_error_case "${prefix}_research_unavailable" "research requires AXON_SEARXNG_URL or TAVILY_API_KEY" call_tool action:research query:'rust async best practices' limit:3 offset:0
    run_error_case "${prefix}_ask_unavailable" "ask 'What is this repository?' failed" call_tool action:ask query:'What is this repository?'
    run_error_case "${prefix}_screenshot_unavailable" "screenshot failed" call_tool_with_timeout 180000 action:screenshot url:"$REAL_PAGE_URL"
  else
    run_json_case "${prefix}_search" '.ok == true and .action == "search" and .subaction == "search" and (.data.data.results | type == "array") and .data.data.query == "rust programming language"' call_tool action:search query:'rust programming language' limit:3 offset:0
    run_json_case "${prefix}_research" '.ok == true and .action == "research" and .subaction == "research" and (((.data.data.search_results | type) == "array" and (.data.data.summary | type) == "string") or (.data.response_mode == "path" and ((.data.shape.search_results | type) == "string" or (.data.shape.search_results | type) == "object") and (.data.shape.summary | type) == "string"))' call_tool action:research query:'rust async best practices' limit:3 offset:0
    run_json_case "${prefix}_ask" '(.ok == true and .action == "ask" and .subaction == "ask" and (((.data.data.answer | type) == "string" and .data.data.query == "What is this repository?") or (.data.shape.query == "What is this repository?" and .data.shape.explain.llm_skipped == true))) or ((.error | type) == "string" and (.error | contains("TEI transport error")))' call_tool action:ask query:'What is this repository?' explain:true response_mode:inline
    run_json_case "${prefix}_screenshot" '.ok == true and .action == "screenshot" and ((.data.data.path | type == "string") or (.data.path | type == "string"))' call_tool_with_timeout 180000 action:screenshot url:"$REAL_PAGE_URL"
  fi
  echo "== $mode removed action guards ==" | tee -a "$SUMMARY"
  run_error_case "${prefix}_removed_crawl" "action \`crawl\` was removed from MCP" call_tool action:crawl subaction:start url:"$REAL_PAGE_URL"
  run_error_case "${prefix}_removed_scrape" "action \`scrape\` was removed from MCP" call_tool action:scrape url:"$REAL_PAGE_URL"
  run_error_case "${prefix}_removed_embed" "action \`embed\` was removed from MCP" call_tool action:embed input:"$REPO_ROOT/docs/reference/mcp/overview.md"
  run_error_case "${prefix}_removed_ingest" "action \`ingest\` was removed from MCP" call_tool action:ingest target:"$REPO_ROOT"
  run_error_case "${prefix}_removed_code_search" "action \`code_search\` was removed from MCP" call_tool action:code_search query:'freshness lease' cwd:"$REPO_ROOT"
  run_error_case "${prefix}_removed_vertical_scrape" "action \`vertical_scrape\` was removed from MCP" call_tool action:vertical_scrape subaction:list
  run_error_case "${prefix}_removed_purge" "action \`purge\` was removed from MCP" call_tool action:purge target:"$REAL_PAGE_URL"
  run_error_case "${prefix}_removed_dedupe" "action \`dedupe\` was removed from MCP" call_tool action:dedupe
  run_error_case "${prefix}_removed_stats" "this action was removed from MCP" call_tool action:stats
  run_error_case "${prefix}_removed_domains" "this action was removed from MCP" call_tool action:domains
  run_error_case "${prefix}_removed_sources" "this action was removed from MCP" call_tool action:sources
  run_json_case "${prefix}_memory_remember" '(.ok == true and .action == "memory" and (.data.memory.id | type == "string")) or ((.error | type) == "string" and (.error | contains("TEI transport error")))' call_tool_json '{"action":"memory","subaction":"remember","body":"mcporter smoke memory content lives in Qdrant","project":"axon"}'
  local memory_id
  if memory_id="$(extract_json_field "$OUTDIR/${prefix}_memory_remember.log" '.data.memory.id' 2>/dev/null)"; then
    run_json_case "${prefix}_memory_replacement" '.ok == true and .action == "memory" and (.data.memory.id | type == "string")' call_tool_json '{"action":"memory","subaction":"remember","body":"mcporter smoke replacement memory content lives in Qdrant","project":"axon"}'
    local replacement_memory_id
    replacement_memory_id="$(extract_json_field "$OUTDIR/${prefix}_memory_replacement.log" '.data.memory.id')"
    run_json_case "${prefix}_memory_show" '.ok == true and .action == "memory" and ((.data.memory == null) or (.data.memory.id | type == "string"))' call_tool action:memory subaction:show id:"$memory_id"
    run_json_case "${prefix}_memory_link" '.ok == true and .action == "memory" and .subaction == "link" and (.data.edge.id | type == "string") and .data.edge.edge_type == "relates_to"' call_tool action:memory subaction:link source_id:"$replacement_memory_id" target_id:"$memory_id"
    run_json_case "${prefix}_memory_supersede" '.ok == true and .action == "memory" and .subaction == "supersede" and (.data.edge.id | type == "string") and .data.edge.edge_type == "supersedes"' call_tool action:memory subaction:supersede source_id:"$replacement_memory_id" target_id:"$memory_id"
    run_json_case "${prefix}_memory_list" '.ok == true and .action == "memory" and .subaction == "list" and (.data.memories | type == "array")' call_tool action:memory subaction:list project:axon status:active limit:3
    run_json_case "${prefix}_memory_search" '.ok == true and .action == "memory" and (.data.memories | type == "array")' call_tool action:memory subaction:search query:'mcporter smoke memory' project:axon limit:3
    run_json_case "${prefix}_memory_context" '.ok == true and .action == "memory" and .subaction == "context" and (.data.context.context | type == "string") and (.data.context.context | contains("trust=\"evidence_only\""))' call_tool action:memory subaction:context query:'mcporter smoke memory' project:axon limit:3 token_budget:1000
  else
    record_pass "${prefix}_memory_followups_skipped_tei_unavailable"
  fi

  echo "== $mode path-mode response ==" | tee -a "$SUMMARY"
  # Artifact-first response mode still persists large payloads to disk and returns
  # a path; the in-process server makes that path directly readable. (The standalone
  # `artifacts` MCP action was removed in 5.0.0.)
  run_json_case "${prefix}_help_path" '.ok == true and .action == "help" and .subaction == "help" and .data.response_mode == "path" and (.data.artifact.artifact_id | type == "string")' call_tool action:help response_mode:path

  echo "== $mode lifecycle start/status/cancel/list ==" | tee -a "$SUMMARY"
  run_json_case "${prefix}_jobs_list" '.ok == true and .action == "jobs" and .subaction == "list" and (.data.data.items | type == "array")' call_tool action:jobs subaction:list limit:5
  run_json_case "${prefix}_watch_create" '.ok == true and .action == "watch" and .subaction == "create" and ((.data.data.watch_id // .data.inline.watch_id) | type == "string")' call_tool action:watch subaction:create source:"$REAL_PAGE_URL" every_seconds:3600 response_mode:inline
  local watch_id
  watch_id="$(extract_json_field "$OUTDIR/${prefix}_watch_create.log" '.data.data.watch_id // .data.inline.watch_id')"
  run_json_case "${prefix}_watch_status" '.ok == true and .action == "watch" and .subaction == "status" and (.data.data.watch.watch_id | type == "string")' call_tool action:watch subaction:status id:"$watch_id" response_mode:inline
  run_json_case "${prefix}_watch_exec" '.ok == true and .action == "watch" and .subaction == "exec" and (.data.data.job_id | type == "string")' call_tool action:watch subaction:exec id:"$watch_id" response_mode:inline
  run_json_case "${prefix}_watch_history" '.ok == true and .action == "watch" and .subaction == "history" and (.data.data.runs | type == "array")' call_tool action:watch subaction:history id:"$watch_id" response_mode:inline
  run_json_case "${prefix}_extract_start" '.ok == true and .action == "extract" and .subaction == "start" and (.data.job_id | type == "string")' call_tool_json "{\"action\":\"extract\",\"subaction\":\"start\",\"urls\":[\"$REAL_PAGE_URL\"],\"prompt\":\"Extract the page title.\",\"max_pages\":1}"
  local extract_job_id
  extract_job_id="$(extract_json_field "$OUTDIR/${prefix}_extract_start.log" '.data.job_id')"
  run_json_case "${prefix}_extract_status" '.ok == true and .action == "extract" and .subaction == "status" and .data.response_mode != null and (((.data.data.job | type) == "object") or (.data.data.job == null))' call_tool action:extract subaction:status job_id:"$extract_job_id"
  run_json_case "${prefix}_extract_cancel" '.ok == true and .action == "extract" and .subaction == "cancel" and (.data.job_id | type == "string") and (.data.canceled | type == "boolean")' call_tool action:extract subaction:cancel job_id:"$extract_job_id"
  run_json_case "${prefix}_extract_list" '.ok == true and .action == "extract" and .subaction == "list" and (((.data.data.jobs | type) == "array" and .data.data.limit == 5) or ((.data.shape.jobs | type) == "object" and .data.shape.limit == 5))' call_tool action:extract subaction:list limit:5 offset:0

  echo "== $mode lifecycle maintenance ==" | tee -a "$SUMMARY"
  run_json_case "${prefix}_extract_cleanup" '.ok == true and .action == "extract" and .subaction == "cleanup" and (.data.deleted | type == "number")' call_tool action:extract subaction:cleanup
  run_json_case "${prefix}_extract_recover" '.ok == true and .action == "extract" and .subaction == "recover" and (.data.recovered | type == "number")' call_tool action:extract subaction:recover
  run_json_case "${prefix}_extract_clear" '.ok == true and .action == "extract" and .subaction == "clear" and (.data.deleted | type == "number")' call_tool action:extract subaction:clear
  run_json_case "${prefix}_jobs_recover" '.ok == true and .action == "jobs" and .subaction == "recover" and ((.data.data.recovered | type) == "number" or (.data.data.recovered_jobs | type) == "number")' call_tool action:jobs subaction:recover dry_run:true limit:5
  run_json_case "${prefix}_jobs_cleanup" '.ok == true and .action == "jobs" and .subaction == "cleanup" and ((.data.data.deleted | type) == "number" or (.data.data.deleted_jobs | type) == "number")' call_tool action:jobs subaction:cleanup dry_run:true limit:5
  run_json_case "${prefix}_prune_plan" '.ok == true and .action == "prune" and .subaction == "plan" and (.data.data.plan | type == "object")' call_tool action:prune subaction:plan target:collection:axon response_mode:inline
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
