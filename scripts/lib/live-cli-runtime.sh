#!/usr/bin/env bash
# Live execution, contracts, server, and setup helpers.

command_contract_filter() {
  case "$1" in
    "watch create"|"watch get"|"watch update"|"watch pause"|"watch resume") echo '(.watch_id | type) == "string"' ;;
    "watch list") echo '(.items | type) == "array"' ;;
    "watch status") echo '(.watch | type) == "object"' ;;
    "watch exec") echo '(.id | type) == "string" and (.status_url | type) == "string"' ;;
    "watch delete") echo '.deleted == true and (.watch_id | type) == "string"' ;;
    "watch history") echo '(.jobs | type) == "array"' ;;
    "monitor jobs") echo 'type == "array"' ;;
    "map") echo '(.urls | type) == "array" and (.map_source | type) == "string"' ;;
    "endpoints") echo '(.endpoints | type) == "array" and (.url | type) == "string"' ;;
    "extract") echo '(.job_id | type) == "string"' ;;
    "extract status") echo '(.id | type) == "string" and (.status | type) == "string"' ;;
    "extract cancel") echo '(.id | type) == "string" and (.canceled | type) == "boolean"' ;;
    "extract errors") echo '(.id | type) == "string" and (.status | type) == "string"' ;;
    "extract list") echo '(.jobs | type) == "array"' ;;
    "extract cleanup"|"extract clear") echo '(.removed | type) == "number"' ;;
    "extract worker") echo '(.status | type) == "string" and (.elapsed_secs | type) == "number"' ;;
    "extract recover") echo '(.reclaimed | type) == "number"' ;;
    "search") echo '(.results | type) == "array" and (.query | type) == "string"' ;;
    "research") echo '(.summary | type) == "string" and (.search_results | type) == "array"' ;;
    "scrape"|"source") echo '(.status == "completed" or .status == "completed_degraded") and (.job_id | type) == "string"' ;;
    "brand") echo '(.url | type) == "string" and (.colors | type) == "array"' ;;
    "debug") echo '(.doctor_report | type) == "object"' ;;
    "diff") echo '(.status | type) == "string" and (.text_diff | type) == "string"' ;;
    "doctor"|"doctor diagnose") echo '(.all_ok | type) == "boolean" and (.services | type) == "object"' ;;
    "query") echo '(.rank | type) == "number" and (.citation | type) == "object"' ;;
    "retrieve") echo '(.chunks | type) == "number" and (.content | type) == "string"' ;;
    "ask") echo '(.answer | type) == "string" and (.citations | type) == "array"' ;;
    "summarize") echo '(.summary | type) == "string" and (.documents | type) == "array"' ;;
    "evaluate") echo '(.query | type) == "string" and (.rag_answer | type) == "string"' ;;
    "train") echo '(.event_id | type) == "string" and (.candidates | type) == "array"' ;;
    "suggest") echo '(.suggestions | type) == "array"' ;;
    "sources") echo '(.urls | type) == "array" and (.count | type) == "number"' ;;
    "domains") echo 'type == "object"' ;;
    "stats") echo '(.status | type) == "string" and (.collection | type) == "string"' ;;
    "status") echo '(.jobs | type) == "array" and (.totals | type) == "object"' ;;
    "jobs list") echo '(.items | type) == "array" and (.limit | type) == "number" and ((.next_cursor == null) or ((.next_cursor | type) == "string"))' ;;
    "jobs get") echo '(.job_id | type) == "string" and (.status | type) == "string"' ;;
    "jobs events"|"jobs stream") echo '(.events | type) == "array" and (.last_sequence | type) == "number"' ;;
    "jobs cancel") echo '(.job_id | type) == "string" and (.status | type) == "string"' ;;
    "jobs retry") echo '(.original_job_id | type) == "string" and (.retry_job | type) == "object"' ;;
    "jobs recover") echo '(.recovered | type) == "number" and (.job_ids | type) == "array"' ;;
    "jobs cleanup") echo '(.dry_run | type) == "boolean" and (.matched | type) == "number"' ;;
    "jobs clear") echo '(.deleted | type) == "number"' ;;
    "jobs worker") echo '(.worker | type) == "object"' ;;
    "memory remember"|"memory show") echo '(.memory | type) == "object"' ;;
    "memory list"|"memory search") echo '(.memories | type) == "array"' ;;
    "memory link") echo '(.edge | type) == "object"' ;;
    "memory supersede") echo '(.edge | type) == "object" and (.replacement_id | type) == "string"' ;;
    "memory context") echo '(.context.context | type) == "string" and (.context.memories | type) == "array"' ;;
    "sessions") echo '(.sessions | type) == "array"' ;;
    "reset") echo '(.plan_id | type) == "string" and (.stores | type) == "array"' ;;
    "prune plan"|"prune exec") echo '.ok == true and (.plan | type) == "object"' ;;
    "preflight") echo '(.has_errors == false and (.phases | type) == "array") or (.status == "ok" and (.removed_keys | type) == "array")' ;;
    "smoke"|"compose up"|"compose down"|"compose restart"|"compose rebuild"|"setup init"|"setup check") echo '.has_errors == false and (.phases | type) == "array"' ;;
    "setup plugin-hook"|"setup hook alias") echo '(.action | type) == "string" and (.exit_policy | type) == "string"' ;;
    "setup targets") echo 'type == "array"' ;;
    "setup install") echo '.status == "installed" and (.path | type) == "string"' ;;
    "setup config rewrite") echo '(.write_count | type) == "number" and (.stale_keys | type) == "array"' ;;
    "migrate") echo '.ok == true and (.points_migrated | type) == "number"' ;;
    "config list") echo '(.env | type) == "object" and (.toml | type) == "object"' ;;
    "config get") echo '(.key | type) == "string" and (.present | type) == "boolean"' ;;
    "config set"|"config unset") echo '(.key | type) == "string" and (.status | type) == "string"' ;;
    "config path") echo '(.env_path | type) == "string" and (.toml_path | type) == "string"' ;;
    "sync pending") echo '(.pending | type) == "number" and (.synced | type) == "number"' ;;
    "update") echo '(.installed | type) == "boolean" and (.install_path | type) == "string"' ;;
    "palette"|"palette launch"|"palette desktop"|"palette autostart") echo '(.status | type) == "string"' ;;
    "artifacts list") echo '(.items | type) == "array"' ;;
    "artifacts get") echo '(.artifact_id | type) == "string"' ;;
    "artifacts content") echo '(.artifact_id | type) == "string" and (.size_bytes | type) == "number"' ;;
    "uploads list") echo '(.items | type) == "array"' ;;
    "uploads create") echo '(.upload | type) == "object"' ;;
    "uploads get") echo '(.upload_id | type) == "string" and (.status | type) == "string"' ;;
    "uploads complete") echo '(.artifact_id | type) == "string" and (.upload_id | type) == "string"' ;;
    "uploads abort") echo '(.deleted | type) == "boolean" and (.upload_id | type) == "string"' ;;
    "collections list") echo '(.collections | type) == "array"' ;;
    "collections get") echo '(.collection | type) == "string" and (.payload_indexes | type) == "array"' ;;
    "graph kinds") echo '(.node_kinds | type) == "array" and (.edge_kinds | type) == "array"' ;;
    "graph resolve") echo '(.resolved | type) == "array" and (.misses | type) == "array"' ;;
    "graph query"|"graph source") echo '(.nodes | type) == "array" and (.edges | type) == "array"' ;;
    "graph node") echo '(.node | type) == "object" and (.edges | type) == "array"' ;;
    "graph edge") echo '(.edge_id | type) == "string" and (.evidence | type) == "array"' ;;
    "providers list") echo '(.providers | type) == "array"' ;;
    providers\ get*) echo '(.id | type) == "string" and (.ok | type) == "boolean"' ;;
    "capabilities") echo '(.supported_routes | type) == "array" and (.version | type) == "string"' ;;
    "chat") echo '(.reply | type) == "string" and (.session_id | type) == "string"' ;;
    *) return 1 ;;
  esac
}

prepare_live_invocation() {
  local -a original=("$@") global_args=() command_path=() local_args=() candidate=()
  local encoded candidate_json option arg
  local candidate_len match_len=0 index matches
  ensure_behavior_global_options

  while IFS= read -r encoded; do
    candidate_json="$(printf '%s' "$encoded" | base64 --decode | jq -c '.path')"
    mapfile -t candidate < <(printf '%s' "$candidate_json" | jq -r '.[]')
    candidate_len="${#candidate[@]}"
    [ "$candidate_len" -le "$match_len" ] && continue
    [ "$candidate_len" -gt "${#original[@]}" ] && continue
    matches=1
    for ((index = 0; index < candidate_len; index++)); do
      if [ "${original[$index]}" != "${candidate[$index]}" ]; then
        matches=0
        break
      fi
    done
    if [ "$matches" -eq 1 ]; then
      command_path=("${candidate[@]}")
      match_len="$candidate_len"
    fi
  done < <(jq -r '.commands[] | @base64' "$REGISTRY")

  if [ "$match_len" -eq 0 ]; then
    PREPARED_ARGS=("${original[@]}")
    return
  fi

  index="$match_len"
  while [ "$index" -lt "${#original[@]}" ]; do
    arg="${original[$index]}"
    case "$arg" in
      --*)
        option="${arg%%=*}"
        if grep -Fqx -- "$option" "$BEHAVIOR_GLOBAL_OPTIONS"; then
          global_args+=("$arg")
          if [[ "$arg" != *=* ]]             && grep -Fqx -- "$option" "$BEHAVIOR_GLOBAL_VALUE_OPTIONS"             && [ $((index + 1)) -lt "${#original[@]}" ]; then
            index=$((index + 1))
            global_args+=("${original[$index]}")
          fi
        else
          local_args+=("$arg")
        fi
        ;;
      *) local_args+=("$arg") ;;
    esac
    index=$((index + 1))
  done

  PREPARED_ARGS=("${global_args[@]}" "${command_path[@]}" "${local_args[@]}")
}

run_live() {
  local name="$1"
  shift
  local logfile stderr_log exit_code result json_expected=0 arg contract_filter started_ms
  prepare_live_invocation "$@"
  for arg in "${PREPARED_ARGS[@]}"; do
    [ "$arg" = "--json" ] && json_expected=1
  done
  local log_slug log_count log_suffix=""
  log_slug="$(printf '%s' "$name" | tr ' /' '__')"
  log_count=$((${LIVE_LOG_COUNTS[$log_slug]:-0} + 1))
  LIVE_LOG_COUNTS["$log_slug"]="$log_count"
  [ "$log_count" -gt 1 ] && log_suffix="-$log_count"
  logfile="$OUTDIR/logs/live-${log_slug}${log_suffix}.log"
  stderr_log="${logfile%.log}.stderr.log"
  LAST_LIVE_LOG="$logfile"
  started_ms="$(now_millis)"
  timeout "${TIMEOUT_SECS}s" "$AXON_BIN" "${PREPARED_ARGS[@]}" >"$logfile" 2>"$stderr_log"
  exit_code=$?
  if [ "$exit_code" -ne 0 ] && retryable_live_failure "$name" "$stderr_log"; then
    mv "$logfile" "${logfile%.log}.attempt-1.log"
    mv "$stderr_log" "${stderr_log%.log}.attempt-1.log"
    timeout "${TIMEOUT_SECS}s" "$AXON_BIN" "${PREPARED_ARGS[@]}" >"$logfile" 2>"$stderr_log"
    exit_code=$?
  fi
  if [ "$exit_code" -eq 0 ] && { [ "$json_expected" -eq 0 ] || jq -e . "$logfile" >/dev/null 2>&1; }; then
    result="PASS"
  else
    result="FAIL"
    failures=$((failures + 1))
  fi
  if [ "$result" = "PASS" ]; then
    set_pending_behavior "$name" "${PREPARED_ARGS[@]}"
    if contract_filter="$(command_contract_filter "$name" 2>/dev/null)"; then
      assert_live_json "$name semantic result" "$logfile" "$contract_filter"
      prove_mapped_command_options "$name"
    fi
  fi
  record "$name" "live" "$result" "$exit_code" "${PREPARED_ARGS[*]}" "$logfile"
  record_timing "$started_ms" "live" "$name" "${PREPARED_ARGS[*]}"
}

retryable_live_failure() {
  local name="$1" stderr_log="$2"
  case "$name" in
    source|scrape|map|brand|search|research|extract|screenshot) ;;
    *) return 1 ;;
  esac
  grep -Eq '\[(fetch\.timeout|fetch\.network|provider\.timeout|provider\.unavailable)\]' \
    "$stderr_log"
}

run_live_expect_failure() {
  local name="$1"
  shift
  local logfile stderr_log exit_code result
  prepare_live_invocation "$@"
  local log_slug log_count log_suffix=""
  log_slug="$(printf '%s' "$name" | tr ' /' '__')"
  log_count=$((${LIVE_LOG_COUNTS[$log_slug]:-0} + 1))
  LIVE_LOG_COUNTS["$log_slug"]="$log_count"
  [ "$log_count" -gt 1 ] && log_suffix="-$log_count"
  logfile="$OUTDIR/logs/live-${log_slug}${log_suffix}.log"
  stderr_log="${logfile%.log}.stderr.log"
  LAST_LIVE_LOG="$logfile"
  timeout "${TIMEOUT_SECS}s" "$AXON_BIN" "${PREPARED_ARGS[@]}" >"$logfile" 2>"$stderr_log"
  exit_code=$?
  if [ "$exit_code" -ne 0 ] && [ "$exit_code" -ne 124 ]; then
    result="PASS"
  else
    result="FAIL"
    failures=$((failures + 1))
  fi
  if [ "$result" = "PASS" ]; then
    set_pending_behavior "$name" "${PREPARED_ARGS[@]}"
  fi
  record "$name" "live-negative" "$result" "$exit_code" "${PREPARED_ARGS[*]}" "$logfile"
}

run_live_monitor_jsonl() {
  local name="$1" logfile stderr_log exit_code result monitor_pid
  logfile="$OUTDIR/logs/live-$(printf '%s' "$name" | tr ' /' '__').log"
  stderr_log="${logfile%.log}.stderr.log"
  LAST_LIVE_LOG="$logfile"
  printf '{"initialized":true,"statuses":{}}\n' >"$OUTDIR/monitor-watch-state.json"
  prepare_live_invocation monitor jobs --watch --jsonl --interval-secs 1 --state-file "$OUTDIR/monitor-watch-state.json" --json
  timeout 12s "$AXON_BIN" "${PREPARED_ARGS[@]}" \
    >"$logfile" 2>"$stderr_log" &
  monitor_pid=$!
  sleep 1
  timeout "${TIMEOUT_SECS}s" "$AXON_BIN" source "$map_fixture_url?monitor=$$" --scope map --wait true \
    --collection "$AXON_COLLECTION" --json \
    >"$OUTDIR/logs/fixture-monitor-transition.json" \
    2>"$OUTDIR/logs/fixture-monitor-transition.stderr.log" || true
  wait "$monitor_pid"
  exit_code=$?
  if [ "$exit_code" -eq 124 ] \
    && [ -s "$logfile" ] \
    && jq -s -e 'length > 0 and all(.[]; type == "object")' "$logfile" >/dev/null 2>&1; then
    result="PASS"
  else
    result="FAIL"
    failures=$((failures + 1))
  fi
  if [ "$result" = "PASS" ]; then
    record_behavior_args "monitor jobs" "${PREPARED_ARGS[@]}"
    prove_option_behavior "@global" "--watch" "bounded stream emitted lifecycle events"
    prove_option_behavior "@global" "--json" "every streamed stdout line parsed as JSON"
    prove_option_behavior "monitor jobs" "--jsonl" "stdout parsed as a non-empty JSONL stream"
    prove_option_behavior "monitor jobs" "--interval-secs" "monitor detected a controlled transition on the configured poll interval"
    prove_option_behavior "monitor jobs" "--state-file" "monitor used the isolated initialized state file"
  fi
  record "$name" "live-stream" "$result" "$exit_code" \
    "${PREPARED_ARGS[*]}" "$logfile"
}

assert_live_json() {
  local name="$1" logfile="$2"
  shift 2
  local result exit_code
  if jq -e "$@" "$logfile" >/dev/null 2>&1; then
    result="PASS"
    exit_code=0
    confirm_pending_behavior
  else
    result="FAIL"
    exit_code=1
    failures=$((failures + 1))
  fi
  record "$name" "contract" "$result" "$exit_code" "jq $*" "$logfile"
}

assert_live_text() {
  local name="$1" logfile="$2" expected="$3"
  local result exit_code
  if grep -Fq -- "$expected" "$logfile"; then
    result="PASS"
    exit_code=0
    confirm_pending_behavior
  else
    result="FAIL"
    exit_code=1
    failures=$((failures + 1))
  fi
  record "$name" "contract" "$result" "$exit_code" "$expected" "$logfile"
}

assert_live_nonempty() {
  local name="$1" logfile="$2"
  local result exit_code
  if [ -s "$logfile" ]; then
    result="PASS"
    exit_code=0
    confirm_pending_behavior
  else
    result="FAIL"
    exit_code=1
    failures=$((failures + 1))
  fi
  record "$name" "contract" "$result" "$exit_code" "non-empty output" "$logfile"
}

# A compose command reporting every phase ok only proves compose accepted the
# stack, not that the container survived its own entrypoint.
#
# This catches the immediate-exit class: `restart: unless-stopped` retries on a
# backoff that starts near 100ms and doubles, so a container that dies in its
# entrypoint has already logged several restarts within the settle window
# below. It is deliberately NOT a health check — a container that starts, runs
# for a minute, then dies still passes here. Widen the window rather than
# reading a PASS as "the service is healthy".
#
# The EXIT trap tears the compose project down, taking the crashed container
# and its logs with it, so capture the evidence here or lose it.
assert_live_container_stable() {
  local name="$1" container="$2" state detail result exit_code evidence slug
  local stderr_file reason
  sleep 5
  slug="$(printf '%s' "$name" | tr ' /' '__')"
  evidence="$OUTDIR/logs/container-stable-${slug}.log"
  stderr_file="$evidence.stderr"
  {
    printf '=== docker inspect %s ===\n' "$container"
    docker inspect "$container" 2>&1
    printf '\n=== docker logs --tail 200 %s ===\n' "$container"
    docker logs --tail 200 "$container" 2>&1
  } >"$evidence"
  # Keep "docker is unusable" distinguishable from "the container died".
  # Collapsing both into one signal is the same blindness this assertion exists
  # to remove, so surface docker's own words rather than a generic failure.
  if state="$(docker inspect --format '{{.State.Running}} {{.RestartCount}}' \
    "$container" 2>"$stderr_file")"; then
    detail="container running with no restarts (got: $state)"
  else
    state=""
    reason="$(tr '\n' ' ' <"$stderr_file" | cut -c1-160)"
    detail="docker inspect failed for $container: ${reason:-no error output}"
  fi
  cat "$stderr_file" >>"$evidence"
  rm -f "$stderr_file"
  if [ "$state" = "true 0" ]; then
    result="PASS"
    exit_code=0
    confirm_pending_behavior
  else
    result="FAIL"
    exit_code=1
    failures=$((failures + 1))
  fi
  record "$name" "contract" "$result" "$exit_code" "$detail" "$evidence"
}

run_live_server() {
  local name="$1" port="$2"
  shift 2
  local logfile stderr_log pid ready=0 protocol_ok=0 auth_ok=0 exit_code result _attempt
  local mcp_config mcp_schema mcp_call token
  prepare_live_invocation "$@"
  logfile="$OUTDIR/logs/live-$(printf '%s' "$name" | tr ' /' '__').log"
  stderr_log="${logfile%.log}.stderr.log"
  LAST_LIVE_LOG="$logfile"
  token="axon-live-server-token-$port"
  AXON_HTTP_HOST=127.0.0.1 AXON_HTTP_PORT="$port" AXON_BIND=127.0.0.1 \
    AXON_HTTP_TOKEN="$token" \
    "$AXON_BIN" "${PREPARED_ARGS[@]}" >"$logfile" 2>"$stderr_log" &
  pid=$!
  for _attempt in $(seq 1 60); do
    if curl -fsS --max-time 1 "http://127.0.0.1:$port/readyz" >/dev/null 2>&1; then
      ready=1
      break
    fi
    if ! kill -0 "$pid" 2>/dev/null; then
      break
    fi
    sleep 0.25
  done
  if [ "$ready" -eq 1 ]; then
    mcp_config="$OUTDIR/logs/mcporter-$port.json"
    mcp_schema="$OUTDIR/logs/mcporter-$port-schema.json"
    mcp_call="$OUTDIR/logs/mcporter-$port-capabilities.json"
    jq -n --arg url "http://127.0.0.1:$port/mcp" --arg token "$token" \
      '{mcpServers:{axon:{url:$url,headers:{Authorization:("Bearer " + $token)}}}}' \
      >"$mcp_config"
    if timeout 30s mcporter --config "$mcp_config" list axon --schema --json \
        >"$mcp_schema" 2>"$mcp_schema.stderr" \
      && jq -e '.status == "ok" and any(.tools[]; .name == "axon")' "$mcp_schema" >/dev/null \
      && timeout 30s mcporter --config "$mcp_config" call axon.axon \
        action=capabilities >"$mcp_call" 2>"$mcp_call.stderr" \
      && jq -e '.ok == true and .action == "capabilities"' "$mcp_call" >/dev/null; then
      protocol_ok=1
    fi
    if [ "$(curl -sS -o "$OUTDIR/logs/mcp-$port-unauthorized.json" -w '%{http_code}' \
      -X POST "http://127.0.0.1:$port/mcp" \
      -H 'content-type: application/json' \
      --data '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"axon-live","version":"1"}}}')" = "401" ]; then
      auth_ok=1
    fi
  fi
  kill -INT "$pid" 2>/dev/null || true
  for _attempt in $(seq 1 20); do
    kill -0 "$pid" 2>/dev/null || break
    sleep 0.25
  done
  if kill -0 "$pid" 2>/dev/null; then
    kill -TERM "$pid" 2>/dev/null || true
  fi
  for _attempt in $(seq 1 20); do
    kill -0 "$pid" 2>/dev/null || break
    sleep 0.25
  done
  if kill -0 "$pid" 2>/dev/null; then
    kill -KILL "$pid" 2>/dev/null || true
  fi
  wait "$pid" 2>/dev/null
  exit_code=$?
  if [ "$ready" -eq 1 ] && [ "$protocol_ok" -eq 1 ] && [ "$auth_ok" -eq 1 ] \
    && { [ "$exit_code" -eq 0 ] || [ "$exit_code" -eq 130 ] || [ "$exit_code" -eq 137 ] || [ "$exit_code" -eq 143 ]; }; then
    result="PASS"
    exit_code=0
    record_behavior_args "$name" "${PREPARED_ARGS[@]}"
    prove_option_behavior "$name" "--transport" \
      "HTTP transport completed initialize, tools/list, tool call, and auth rejection"
    record "$name MCP protocol" "contract" "PASS" "0" \
      "initialize + tools/list + capabilities tool call + unauthorized rejection" "$mcp_call"
  else
    result="FAIL"
    failures=$((failures + 1))
  fi
  record "$name" "live" "$result" "$exit_code" "${PREPARED_ARGS[*]}" "$logfile"
}

run_live_setup_home() {
  local name="$1"
  shift
  local logfile stderr_log exit_code result json_expected=0 arg contract_filter
  prepare_live_invocation "$@"
  for arg in "${PREPARED_ARGS[@]}"; do
    [ "$arg" = "--json" ] && json_expected=1
  done
  local log_slug log_count log_suffix=""
  log_slug="$(printf '%s' "$name" | tr ' /' '__')"
  log_count=$((${LIVE_LOG_COUNTS[$log_slug]:-0} + 1))
  LIVE_LOG_COUNTS["$log_slug"]="$log_count"
  [ "$log_count" -gt 1 ] && log_suffix="-$log_count"
  logfile="$OUTDIR/logs/live-${log_slug}${log_suffix}.log"
  stderr_log="${logfile%.log}.stderr.log"
  LAST_LIVE_LOG="$logfile"
  timeout "${TIMEOUT_SECS}s" env \
    -u AXON_HOME -u AXON_ENV_FILE -u AXON_CONFIG_PATH -u AXON_IMAGE \
    -u TEI_HTTP_PORT -u AXON_CHROME_MANAGEMENT_PORT \
    -u AXON_CHROME_CDP_PORT -u AXON_CHROME_DEVTOOLS_PORT \
    HOME="$SETUP_HOME" AXON_DATA_DIR="$SETUP_HOME/.axon" \
    TEI_HTTP_PORT="$LIVE_TEI_PORT" AXON_CHROME_MANAGEMENT_PORT="$LIVE_CHROME_MANAGEMENT_PORT" \
    AXON_CHROME_CDP_PORT="$LIVE_CHROME_CDP_PORT" AXON_CHROME_DEVTOOLS_PORT="$LIVE_CHROME_DEVTOOLS_PORT" \
    PATH="$SETUP_HELPER_BIN:$PATH" \
    "$AXON_BIN" "${PREPARED_ARGS[@]}" >"$logfile" 2>"$stderr_log"
  exit_code=$?
  if [ "$exit_code" -eq 0 ] && { [ "$json_expected" -eq 0 ] || jq -e . "$logfile" >/dev/null 2>&1; }; then
    result="PASS"
  else
    result="FAIL"
    failures=$((failures + 1))
  fi
  if [ "$result" = "PASS" ]; then
    set_pending_behavior "$name" "${PREPARED_ARGS[@]}"
    if contract_filter="$(command_contract_filter "$name" 2>/dev/null)"; then
      assert_live_json "$name semantic result" "$logfile" "$contract_filter"
      prove_mapped_command_options "$name"
    fi
  fi
  record "$name" "live" "$result" "$exit_code" "${PREPARED_ARGS[*]}" "$logfile"
}

run_live_setup_check() {
  local pid ready=0 _attempt
  AXON_HTTP_HOST=127.0.0.1 AXON_HTTP_PORT="$LIVE_SETUP_PORT" AXON_BIND=127.0.0.1 \
    "$AXON_BIN" serve >"$OUTDIR/logs/setup-check-server.log" \
    2>"$OUTDIR/logs/setup-check-server.stderr.log" &
  pid=$!
  for _attempt in $(seq 1 60); do
    if curl -fsS --max-time 1 "http://127.0.0.1:$LIVE_SETUP_PORT/readyz" >/dev/null 2>&1; then
      ready=1
      break
    fi
    kill -0 "$pid" 2>/dev/null || break
    sleep 0.25
  done
  if [ "$ready" -eq 1 ]; then
    run_live_setup_home "setup check" setup check --json
  else
    missing_live "setup check" "temporary Axon readiness fixture failed to start"
  fi
  kill -TERM "$pid" 2>/dev/null || true
  sleep 0.25
  kill -KILL "$pid" 2>/dev/null || true
  wait "$pid" 2>/dev/null || true
}

missing_live() {
  local name="$1" reason="$2"
  failures=$((failures + 1))
  record "$name" "live" "FAIL" "-" "$reason" "-"
}
