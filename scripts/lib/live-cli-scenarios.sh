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

  # shellcheck disable=SC1091
  source "$ROOT_DIR/scripts/lib/axon-env.sh"
  load_axon_env_file "$ROOT_DIR"
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
  if ! run_live_provider_preflight; then
    exit 1
  fi
  SETUP_HOME="$OUTDIR/setup-home"
  SETUP_HELPER_BIN="$OUTDIR/setup-helper-bin"
  mkdir -p "$SETUP_HOME" "$SETUP_HELPER_BIN"
  install -m 0755 /bin/true "$SETUP_HELPER_BIN/axon-palette-tauri"
  env -u AXON_HOME -u AXON_ENV_FILE -u AXON_CONFIG_PATH \
    HOME="$SETUP_HOME" AXON_DATA_DIR="$SETUP_HOME/.axon" \
    QDRANT_URL="${QDRANT_URL:-http://127.0.0.1:53333}" \
    TEI_URL="${TEI_URL:-http://127.0.0.1:52000}" \
    AXON_CHROME_REMOTE_URL="${AXON_CHROME_REMOTE_URL:-http://127.0.0.1:6000}" \
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
    printf 'AXON_EXTERNAL_CHROME_REMOTE_URL=http://host.docker.internal:6000\n'
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
  graph_fixture_url="$fixture_url"
  main_data_dir="$AXON_DATA_DIR"
  watch_id=""
  extract_job_id=""
  job_id=""
  memory_id=""
  replacement_memory_id=""
  prune_plan_id=""
  screenshot_artifact_id=""
  upload_id=""
  abort_upload_id=""
  graph_node_id=""
  graph_edge_id=""

  if [ "$SCENARIO_GROUP" = "resources" ]; then
    graph_fixture_url="$map_fixture_url"
    run_fixture_json "resource source prerequisite" \
      "$OUTDIR/logs/fixture-resource-source.json" \
      source "$graph_fixture_url" --scope site --max-pages 3 --wait true \
      --collection "$AXON_COLLECTION" --json || true
    job_id="$(jq -r '.job_id // .job.id // empty' "$OUTDIR/logs/fixture-resource-source.json")"
    source_id="$(jq -r '.source_id // empty' "$OUTDIR/logs/fixture-resource-source.json")"
    run_fixture_json "resource screenshot prerequisite" \
      "$OUTDIR/logs/fixture-resource-screenshot.json" \
      screenshot "$fixture_url" --output "$OUTDIR/screenshot.png" \
      --screenshot-full-page false --json || true
    screenshot_artifact_id="$(jq -r '.artifact_id // empty' "$OUTDIR/logs/fixture-resource-screenshot.json")"
  fi

  while IFS= read -r name; do
    case "$SCENARIO_GROUP" in
      web-rag) handle_live_web_rag_scenario "$name" || true ;;
      jobs-source) handle_live_jobs_memory_source_scenario "$name" || true ;;
      admin) handle_live_admin_setup_scenario "$name" || true ;;
      resources) handle_live_resources_graph_scenario "$name" || true ;;
      all)
        handle_live_web_rag_scenario "$name" && continue
        handle_live_jobs_memory_source_scenario "$name" && continue
        handle_live_admin_setup_scenario "$name" && continue
        handle_live_resources_graph_scenario "$name" && continue
        missing_live "$name" "no stateful live scenario is registered"
        ;;
    esac
  done < <(jq -r '.commands[].name' "$REGISTRY")

  if [ "$SCENARIO_GROUP" = "all" ] || [ "$SCENARIO_GROUP" = "jobs-source" ]; then
    run_operator_output_contracts
  fi
fi
