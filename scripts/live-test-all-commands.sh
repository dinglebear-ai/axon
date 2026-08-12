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
SCENARIO_GROUP="all"
while [ "$#" -gt 0 ]; do
  case "$1" in
    --mode)
      MODE="${2:-}"
      shift 2
      ;;
    --scenario-group)
      SCENARIO_GROUP="${2:-}"
      shift 2
      ;;
    -h|--help)
      echo "usage: $0 [--mode registry|scenarios|live] [--scenario-group all|web-rag|jobs-source|admin|resources]"
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
case "$SCENARIO_GROUP" in
  all|web-rag|jobs-source|admin|resources) ;;
  *)
    echo "invalid scenario group '$SCENARIO_GROUP'" >&2
    exit 2
    ;;
esac
if [ "$SCENARIO_GROUP" != "all" ] && [ "$MODE" != "scenarios" ]; then
  echo "--scenario-group requires --mode scenarios" >&2
  exit 2
fi

AXON_BIN="${AXON_BIN:-$ROOT_DIR/target/debug/axon}"
REGISTRY="${AXON_COMMAND_REGISTRY:-$ROOT_DIR/docs/reference/cli/commands.json}"
TIMEOUT_SECS="${AXON_LIVE_COMMAND_TIMEOUT_SECS:-120}"
PARSER_JOBS="${AXON_LIVE_PARSER_JOBS:-4}"
if ! [[ "$PARSER_JOBS" =~ ^[1-9][0-9]*$ ]]; then
  echo "AXON_LIVE_PARSER_JOBS must be a positive integer" >&2
  exit 2
fi
TS="$(date +%Y%m%d-%H%M%S)"
if [ -n "${AXON_LIVE_TEST_OUTDIR:-}" ]; then
  OUTDIR="$AXON_LIVE_TEST_OUTDIR"
  mkdir -p "$OUTDIR"
else
  live_test_root="${AXON_LIVE_TEST_ROOT:-$ROOT_DIR/.cache/live-test}"
  mkdir -p "$live_test_root"
  OUTDIR="$(mktemp -d "$live_test_root/${SCENARIO_GROUP}-${TS}.XXXXXX")" || {
    echo "failed to allocate a unique live-test output directory" >&2
    exit 2
  }
fi
LIVE_RUN_ID="${TS//[^0-9]/}_$(stat -c '%d_%i' "$OUTDIR")"
RUN_PORT_BASE="${AXON_LIVE_PORT_BASE:-$((40000 + (RANDOM % 15000)))}"
export AXON_LIVE_PORT_BASE="$RUN_PORT_BASE"
mkdir -p "$OUTDIR/logs"
AXON_BIN="$(realpath -- "$AXON_BIN")"
REGISTRY="$(realpath -- "$REGISTRY")"
OUTDIR="$(realpath -- "$OUTDIR")"
REPORT="$OUTDIR/report.tsv"
BEHAVIOR_REPORT="$OUTDIR/behavioral-coverage.tsv"
TIMINGS_REPORT="$OUTDIR/timings.tsv"

command -v jq >/dev/null 2>&1 || {
  echo "jq is required" >&2
  exit 2
}
command -v flock >/dev/null 2>&1 || {
  echo "flock is required" >&2
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
printf 'milliseconds\tphase\tcommand\tinvocation\n' >"$TIMINGS_REPORT"
BEHAVIOR_ACTUAL="$OUTDIR/behavioral-actual.tsv"
BEHAVIOR_SEMANTIC="$OUTDIR/behavioral-semantic.tsv"
BEHAVIOR_EXPECTED="$OUTDIR/behavioral-expected.tsv"
BEHAVIOR_GLOBAL_OPTIONS="$OUTDIR/behavioral-global-options.txt"
BEHAVIOR_GLOBAL_VALUE_OPTIONS="$OUTDIR/behavioral-global-value-options.txt"
: >"$BEHAVIOR_ACTUAL"
: >"$BEHAVIOR_SEMANTIC"
LAST_BEHAVIOR_NAME=""
LAST_BEHAVIOR_ARGS=()
LAST_CONTRACT_EVIDENCE=""
declare -A LIVE_LOG_COUNTS=()
failures=0
isolated_collection=""
isolated_collections=()
isolated_compose_project=""
isolated_compose_network=""
live_chrome_pid=""
live_chrome_pgid=""
live_chrome_start_time=""
live_chrome_session_token=""

# shellcheck disable=SC1091
source "$ROOT_DIR/scripts/lib/live-cli-reporting.sh"
source "$ROOT_DIR/scripts/lib/live-cli-parser.sh"
source "$ROOT_DIR/scripts/lib/live-cli-runtime.sh"
source "$ROOT_DIR/scripts/lib/live-cli-fixtures.sh"
source "$ROOT_DIR/scripts/lib/live-cli-scenarios-web-rag.sh"
source "$ROOT_DIR/scripts/lib/live-cli-scenarios-jobs-source.sh"
source "$ROOT_DIR/scripts/lib/live-cli-scenarios-admin.sh"
source "$ROOT_DIR/scripts/lib/live-cli-scenarios-resources.sh"
source "$ROOT_DIR/scripts/lib/live-cli-scenarios.sh"
source "$ROOT_DIR/scripts/lib/live-cli-summary.sh"
