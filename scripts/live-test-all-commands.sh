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
mkdir -p "$OUTDIR/logs"
AXON_BIN="$(realpath -- "$AXON_BIN")"
REGISTRY="$(realpath -- "$REGISTRY")"
OUTDIR="$(realpath -- "$OUTDIR")"
REPORT="$OUTDIR/report.tsv"
BEHAVIOR_REPORT="$OUTDIR/behavioral-coverage.tsv"

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
BEHAVIOR_GLOBAL_OPTIONS="$OUTDIR/behavioral-global-options.txt"
BEHAVIOR_GLOBAL_VALUE_OPTIONS="$OUTDIR/behavioral-global-value-options.txt"
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

# shellcheck disable=SC1091
source "$ROOT_DIR/scripts/lib/live-cli-reporting.sh"
source "$ROOT_DIR/scripts/lib/live-cli-parser.sh"
source "$ROOT_DIR/scripts/lib/live-cli-runtime.sh"
source "$ROOT_DIR/scripts/lib/live-cli-scenarios-web-rag.sh"
source "$ROOT_DIR/scripts/lib/live-cli-scenarios-jobs-source.sh"
source "$ROOT_DIR/scripts/lib/live-cli-scenarios-admin.sh"
source "$ROOT_DIR/scripts/lib/live-cli-scenarios-resources.sh"
source "$ROOT_DIR/scripts/lib/live-cli-scenarios.sh"
source "$ROOT_DIR/scripts/lib/live-cli-summary.sh"
