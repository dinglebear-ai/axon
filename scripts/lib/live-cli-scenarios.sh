#!/usr/bin/env bash
# Isolated live fixture setup and scenario dispatch.

if [ "$MODE" = "live" ] || [ "$MODE" = "scenarios" ]; then
  if [ "${AXON_LIVE_USE_PRODUCTION_STATE:-0}" = "1" ]; then
    echo "production-state live scenarios are forbidden; use the harness-owned isolated state" >&2
    exit 2
  fi
  outdir_real="$(realpath -m "$OUTDIR")"
  home_axon_real="$(realpath -m "${HOME:-/nonexistent}/.axon")"
  requested_data_real="$(realpath -m "${AXON_LIVE_DATA_DIR:-$OUTDIR/data}")"
  case "$outdir_real" in
    "$home_axon_real"|"$home_axon_real"/*)
      echo "live test output must not overlap the production ~/.axon state: $outdir_real" >&2
      exit 2
      ;;
  esac
  case "$requested_data_real" in
    "$outdir_real"/*) ;;
    *)
      echo "AXON_LIVE_DATA_DIR must remain inside the harness output tree: $requested_data_real" >&2
      exit 2
      ;;
  esac
  isolated_bin_dir="$OUTDIR/isolated-bin"
  mkdir -p "$isolated_bin_dir"
  HARNESS_SOURCE_BIN="$AXON_BIN"
  HARNESS_SOURCE_BIN_SHA256="$(sha256sum -- "$HARNESS_SOURCE_BIN" | awk '{print $1}')"
  WORKTREE_CONTENT_SHA256="$(worktree_content_fingerprint)"
  install -m 0755 "$AXON_BIN" "$isolated_bin_dir/axon"
  AXON_BIN="$isolated_bin_dir/axon"
  export AXON_UPDATE_INSTALL_PATH="$isolated_bin_dir/axon"
  HARNESS_COMMAND_CWD="$OUTDIR/command-workdir"
  mkdir -p "$HARNESS_COMMAND_CWD"
  cd "$HARNESS_COMMAND_CWD" || exit 2

  # shellcheck disable=SC1091
  source "$ROOT_DIR/scripts/lib/axon-env.sh"
  load_axon_env_file "$ROOT_DIR"
  configured_chrome_remote_url="${AXON_CHROME_REMOTE_URL:-http://127.0.0.1:6000}"

  probe_live_chrome() {
    local endpoint="$1" diagnostic="$2"
    if ! curl -fsS --max-time 2 "${endpoint%/}/json/version" >"$diagnostic" 2>&1; then
      return 1
    fi
    jq -e '
      (.Browser | type == "string")
      and ((.Browser | startswith("HeadlessChrome")) or (.Browser | startswith("Chrome")))
      and (.webSocketDebuggerUrl | type == "string")
      and ((.webSocketDebuggerUrl | startswith("ws://"))
        or (.webSocketDebuggerUrl | startswith("wss://")))
    ' "$diagnostic" >/dev/null 2>&1
  }

  if [ -n "${AXON_LIVE_CHROME_REMOTE_URL:-}" ]; then
    live_chrome_remote_url="$AXON_LIVE_CHROME_REMOTE_URL"
    if ! probe_live_chrome "$live_chrome_remote_url" "$OUTDIR/logs/chrome-explicit-probe.log"; then
      echo "explicit Chrome endpoint is not a reachable CDP endpoint: $live_chrome_remote_url" >&2
      exit 2
    fi
  elif probe_live_chrome "$configured_chrome_remote_url" "$OUTDIR/logs/chrome-configured-probe.log"; then
    live_chrome_remote_url="$configured_chrome_remote_url"
  elif probe_live_chrome "http://127.0.0.1:9222" "$OUTDIR/logs/chrome-fallback-probe.log"; then
    live_chrome_remote_url="http://127.0.0.1:9222"
  else
    live_chrome_port="${AXON_LIVE_CHROME_PORT:-0}"
    live_chrome_binary="$(command -v google-chrome || command -v chromium || command -v chromium-browser || true)"
    if [ -z "$live_chrome_binary" ]; then
      echo "live scenarios require Chrome; set AXON_LIVE_CHROME_REMOTE_URL or install Chrome/Chromium" >&2
      exit 2
    fi
    live_chrome_session_token="axon-live-chrome-${TS//[^0-9]/}-$$"
    AXON_LIVE_CHROME_SESSION_TOKEN="$live_chrome_session_token" \
      "$live_chrome_binary" --headless=new --no-sandbox --disable-gpu \
      --remote-debugging-address=127.0.0.1 --remote-debugging-port="$live_chrome_port" \
      --user-data-dir="$OUTDIR/chrome-profile" about:blank \
      >"$OUTDIR/logs/chrome.log" 2>"$OUTDIR/logs/chrome.stderr.log" &
    live_chrome_pid=$!
    live_chrome_start_time="$(awk '{print $22}' "/proc/$live_chrome_pid/stat" 2>/dev/null)"
    live_chrome_ready=0
    for _attempt in $(seq 1 60); do
      if [ "$live_chrome_port" = "0" ] && [ -s "$OUTDIR/chrome-profile/DevToolsActivePort" ]; then
        live_chrome_port="$(sed -n '1p' "$OUTDIR/chrome-profile/DevToolsActivePort")"
      fi
      live_chrome_remote_url="http://127.0.0.1:$live_chrome_port"
      if [ "$live_chrome_port" != "0" ] \
        && probe_live_chrome "$live_chrome_remote_url" "$OUTDIR/logs/chrome-owned-probe.log"; then
        live_chrome_ready=1
        break
      fi
      kill -0 "$live_chrome_pid" 2>/dev/null || break
      sleep 0.25
    done
    if [ "$live_chrome_ready" -ne 1 ]; then
      echo "harness-owned Chrome did not become ready at $live_chrome_remote_url" >&2
      exit 2
    fi
  fi
  export AXON_CHROME_REMOTE_URL="$live_chrome_remote_url"
  external_chrome_remote_url="${live_chrome_remote_url/127.0.0.1/host.docker.internal}"
  external_chrome_remote_url="${external_chrome_remote_url/localhost/host.docker.internal}"
  unset AXON_HOME AXON_SERVER_URL AXON_SQLITE_PATH AXON_OUTPUT_DIR \
    AXON_ARTIFACT_BIN_DIR AXON_ARTIFACT_ROOT AXON_CONFIG_PATH AXON_ENV_FILE
  export AXON_DATA_DIR="${AXON_LIVE_DATA_DIR:-$OUTDIR/data}"
  export AXON_COLLECTION="${AXON_LIVE_COLLECTION:-axon_live_${TS//[^0-9]/}}"
  if [[ "$AXON_COLLECTION" != axon_live_* ]]; then
    echo "isolated live collection must start with axon_live_: $AXON_COLLECTION" >&2
    exit 2
  fi
  isolated_collection="$AXON_COLLECTION"
  isolated_collections+=("$isolated_collection")
  export AXON_CONFIG_PATH="$OUTDIR/config.toml"
  export AXON_ENV_FILE="$OUTDIR/.env"
  mkdir -p "$AXON_DATA_DIR"
  install -m 0600 /dev/null "$AXON_CONFIG_PATH"
  install -m 0600 /dev/null "$AXON_ENV_FILE"
  "$AXON_BIN" config set jobs.auto-worker false --json >"$OUTDIR/logs/fixture-disable-auto-worker.json"
  "$AXON_BIN" config set jobs.worker-idle-exit-secs 2 --json >"$OUTDIR/logs/fixture-worker-idle.json"
  SETUP_HOME="$OUTDIR/setup-home"
  SETUP_HELPER_BIN="$OUTDIR/setup-helper-bin"
  mkdir -p "$SETUP_HOME" "$SETUP_HELPER_BIN"
  install -m 0755 /bin/true "$SETUP_HELPER_BIN/axon-palette-tauri"
  env -u AXON_HOME -u AXON_ENV_FILE -u AXON_CONFIG_PATH \
    HOME="$SETUP_HOME" AXON_DATA_DIR="$SETUP_HOME/.axon" \
    QDRANT_URL="${QDRANT_URL:-http://127.0.0.1:53333}" \
    TEI_URL="${TEI_URL:-http://127.0.0.1:52000}" \
    AXON_CHROME_REMOTE_URL="$live_chrome_remote_url" \
    "$AXON_BIN" setup init --mcp-host 127.0.0.1 --mcp-port 38133 --auth-mode bearer --json \
    >"$OUTDIR/logs/fixture-setup-init.json" 2>"$OUTDIR/logs/fixture-setup-init.stderr.log"
  {
    isolated_compose_project="axon-live-${TS//[^0-9]/}"
    isolated_compose_network="$isolated_compose_project"
    printf 'AXON_COMPOSE_PROJECT_NAME=%s\n' "$isolated_compose_project"
    printf 'AXON_CONTAINER_NAME=%s-axon\n' "$isolated_compose_project"
    printf 'AXON_QDRANT_CONTAINER_NAME=%s-qdrant\n' "$isolated_compose_project"
    printf 'AXON_TEI_CONTAINER_NAME=%s-tei\n' "$isolated_compose_project"
    printf 'AXON_CHROME_CONTAINER_NAME=%s-chrome\n' "$isolated_compose_project"
    printf 'DOCKER_NETWORK=%s\n' "$isolated_compose_network"
    printf 'AXON_CHROME_MANAGEMENT_PORT=38600\n'
    printf 'AXON_CHROME_CDP_PORT=39222\n'
    printf 'AXON_CHROME_DEVTOOLS_PORT=39223\n'
    printf 'TEI_HTTP_PORT=38200\n'
    printf 'AXON_EXTERNAL_QDRANT_URL=%s\n' "${QDRANT_URL:-http://127.0.0.1:53333}"
    printf 'AXON_EXTERNAL_TEI_URL=http://host.docker.internal:52000\n'
    printf 'AXON_EXTERNAL_CHROME_REMOTE_URL=%s\n' "$external_chrome_remote_url"
  } >>"$SETUP_HOME/.axon/.env"
  AXON_DATA_DIR="$SETUP_HOME/.axon" \
    AXON_CONFIG_PATH="$SETUP_HOME/.axon/config.toml" \
    AXON_ENV_FILE="$SETUP_HOME/.axon/.env" \
    "$AXON_BIN" config set AXON_HTTP_PUBLISH 38135 --env --json \
    >"$OUTDIR/logs/fixture-compose-port.json" \
    2>"$OUTDIR/logs/fixture-compose-port.stderr.log"
  if jq -e '.commands[] | select(.name | startswith("compose "))' "$REGISTRY" >/dev/null; then
    docker compose --env-file "$SETUP_HOME/.axon/.env" \
      -f "$SETUP_HOME/.axon/compose/docker-compose.yaml" \
      -f "$SETUP_HOME/.axon/compose/docker-compose.external-qdrant.yaml" \
      -f "$SETUP_HOME/.axon/compose/docker-compose.external-providers.yaml" \
      config --format json >"$OUTDIR/logs/fixture-compose-rendered.json"
    assert_live_json "compose isolated loopback port" \
      "$OUTDIR/logs/fixture-compose-rendered.json" \
      '.services.axon.ports
       | any(.target == 8001 and .published == "38135" and .host_ip == "127.0.0.1")'
  fi
  fixture_url="${AXON_LIVE_FIXTURE_URL:-https://example.com}"
  map_fixture_url="${AXON_LIVE_MAP_FIXTURE_URL:-https://www.rust-lang.org/}"
  main_data_dir="$AXON_DATA_DIR"
  watch_id=""
  extract_job_id=""
  job_id=""
  memory_id=""
  replacement_memory_id=""
  prune_plan_id=""
  artifact_fixture_id=""
  artifact_fixture_second_id=""
  upload_id=""
  abort_upload_id=""
  graph_node_id=""
  graph_edge_id=""

  while IFS= read -r name; do
    handle_live_web_rag_scenario "$name" && continue
    handle_live_jobs_memory_source_scenario "$name" && continue
    handle_live_admin_setup_scenario "$name" && continue
    handle_live_resources_graph_scenario "$name" && continue
    missing_live "$name" "no stateful live scenario is registered"
  done < <(jq -r '.commands[].name' "$REGISTRY")
fi
