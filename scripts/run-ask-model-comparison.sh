#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
LIB_DIR="$SCRIPT_DIR/run-ask-model-comparison.d"

source "$LIB_DIR/common.sh"
source "$LIB_DIR/profiles.sh"
source "$LIB_DIR/runner.sh"
source "$LIB_DIR/self-test.sh"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --questions) QUESTIONS_FILE="$2"; shift 2 ;;
    --out-dir) OUT_DIR="$2"; shift 2 ;;
    --axon-bin) AXON_BIN="$2"; shift 2 ;;
    --models) MODELS="$2"; shift 2 ;;
    --base-env) BASE_ENV_FILE="$2"; shift 2 ;;
    --dry-run) DRY_RUN=1; shift ;;
    --serial) SERIAL=1; shift ;;
    --no-explain) CAPTURE_EXPLAIN=0; shift ;;
    --skip-preflight) SKIP_PREFLIGHT=1; shift ;;
    --self-test) self_test; exit 0 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown option: $1" >&2; usage; exit 2 ;;
  esac
done

run_all
