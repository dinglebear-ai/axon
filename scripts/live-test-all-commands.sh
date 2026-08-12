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
  OUTDIR="$(mktemp -d "$live_test_root/$TS.XXXXXX")" || {
    echo "failed to allocate a unique live-test output directory" >&2
    exit 2
  }
fi
LIVE_RUN_ID="${TS//[^0-9]/}_$(stat -c '%d_%i' "$OUTDIR")"
PORT_LEASE_ROOT="${TMPDIR:-/tmp}/axon-live-port-leases"
mkdir -p "$PORT_LEASE_ROOT"
port_seed="$(printf '%s' "$LIVE_RUN_ID" | cksum | awk '{print $1}')"
for port_attempt in $(seq 0 1999); do
  LIVE_PORT_BASE=$((40000 + ((port_seed + port_attempt) % 2000) * 10))
  PORT_LEASE_DIR="$PORT_LEASE_ROOT/$LIVE_PORT_BASE"
  if mkdir "$PORT_LEASE_DIR" 2>/dev/null; then
    busy=0
    for port_offset in $(seq 0 9); do
      if ss -H -ltn "sport = :$((LIVE_PORT_BASE + port_offset))" 2>/dev/null | grep -q .; then
        busy=1
        break
      fi
    done
    if [ "$busy" -eq 0 ]; then
      break
    fi
    rmdir "$PORT_LEASE_DIR"
    PORT_LEASE_DIR=""
  fi
done
[ -n "${PORT_LEASE_DIR:-}" ] || {
  echo "failed to reserve an isolated live-test port block" >&2
  exit 2
}
LIVE_SERVE_PORT=$LIVE_PORT_BASE
LIVE_MCP_SERVE_PORT=$((LIVE_PORT_BASE + 1))
LIVE_SETUP_PORT=$((LIVE_PORT_BASE + 2))
LIVE_MCP_PORT=$((LIVE_PORT_BASE + 3))
LIVE_COMPOSE_PORT=$((LIVE_PORT_BASE + 4))
LIVE_TEI_PORT=$((LIVE_PORT_BASE + 5))
LIVE_CHROME_MANAGEMENT_PORT=$((LIVE_PORT_BASE + 6))
LIVE_CHROME_CDP_PORT=$((LIVE_PORT_BASE + 7))
LIVE_CHROME_DEVTOOLS_PORT=$((LIVE_PORT_BASE + 8))
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
source "$ROOT_DIR/scripts/lib/live-cli-scenarios-web-rag.sh"
source "$ROOT_DIR/scripts/lib/live-cli-scenarios-jobs-source.sh"
source "$ROOT_DIR/scripts/lib/live-cli-scenarios-admin.sh"
source "$ROOT_DIR/scripts/lib/live-cli-scenarios-resources.sh"
source "$ROOT_DIR/scripts/lib/live-cli-scenarios.sh"
source "$ROOT_DIR/scripts/lib/live-cli-summary.sh"
