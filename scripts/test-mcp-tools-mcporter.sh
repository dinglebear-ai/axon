#!/usr/bin/env bash
set -euo pipefail

# MCP smoke test runner via mcporter for Axon MCP server.
# - Verifies tool schema visibility
# - Verifies resource exposure via action:help response
# - Exercises top-level actions with minimal arguments
# - Covers all lifecycle families and subactions
#
# Usage:
#   ./scripts/test-mcp-tools-mcporter.sh
#   MCP_SERVER=axon ./scripts/test-mcp-tools-mcporter.sh
#   ./scripts/test-mcp-tools-mcporter.sh --full
#
# Notes:
# - mcporter currently exposes list/call; resource checks are performed via action:help
#   and list --schema output.
# - --full includes network-heavy/side-effect actions (start subactions, graph build).
# - Skipped: lifecycle start (mcporter array syntax unclear), status/cancel (need real
#   job_id), clear (destructive), elicit_demo (interactive MCP elicitation hangs).

SERVER="${MCP_SERVER:-axon}"
SELECTOR="${SERVER}.axon"
FULL=0

if [[ "${1:-}" == "--full" ]]; then
  FULL=1
fi

if ! command -v mcporter >/dev/null 2>&1; then
  echo "FAIL: mcporter not found in PATH" >&2
  exit 2
fi

if ! command -v jq >/dev/null 2>&1; then
  echo "FAIL: jq not found in PATH" >&2
  exit 2
fi

OUTDIR=".cache/mcporter-test"
mkdir -p "$OUTDIR"
SUMMARY="$OUTDIR/summary.txt"
: > "$SUMMARY"

pass=0
fail=0

run_case() {
  local name="$1"
  shift
  local logfile="$OUTDIR/${name}.log"
  if "$@" >"$logfile" 2>&1; then
    echo "PASS $name" | tee -a "$SUMMARY"
    pass=$((pass + 1))
  else
    echo "FAIL $name (see $logfile)" | tee -a "$SUMMARY"
    fail=$((fail + 1))
  fi
}

run_pipe_case() {
  local name="$1"
  local script="$2"
  local logfile="$OUTDIR/${name}.log"
  if bash -lc "$script" >"$logfile" 2>&1; then
    echo "PASS $name" | tee -a "$SUMMARY"
    pass=$((pass + 1))
  else
    echo "FAIL $name (see $logfile)" | tee -a "$SUMMARY"
    fail=$((fail + 1))
  fi
}

echo "Server: $SERVER" | tee -a "$SUMMARY"

echo "== Schema checks ==" | tee -a "$SUMMARY"
run_case list_server mcporter list "$SERVER"
run_case list_schema mcporter list "$SERVER" --schema

echo "== Resource checks ==" | tee -a "$SUMMARY"
run_pipe_case help_inline "mcporter call '$SELECTOR' action:help response_mode:inline --output json | jq -e '.ok == true' >/dev/null"
run_pipe_case help_has_resource_uri "mcporter call '$SELECTOR' action:help response_mode:inline --output json | jq -e '.data.inline.resources | index(\"axon://schema/mcp-tool\") != null' >/dev/null"
run_pipe_case schema_mentions_resource "mcporter list '$SERVER' --schema | grep -q 'axon://schema/mcp-tool'"

echo "== Core action checks ==" | tee -a "$SUMMARY"
run_pipe_case action_status "mcporter call '$SELECTOR' action:status --output json | jq -e '.ok == true and .action == \"status\"' >/dev/null"
run_pipe_case action_help "mcporter call '$SELECTOR' action:help --output json | jq -e '.ok == true and .action == \"help\"' >/dev/null"
run_pipe_case action_doctor "mcporter call '$SELECTOR' action:doctor --output json | jq -e '.ok == true and .action == \"doctor\"' >/dev/null"
run_pipe_case action_stats "mcporter call '$SELECTOR' action:stats --output json | jq -e '.ok == true and .action == \"stats\"' >/dev/null"
run_pipe_case action_domains "mcporter call '$SELECTOR' action:domains limit:5 offset:0 --output json | jq -e '.ok == true and .action == \"domains\"' >/dev/null"
run_pipe_case action_sources "mcporter call '$SELECTOR' action:sources limit:5 offset:0 --output json | jq -e '.ok == true and .action == \"sources\"' >/dev/null"
run_pipe_case action_query "mcporter call '$SELECTOR' action:query query:'rust mcp sdk' limit:3 offset:0 --output json | jq -e '.ok == true and .action == \"query\"' >/dev/null"
run_pipe_case action_retrieve "mcporter call '$SELECTOR' action:retrieve url:'$PWD/docs/MCP.md' --output json | jq -e '.ok == true and .action == \"retrieve\"' >/dev/null"
run_pipe_case action_map "mcporter call '$SELECTOR' action:map url:'https://example.com' limit:5 offset:0 --output json | jq -e '.ok == true and .action == \"map\"' >/dev/null"
run_pipe_case action_scrape "mcporter call '$SELECTOR' action:scrape url:'https://example.com' --output json | jq -e '.ok == true and .action == \"scrape\"' >/dev/null"
run_pipe_case action_crawl_list "mcporter call '$SELECTOR' action:crawl subaction:list limit:5 offset:0 --output json | jq -e '.ok == true and .action == \"crawl\" and .subaction == \"list\"' >/dev/null"
run_pipe_case action_extract_list "mcporter call '$SELECTOR' action:extract subaction:list limit:5 offset:0 --output json | jq -e '.ok == true and .action == \"extract\" and .subaction == \"list\"' >/dev/null"
run_pipe_case action_embed_list "mcporter call '$SELECTOR' action:embed subaction:list limit:5 offset:0 --output json | jq -e '.ok == true and .action == \"embed\" and .subaction == \"list\"' >/dev/null"
run_pipe_case action_ingest_list "mcporter call '$SELECTOR' action:ingest subaction:list limit:5 offset:0 --output json | jq -e '.ok == true and .action == \"ingest\" and .subaction == \"list\"' >/dev/null"
run_pipe_case action_refresh_list "mcporter call '$SELECTOR' action:refresh subaction:list limit:5 offset:0 --output json | jq -e '.ok == true and .action == \"refresh\" and .subaction == \"list\"' >/dev/null"

run_pipe_case action_artifacts_head "mcporter call '$SELECTOR' action:artifacts subaction:head path:'help/actions.json' limit:10 --output json | jq -e '.ok == true and .action == \"artifacts\" and .subaction == \"head\"' >/dev/null"
run_pipe_case action_artifacts_wc "mcporter call '$SELECTOR' action:artifacts subaction:wc path:'help/actions.json' --output json | jq -e '.ok == true and .action == \"artifacts\" and .subaction == \"wc\"' >/dev/null"
run_pipe_case action_artifacts_read "mcporter call '$SELECTOR' action:artifacts subaction:read path:'help/actions.json' full:true limit:20 offset:0 --output json | jq -e '.ok == true and .action == \"artifacts\" and .subaction == \"read\"' >/dev/null"
run_pipe_case action_artifacts_grep "mcporter call '$SELECTOR' action:artifacts subaction:grep path:'help/actions.json' pattern:'action' limit:10 offset:0 --output json | jq -e '.ok == true and .action == \"artifacts\" and .subaction == \"grep\"' >/dev/null"
run_pipe_case action_artifacts_list "mcporter call '$SELECTOR' action:artifacts subaction:list --output json | jq -e '.ok == true and .action == \"artifacts\" and .subaction == \"list\"' >/dev/null"
run_pipe_case action_artifacts_search "mcporter call '$SELECTOR' action:artifacts subaction:search pattern:'action' limit:10 --output json | jq -e '.ok == true and .action == \"artifacts\" and .subaction == \"search\"' >/dev/null"
run_pipe_case action_artifacts_clean_dry "mcporter call '$SELECTOR' action:artifacts subaction:clean max_age_hours:24 --output json | jq -e '.ok == true and .action == \"artifacts\" and .subaction == \"clean\"' >/dev/null"

echo "== Lifecycle cleanup/recover checks ==" | tee -a "$SUMMARY"
run_pipe_case action_crawl_cleanup "mcporter call '$SELECTOR' action:crawl subaction:cleanup --output json | jq -e '.ok == true and .action == \"crawl\" and .subaction == \"cleanup\"' >/dev/null"
run_pipe_case action_crawl_recover "mcporter call '$SELECTOR' action:crawl subaction:recover --output json | jq -e '.ok == true and .action == \"crawl\" and .subaction == \"recover\"' >/dev/null"
run_pipe_case action_extract_cleanup "mcporter call '$SELECTOR' action:extract subaction:cleanup --output json | jq -e '.ok == true and .action == \"extract\" and .subaction == \"cleanup\"' >/dev/null"
run_pipe_case action_extract_recover "mcporter call '$SELECTOR' action:extract subaction:recover --output json | jq -e '.ok == true and .action == \"extract\" and .subaction == \"recover\"' >/dev/null"
run_pipe_case action_embed_cleanup "mcporter call '$SELECTOR' action:embed subaction:cleanup --output json | jq -e '.ok == true and .action == \"embed\" and .subaction == \"cleanup\"' >/dev/null"
run_pipe_case action_embed_recover "mcporter call '$SELECTOR' action:embed subaction:recover --output json | jq -e '.ok == true and .action == \"embed\" and .subaction == \"recover\"' >/dev/null"
run_pipe_case action_ingest_cleanup "mcporter call '$SELECTOR' action:ingest subaction:cleanup --output json | jq -e '.ok == true and .action == \"ingest\" and .subaction == \"cleanup\"' >/dev/null"
run_pipe_case action_ingest_recover "mcporter call '$SELECTOR' action:ingest subaction:recover --output json | jq -e '.ok == true and .action == \"ingest\" and .subaction == \"recover\"' >/dev/null"
run_pipe_case action_refresh_cleanup "mcporter call '$SELECTOR' action:refresh subaction:cleanup --output json | jq -e '.ok == true and .action == \"refresh\" and .subaction == \"cleanup\"' >/dev/null"
run_pipe_case action_refresh_recover "mcporter call '$SELECTOR' action:refresh subaction:recover --output json | jq -e '.ok == true and .action == \"refresh\" and .subaction == \"recover\"' >/dev/null"

echo "== Refresh schedule checks ==" | tee -a "$SUMMARY"
run_pipe_case action_refresh_schedule_list "mcporter call '$SELECTOR' action:refresh subaction:schedule schedule_subaction:list --output json | jq -e '.ok == true and .action == \"refresh\" and .subaction == \"schedule\"' >/dev/null"

echo "== Graph action checks ==" | tee -a "$SUMMARY"
run_pipe_case action_graph_status "mcporter call '$SELECTOR' action:graph subaction:status --output json | jq -e '.ok == true and .action == \"graph\" and .subaction == \"status\"' >/dev/null"
run_pipe_case action_graph_stats "mcporter call '$SELECTOR' action:graph subaction:stats --output json | jq -e '.ok == true and .action == \"graph\" and .subaction == \"stats\"' >/dev/null"
run_pipe_case action_graph_explore "mcporter call '$SELECTOR' action:graph subaction:explore entity:'axon' --output json | jq -e '.ok == true and .action == \"graph\" and .subaction == \"explore\"' >/dev/null"

if [[ "$FULL" -eq 1 ]]; then
  echo "== Full/side-effect checks ==" | tee -a "$SUMMARY"
  run_pipe_case action_search "mcporter call '$SELECTOR' action:search query:'rust programming language' limit:3 offset:0 --output json | jq -e '.ok == true and .action == \"search\"' >/dev/null"
  run_pipe_case action_research "mcporter call '$SELECTOR' action:research query:'rust async best practices' limit:3 offset:0 --output json | jq -e '.ok == true and .action == \"research\"' >/dev/null"
  run_pipe_case action_ask "mcporter call '$SELECTOR' action:ask query:'What is this repository?' --output json | jq -e '.ok == true and .action == \"ask\"' >/dev/null"
  run_pipe_case action_screenshot "mcporter call '$SELECTOR' action:screenshot url:'https://example.com' --output json | jq -e '.ok == true and .action == \"screenshot\"' >/dev/null"
  run_pipe_case action_crawl_start "mcporter call '$SELECTOR' --args '{\"action\":\"crawl\",\"subaction\":\"start\",\"urls\":[\"https://example.com\"],\"max_pages\":1}' --output json | jq -e '.ok == true and .action == \"crawl\" and .subaction == \"start\"' >/dev/null"
  run_pipe_case action_extract_start "mcporter call '$SELECTOR' --args '{\"action\":\"extract\",\"subaction\":\"start\",\"urls\":[\"https://example.com\"]}' --output json | jq -e '.ok == true and .action == \"extract\" and .subaction == \"start\"' >/dev/null"
  run_pipe_case action_embed_start "mcporter call '$SELECTOR' action:embed subaction:start input:'https://example.com' --output json | jq -e '.ok == true and .action == \"embed\" and .subaction == \"start\"' >/dev/null"
  run_pipe_case action_ingest_start "mcporter call '$SELECTOR' action:ingest subaction:start source_type:github target:'steipete/mcporter' --output json | jq -e '.ok == true and .action == \"ingest\" and .subaction == \"start\"' >/dev/null"
  run_pipe_case action_refresh_start "mcporter call '$SELECTOR' action:refresh subaction:start url:'https://example.com' --output json | jq -e '.ok == true and .action == \"refresh\" and .subaction == \"start\"' >/dev/null"
  run_pipe_case action_graph_build "mcporter call '$SELECTOR' action:graph subaction:build --output json | jq -e '.ok == true and .action == \"graph\" and .subaction == \"build\"' >/dev/null"
fi

echo "" | tee -a "$SUMMARY"
echo "Results: PASS=$pass FAIL=$fail" | tee -a "$SUMMARY"
echo "Summary: $SUMMARY"

if [[ "$fail" -gt 0 ]]; then
  exit 1
fi
