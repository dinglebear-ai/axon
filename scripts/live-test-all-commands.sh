#!/usr/bin/env bash
set -uo pipefail

# Registry-driven Axon CLI smoke harness.
#
# Registry mode is parser coverage only: every advertised command and option is
# invoked through `--help`, proving only that the parser recognizes each option
# token. It does not prove values, conflicts, or runtime behavior. Scenario mode executes
# behavioral contracts for every advertised command against configured
# providers. Live mode runs both sets.

ROOT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
MODE="live"
while [ "$#" -gt 0 ]; do
  case "$1" in
    --mode)
      MODE="${2:-}"
      shift 2
      ;;
    -h|--help)
      echo "usage: $0 [--mode registry|scenarios|live]"
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      exit 2
      ;;
  esac
done
case "$MODE" in
  registry|scenarios|live) ;;
  *)
    echo "invalid mode '$MODE' (expected registry, scenarios, or live)" >&2
    exit 2
    ;;
esac

AXON_BIN="${AXON_BIN:-$ROOT_DIR/target/debug/axon}"
REGISTRY="${AXON_COMMAND_REGISTRY:-$ROOT_DIR/docs/reference/cli/commands.json}"
TIMEOUT_SECS="${AXON_LIVE_COMMAND_TIMEOUT_SECS:-120}"
TS="$(date +%Y%m%d-%H%M%S)"
OUTDIR="${AXON_LIVE_TEST_OUTDIR:-$ROOT_DIR/.cache/live-test/$TS}"
REPORT="$OUTDIR/report.tsv"
BEHAVIOR_REPORT="$OUTDIR/behavioral-coverage.tsv"
mkdir -p "$OUTDIR/logs"

command -v jq >/dev/null 2>&1 || {
  echo "jq is required" >&2
  exit 2
}
[ -x "$AXON_BIN" ] || {
  echo "Axon binary is not executable: $AXON_BIN" >&2
  exit 2
}
jq -e '.commands | type == "array"' "$REGISTRY" >/dev/null || {
  echo "invalid command registry: $REGISTRY" >&2
  exit 2
}

printf 'command\tphase\tresult\texit\tinvocation\tlog\n' > "$REPORT"
printf 'command\toption\tresult\tevidence\n' >"$BEHAVIOR_REPORT"
BEHAVIOR_ACTUAL="$OUTDIR/behavioral-actual.tsv"
BEHAVIOR_SEMANTIC="$OUTDIR/behavioral-semantic.tsv"
BEHAVIOR_EXPECTED="$OUTDIR/behavioral-expected.tsv"
: >"$BEHAVIOR_ACTUAL"
: >"$BEHAVIOR_SEMANTIC"
LAST_BEHAVIOR_NAME=""
LAST_BEHAVIOR_ARGS=()
declare -A LIVE_LOG_COUNTS=()
failures=0
isolated_collection=""
isolated_collections=()
isolated_compose_project=""
isolated_compose_network=""

cleanup_live_fixtures() {
  local collection
  if [[ "$isolated_compose_project" == axon-live-* ]] \
    && [[ "$isolated_compose_network" == axon-live-* ]] \
    && [ -f "${SETUP_HOME:-}/.axon/.env" ] \
    && [ -f "${SETUP_HOME:-}/.axon/compose/docker-compose.yaml" ]; then
    docker compose --env-file "$SETUP_HOME/.axon/.env" \
      -f "$SETUP_HOME/.axon/compose/docker-compose.yaml" \
      -f "$SETUP_HOME/.axon/compose/docker-compose.external-qdrant.yaml" \
      -f "$SETUP_HOME/.axon/compose/docker-compose.external-providers.yaml" \
      down --remove-orphans \
      >"$OUTDIR/logs/cleanup-compose.log" 2>"$OUTDIR/logs/cleanup-compose.stderr.log" || true
    docker network rm "$isolated_compose_network" \
      >"$OUTDIR/logs/cleanup-network.log" 2>"$OUTDIR/logs/cleanup-network.stderr.log" || true
  fi
  for collection in "${isolated_collections[@]}"; do
    if [[ "$collection" == axon_live_* ]] && [ -n "${QDRANT_URL:-}" ]; then
      curl -fsS -X DELETE \
        "${QDRANT_URL%/}/collections/$collection" \
        >>"$OUTDIR/logs/cleanup-qdrant.json" 2>>"$OUTDIR/logs/cleanup-qdrant.stderr.log" || true
    fi
  done
}
trap cleanup_live_fixtures EXIT

record() {
  local first=1 field
  for field in "$@"; do
    field="${field//$'\t'/ }"
    field="${field//$'\r'/ }"
    field="${field//$'\n'/ }"
    if [ "$first" -eq 1 ]; then
      first=0
    else
      printf '\t' >>"$REPORT"
    fi
    printf '%s' "$field" >>"$REPORT"
  done
  printf '\n' >>"$REPORT"
}

record_behavior_args() {
  local name="$1" arg option
  shift
  printf '%s\t%s\n' "$name" "__command__" >>"$BEHAVIOR_ACTUAL"
  for arg in "$@"; do
    case "$arg" in
      --*)
        option="${arg%%=*}"
        printf '%s\t%s\n' "$name" "$option" >>"$BEHAVIOR_ACTUAL"
        ;;
      -[A-Za-z])
        printf '%s\t%s\n' "$name" "$arg" >>"$BEHAVIOR_ACTUAL"
        ;;
    esac
  done
}

set_pending_behavior() {
  LAST_BEHAVIOR_NAME="$1"
  shift
  LAST_BEHAVIOR_ARGS=("$@")
}

confirm_pending_behavior() {
  if [ -n "$LAST_BEHAVIOR_NAME" ]; then
    record_behavior_args "$LAST_BEHAVIOR_NAME" "${LAST_BEHAVIOR_ARGS[@]}"
    LAST_BEHAVIOR_NAME=""
    LAST_BEHAVIOR_ARGS=()
  fi
}

prove_option_behavior() {
  local name="$1" option="$2" evidence="$3" key
  key="$(printf '%s\t%s' "$name" "$option")"
  if { [ "$name" = "@global" ] \
      && awk -F '\t' -v option="$option" '$2 == option { found=1 } END { exit !found }' "$BEHAVIOR_ACTUAL"; } \
    || grep -Fqx -- "$key" "$BEHAVIOR_ACTUAL"; then
    printf '%s\t%s\t%s\n' "$name" "$option" "$evidence" >>"$BEHAVIOR_SEMANTIC"
  fi
}

prove_options() {
  local name="$1" evidence="$2" option
  shift 2
  for option in "$@"; do
    prove_option_behavior "$name" "$option" "$evidence"
  done
}

prove_mapped_command_options() {
  local name="$1"
  case "$name" in
    "watch create"|"watch update") prove_options "$name" "watch schedule persisted and was read back by lifecycle commands" --every-seconds ;;
    "map") prove_options "@global" \
      "map returned bounded sitemap results with the requested path exclusions" \
      --max-pages --sitemap-only --exclude-path-prefix ;;
    "extract") prove_options "@global" "detached extraction returned a durable job descriptor" --wait ;;
    "search") prove_options "@global" "search returned a bounded, time-filtered result collection" --limit --search-time-range ;;
    "research") prove_options "@global" "research returned synthesis and source results at requested depth" --research-depth ;;
    "scrape")
      prove_options "$name" "one-page projection returned inline content while explicitly skipping embeddings" --inline --no-embed
      prove_options "@global" "one-page source honored wait, collection, and isolated output directory" --wait --collection --output-dir
      ;;
    "query")
      prove_options "$name" "query emitted retrieval diagnostics for the bounded result" --diagnostics
      prove_options "@global" \
        "query used explicit text, provider URLs, temporal bounds, dense-only mode, and script-safe output" \
        --limit --collection --query --tei-url --qdrant-url --since --before \
        --no-hybrid-search --color --quiet
      ;;
    "retrieve")
      prove_options "$name" "retrieve returned bounded stored chunks for the requested URL" --max-points
      prove_options "@global" "retrieve used the isolated collection" --collection
      ;;
    "ask")
      prove_options "$name" \
        "ask exercised diagnostic, explanation, stream control, and named session lifecycle modes" \
        --diagnostics --explain --stream --no-stream --session --reset-session \
        --new-session --list-sessions --resume
      prove_options "@global" "grounded answer returned bounded citations from isolated collection" --limit --collection
      ;;
    "summarize"|"suggest") prove_options "@global" "RAG operation completed against isolated collection" --collection ;;
    "evaluate")
      prove_options "$name" \
        "evaluation returned comparison responses plus diagnostic and retrieval A/B projections" \
        --responses-mode --diagnostics --retrieval-ab
      prove_options "@global" "evaluation used isolated collection" --collection
      ;;
    "train")
      prove_options "$name" "training event persisted selected preference and notes" --best --notes
      prove_options "@global" "training event used isolated collection" --collection
      ;;
    "jobs events") prove_options "$name" "event page honored sequence and cursor bounds" --after-sequence --cursor ;;
    "jobs stream") prove_options "$name" "stream page honored sequence bound" --after-sequence ;;
    "jobs cancel") prove_options "$name" "cancellation persisted the caller supplied reason" --reason ;;
    "jobs retry") prove_options "$name" "retry response linked original and new attempts" --mode ;;
    "jobs recover") prove_options "$name" "recovery applied the stale cutoff and requested job kind" --stale-before --kind ;;
    "jobs cleanup") prove_options "$name" "cleanup applied terminal status, job kind, age, and preview policy" --status --kind --older-than --dry-run ;;
    "jobs clear") prove_options "$name" "clear returned an explicit deletion count" --confirm ;;
    "jobs worker") prove_options "$name" "worker exited cleanly at the requested idle bound" --idle-exit-secs ;;
    "memory remember")
      prove_options "$name" \
        "memory persisted the requested title, project, type, repository, file, and confidence metadata" \
        --title --project --type --repo --file --confidence
      ;;
    "memory search") prove_options "$name" "memory search applied project, repository, and file filters" --project --repo --file ;;
    "memory link") prove_options "$name" "memory edge persisted requested relationship type" --type ;;
    "memory context")
      prove_options "$name" "memory context applied project, repository, file, result, and token bounds" \
        --project --repo --file --token-budget
      ;;
    "sessions") prove_options "$name" "session scan returned only the requested provider/project projection" --claude --codex --gemini --project ;;
    "sources") prove_options "$name" "source inventory applied the explicit all/domain projection" --all --domain ;;
    "domains") prove_options "$name" "domain inventory applied the requested domain filter" --domain ;;
    "endpoints")
      prove_options "$name" \
        "endpoint discovery completed with bundle, uniqueness, scan-bound, verification, network, and RPC controls" \
        --include-bundles --first-party-only --unique-only --max-scripts \
        --max-scan-bytes --verify --capture-network --probe-rpc --probe-rpc-subdomains
      ;;
    "prune exec") prove_options "$name" "reviewed generation-scoped prune plan executed only after confirmation" --generation --confirm ;;
    "setup plugin-hook"|"setup hook alias") prove_options "$name" "hook-safe setup returned explicit no-setup exit policy" --no-setup ;;
    "setup config rewrite") prove_options "$name" "config rewrite preview completed without modifying isolated files" --dry-run ;;
    "setup init")
      prove_options "$name" \
        "isolated setup persisted bind, auth, OAuth, and optional provider credentials" \
        --mcp-host --mcp-port --auth-mode --mcp-token --oauth-public-url \
        --google-client-id --google-client-secret --auth-admin-email \
        --tavily-api-key --github-token --reddit-client-id --reddit-client-secret
      ;;
    "migrate") prove_options "$name" "named collection migration reported source, destination, and migrated points" --from --to ;;
    "config list"|"config get") prove_options "$name" "isolated config read honored storage layer and reveal policy" --env --toml --reveal ;;
    "config set"|"config unset") prove_options "$name" "isolated config mutation honored the explicit storage layer" --env --toml ;;
    "update") prove_options "$name" "forced version-pinned update used the requested repository and isolated destination without container sync" --repo --version --force --no-container ;;
    "palette") prove_options "$name" "palette install resolved requested acquisition method in isolated bin dir" --method ;;
    "artifacts get") prove_options "$name" "artifact detail included the requested authenticated content URL" --include-content-url ;;
    "artifacts content")
      prove_options "$name" "artifact content returned the requested byte range and default download" --range --download
      prove_options "@global" "artifact bytes were copied to the requested isolated output" --output
      ;;
    "artifacts list") prove_options "$name" "artifact inventory applied kind, source, job, and cursor filters" --kind --source-id --job-id --cursor ;;
    "uploads create") prove_options "$name" "upload staging record persisted purpose and source hint" --purpose --source-hint ;;
    "uploads complete") prove_options "$name" "upload completion verified the digest and produced an artifact/source reference" --sha256 --source-option ;;
    "collections get")
      prove_options "$name" "collection detail included schema and payload-index projections" --include-schema --include-indexes
      prove_options "@global" "collection detail matched isolated collection" --collection
      ;;
    "graph resolve")
      prove_options "$name" "graph identifier resolution applied the requested node kind" --kind
      prove_options "@global" "graph resolution returned bounded resolved/missing sets" --limit
      ;;
    "graph query")
      prove_options "$name" "graph traversal continued from the returned edge cursor" --cursor
      prove_options "@global" "graph query returned bounded nodes and edges" --limit
      ;;
    "graph node") prove_options "$name" "node detail included requested edges and evidence" --include-edges --include-evidence ;;
    "graph edge") prove_options "$name" "edge detail included evidence" --include-evidence ;;
    "graph source") prove_options "$name" "source traversal honored depth and edge-kind filter" --depth --edge-kind ;;
    "providers get") prove_options "$name" "provider detail included health and limits" --include-health --include-limits ;;
    "status")
      prove_options "@global" \
        "status returned a valid isolated job projection with the requested presentation and state filter" \
        --active --recent --reclaimed --color --quiet
      ;;
    "jobs list")
      prove_options "$name" "job listing applied status, kind, and cursor pagination filters" \
        --status --kind --cursor
      ;;
    "memory list")
      prove_options "$name" "memory list applied project, repository, file, type, and status filters" \
        --project --repo --file --type --status
      ;;
    "preflight") prove_options "$name" "config-only preflight completed without provider probes" --config ;;
    "providers list") prove_options "$name" "provider inventory applied kind and health-status filters" --kind --status ;;
    "prune plan") prove_options "$name" "prune plan remained scoped to the requested generation" --generation ;;
    "uploads list") prove_options "$name" "upload inventory applied status and cursor pagination filters" --status --cursor ;;
    "uploads abort") prove_options "$name" "upload abort receipt retained the caller supplied reason" --reason ;;
  esac
}

run_help_path() {
  local encoded="$1" name path_json logfile exit_code result
  name="$(printf '%s' "$encoded" | base64 --decode | jq -r '.name')"
  path_json="$(printf '%s' "$encoded" | base64 --decode | jq -c '.path')"
  logfile="$OUTDIR/logs/help-$(printf '%s' "$name" | tr ' /' '__').log"
  mapfile -t path < <(printf '%s' "$path_json" | jq -r '.[]')

  timeout "${TIMEOUT_SECS}s" "$AXON_BIN" "${path[@]}" --help >"$logfile" 2>&1
  exit_code=$?
  if [ "$exit_code" -eq 0 ] && grep -Fq "axon ${path[*]}" "$logfile"; then
    result="PASS"
  else
    result="FAIL"
    failures=$((failures + 1))
  fi
  record "$name" "parser-command" "$result" "$exit_code" "${path[*]} --help" "$logfile"
}

run_root_help() {
  local logfile="$OUTDIR/logs/help-root.log" exit_code result
  timeout "${TIMEOUT_SECS}s" "$AXON_BIN" --help >"$logfile" 2>&1
  exit_code=$?
  if [ "$exit_code" -eq 0 ]; then
    result="PASS"
  else
    result="FAIL"
    failures=$((failures + 1))
  fi
  record "<root>" "parser-command" "$result" "$exit_code" "--help" "$logfile"
}

option_value() {
  local command_name="$1" option="$2" value_name="${3^^}"
  case "$option" in
    --wait|--cache|--include-subdomains|--normalize|--block-assets|--chrome-screenshot|--screenshot-full-page)
      echo "true"
      return
      ;;
    --performance-profile) echo "high-stable"; return ;;
    --render-mode) echo "http"; return ;;
    --format) echo "json"; return ;;
    --color) echo "never"; return ;;
    --search-time-range) echo "day"; return ;;
    --responses-mode) echo "side-by-side"; return ;;
    --transport) echo "stdio"; return ;;
    --method) echo "build"; return ;;
    --auth-mode) echo "bearer"; return ;;
    --stale-before|--older-than) echo "2026-01-01T00:00:00Z"; return ;;
    --purpose) echo "source_artifact"; return ;;
    --shell) echo "bash"; return ;;
    --mode)
      case "$command_name" in
        "jobs retry") echo "same_config" ;;
        *) echo "http" ;;
      esac
      return
      ;;
  esac
  case "$value_name" in
    *BOOL*) echo "true" ;;
    *URL*) echo "http://127.0.0.1:8001" ;;
    *DATE*|*SINCE*) echo "2026-01-01" ;;
    *BEFORE*) echo "2026-01-01T00:00:00Z" ;;
    *HEADER*) echo "X-Axon-Smoke: enabled" ;;
    *COLLECTION*) echo "axon" ;;
    *TRANSPORT*) echo "stdio" ;;
    *SHELL*) echo "bash" ;;
    *COLOR*) echo "never" ;;
    *RENDER_MODE*) echo "http" ;;
    *RESPONSES_MODE*) echo "side-by-side" ;;
    *PROFILE*) echo "high-stable" ;;
    *SEARCH_TIME_RANGE*|*RANGE*) echo "day" ;;
    *STATUS*) echo "completed" ;;
    *KIND*) echo "source" ;;
    *SCOPE*) echo "page" ;;
    *FORMAT*) echo "json" ;;
    *PATH=N*) echo "*=1" ;;
    *VIEWPORT*) echo "1280x720" ;;
    *SELECTOR*) echo "body" ;;
    *PATTERN*) echo "https://example.com/*" ;;
    *FILE*|*PATH*|*DIR*) echo "/tmp/axon-live-cli-contract" ;;
    *EMAIL*) echo "smoke@example.com" ;;
    *HOST*) echo "127.0.0.1" ;;
    *PORT*) echo "38001" ;;
    *ID*) echo "00000000-0000-0000-0000-000000000000" ;;
    *TEXT*|*QUERY*|*NAME*|*NOTES*) echo "smoke" ;;
    *TOKEN*|*SECRET*|*KEY*) echo "axon-live-smoke-secret" ;;
    *SUBCOMMAND*) echo "launch" ;;
    *) echo "1" ;;
  esac
}

run_option_probe() {
  local name="$1" option="$2" value_name="$3"
  shift 3
  local logfile exit_code result invocation
  local -a path=("$@")
  logfile="$OUTDIR/logs/option-$(printf '%s-%s' "$name" "$option" | tr ' /' '___').log"
  invocation="${path[*]} $option"
  if [ -n "$value_name" ]; then
    local value
    value="$(option_value "$name" "$option" "$value_name")"
    timeout "${TIMEOUT_SECS}s" "$AXON_BIN" "${path[@]}" "$option" "$value" --help >"$logfile" 2>&1
    invocation="$invocation $value --help"
  else
    timeout "${TIMEOUT_SECS}s" "$AXON_BIN" "${path[@]}" "$option" --help >"$logfile" 2>&1
    invocation="$invocation --help"
  fi
  exit_code=$?
  if [ "$exit_code" -eq 0 ]; then
    result="PASS"
  else
    result="FAIL"
    failures=$((failures + 1))
  fi
  record "$name" "parser-option" "$result" "$exit_code" "$invocation" "$logfile"
}

probe_help_options() {
  local name="$1" logfile="$2"
  shift 2
  local -a path=("$@")
  local spec long short value_name
  while IFS= read -r spec; do
    long="$(grep -oE -- '--[a-z0-9][a-z0-9-]*' <<<"$spec" | head -1 || true)"
    short="$(grep -oE -- '(^|, )-[A-Za-z](,| |$)' <<<"$spec" | grep -oE -- '-[A-Za-z]' | head -1 || true)"
    value_name="$(grep -oE '<[^>]+>' <<<"$spec" | head -1 | tr -d '<>' || true)"
    if [ -n "$long" ] && [ "$long" != "--help" ]; then
      run_option_probe "$name" "$long" "$value_name" "${path[@]}"
    fi
    if [ -n "$short" ] && [ "$short" != "-h" ]; then
      run_option_probe "$name" "$short" "$value_name" "${path[@]}"
    fi
  done < <(
    awk '
      /^Options$/ { in_options=1; next }
      in_options && /^  -/ {
        line=$0
        sub(/^  /, "", line)
        sub(/[[:space:]][[:space:]]+.*/, "", line)
        print line
      }
      in_options && /^[^ ]/ { in_options=0 }
    ' "$logfile"
  )
}

if [ "$MODE" != "scenarios" ]; then
  run_root_help
  probe_help_options "<root>" "$OUTDIR/logs/help-root.log"

  while IFS= read -r encoded; do
    run_help_path "$encoded"
    name="$(printf '%s' "$encoded" | base64 --decode | jq -r '.name')"
    path_json="$(printf '%s' "$encoded" | base64 --decode | jq -c '.path')"
    mapfile -t path < <(printf '%s' "$path_json" | jq -r '.[]')
    help_log="$OUTDIR/logs/help-$(printf '%s' "$name" | tr ' /' '__').log"
    probe_help_options "$name" "$help_log" "${path[@]}"
  done < <(jq -r '.commands[] | @base64' "$REGISTRY")
fi

probe_hidden_global_options() {
  run_option_probe "<root>" "--automation-script" "PATH"
  run_option_probe "<root>" "--batch-concurrency" "N"
  run_option_probe "<root>" "--block-assets" "BOOL"
  run_option_probe "<root>" "--budget" "PATH=N"
  run_option_probe "<root>" "--cache" "BOOL"
  run_option_probe "<root>" "--cache-http-only" ""
  run_option_probe "<root>" "--chrome-screenshot" "BOOL"
  run_option_probe "<root>" "--chrome-wait-for-selector" "SELECTOR"
  run_option_probe "<root>" "--color" "COLOR"
  run_option_probe "<root>" "--cron-every-seconds" "N"
  run_option_probe "<root>" "--cron-max-runs" "N"
  run_option_probe "<root>" "--etag-conditional" ""
  run_option_probe "<root>" "--exclude-path" "PATH"
  run_option_probe "<root>" "--exclude-path-prefix" "PATH"
  run_option_probe "<root>" "--exclude-selector" "SELECTOR"
  run_option_probe "<root>" "--format" "FORMAT"
  run_option_probe "<root>" "--normalize" "BOOL"
  run_option_probe "<root>" "--output-dir" "DIR"
  run_option_probe "<root>" "--performance-profile" "PROFILE"
  run_option_probe "<root>" "--quiet" ""
  run_option_probe "<root>" "--root-selector" "SELECTOR"
  run_option_probe "<root>" "--screenshot-full-page" "BOOL"
  run_option_probe "<root>" "--sitemap-only" ""
  run_option_probe "<root>" "--url-glob" "PATTERN"
  run_option_probe "<root>" "--urls" "URLS"
  run_option_probe "<root>" "--viewport" "VIEWPORT"
  run_option_probe "<root>" "--warc" "PATH"
  run_option_probe "<root>" "--yes" ""
}

if [ "$MODE" != "scenarios" ]; then
  probe_hidden_global_options
  run_option_probe "ask" "--continue" "" ask
fi

probe_command_alias() {
  local name="$1"
  shift
  local logfile
  local exit_code result
  logfile="$OUTDIR/logs/alias-$(printf '%s' "$name" | tr ' /' '__').log"
  timeout "${TIMEOUT_SECS}s" "$AXON_BIN" "$@" --help >"$logfile" 2>&1
  exit_code=$?
  if [ "$exit_code" -eq 0 ]; then
    result="PASS"
  else
    result="FAIL"
    failures=$((failures + 1))
  fi
  record "$name" "parser-alias" "$result" "$exit_code" "$* --help" "$logfile"
}

if [ "$MODE" != "scenarios" ]; then
  probe_command_alias "completions" completion
  probe_command_alias "setup plugin-hook" setup hook
fi

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
    "scrape"|"source") echo '.status == "completed" and (.job_id | type) == "string"' ;;
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

run_live() {
  local name="$1"
  shift
  local logfile stderr_log exit_code result json_expected=0 arg contract_filter
  for arg in "$@"; do
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
  timeout "${TIMEOUT_SECS}s" "$AXON_BIN" "$@" >"$logfile" 2>"$stderr_log"
  exit_code=$?
  if [ "$exit_code" -eq 0 ] && { [ "$json_expected" -eq 0 ] || jq -e . "$logfile" >/dev/null 2>&1; }; then
    result="PASS"
  else
    result="FAIL"
    failures=$((failures + 1))
  fi
  if [ "$result" = "PASS" ]; then
    set_pending_behavior "$name" "$@"
    if contract_filter="$(command_contract_filter "$name" 2>/dev/null)"; then
      assert_live_json "$name semantic result" "$logfile" "$contract_filter"
      prove_mapped_command_options "$name"
    fi
  fi
  record "$name" "live" "$result" "$exit_code" "$*" "$logfile"
}

run_live_expect_failure() {
  local name="$1"
  shift
  local logfile stderr_log exit_code result
  local log_slug log_count log_suffix=""
  log_slug="$(printf '%s' "$name" | tr ' /' '__')"
  log_count=$((${LIVE_LOG_COUNTS[$log_slug]:-0} + 1))
  LIVE_LOG_COUNTS["$log_slug"]="$log_count"
  [ "$log_count" -gt 1 ] && log_suffix="-$log_count"
  logfile="$OUTDIR/logs/live-${log_slug}${log_suffix}.log"
  stderr_log="${logfile%.log}.stderr.log"
  LAST_LIVE_LOG="$logfile"
  timeout "${TIMEOUT_SECS}s" "$AXON_BIN" "$@" >"$logfile" 2>"$stderr_log"
  exit_code=$?
  if [ "$exit_code" -ne 0 ] && [ "$exit_code" -ne 124 ]; then
    result="PASS"
  else
    result="FAIL"
    failures=$((failures + 1))
  fi
  if [ "$result" = "PASS" ]; then
    set_pending_behavior "$name" "$@"
  fi
  record "$name" "live-negative" "$result" "$exit_code" "$*" "$logfile"
}

run_live_monitor_jsonl() {
  local name="$1" logfile stderr_log exit_code result monitor_pid
  logfile="$OUTDIR/logs/live-$(printf '%s' "$name" | tr ' /' '__').log"
  stderr_log="${logfile%.log}.stderr.log"
  LAST_LIVE_LOG="$logfile"
  printf '{"initialized":true,"statuses":{}}\n' >"$OUTDIR/monitor-watch-state.json"
  timeout 12s "$AXON_BIN" monitor jobs --watch --jsonl --interval-secs 1 \
    --state-file "$OUTDIR/monitor-watch-state.json" --json \
    >"$logfile" 2>"$stderr_log" &
  monitor_pid=$!
  sleep 1
  "$AXON_BIN" source "$map_fixture_url?monitor=$$" --scope map --wait true \
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
    record_behavior_args "monitor jobs" \
      monitor jobs --watch --jsonl --interval-secs 1 \
      --state-file "$OUTDIR/monitor-watch-state.json" --json
    prove_option_behavior "@global" "--watch" "bounded stream emitted lifecycle events"
    prove_option_behavior "@global" "--json" "every streamed stdout line parsed as JSON"
    prove_option_behavior "monitor jobs" "--jsonl" "stdout parsed as a non-empty JSONL stream"
    prove_option_behavior "monitor jobs" "--interval-secs" "monitor detected a controlled transition on the configured poll interval"
    prove_option_behavior "monitor jobs" "--state-file" "monitor used the isolated initialized state file"
  fi
  record "$name" "live-stream" "$result" "$exit_code" \
    "monitor jobs --watch --jsonl --interval-secs 1 --json" "$logfile"
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

run_live_server() {
  local name="$1" port="$2"
  shift 2
  local logfile stderr_log pid ready=0 protocol_ok=0 auth_ok=0 exit_code result _attempt
  local mcp_config mcp_schema mcp_call token
  logfile="$OUTDIR/logs/live-$(printf '%s' "$name" | tr ' /' '__').log"
  stderr_log="${logfile%.log}.stderr.log"
  LAST_LIVE_LOG="$logfile"
  token="axon-live-server-token-$port"
  AXON_HTTP_HOST=127.0.0.1 AXON_HTTP_PORT="$port" AXON_BIND=127.0.0.1 \
    AXON_HTTP_TOKEN="$token" \
    "$AXON_BIN" "$@" >"$logfile" 2>"$stderr_log" &
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
    record_behavior_args "$name" "$@"
    prove_option_behavior "$name" "--transport" \
      "HTTP transport completed initialize, tools/list, tool call, and auth rejection"
    record "$name MCP protocol" "contract" "PASS" "0" \
      "initialize + tools/list + capabilities tool call + unauthorized rejection" "$mcp_call"
  else
    result="FAIL"
    failures=$((failures + 1))
  fi
  record "$name" "live" "$result" "$exit_code" "$*" "$logfile"
}

run_live_setup_home() {
  local name="$1"
  shift
  local logfile stderr_log exit_code result json_expected=0 arg contract_filter
  for arg in "$@"; do
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
    TEI_HTTP_PORT=38200 AXON_CHROME_MANAGEMENT_PORT=38600 \
    AXON_CHROME_CDP_PORT=39222 AXON_CHROME_DEVTOOLS_PORT=39223 \
    PATH="$SETUP_HELPER_BIN:$PATH" \
    "$AXON_BIN" "$@" >"$logfile" 2>"$stderr_log"
  exit_code=$?
  if [ "$exit_code" -eq 0 ] && { [ "$json_expected" -eq 0 ] || jq -e . "$logfile" >/dev/null 2>&1; }; then
    result="PASS"
  else
    result="FAIL"
    failures=$((failures + 1))
  fi
  if [ "$result" = "PASS" ]; then
    set_pending_behavior "$name" "$@"
    if contract_filter="$(command_contract_filter "$name" 2>/dev/null)"; then
      assert_live_json "$name semantic result" "$logfile" "$contract_filter"
      prove_mapped_command_options "$name"
    fi
  fi
  record "$name" "live" "$result" "$exit_code" "$*" "$logfile"
}

run_live_setup_check() {
  local pid ready=0 _attempt
  AXON_HTTP_HOST=127.0.0.1 AXON_HTTP_PORT=38133 AXON_BIND=127.0.0.1 \
    "$AXON_BIN" serve >"$OUTDIR/logs/setup-check-server.log" \
    2>"$OUTDIR/logs/setup-check-server.stderr.log" &
  pid=$!
  for _attempt in $(seq 1 60); do
    if curl -fsS --max-time 1 "http://127.0.0.1:38133/readyz" >/dev/null 2>&1; then
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

if [ "$MODE" = "live" ] || [ "$MODE" = "scenarios" ]; then
  if [ "${AXON_LIVE_USE_PRODUCTION_STATE:-0}" = "1" ]; then
    echo "production-state live scenarios are forbidden; use the harness-owned isolated state" >&2
    exit 2
  fi
  isolated_bin_dir="$OUTDIR/isolated-bin"
  mkdir -p "$isolated_bin_dir"
  HARNESS_SOURCE_BIN="$AXON_BIN"
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
    "$AXON_BIN" config set AXON_HTTP_PUBLISH 127.0.0.1:38135 --env --json \
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
  screenshot_artifact_id=""
  upload_id=""
  abort_upload_id=""
  graph_node_id=""
  graph_edge_id=""

  while IFS= read -r name; do
    case "$name" in
      "watch create")
        run_live "$name" watch create "$fixture_url" --every-seconds 3600 --collection "$AXON_COLLECTION" --json
        watch_id="$(jq -r '.watch_id // empty' "$LAST_LIVE_LOG" 2>/dev/null)"
        ;;
      "watch list") run_live "$name" watch list --json ;;
      "watch get") run_live "$name" watch get "$watch_id" --json ;;
      "watch status") run_live "$name" watch status "$watch_id" --json ;;
      "watch update") run_live "$name" watch update "$watch_id" --every-seconds 7200 --collection "$AXON_COLLECTION" --json ;;
      "watch exec") run_live "$name" watch exec "$watch_id" --json ;;
      "watch pause") run_live "$name" watch pause "$watch_id" --json ;;
      "watch resume") run_live "$name" watch resume "$watch_id" --json ;;
      "watch delete")
        run_live "watch history" watch history "$watch_id" --json
        run_live "$name" watch delete "$watch_id" --json
        ;;
      "watch history") : ;;
      "monitor jobs")
        run_live "$name" monitor jobs --json
        run_live_monitor_jsonl "monitor jobs watch JSONL"
        ;;
      "map")
        run_live "$name" map "$map_fixture_url" --max-pages 10 --json
        assert_live_json "map discovered URLs" "$LAST_LIVE_LOG" \
          '.map_source != null and (.urls | length) > 0'
        prove_option_behavior "@global" "--max-pages" "map returned a bounded non-empty URL set"
        prove_option_behavior "@global" "--json" "map stdout parsed as the map result object"
        run_live "$name" map "https://gofastmcp.com/" --sitemap-only \
          --exclude-path-prefix /apps --max-pages 25 --json
        assert_live_json "map sitemap path exclusion" "$LAST_LIVE_LOG" \
          '.map_source != null and (.urls | length) > 0
           and all(.urls[]; (startswith("https://gofastmcp.com/apps") | not))'
        "$AXON_BIN" source "$map_fixture_url" --scope map --wait true \
          --collection "$AXON_COLLECTION" --json \
          >"$OUTDIR/logs/fixture-map-job.json" \
          2>"$OUTDIR/logs/fixture-map-job.stderr.log"
        map_job_id="$(jq -r '.job_id // .job.id // empty' "$OUTDIR/logs/fixture-map-job.json")"
        run_live "jobs get (map item-only)" jobs get "$map_job_id" --json
        assert_live_json "jobs get map item counts" "$LAST_LIVE_LOG" \
          '.counts.items_done > 0 and .counts.documents_done == 0 and .counts.chunks_done == 0'
        "$AXON_BIN" jobs get "$map_job_id" \
          >"$OUTDIR/logs/live-jobs_get-map-human.log" \
          2>"$OUTDIR/logs/live-jobs_get-map-human.stderr.log"
        assert_live_text "jobs get map item details" \
          "$OUTDIR/logs/live-jobs_get-map-human.log" " items · 100%"
        ;;
      "endpoints")
        run_live "$name" endpoints "$fixture_url" \
          --include-bundles true --first-party-only true --unique-only true \
          --max-scripts 2 --max-scan-bytes 1000000 --verify --capture-network \
          --probe-rpc --probe-rpc-subdomains --json
        ;;
      "extract")
        run_live "$name" extract "$fixture_url" --query "extract the page title" --wait false --json
        extract_job_id="$(jq -r '.job_id // .job.id // empty' "$LAST_LIVE_LOG" 2>/dev/null)"
        ;;
      "extract status") run_live "$name" extract status "$extract_job_id" --json ;;
      "extract cancel") run_live "$name" extract cancel "$extract_job_id" --json ;;
      "extract errors") run_live "$name" extract errors "$extract_job_id" --json ;;
      "extract list") run_live "$name" extract list --json ;;
      "extract cleanup") run_live "$name" extract cleanup --json ;;
      "extract clear") run_live "$name" extract clear --json ;;
      "extract worker") run_live "$name" extract worker --json ;;
      "extract recover") run_live "$name" extract recover --json ;;
      "search")
        run_live "$name" search "OpenAI GPT-5.4 release" \
          --limit 1 --search-time-range day --json
        assert_live_json "search recent time filter" "$LAST_LIVE_LOG" \
          '(.results | length) > 0'
        ;;
      "research")
        run_live "$name" research "FastMCP official documentation" \
          --research-depth 1 --json
        assert_live_json "research synthesized official documentation" "$LAST_LIVE_LOG" \
          '(.summary | type) == "string" and (.summary | length) > 0
           and (.search_results | length) > 0'
        ;;
      "scrape")
        run_live "$name" scrape "$fixture_url" --wait true --collection "$AXON_COLLECTION" --json
        run_live "$name" scrape "$fixture_url?no_embed=$$" --no-embed --inline \
          --output-dir "$OUTDIR/scrape-output" --wait true \
          --collection "$AXON_COLLECTION" --json
        assert_live_json "scrape no-embed inline projection" "$LAST_LIVE_LOG" \
          '.status == "completed" and .vector_points_written == 0
           and (.inline.content.text | length) > 0'
        ;;
      "brand") run_live "$name" brand "$fixture_url" --json ;;
      "debug") run_live "$name" debug --json ;;
      "diff") run_live "$name" diff "$fixture_url" "https://www.iana.org/help/example-domains" --json ;;
      "doctor")
        run_live "$name" doctor --json
        assert_live_json "doctor providers healthy" "$LAST_LIVE_LOG" \
          '.all_ok == true'
        ;;
      "doctor diagnose") run_live "$name" doctor diagnose --json ;;
      "query")
        run_live "$name" query "Example Domain" --limit 2 --collection "$AXON_COLLECTION" --json
        assert_live_json "query returns ranked result" "$LAST_LIVE_LOG" \
          'select(.rank == 1 and .url != null and .snippet != null)'
        run_live "$name" query --query "Example Domain" --diagnostics \
          --no-hybrid-search --since 30d --before 2099-01-01 \
          --tei-url "$TEI_URL" --qdrant-url "$QDRANT_URL" \
          --limit 2 --collection "$AXON_COLLECTION" --color never --quiet --json
        assert_live_json "query explicit dense diagnostic projection" "$LAST_LIVE_LOG" \
          'select(.rank == 1 and .url != null and .snippet != null)'
        ;;
      "retrieve")
        run_live "$name" retrieve "$fixture_url" --max-points 10 --collection "$AXON_COLLECTION" --json
        assert_live_json "retrieve returns indexed content" "$LAST_LIVE_LOG" \
          '.chunks > 0 and (.content | length) > 0'
        ;;
      "ask")
        run_live "$name" ask "What is the Example Domain used for?" \
          --limit 2 --collection "$AXON_COLLECTION" --json
        assert_live_json "ask returns grounded answer" "$LAST_LIVE_LOG" \
          '(.answer | length) > 0 and (.citations | length) > 0'
        run_live "$name" ask "Restate that purpose in one sentence." \
          --continue --limit 2 --collection "$AXON_COLLECTION" --json
        assert_live_json "ask continue alias" "$LAST_LIVE_LOG" \
          '(.answer | length) > 0'
        prove_option_behavior "ask" "--continue" "follow-up answer was generated from prior session context"
        run_live "$name" ask "Restate that purpose in five words." \
          --follow-up --limit 2 --collection "$AXON_COLLECTION" --json
        assert_live_json "ask follow-up long flag" "$LAST_LIVE_LOG" \
          '(.answer | length) > 0'
        prove_option_behavior "ask" "--follow-up" "long follow-up flag generated a non-empty answer"
        run_live "$name" ask "Restate that purpose in three words." \
          -c --limit 2 --collection "$AXON_COLLECTION" --json
        assert_live_json "ask follow-up short flag" "$LAST_LIVE_LOG" \
          '(.answer | length) > 0'
        prove_option_behavior "ask" "-c" "short follow-up alias generated a non-empty answer"
        run_live "ask session create" ask "State the Example Domain purpose." \
          --session axon-live-session --new-session --no-stream \
          --limit 2 --collection "$AXON_COLLECTION" --json
        assert_live_json "ask named session create" "$LAST_LIVE_LOG" \
          '.session == "axon-live-session" and (.answer | length) > 0'
        record_behavior_args "ask" ask "State the Example Domain purpose." \
          --session axon-live-session --new-session --no-stream \
          --limit 2 --collection "$AXON_COLLECTION" --json
        run_live "ask session resume stream" ask "Repeat it more briefly." \
          --resume axon-live-session --stream \
          --limit 2 --collection "$AXON_COLLECTION"
        assert_live_text "ask resumed human stream" "$LAST_LIVE_LOG" "Assistant:"
        record_behavior_args "ask" ask "Repeat it more briefly." \
          --resume axon-live-session --stream \
          --limit 2 --collection "$AXON_COLLECTION"
        run_live "ask session reset" ask "Start over with the same factual answer." \
          --session axon-live-session --reset-session --no-stream \
          --limit 2 --collection "$AXON_COLLECTION" --json
        assert_live_json "ask named session reset" "$LAST_LIVE_LOG" \
          '.session == "axon-live-session" and (.answer | length) > 0'
        record_behavior_args "ask" ask "Start over with the same factual answer." \
          --session axon-live-session --reset-session --no-stream \
          --limit 2 --collection "$AXON_COLLECTION" --json
        run_live "ask explanation" ask "Explain the retrieval plan for Example Domain." \
          --diagnostics --explain --no-stream \
          --limit 2 --collection "$AXON_COLLECTION" --json
        assert_live_json "ask explanation diagnostics" "$LAST_LIVE_LOG" \
          '.answer == "" and (.diagnostics | type) == "object"
           and (.explain | type) == "object"
           and .explain.mode == "explain_only"'
        record_behavior_args "ask" ask "Explain the retrieval plan for Example Domain." \
          --diagnostics --explain --no-stream \
          --limit 2 --collection "$AXON_COLLECTION" --json
        run_live "ask list sessions" ask --list-sessions --json
        assert_live_json "ask list named session" "$LAST_LIVE_LOG" \
          'any(.[]; .name == "axon-live-session" and .turn_count > 0)'
        record_behavior_args "ask" ask --list-sessions --json
        prove_mapped_command_options "ask"
        ;;
      "summarize") run_live "$name" summarize "$fixture_url" --collection "$AXON_COLLECTION" --json ;;
      "evaluate") run_live "$name" evaluate "What is the Example Domain used for?" --diagnostics --retrieval-ab --responses-mode side-by-side --collection "$AXON_COLLECTION" --json ;;
      "train") run_live "$name" train "Example Domain" --best 1 --notes "live CLI smoke" --collection "$AXON_COLLECTION" --json ;;
      "suggest") run_live "$name" suggest "Example Domain documentation" --limit 2 --collection "$AXON_COLLECTION" --json ;;
      "sources") run_live "$name" sources --all --domain example.com --json ;;
      "domains") run_live "$name" domains --domain example.com --json ;;
      "stats") run_live "$name" stats --json ;;
      "status")
        export AXON_DATA_DIR="$OUTDIR/jobs-data"
        mkdir -p "$AXON_DATA_DIR"
        "$AXON_BIN" source "$fixture_url" --scope page --wait true --collection "$AXON_COLLECTION" --json >"$OUTDIR/logs/fixture-job.json" 2>"$OUTDIR/logs/fixture-job.stderr.log"
        job_id="$(jq -r '.job_id // .job.id // empty' "$OUTDIR/logs/fixture-job.json")"
        source_id="$(jq -r '.source_id // empty' "$OUTDIR/logs/fixture-job.json")"
        run_live "$name" status --json
        # shellcheck disable=SC2016
        assert_live_json "status completed counts" "$LAST_LIVE_LOG" \
          --arg job_id "$job_id" \
          '.jobs[] | select(.job_id == $job_id) | (.result.documents_done > 0 and .result.chunks_done > 0)'
        "$AXON_BIN" status >"$OUTDIR/logs/live-status-human.log" 2>"$OUTDIR/logs/live-status-human.stderr.log"
        assert_live_text "status completed details" "$OUTDIR/logs/live-status-human.log" " docs · 100% · "
        run_live "$name" status --active --color never --quiet --json
        run_live "$name" status --recent --color never --quiet --json
        run_live "$name" status --reclaimed --color never --quiet --json
        ;;
      "jobs list")
        run_live "$name" jobs list --json
        # shellcheck disable=SC2016
        assert_live_json "jobs list completed counts" "$LAST_LIVE_LOG" \
          --arg job_id "$job_id" \
          '.items[] | select(.job_id == $job_id) | (.counts.documents_done > 0 and .counts.chunks_done > 0)'
        "$AXON_BIN" source "$fixture_url?jobs_page=2" --scope page --wait true \
          --collection "$AXON_COLLECTION" --json \
          >"$OUTDIR/logs/fixture-job-page-2.json" \
          2>"$OUTDIR/logs/fixture-job-page-2.stderr.log"
        run_live "$name" jobs list --status completed --kind source --limit 1 --json
        jobs_cursor="$(jq -r '.next_cursor // empty' "$LAST_LIVE_LOG")"
        if [ -n "$jobs_cursor" ]; then
          run_live "$name" jobs list --status completed --kind source \
            --limit 1 --cursor "$jobs_cursor" --json
        else
          missing_live "jobs list cursor" "fixture did not produce a second filtered job page"
        fi
        ;;
      "jobs get")
        run_live "$name" jobs get "$job_id" --json
        assert_live_json "jobs get completed counts" "$LAST_LIVE_LOG" \
          '.counts.documents_done > 0 and .counts.chunks_done > 0'
        "$AXON_BIN" jobs get "$job_id" >"$OUTDIR/logs/live-jobs_get-human.log" 2>"$OUTDIR/logs/live-jobs_get-human.stderr.log"
        assert_live_text "jobs get completed details" "$OUTDIR/logs/live-jobs_get-human.log" "Result:"
        assert_live_text "jobs get completed totals" "$OUTDIR/logs/live-jobs_get-human.log" " docs · 100% · "
        ;;
      "jobs events")
        run_live "$name" jobs events "$job_id" --after-sequence 0 --limit 1 --json
        jobs_events_cursor="$(jq -r '.next_cursor // empty' "$LAST_LIVE_LOG")"
        if [ -n "$jobs_events_cursor" ]; then
          run_live "$name" jobs events "$job_id" --cursor "$jobs_events_cursor" --limit 1 --json
        else
          missing_live "jobs events cursor" "fixture did not produce a second event page"
        fi
        ;;
      "jobs stream") run_live "$name" jobs stream "$job_id" --after-sequence 0 --limit 100 --json ;;
      "jobs cancel")
        "$AXON_BIN" source "$fixture_url?axon_cancel_fixture=$$" --scope page --wait false \
          --collection "$AXON_COLLECTION" --json \
          >"$OUTDIR/logs/fixture-job-cancel.json" \
          2>"$OUTDIR/logs/fixture-job-cancel.stderr.log"
        job_id="$(jq -r '.job_id // .job.id // empty' "$OUTDIR/logs/fixture-job-cancel.json")"
        run_live "$name" jobs cancel "$job_id" --reason "live harness cancellation" --json
        ;;
      "jobs retry") run_live "$name" jobs retry "$job_id" --mode same_config --json ;;
      "jobs recover") run_live "$name" jobs recover --kind source --stale-before "2026-01-01T00:00:00Z" --json ;;
      "jobs cleanup") run_live "$name" jobs cleanup --status completed --kind source --older-than "2099-01-01T00:00:00Z" --dry-run --json ;;
      "jobs clear") run_live "$name" jobs clear --confirm --json ;;
      "jobs worker")
        run_live "$name" jobs worker --idle-exit-secs 2 --json
        export AXON_DATA_DIR="$main_data_dir"
        ;;
      "memory remember")
        run_live "$name" memory remember "Axon live CLI fixture memory" \
          --title "Live fixture" --type fact --project "axon-live-cli" \
          --repo "dinglebear-ai/axon" --file "scripts/live-test-all-commands.sh" \
          --confidence 0.92 --json
        memory_id="$(jq -r '.memory.id // .id // empty' "$LAST_LIVE_LOG" 2>/dev/null)"
        "$AXON_BIN" memory remember "Axon replacement fixture memory" --title "Replacement fixture" --project "axon-live-cli" --json >"$OUTDIR/logs/fixture-memory.json" 2>"$OUTDIR/logs/fixture-memory.stderr.log"
        replacement_memory_id="$(jq -r '.memory.id // .id // empty' "$OUTDIR/logs/fixture-memory.json")"
        ;;
      "memory list")
        run_live "$name" memory list --project "axon-live-cli" --repo "dinglebear-ai/axon" \
          --file "scripts/live-test-all-commands.sh" --type fact --status active --limit 5 --json
        ;;
      "memory search")
        run_live "$name" memory search "fixture memory" --project "axon-live-cli" \
          --repo "dinglebear-ai/axon" --file "scripts/live-test-all-commands.sh" \
          --limit 5 --json
        ;;
      "memory show") run_live "$name" memory show "$memory_id" --json ;;
      "memory link") run_live "$name" memory link "$memory_id" "$replacement_memory_id" --type relates_to --json ;;
      "memory supersede") run_live "$name" memory supersede "$replacement_memory_id" "$memory_id" --json ;;
      "memory context")
        run_live "$name" memory context --query fixture --project "axon-live-cli" \
          --repo "dinglebear-ai/axon" --file "scripts/live-test-all-commands.sh" \
          --limit 5 --token-budget 1000 --json
        ;;
      "sessions") run_live "$name" sessions --claude --codex --gemini --project "axon-cli-nonexistent-project-zz" --wait true --json ;;
      "source")
        run_live "$name" source "$fixture_url" --scope page --max-pages 1 --max-depth 0 --include-subdomains false --render-mode chrome --cache true --format json --limit 2 --batch-concurrency 1 --wait true --performance-profile high-stable --normalize true --block-assets true --chrome-wait-for-selector body --root-selector body --exclude-selector ".axon-never" --header "X-Axon-Live: enabled" --budget "*=1" --collection "$AXON_COLLECTION" --json
        assert_live_json "source completed content" "$LAST_LIVE_LOG" \
          '.status == "completed" and .documents_prepared > 0 and .vector_points_written > 0'
        prove_options "source" "source completed through the requested page scope" --scope
        prove_options "@global" \
          "source completed with the requested acquisition, preparation, and publication controls" \
          --max-pages --max-depth --include-subdomains --render-mode --cache --format \
          --limit --batch-concurrency --wait --performance-profile --normalize \
          --block-assets --chrome-wait-for-selector --root-selector --exclude-selector \
          --header --budget --collection
        jq -n '{"/":[{"action":"wait","ms":25}]}' >"$OUTDIR/automation-script.json"
        run_live "$name" source "$fixture_url?automation=$$" --scope site \
          --max-pages 1 --max-depth 0 --render-mode chrome \
          --automation-script "$OUTDIR/automation-script.json" \
          --chrome-screenshot true --output-dir "$OUTDIR/source-output" \
          --warc "$OUTDIR/source-output/live.warc" --skip-embed \
          --wait true --collection "$AXON_COLLECTION" --json
        assert_live_json "source automation artifact-only crawl" "$LAST_LIVE_LOG" \
          '.status == "completed" and .documents_prepared > 0
           and .vector_points_written == 0'
        warc_artifact="$(find "$OUTDIR/source-output/artifacts" -maxdepth 1 \
          -type f -name 'art_warc_*.bin' -print -quit 2>/dev/null)"
        if [ -n "$warc_artifact" ] && [ -s "$warc_artifact" ] \
          && head -1 "$warc_artifact" | grep -Fq "WARC/1.1"; then
          record "source WARC archive" "contract" "PASS" "0" \
            "non-empty WARC/1.1 artifact" "$warc_artifact"
          prove_options "@global" \
            "Chrome crawl loaded the automation script, captured a screenshot, saved output, wrote WARC, and skipped embeddings" \
            --automation-script --chrome-screenshot --output-dir --warc --skip-embed
        else
          missing_live "source WARC archive" "crawl did not create a valid WARC/1.1 archive"
        fi
        run_live "$name" source "$fixture_url?cache_fixture=$$" --scope page \
          --render-mode http --cache true --cache-http-only --etag-conditional \
          --output-dir "$OUTDIR/cache-output" --skip-embed --wait true \
          --collection "$AXON_COLLECTION" --json
        assert_live_json "source conditional HTTP cache first pass" "$LAST_LIVE_LOG" \
          '.status == "completed" and .vector_points_written == 0'
        run_live "$name" source "$fixture_url?cache_fixture=$$" --scope page \
          --render-mode http --cache true --cache-http-only --etag-conditional \
          --output-dir "$OUTDIR/cache-output" --skip-embed --wait true \
          --collection "$AXON_COLLECTION" --json
        assert_live_json "source conditional HTTP cache reuse" "$LAST_LIVE_LOG" \
          '.status == "completed" and .vector_points_written == 0'
        prove_options "@global" \
          "repeat HTTP crawl completed through the cache-only conditional request path" \
          --cache-http-only --etag-conditional
        local_fixture="$OUTDIR/local-source"
        mkdir -p "$local_fixture/keep" "$local_fixture/vendor" "$local_fixture/docs/private"
        printf '# Keep\nAxon live included fixture.\n' >"$local_fixture/keep/keep.md"
        printf '# Excluded\nVendor fixture.\n' >"$local_fixture/vendor/excluded.md"
        printf '# Excluded\nPrivate fixture.\n' >"$local_fixture/docs/private/excluded.md"
        run_live "$name" source "$local_fixture" \
          --exclude-path vendor/ --exclude-path docs/private/ \
          --skip-embed --wait true --collection "$AXON_COLLECTION" --json
        assert_live_json "source local path exclusions" "$LAST_LIVE_LOG" \
          '.status == "completed" and .documents_prepared == 1
           and .vector_points_written == 0'
        prove_option_behavior "@global" "--exclude-path" \
          "local source prepared only the included file while excluding both requested paths"
        run_live "source URL glob" source \
          --url-glob "https://www.rust-lang.org/{learn,tools}" \
          --scope page --skip-embed --batch-concurrency 2 --wait true \
          --collection "$AXON_COLLECTION" --json
        assert_live_json "source URL glob expansion" "$LAST_LIVE_LOG" \
          '.count == 2 and .succeeded == 2 and .failed == 0'
        record_behavior_args "source" source \
          --url-glob "https://www.rust-lang.org/{learn,tools}" \
          --scope page --skip-embed --batch-concurrency 2 --wait true \
          --collection "$AXON_COLLECTION" --json
        prove_option_behavior "@global" "--url-glob" \
          "brace expansion produced two successful source members"
        run_live "$name" source "$fixture_url?cron=$$" --scope page \
          --cron-every-seconds 1 --cron-max-runs 2 --skip-embed --wait true \
          --collection "$AXON_COLLECTION" --json
        prove_options "@global" \
          "bounded cron source emitted two completed runs and then exited" \
          --cron-every-seconds --cron-max-runs
        mixed_success_url="$fixture_url?axon_mixed_success=$$"
        run_live_expect_failure "source mixed batch" source \
          --urls "$mixed_success_url,http://127.0.0.1:1/axon-live-unreachable" \
          --scope page --batch-concurrency 2 --wait true --collection "$AXON_COLLECTION" --json
        assert_live_json "source mixed batch reports intended failure" "$LAST_LIVE_LOG" \
          '.count == 2 and .succeeded == 1 and .failed == 1
           and any(.results[]; .status == "failed" and (.error | length) > 0)'
        prove_option_behavior "@global" "--urls" "mixed batch reported both inputs and the intended failed member"
        prove_option_behavior "@global" "--batch-concurrency" "mixed siblings both reached terminal outcomes"
        prove_option_behavior "@global" "--wait" "command returned only after both batch outcomes were terminal"
        mixed_success_job_id="$(jq -r \
          '.results[] | select(.status == "completed") | .job_id // empty' \
          "$LAST_LIVE_LOG" | head -1)"
        "$AXON_BIN" jobs get "$mixed_success_job_id" --json \
          >"$OUTDIR/logs/fixture-mixed-job.json" \
          2>"$OUTDIR/logs/fixture-mixed-job.stderr.log"
        assert_live_json "source mixed batch drains successful sibling" \
          "$OUTDIR/logs/fixture-mixed-job.json" \
          '.status == "completed" and .counts.documents_done > 0'
        ;;
      "screenshot")
        run_live "$name" screenshot "$fixture_url" --output "$OUTDIR/screenshot.png" \
          --screenshot-full-page false --viewport 1280x720 --json
        screenshot_artifact_id="$(jq -r '.artifact_id // empty' "$LAST_LIVE_LOG" 2>/dev/null)"
        assert_live_json "screenshot artifact contract" "$LAST_LIVE_LOG" \
          '.artifact_id != null and .artifact_id != ""
           and .width == 1280 and .height == 720'
        if [ -s "$OUTDIR/screenshot.png" ]; then
          prove_option_behavior "@global" "--output" "requested screenshot path contains a non-empty PNG"
          prove_option_behavior "@global" "--viewport" "capture returned dimensions for the requested viewport"
          record "screenshot output file" "contract" "PASS" "0" \
            "non-empty $OUTDIR/screenshot.png" "$OUTDIR/screenshot.png"
        else
          failures=$((failures + 1))
          record "screenshot output file" "contract" "FAIL" "1" \
            "non-empty $OUTDIR/screenshot.png" "$OUTDIR/screenshot.png"
        fi
        ;;
      "completions")
        run_live "$name" completions bash
        assert_live_nonempty "completions bash output" "$LAST_LIVE_LOG"
        cp "$LAST_LIVE_LOG" "$OUTDIR/logs/completions-bash-primary.log"
        run_live "completion alias" completion bash
        assert_live_nonempty "completion alias output" "$LAST_LIVE_LOG"
        if cmp -s "$OUTDIR/logs/completions-bash-primary.log" "$LAST_LIVE_LOG"; then
          record "completion alias equivalence" "contract" "PASS" "0" \
            "completion bash == completions bash" "$LAST_LIVE_LOG"
          record_behavior_args "completion alias" completion bash
        else
          failures=$((failures + 1))
          record "completion alias equivalence" "contract" "FAIL" "1" \
            "completion bash == completions bash" "$LAST_LIVE_LOG"
        fi
        "$AXON_BIN" completions zsh >"$OUTDIR/logs/completions-zsh.log" 2>&1 || failures=$((failures + 1))
        "$AXON_BIN" completions fish >"$OUTDIR/logs/completions-fish.log" 2>&1 || failures=$((failures + 1))
        ;;
      "serve") run_live_server "$name" 38131 serve ;;
      "serve mcp") run_live_server "$name" 38132 serve mcp --transport http ;;
      "reset")
        reset_data="$OUTDIR/reset-data"
        mkdir -p "$reset_data"
        saved_data_dir="$AXON_DATA_DIR"
        export AXON_DATA_DIR="$reset_data"
        run_live "$name" reset --stores jobs --json
        reset_plan_log="$OUTDIR/logs/reset-plan.log"
        cp "$LAST_LIVE_LOG" "$reset_plan_log"
        assert_live_json "reset plan is isolated and reviewable" "$reset_plan_log" \
          '.dry_run == true
           and .execution_state == "Planned"
           and .stores == ["jobs", "ledger", "code_index", "watch", "graph", "memory"]
           and (.stores | index("vectors") | not)
           and (.stores | index("artifacts") | not)
           and (.plan_id | length) > 0'
        if jq -e \
          '.dry_run == true
           and .execution_state == "Planned"
           and .stores == ["jobs", "ledger", "code_index", "watch", "graph", "memory"]
           and (.plan_id | length) > 0' \
          "$reset_plan_log" >/dev/null 2>&1; then
          reset_plan_id="$(jq -r '.plan_id' "$reset_plan_log")"
          prove_option_behavior "reset" "--stores" \
            "jobs scope expanded only to the six logical owners of the shared SQLite file"
          run_live "$name" reset --stores jobs --dry-run --yes --json
          assert_live_json "reset explicit dry-run pins preview mode" "$LAST_LIVE_LOG" \
            '.dry_run == true
             and .execution_state == "Planned"
             and .stores == ["jobs", "ledger", "code_index", "watch", "graph", "memory"]'
          prove_option_behavior "reset" "--dry-run" \
            "explicit dry-run remained non-destructive even alongside --yes"
          if jq -e \
            '.dry_run == true
             and .execution_state == "Planned"
             and .stores == ["jobs", "ledger", "code_index", "watch", "graph", "memory"]' \
            "$LAST_LIVE_LOG" >/dev/null 2>&1; then
            run_live "$name" reset --stores jobs --plan-id "$reset_plan_id" --yes --json
            cp "$LAST_LIVE_LOG" "$OUTDIR/logs/reset-exec.log"
            # shellcheck disable=SC2016
            assert_live_json "reset execution receipt" "$OUTDIR/logs/reset-exec.log" \
              --arg plan_id "$reset_plan_id" \
              '.dry_run == false
               and .execution_state == "Completed"
               and .plan_id == $plan_id
               and .stores == ["jobs", "ledger", "code_index", "watch", "graph", "memory"]
               and (.receipt_path | length) > 0'
            prove_option_behavior "reset" "--plan-id" \
              "execution returned the same reviewed plan id"
            prove_option_behavior "@global" "--yes" \
              "reviewed reset plan executed and emitted a durable receipt"
          else
            record "reset execution" "contract" "FAIL" "1" \
              "skipped because --dry-run did not remain non-destructive alongside --yes" \
              "$LAST_LIVE_LOG"
          fi
        else
          record "reset execution" "contract" "FAIL" "1" \
            "skipped because reset plan was not dry-run, SQLite-scoped, and reviewable" \
            "$reset_plan_log"
        fi
        export AXON_DATA_DIR="$saved_data_dir"
        ;;
      "prune plan")
        prune_fixture="$OUTDIR/prune-source"
        mkdir -p "$prune_fixture"
        printf '# Prune fixture\nGeneration one.\n' >"$prune_fixture/content.md"
        "$AXON_BIN" source "$prune_fixture" --wait true \
          --collection "$AXON_COLLECTION" --json \
          >"$OUTDIR/logs/fixture-prune-source.json" \
          2>"$OUTDIR/logs/fixture-prune-source.stderr.log"
        prune_source_id="$(jq -r '.source_id // empty' "$OUTDIR/logs/fixture-prune-source.json")"
        prune_generation="$(jq -r '.generation // empty' "$OUTDIR/logs/fixture-prune-source.json")"
        printf '# Prune fixture\nGeneration two has changed content.\n' \
          >"$prune_fixture/content.md"
        "$AXON_BIN" source "$prune_fixture" --wait true \
          --collection "$AXON_COLLECTION" --json \
          >"$OUTDIR/logs/fixture-prune-current-source.json" \
          2>"$OUTDIR/logs/fixture-prune-current-source.stderr.log"
        # shellcheck disable=SC2016
        assert_live_json "prune fixture advanced generation" \
          "$OUTDIR/logs/fixture-prune-current-source.json" \
          --arg old_generation "$prune_generation" \
          '.status == "completed" and .generation != $old_generation'
        run_live "$name" prune plan "$prune_source_id" --generation "$prune_generation" --json
        prune_plan_id="$(jq -r '.plan_id // .plan.plan_id // .plan.job_id // .plan.id // empty' "$LAST_LIVE_LOG" 2>/dev/null)"
        ;;
      "prune exec") run_live "$name" prune exec "$prune_plan_id" --generation "$prune_generation" --confirm --json ;;
      "preflight") run_live "$name" preflight --config --json ;;
      "smoke") run_live "$name" smoke --json ;;
      "compose up")
        run_live_setup_home "$name" compose up --json
        assert_live_json "compose up phases" "$LAST_LIVE_LOG" \
          '.has_errors == false and any(.phases[]; .name == "compose-up" and .status == "ok")'
        ;;
      "compose down")
        run_live_setup_home "$name" compose down --json
        assert_live_json "compose down phases" "$LAST_LIVE_LOG" \
          '.has_errors == false and any(.phases[]; .name == "compose-down" and .status == "ok")'
        ;;
      "compose restart")
        run_live_setup_home "compose restart fixture up" compose up --json
        assert_live_json "compose restart fixture up phases" "$LAST_LIVE_LOG" \
          '.has_errors == false and any(.phases[]; .name == "compose-up" and .status == "ok")'
        run_live_setup_home "$name" compose restart --json
        assert_live_json "compose restart phases" "$LAST_LIVE_LOG" \
          '.has_errors == false and any(.phases[]; .name == "compose-restart" and .status == "ok")'
        ;;
      "compose rebuild")
        run_live_setup_home "$name" compose rebuild --json
        assert_live_json "compose rebuild phases" "$LAST_LIVE_LOG" \
          '.has_errors == false
           and any(.phases[]; .name == "compose-build" and .status == "ok")
           and any(.phases[]; .name == "compose-up" and .status == "ok")'
        ;;
      "setup plugin-hook")
        run_live_setup_home "$name" setup plugin-hook --no-setup --json
        assert_live_json "setup plugin-hook policy" "$LAST_LIVE_LOG" \
          '.action != null and .exit_policy != null and .stack != null'
        cp "$LAST_LIVE_LOG" "$OUTDIR/logs/setup-plugin-hook-primary.json"
        run_live_setup_home "setup hook alias" setup hook --no-setup --json
        assert_live_json "setup hook alias policy" "$LAST_LIVE_LOG" \
          '.action != null and .exit_policy != null and .stack != null'
        if jq -S '{action,exit_policy,stack}' "$OUTDIR/logs/setup-plugin-hook-primary.json" \
          | cmp -s - <(jq -S '{action,exit_policy,stack}' "$LAST_LIVE_LOG"); then
          record "setup hook alias equivalence" "contract" "PASS" "0" \
            "setup hook == setup plugin-hook" "$LAST_LIVE_LOG"
          record_behavior_args "setup hook alias" setup hook --no-setup --json
        else
          failures=$((failures + 1))
          record "setup hook alias equivalence" "contract" "FAIL" "1" \
            "setup hook == setup plugin-hook" "$LAST_LIVE_LOG"
        fi
        ;;
      "setup init")
        run_live_setup_home "$name" setup init \
          --mcp-host 127.0.0.1 --mcp-port 38133 --auth-mode oauth \
          --mcp-token live-fixture-token \
          --oauth-public-url https://axon.invalid \
          --google-client-id live-fixture-google-client \
          --google-client-secret live-fixture-google-secret \
          --auth-admin-email live-fixture@example.invalid \
          --tavily-api-key live-fixture-tavily \
          --github-token live-fixture-github \
          --reddit-client-id live-fixture-reddit-client \
          --reddit-client-secret live-fixture-reddit-secret \
          --json
        assert_live_text "setup init OAuth URL persisted" "$SETUP_HOME/.axon/.env" \
          "AXON_PUBLIC_URL=https://axon.invalid"
        assert_live_text "setup init optional credentials persisted" "$SETUP_HOME/.axon/.env" \
          "REDDIT_CLIENT_ID=live-fixture-reddit-client"
        ;;
      "setup check") run_live_setup_check ;;
      "setup targets") run_live_setup_home "$name" setup targets --json ;;
      "setup install") run_live_setup_home "$name" setup install --json ;;
      "setup config rewrite") run_live_setup_home "$name" setup config rewrite --dry-run --json ;;
      "mcp") run_live_server "$name" 38134 mcp --transport http ;;
      "migrate")
        migrate_from="${AXON_COLLECTION}_unnamed"
        migrate_to="${AXON_COLLECTION}_named"
        isolated_collections+=("$migrate_from" "$migrate_to")
        curl -fsS -X PUT "${QDRANT_URL%/}/collections/$migrate_from" -H 'content-type: application/json' \
          --data '{"vectors":{"size":1024,"distance":"Cosine"}}' >"$OUTDIR/logs/migrate-create.json"
        jq -nc '{points:[{id:1,vector:[range(1024)|0],payload:{chunk_text:"Axon live migration fixture"}}]}' \
          | curl -fsS -X PUT "${QDRANT_URL%/}/collections/$migrate_from/points?wait=true" -H 'content-type: application/json' --data-binary @- \
            >"$OUTDIR/logs/migrate-upsert.json"
        run_live "$name" migrate --from "$migrate_from" --to "$migrate_to" --json
        curl -fsS -X DELETE "${QDRANT_URL%/}/collections/$migrate_from" >"$OUTDIR/logs/migrate-delete-from.json"
        curl -fsS -X DELETE "${QDRANT_URL%/}/collections/$migrate_to" >"$OUTDIR/logs/migrate-delete-to.json"
        ;;
      "config list")
        "$AXON_BIN" config set retrieval.hybrid-candidates 72 --json >"$OUTDIR/logs/fixture-config-set.json" 2>"$OUTDIR/logs/fixture-config-set.stderr.log"
        "$AXON_BIN" config set AXON_LIVE_HARNESS_TOKEN live-fixture-value --env --json \
          >"$OUTDIR/logs/fixture-config-set-env.json"
        run_live "$name" config list --env --reveal --json
        assert_live_json "config list env reveal" "$LAST_LIVE_LOG" \
          '.env.AXON_LIVE_HARNESS_TOKEN == "live-fixture-value"'
        run_live "$name" config list --toml --reveal --json
        assert_live_json "config list TOML reveal" "$LAST_LIVE_LOG" \
          '.toml["retrieval.hybrid-candidates"] == "72"'
        ;;
      "config get")
        run_live "$name" config get AXON_LIVE_HARNESS_TOKEN --env --reveal --json
        assert_live_json "config get env reveal" "$LAST_LIVE_LOG" \
          '.present == true and .value == "live-fixture-value"'
        run_live "$name" config get retrieval.hybrid-candidates --toml --reveal --json
        assert_live_json "config get TOML reveal" "$LAST_LIVE_LOG" \
          '.present == true and .value == "72"'
        ;;
      "config set")
        run_live "$name" config set AXON_LIVE_HARNESS_SET live-fixture-set --env --json
        run_live "$name" config set retrieval.hybrid-candidates 73 --toml --json
        ;;
      "config unset")
        run_live "$name" config unset AXON_LIVE_HARNESS_SET --env --json
        run_live "$name" config unset retrieval.hybrid-candidates --toml --json
        ;;
      "config path")
        run_live "$name" config path --json
        # shellcheck disable=SC2016
        assert_live_json "config path uses isolated files" "$LAST_LIVE_LOG" \
          --arg toml "$AXON_CONFIG_PATH" --arg env "$AXON_ENV_FILE" \
          '.toml_path == $toml and .env_path == $env'
        prove_option_behavior "@global" "--json" "config path stdout parsed and matched isolated paths"
        ;;
      "sync pending") run_live "$name" sync pending --json ;;
      "update")
        run_live_setup_home "$name" update --repo dinglebear-ai/axon \
          --version v6.2.1 --force --no-container --json
        assert_live_json "update pinned Axon release" "$LAST_LIVE_LOG" \
          '.installed == true and .version == "6.2.1"'
        install -m 0755 "$HARNESS_SOURCE_BIN" "$AXON_BIN"
        ;;
      "palette")
        palette_display=":$((100 + $$ % 500))"
        Xvfb "$palette_display" -screen 0 1280x720x24 -nolisten tcp \
          >"$OUTDIR/logs/palette-xvfb.log" 2>&1 &
        palette_xvfb_pid=$!
        sleep 0.5
        DISPLAY="$palette_display" run_live_setup_home "$name" palette install --method pull --json
        DISPLAY="$palette_display" run_live_setup_home "palette launch" palette launch --json
        DISPLAY="$palette_display" run_live_setup_home "palette desktop" palette desktop --json
        DISPLAY="$palette_display" run_live_setup_home "palette autostart" palette autostart --json
        kill -TERM "$palette_xvfb_pid" 2>/dev/null || true
        wait "$palette_xvfb_pid" 2>/dev/null || true
        ;;
      "artifacts list")
        "$AXON_BIN" screenshot "$fixture_url?artifact_page=2" \
          --output "$OUTDIR/screenshot-2.png" --json \
          >"$OUTDIR/logs/fixture-screenshot-2.json" \
          2>"$OUTDIR/logs/fixture-screenshot-2.stderr.log"
        run_live "$name" artifacts list --kind screenshot --limit 1 --json
        artifacts_cursor="$(jq -r '.next_cursor // empty' "$LAST_LIVE_LOG")"
        if [ -n "$artifacts_cursor" ]; then
          run_live "$name" artifacts list --kind screenshot --cursor "$artifacts_cursor" --limit 1 --json
        else
          missing_live "artifacts list cursor" "fixture did not produce a second screenshot page"
        fi
        run_live "$name" artifacts list --source-id "$source_id" --json
        assert_live_json "artifacts source filter excludes unrelated screenshots" "$LAST_LIVE_LOG" \
          '.items | length == 0'
        run_live "$name" artifacts list --job-id "$job_id" --json
        assert_live_json "artifacts job filter excludes unrelated screenshots" "$LAST_LIVE_LOG" \
          '.items | length == 0'
        ;;
      "artifacts get")
        run_live "$name" artifacts get "$screenshot_artifact_id" --include-content-url --json
        assert_live_json "artifact content URL projection" "$LAST_LIVE_LOG" \
          '(.content_url | type) == "string" and (.content_url | length) > 0
           and .metadata.full_page == false'
        prove_option_behavior "@global" "--screenshot-full-page" \
          "stored screenshot metadata reported viewport-only capture"
        ;;
      "artifacts content")
        run_live "$name" artifacts content "$screenshot_artifact_id" --output "$OUTDIR/artifact-content.png" --json
        run_live "$name" artifacts content "$screenshot_artifact_id" \
          --range bytes=0-15 --output "$OUTDIR/artifact-range.bin" --json
        assert_live_json "artifact byte range receipt" "$LAST_LIVE_LOG" \
          '.size_bytes == 16'
        if [ "$(wc -c <"$OUTDIR/artifact-range.bin")" -eq 16 ]; then
          prove_option_behavior "artifacts content" "--range" \
            "requested inclusive byte range wrote exactly 16 bytes"
        else
          missing_live "artifact byte range file" "range output was not exactly 16 bytes"
        fi
        pushd "$OUTDIR" >/dev/null || exit 1
        run_live "$name" artifacts content "$screenshot_artifact_id" --download --json
        popd >/dev/null || exit 1
        # shellcheck disable=SC2016
        assert_live_json "artifact default download path" "$LAST_LIVE_LOG" \
          --arg expected "$screenshot_artifact_id.png" \
          '.output == $expected and .size_bytes > 0'
        artifact_download_name="$(jq -r '.output // empty' "$LAST_LIVE_LOG")"
        if [ -n "$artifact_download_name" ] \
          && [ -s "$OUTDIR/$artifact_download_name" ]; then
          prove_option_behavior "artifacts content" "--download" \
            "download mode wrote the artifact-id-derived default filename"
        else
          missing_live "artifact default download file" "download did not create a non-empty default file"
        fi
        ;;
      "uploads list")
        "$AXON_BIN" uploads create /etc/hosts --purpose source_artifact --json >"$OUTDIR/logs/fixture-upload-complete.json" 2>"$OUTDIR/logs/fixture-upload-complete.stderr.log"
        upload_id="$(jq -r '.upload.upload_id // .status.upload_id // empty' "$OUTDIR/logs/fixture-upload-complete.json")"
        "$AXON_BIN" uploads create /etc/hosts --purpose source_artifact --json >"$OUTDIR/logs/fixture-upload-abort.json" 2>"$OUTDIR/logs/fixture-upload-abort.stderr.log"
        abort_upload_id="$(jq -r '.upload.upload_id // .status.upload_id // empty' "$OUTDIR/logs/fixture-upload-abort.json")"
        run_live "$name" uploads list --status received --limit 1 --json
        uploads_cursor="$(jq -r '.next_cursor // empty' "$LAST_LIVE_LOG")"
        if [ -n "$uploads_cursor" ]; then
          run_live "$name" uploads list --status received --cursor "$uploads_cursor" --limit 1 --json
        else
          missing_live "uploads list cursor" "fixture did not produce a second received upload page"
        fi
        ;;
      "uploads create")
        run_live "$name" uploads create /etc/hosts --purpose source_artifact --source-hint live-smoke --json
        ;;
      "uploads get") run_live "$name" uploads get "$upload_id" --json ;;
      "uploads complete")
        upload_sha256="$(sha256sum /etc/hosts | awk '{print $1}')"
        run_live "$name" uploads complete "$upload_id" --sha256 "$upload_sha256" \
          --source-option "scope=page" --json
        ;;
      "uploads abort") run_live "$name" uploads abort "$abort_upload_id" --reason "live harness cleanup" --json ;;
      "collections list") run_live "$name" collections list --json ;;
      "collections get") run_live "$name" collections get "$AXON_COLLECTION" --include-schema --include-indexes --json ;;
      "graph kinds") run_live "$name" graph kinds --json ;;
      "graph resolve")
        run_live "$name" graph resolve "$fixture_url" --kind web_origin --limit 10 --json
        "$AXON_BIN" graph source "src_0272b3e7006f0910" --depth 2 --limit 100 --json >"$OUTDIR/logs/fixture-graph-source.json" 2>"$OUTDIR/logs/fixture-graph-source.stderr.log" || true
        graph_node_id="$(jq -r '.. | .node_id? // empty' "$OUTDIR/logs/fixture-graph-source.json" 2>/dev/null | head -1)"
        graph_edge_id="$(jq -r '.. | .edge_id? // empty' "$OUTDIR/logs/fixture-graph-source.json" 2>/dev/null | head -1)"
        ;;
      "graph query")
        run_live "$name" graph query "$fixture_url" --limit 1 --json
        graph_cursor="$(jq -r '.next_cursor // empty' "$LAST_LIVE_LOG")"
        if [ -n "$graph_cursor" ]; then
          run_live "$name" graph query "$fixture_url" --cursor "$graph_cursor" --limit 1 --json
        else
          missing_live "graph query cursor" "fixture traversal did not produce a second edge page"
        fi
        ;;
      "graph node")
        run_live "$name" graph node "$graph_node_id" --include-edges --include-evidence --json
        assert_live_json "graph node evidence projection" "$LAST_LIVE_LOG" \
          'any(.edges[]; (.evidence | length) > 0)'
        ;;
      "graph edge")
        run_live "$name" graph edge "$graph_edge_id" --include-evidence --json
        assert_live_json "graph edge evidence projection" "$LAST_LIVE_LOG" \
          '(.evidence | length) > 0'
        ;;
      "graph source") run_live "$name" graph source "src_0272b3e7006f0910" --depth 2 --edge-kind docs_site_contains_page --limit 100 --json ;;
      "providers list")
        run_live "$name" providers list --kind qdrant --status healthy --json
        assert_live_json "providers list filtered qdrant" "$LAST_LIVE_LOG" \
          '.providers | length == 1 and .[0].id == "qdrant" and .[0].ok == true'
        ;;
      "providers get")
        run_live "$name" providers get qdrant --include-health --include-limits --json
        assert_live_json "providers get qdrant identity" "$LAST_LIVE_LOG" \
          '.id == "qdrant" and .ok == true'
        while IFS= read -r provider; do
          [ "$provider" = "qdrant" ] && continue
          run_live "providers get ($provider)" providers get "$provider" --include-health --include-limits --json
        done < <(jq -r '.providers[]?.id' "$OUTDIR/logs/live-providers_list.log")
        ;;
      "capabilities") run_live "$name" capabilities --json ;;
      "chat") run_live "$name" chat --query "Reply with exactly: axon live chat ok" --json ;;
      *)
        missing_live "$name" "no stateful live scenario is registered"
        ;;
    esac
  done < <(jq -r '.commands[].name' "$REGISTRY")
fi

build_behavioral_coverage_report() {
  local root_help root_options encoded name path_json help_log option key result evidence
  root_help="$OUTDIR/logs/behavior-root-help.log"
  root_options="$OUTDIR/behavioral-global-options.txt"
  "$AXON_BIN" --help >"$root_help" 2>&1
  awk '
    /^  Global Options$/ { in_options=1; next }
    in_options && /^  Commands$/ { exit }
    in_options && /^  (-[A-Za-z], )?--[a-z0-9]/ {
      line=$0
      sub(/^  (-[A-Za-z], )?/, "", line)
      sub(/[[:space:]].*/, "", line)
      print line
    }
  ' "$root_help" | sort -u >"$root_options"

  : >"$BEHAVIOR_EXPECTED"
  if [ "$REGISTRY" = "$ROOT_DIR/docs/reference/cli/commands.json" ]; then
    while IFS= read -r option; do
      [ "$option" = "--help" ] && continue
      printf '%s\t%s\n' "@global" "$option" >>"$BEHAVIOR_EXPECTED"
    done <"$root_options"
    for option in \
      --automation-script --batch-concurrency --block-assets --budget --cache \
      --cache-http-only --chrome-screenshot --chrome-wait-for-selector --color \
      --cron-every-seconds --cron-max-runs --etag-conditional --exclude-path \
      --exclude-path-prefix --exclude-selector --format --normalize --output-dir \
      --performance-profile --quiet --root-selector --screenshot-full-page \
      --sitemap-only --url-glob --urls --viewport --warc --yes; do
      printf '%s\t%s\n' "@global" "$option" >>"$BEHAVIOR_EXPECTED"
    done
    {
      printf '%s\t%s\n' "ask" "--continue"
      printf '%s\t%s\n' "completion alias" "__command__"
      printf '%s\t%s\n' "setup hook alias" "__command__"
    } >>"$BEHAVIOR_EXPECTED"
  fi

  while IFS= read -r encoded; do
    name="$(printf '%s' "$encoded" | base64 --decode | jq -r '.name')"
    path_json="$(printf '%s' "$encoded" | base64 --decode | jq -c '.path')"
    mapfile -t path < <(printf '%s' "$path_json" | jq -r '.[]')
    help_log="$OUTDIR/logs/behavior-help-$(printf '%s' "$name" | tr ' /' '__').log"
    "$AXON_BIN" "${path[@]}" --help >"$help_log" 2>&1
    printf '%s\t%s\n' "$name" "__command__" >>"$BEHAVIOR_EXPECTED"
    while IFS= read -r option; do
      [ "$option" = "--help" ] && continue
      if ! grep -Fqx -- "$option" "$root_options"; then
        printf '%s\t%s\n' "$name" "$option" >>"$BEHAVIOR_EXPECTED"
      fi
    done < <(
      awk '
        /^Options$/ { in_options=1; next }
        in_options && /^[^ ]/ { exit }
        in_options && /^  (-[A-Za-z], )?--[a-z0-9]/ {
          line=$0
          sub(/^  (-[A-Za-z], )?/, "", line)
          sub(/[[:space:]].*/, "", line)
          print line
        }
      ' "$help_log"
    )
    while IFS= read -r option; do
      [ "$option" = "-h" ] && continue
      if ! grep -Fqx -- "$option" "$root_options"; then
        printf '%s\t%s\n' "$name" "$option" >>"$BEHAVIOR_EXPECTED"
      fi
    done < <(
      awk '/^  -[A-Za-z], --[a-z0-9]/ { print substr($1, 1, 2) }' "$help_log"
    )
  done < <(jq -r '.commands[] | @base64' "$REGISTRY")

  sort -u -o "$BEHAVIOR_EXPECTED" "$BEHAVIOR_EXPECTED"
  sort -u -o "$BEHAVIOR_ACTUAL" "$BEHAVIOR_ACTUAL"
  sort -u -o "$BEHAVIOR_SEMANTIC" "$BEHAVIOR_SEMANTIC"
  while IFS= read -r key; do
    name="${key%%	*}"
    option="${key#*	}"
    evidence=""
    if [ "$option" = "__command__" ] && grep -Fqx -- "$key" "$BEHAVIOR_ACTUAL"; then
      result="PASS"
      evidence="executed with a command-specific contract"
    elif evidence="$(awk -F '\t' -v key="$key" '$1 FS $2 == key { print $3; exit }' "$BEHAVIOR_SEMANTIC")" \
      && [ -n "$evidence" ]; then
      result="PASS"
    else
      result="FAIL"
      failures=$((failures + 1))
    fi
    evidence="${evidence//$'\t'/ }"
    evidence="${evidence//$'\r'/ }"
    evidence="${evidence//$'\n'/ }"
    printf '%s\t%s\t%s\t%s\n' "$name" "$option" "$result" "$evidence" >>"$BEHAVIOR_REPORT"
  done <"$BEHAVIOR_EXPECTED"
}

if [ "$MODE" = "live" ] || [ "$MODE" = "scenarios" ]; then
  build_behavioral_coverage_report
fi

if ! awk -F '\t' 'NF != 6 { bad=1 } END { exit bad }' "$REPORT"; then
  echo "report structural validation failed: expected 6 TSV fields per physical line" >&2
  failures=$((failures + 1))
fi
if [ "$MODE" != "registry" ] \
  && ! awk -F '\t' 'NF != 4 { bad=1 } END { exit bad }' "$BEHAVIOR_REPORT"; then
  echo "behavior report structural validation failed: expected 4 TSV fields per physical line" >&2
  failures=$((failures + 1))
fi

passed="$(awk -F '\t' 'NR > 1 && $3 == "PASS" { count++ } END { print count + 0 }' "$REPORT")"
skipped="$(awk -F '\t' 'NR > 1 && $3 == "SKIP" { count++ } END { print count + 0 }' "$REPORT")"
behavior_passed="$(awk -F '\t' 'NR > 1 && $3 == "PASS" { count++ } END { print count + 0 }' "$BEHAVIOR_REPORT")"
behavior_failed="$(awk -F '\t' 'NR > 1 && $3 == "FAIL" { count++ } END { print count + 0 }' "$BEHAVIOR_REPORT")"
echo "Axon CLI smoke: mode=$MODE passed=$passed skipped=$skipped failed=$failures"
if [ "$MODE" = "registry" ]; then
  echo "Parser checks: $passed passed; no command behavior was executed."
else
  echo "Behavioral coverage: $behavior_passed passed, $behavior_failed missing."
  echo "Behavioral coverage report: $BEHAVIOR_REPORT"
fi
echo "Report: $REPORT"
[ "$failures" -eq 0 ] && [ "$skipped" -eq 0 ]
